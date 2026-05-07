<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";

import { useAuthStore } from "./stores/auth";

const auth = useAuthStore();
const route = useRoute();
const router = useRouter();
const accountMenuOpen = ref(false);
const accountMenuRef = ref<HTMLElement | null>(null);

const isAuthPage = computed(() => route.path.startsWith("/login"));
const canManageAccounts = computed(() =>
  auth.user ? ["privileged", "admin"].includes(auth.user.role) : false,
);

function toggleAccountMenu() {
  accountMenuOpen.value = !accountMenuOpen.value;
}

function closeAccountMenu() {
  accountMenuOpen.value = false;
}

async function handleLogout() {
  await auth.logout();
  closeAccountMenu();
  await router.push("/login");
}

async function handleAddAccount() {
  closeAccountMenu();
  await router.push({ path: "/login", query: { mode: "register" } });
}

async function handleLoginAnotherUser() {
  closeAccountMenu();
  await router.push({ path: "/login", query: { mode: "login" } });
}

async function handleSwitchToAccount(sessionToken: string) {
  await auth.switchAccount(sessionToken);
  closeAccountMenu();
}

function handleDocumentClick(event: MouseEvent) {
  if (!accountMenuOpen.value || !accountMenuRef.value) {
    return;
  }

  const target = event.target;
  if (!(target instanceof Node)) {
    return;
  }

  if (!accountMenuRef.value.contains(target)) {
    closeAccountMenu();
  }
}

onMounted(() => {
  void auth.restoreSession();
  document.addEventListener("click", handleDocumentClick);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", handleDocumentClick);
});
</script>

<template>
  <div class="app-shell">
    <header v-if="!isAuthPage" class="topbar">
      <RouterLink class="brand" to="/">Rimbun</RouterLink>
      <div class="topbar-meta">
        <div v-if="auth.user" ref="accountMenuRef" class="account-menu">
          <button class="account-trigger" type="button" @click.stop="toggleAccountMenu">
            <span class="identity">
              {{ auth.user.display_name }}
              <small>@{{ auth.user.username }}</small>
            </span>
            <span class="account-chevron">{{ accountMenuOpen ? "▲" : "▼" }}</span>
          </button>

          <div v-if="accountMenuOpen" class="account-dropdown">
            <div class="account-summary">
              <strong>{{ auth.user.display_name }}</strong>
              <span>@{{ auth.user.username }}</span>
              <span class="role-badge">{{ auth.user.role }}</span>
            </div>

            <RouterLink class="menu-link" to="/profile" @click="closeAccountMenu">
              Your profile
              <small>View your account details and change display name or password.</small>
            </RouterLink>

            <RouterLink
              v-if="canManageAccounts"
              class="menu-link"
              to="/admin/users"
              @click="closeAccountMenu"
            >
              User administration
              <small>See all registered users.</small>
            </RouterLink>

            <div v-if="auth.availableAccounts.length" class="account-list">
              <span class="account-list-label">Available accounts</span>
              <button
                v-for="account in auth.availableAccounts"
                :key="account.sessionToken"
                type="button"
                class="menu-item"
                :class="{ selected: account.sessionToken === auth.activeSessionToken }"
                @click="handleSwitchToAccount(account.sessionToken)"
              >
                {{ account.user.display_name }} @{{ account.user.username }}
                <small>
                  {{ account.user.role }}
                  <template v-if="account.sessionToken === auth.activeSessionToken"> • active</template>
                </small>
              </button>
            </div>

            <button type="button" class="menu-item" @click="handleLoginAnotherUser">
              Log in existing account
              <small>Authenticate an already existing user and keep it available in this browser.</small>
            </button>

            <button
              v-if="canManageAccounts"
              type="button"
              class="menu-item"
              @click="handleAddAccount"
            >
              Create new account
              <small>Create another user account and keep it available in this browser.</small>
            </button>

            <button type="button" class="menu-item danger" @click="handleLogout">
              Logout current account
              <small>End only the currently active account session.</small>
            </button>
          </div>
        </div>
        <RouterLink v-else class="topbar-link" to="/login">Login</RouterLink>
      </div>
    </header>
    <RouterView />
  </div>
</template>

<style>
:root {
  color-scheme: light;
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
  background:
    radial-gradient(circle at top left, rgba(201, 146, 72, 0.18), transparent 32%),
    linear-gradient(180deg, #f6f0e8 0%, #f1ebe2 100%);
  color: #23180f;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  min-width: 320px;
  min-height: 100vh;
}

a {
  color: inherit;
  text-decoration: none;
}

button,
input,
textarea,
select {
  font: inherit;
}

#app {
  min-height: 100vh;
}

.app-shell {
  min-height: 100vh;
}

.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem 1.5rem;
  border-bottom: 1px solid rgba(35, 24, 15, 0.12);
  background: rgba(255, 251, 246, 0.76);
  backdrop-filter: blur(14px);
  position: sticky;
  top: 0;
  z-index: 10;
}

.brand {
  font-size: 1.1rem;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.topbar-meta {
  display: flex;
  align-items: center;
  gap: 1rem;
}

.account-menu {
  position: relative;
}

.account-trigger {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  border: 0;
  border-radius: 1rem;
  padding: 0.55rem 0.75rem;
  background: rgba(255, 255, 255, 0.72);
  cursor: pointer;
}

.identity {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  font-size: 0.95rem;
}

.identity small {
  color: #705948;
}

.account-chevron {
  color: #705948;
  font-size: 0.72rem;
}

.account-dropdown {
  position: absolute;
  right: 0;
  top: calc(100% + 0.55rem);
  min-width: 280px;
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  padding: 0.8rem;
  border-radius: 1rem;
  background: rgba(255, 252, 247, 0.98);
  box-shadow:
    0 18px 40px rgba(35, 24, 15, 0.12),
    inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

.account-summary {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  padding: 0.35rem 0.4rem 0.6rem;
  border-bottom: 1px solid rgba(35, 24, 15, 0.08);
}

.account-list {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.account-list-label {
  padding: 0 0.4rem;
  color: #705948;
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.account-summary span {
  color: #705948;
}

.role-badge {
  display: inline-flex;
  width: fit-content;
  margin-top: 0.2rem;
  padding: 0.2rem 0.5rem;
  border-radius: 999px;
  background: #f1dcc4;
  color: #5f3b1c;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  font-size: 0.68rem;
}

.menu-item {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  width: 100%;
  border: 0;
  border-radius: 0.85rem;
  padding: 0.75rem 0.8rem;
  text-align: left;
  background: transparent;
  color: #2d1d12;
  cursor: pointer;
}

.menu-link {
  display: flex;
  flex-direction: column;
  gap: 0.15rem;
  width: 100%;
  border-radius: 0.85rem;
  padding: 0.75rem 0.8rem;
  text-align: left;
  color: #2d1d12;
}

.menu-item:hover {
  background: #f5e7d5;
}

.menu-link:hover {
  background: #f5e7d5;
}

.menu-item.selected {
  background: #f1dcc4;
}

.menu-item small {
  color: #705948;
}

.menu-link small {
  color: #705948;
}

.menu-item.danger {
  color: #8e2616;
}

.topbar-link {
  color: #8e4b16;
}
</style>
