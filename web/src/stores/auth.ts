import { defineStore } from "pinia";

import * as authApi from "../api/auth";
import { ApiError } from "../api/client";
import type { LoginPayload, RegisterPayload } from "../api/auth";
import type { User } from "../api/types";

type AuthState = {
  user: User | null;
  isLoading: boolean;
  error: string | null;
};

export const useAuthStore = defineStore("auth", {
  state: (): AuthState => ({
    user: null,
    isLoading: false,
    error: null,
  }),
  actions: {
    async restoreSession() {
      if (this.user || this.isLoading) {
        return;
      }

      this.isLoading = true;
      this.error = null;
      try {
        this.user = await authApi.me();
      } catch (error) {
        if (!(error instanceof ApiError) || error.status !== 401) {
          this.error = error instanceof Error ? error.message : "Session restore failed";
        }
        this.user = null;
      } finally {
        this.isLoading = false;
      }
    },
    async login(payload: LoginPayload) {
      this.isLoading = true;
      this.error = null;
      try {
        this.user = await authApi.login(payload);
      } catch (error) {
        this.error = error instanceof Error ? error.message : "Login failed";
        throw error;
      } finally {
        this.isLoading = false;
      }
    },
    async register(payload: RegisterPayload) {
      this.isLoading = true;
      this.error = null;
      try {
        this.user = await authApi.register(payload);
      } catch (error) {
        this.error = error instanceof Error ? error.message : "Registration failed";
        throw error;
      } finally {
        this.isLoading = false;
      }
    },
    async logout() {
      this.isLoading = true;
      this.error = null;
      try {
        await authApi.logout();
        this.user = null;
      } catch (error) {
        this.error = error instanceof Error ? error.message : "Logout failed";
        throw error;
      } finally {
        this.isLoading = false;
      }
    },
  },
});
