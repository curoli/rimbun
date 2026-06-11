import { defineStore } from "pinia";

import * as authApi from "../api/auth";
import { ApiError, setActiveSessionToken } from "../api/client";
import type { LoginPayload, RegisterPayload } from "../api/auth";
import type { AuthSession, User } from "../api/types";

const STORAGE_KEY = "rimbun.available_accounts";
const ACTIVE_STORAGE_KEY = "rimbun.active_session_token";

export type AvailableAccount = {
  user: User;
  sessionToken: string;
};

type AuthState = {
  user: User | null;
  availableAccounts: AvailableAccount[];
  activeSessionToken: string | null;
  isLoading: boolean;
  error: string | null;
  hydrated: boolean;
  sessionChecked: boolean;
};

function isUser(value: unknown): value is User {
  if (!value || typeof value !== "object") {
    return false;
  }

  const candidate = value as Partial<User>;
  return (
    typeof candidate.id === "string" &&
    typeof candidate.username === "string" &&
    typeof candidate.display_name === "string" &&
    typeof candidate.email === "string" &&
    typeof candidate.role === "string" &&
    typeof candidate.created_at === "string"
  );
}

function normalizeStoredAccount(value: unknown): AvailableAccount | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const candidate = value as {
    user?: unknown;
    sessionToken?: unknown;
    session_token?: unknown;
  };

  const sessionToken =
    typeof candidate.sessionToken === "string"
      ? candidate.sessionToken
      : typeof candidate.session_token === "string"
        ? candidate.session_token
        : null;

  if (!sessionToken || !isUser(candidate.user)) {
    return null;
  }

  return {
    user: candidate.user,
    sessionToken,
  };
}

function loadStoredAccounts(): AvailableAccount[] {
  if (typeof window === "undefined") {
    return [];
  }

  const raw = window.localStorage.getItem(STORAGE_KEY);
  if (!raw) {
    return [];
  }

  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }

    return parsed
      .map(normalizeStoredAccount)
      .filter((account): account is AvailableAccount => account !== null);
  } catch {
    return [];
  }
}

function loadActiveSessionToken(): string | null {
  if (typeof window === "undefined") {
    return null;
  }
  return window.localStorage.getItem(ACTIVE_STORAGE_KEY);
}

function persistAccounts(accounts: AvailableAccount[], activeSessionToken: string | null) {
  if (typeof window === "undefined") {
    return;
  }

  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(accounts));
  if (activeSessionToken) {
    window.localStorage.setItem(ACTIVE_STORAGE_KEY, activeSessionToken);
  } else {
    window.localStorage.removeItem(ACTIVE_STORAGE_KEY);
  }
}

export const useAuthStore = defineStore("auth", {
  state: (): AuthState => ({
    user: null,
    availableAccounts: [],
    activeSessionToken: null,
    isLoading: false,
    error: null,
    hydrated: false,
    sessionChecked: false,
  }),
  actions: {
    hydrateFromStorage() {
      if (this.hydrated) {
        return;
      }

      this.availableAccounts = loadStoredAccounts();
      this.activeSessionToken = loadActiveSessionToken();
      setActiveSessionToken(this.activeSessionToken);

      const activeAccount = this.availableAccounts.find(
        (account) => account.sessionToken === this.activeSessionToken,
      );
      this.user = activeAccount?.user ?? null;
      this.hydrated = true;
    },
    storeAccount(authSession: AuthSession) {
      const filtered = this.availableAccounts.filter(
        (account) => account.user.id !== authSession.user.id && account.sessionToken !== authSession.session_token,
      );
      this.availableAccounts = [
        ...filtered,
        {
          user: authSession.user,
          sessionToken: authSession.session_token,
        },
      ];
      this.activeSessionToken = authSession.session_token;
      this.user = authSession.user;
      setActiveSessionToken(authSession.session_token);
      persistAccounts(this.availableAccounts, this.activeSessionToken);
    },
    updateCurrentUser(user: User) {
      this.user = user;
      if (this.activeSessionToken) {
        this.availableAccounts = this.availableAccounts.map((account) =>
          account.sessionToken === this.activeSessionToken ? { ...account, user } : account,
        );
        persistAccounts(this.availableAccounts, this.activeSessionToken);
      }
    },
    removeAccount(sessionToken: string) {
      this.availableAccounts = this.availableAccounts.filter((account) => account.sessionToken !== sessionToken);
      if (this.activeSessionToken === sessionToken) {
        this.activeSessionToken = null;
      }
      persistAccounts(this.availableAccounts, this.activeSessionToken);
    },
    async restoreSession() {
      this.hydrateFromStorage();
      if (this.isLoading || this.sessionChecked) {
        return;
      }

      this.isLoading = true;
      this.error = null;
      try {
        setActiveSessionToken(this.activeSessionToken);
        try {
          this.user = await authApi.me();
          if (this.activeSessionToken && this.user) {
            this.updateCurrentUser(this.user);
          }
        } catch (error) {
          if (error instanceof ApiError && error.status === 401 && this.activeSessionToken) {
            const staleSessionToken = this.activeSessionToken;
            this.removeAccount(staleSessionToken);
            this.activeSessionToken = null;
            setActiveSessionToken(null);
            this.user = await authApi.me();
          } else {
            throw error;
          }
        }
      } catch (error) {
        if (!(error instanceof ApiError) || error.status !== 401) {
          this.error = error instanceof Error ? error.message : "Session restore failed";
        }
        if (this.activeSessionToken) {
          this.removeAccount(this.activeSessionToken);
        }
        setActiveSessionToken(null);
        this.user = null;
      } finally {
        this.sessionChecked = true;
        this.isLoading = false;
      }
    },
    async login(payload: LoginPayload) {
      this.hydrateFromStorage();
      this.isLoading = true;
      this.error = null;
      try {
        const authSession = await authApi.login(payload);
        this.storeAccount(authSession);
        this.sessionChecked = true;
      } catch (error) {
        this.error = error instanceof Error ? error.message : "Login failed";
        throw error;
      } finally {
        this.isLoading = false;
      }
    },
    async register(payload: RegisterPayload) {
      this.hydrateFromStorage();
      this.isLoading = true;
      this.error = null;
      try {
        const authSession = await authApi.register(payload);
        this.storeAccount(authSession);
        this.sessionChecked = true;
      } catch (error) {
        this.error = error instanceof Error ? error.message : "Registration failed";
        throw error;
      } finally {
        this.isLoading = false;
      }
    },
    async switchAccount(sessionToken: string) {
      this.hydrateFromStorage();
      this.isLoading = true;
      this.error = null;
      try {
        this.activeSessionToken = sessionToken;
        setActiveSessionToken(sessionToken);
        const user = await authApi.me();
        this.updateCurrentUser(user);
        this.sessionChecked = true;
      } catch (error) {
        this.removeAccount(sessionToken);
        setActiveSessionToken(this.activeSessionToken);
        this.error = error instanceof Error ? error.message : "Account switch failed";
        throw error;
      } finally {
        this.isLoading = false;
      }
    },
    async logout() {
      this.hydrateFromStorage();
      this.isLoading = true;
      this.error = null;
      const currentSessionToken = this.activeSessionToken;
      try {
        await authApi.logout();
        if (currentSessionToken) {
          this.removeAccount(currentSessionToken);
        }
        const nextAccount = this.availableAccounts[this.availableAccounts.length - 1] ?? null;
        if (nextAccount) {
          this.activeSessionToken = nextAccount.sessionToken;
          setActiveSessionToken(nextAccount.sessionToken);
          const user = await authApi.me();
          this.updateCurrentUser(user);
        } else {
          this.activeSessionToken = null;
          this.user = null;
          setActiveSessionToken(null);
          persistAccounts(this.availableAccounts, this.activeSessionToken);
        }
        this.sessionChecked = true;
      } catch (error) {
        this.error = error instanceof Error ? error.message : "Logout failed";
        throw error;
      } finally {
        this.isLoading = false;
      }
    },
  },
});
