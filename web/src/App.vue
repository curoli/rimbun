<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { RouterLink, RouterView, useRoute, useRouter } from "vue-router";

import { useAuthStore } from "./stores/auth";
import { useSiteStore } from "./stores/site";
import { getDocument } from "./api/documents";
import LanguageSelector from "./components/LanguageSelector.vue";
import { resetDocumentLanguage, setDocumentLanguage } from "./i18n";

const auth = useAuthStore();
const site = useSiteStore();
const route = useRoute();
const router = useRouter();
const accountMenuOpen = ref(false);
const accountMenuRef = ref<HTMLElement | null>(null);
let languageLoadSequence = 0;

const isAuthPage = computed(() => route.path.startsWith("/login"));
const canManageAccounts = computed(() =>
  auth.user ? auth.user.role === "admin" : false,
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

async function applyRouteLanguage() {
  const sequence = ++languageLoadSequence;
  resetDocumentLanguage();
  const documentRef = route.params.documentRef;
  if (typeof documentRef !== "string") {
    return;
  }

  try {
    const data = await getDocument(documentRef);
    if (sequence === languageLoadSequence) {
      setDocumentLanguage(data.document.markdown_policy?.ui_language);
    }
  } catch {
    // The page itself presents access and loading errors.
  }
}

watch(() => route.fullPath, () => void applyRouteLanguage(), { immediate: true });

onMounted(() => {
  void auth.restoreSession();
  void site.load();
  document.addEventListener("click", handleDocumentClick);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", handleDocumentClick);
});
</script>

<template>
  <div class="app-shell">
    <header v-if="!isAuthPage" class="topbar">
      <RouterLink class="brand" to="/">{{ site.brandName }}</RouterLink>
      <div class="topbar-meta">
        <LanguageSelector />
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
              {{ $t("Your profile") }}
              <small>{{ $t("View your account details and change display name or password.") }}</small>
            </RouterLink>

            <RouterLink
              v-if="canManageAccounts"
              class="menu-link"
              to="/admin/site-settings"
              @click="closeAccountMenu"
            >
              {{ $t("Site Settings") }}
              <small>{{ $t("Change the site name and browser title.") }}</small>
            </RouterLink>

            <RouterLink
              v-if="canManageAccounts"
              class="menu-link"
              to="/admin/users"
              @click="closeAccountMenu"
            >
              {{ $t("User administration") }}
              <small>{{ $t("See all registered users.") }}</small>
            </RouterLink>

            <RouterLink
              v-if="canManageAccounts"
              class="menu-link"
              to="/admin/variant-collections"
              @click="closeAccountMenu"
            >
              {{ $t("Variant Collections") }}
              <small>{{ $t("Manage reusable test variants and generate test documents.") }}</small>
            </RouterLink>

            <div v-if="auth.availableAccounts.length" class="account-list">
              <span class="account-list-label">{{ $t("Available accounts") }}</span>
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
                  <template v-if="account.sessionToken === auth.activeSessionToken"> • {{ $t("active") }}</template>
                </small>
              </button>
            </div>

            <button type="button" class="menu-item" @click="handleLoginAnotherUser">
              {{ $t("Log in existing account") }}
              <small>{{ $t("Authenticate an already existing user and keep it available in this browser.") }}</small>
            </button>

            <button
              v-if="canManageAccounts"
              type="button"
              class="menu-item"
              @click="handleAddAccount"
            >
              {{ $t("Create new account") }}
              <small>{{ $t("Create another user account and keep it available in this browser.") }}</small>
            </button>

            <button type="button" class="menu-item danger" @click="handleLogout">
              {{ $t("Logout current account") }}
              <small>{{ $t("End only the currently active account session.") }}</small>
            </button>
          </div>
        </div>
        <RouterLink v-else class="topbar-link" to="/login">{{ $t("Login") }}</RouterLink>
      </div>
    </header>
    <div v-if="isAuthPage" class="auth-language"><LanguageSelector /></div>
    <RouterView />
  </div>
</template>

<style>
.auth-language {
  position: fixed;
  top: 1rem;
  right: 1rem;
  z-index: 10;
}

:root {
  color-scheme: light;
  font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
  --bg-radial: rgba(201, 146, 72, 0.18);
  --bg-start: #f6f0e8;
  --bg-end: #f1ebe2;
  --text-primary: #23180f;
  --text-secondary: #6f5947;
  --text-muted: #705948;
  --text-soft: #5b4331;
  --text-strong: #2d1d12;
  --text-on-accent: #fff8ef;
  --accent: #8e4b16;
  --accent-soft: #f1dcc4;
  --accent-hover: #f5e7d5;
  --accent-contrast: #5f3b1c;
  --surface-hero: linear-gradient(135deg, rgba(255, 248, 238, 0.98), rgba(235, 212, 184, 0.94));
  --surface-panel: rgba(255, 252, 247, 0.94);
  --surface-panel-strong: rgba(255, 252, 247, 0.98);
  --surface-topbar: rgba(255, 251, 246, 0.76);
  --surface-input: #ffffff;
  --surface-raised: #fffaf4;
  --surface-toggle: #f0e4d7;
  --border-soft: rgba(35, 24, 15, 0.08);
  --border-medium: rgba(35, 24, 15, 0.12);
  --border-strong: rgba(35, 24, 15, 0.14);
  --shadow-soft: rgba(35, 24, 15, 0.12);
  --danger: #9d2a16;
  --danger-strong: #8e2616;
  --hero-gradient:
    linear-gradient(160deg, rgba(142, 75, 22, 0.9), rgba(196, 130, 49, 0.76)),
    #8e4b16;
  background:
    radial-gradient(circle at top left, var(--bg-radial), transparent 32%),
    linear-gradient(180deg, var(--bg-start) 0%, var(--bg-end) 100%);
  color: var(--text-primary);
}

:root[data-rimbun-theme="forest-paper"] {
  --bg-radial: rgba(80, 119, 89, 0.16);
  --bg-start: #edf1e6;
  --bg-end: #e3e7de;
  --text-primary: #182016;
  --text-secondary: #566452;
  --text-muted: #5a6856;
  --text-soft: #495846;
  --text-strong: #1f291d;
  --text-on-accent: #f7fbf4;
  --accent: #47624a;
  --accent-soft: #d7e3cf;
  --accent-hover: #e6efe0;
  --accent-contrast: #27402b;
  --surface-hero: linear-gradient(135deg, rgba(247, 250, 241, 0.98), rgba(212, 224, 199, 0.94));
  --surface-panel: rgba(249, 252, 246, 0.94);
  --surface-panel-strong: rgba(249, 252, 246, 0.98);
  --surface-topbar: rgba(247, 250, 243, 0.78);
  --surface-input: #ffffff;
  --surface-raised: #f7fbf4;
  --surface-toggle: #dde8d8;
  --border-soft: rgba(24, 32, 22, 0.08);
  --border-medium: rgba(24, 32, 22, 0.12);
  --border-strong: rgba(24, 32, 22, 0.16);
  --shadow-soft: rgba(24, 32, 22, 0.12);
  --danger: #9b3f2a;
  --danger-strong: #843020;
  --hero-gradient:
    linear-gradient(160deg, rgba(71, 98, 74, 0.9), rgba(125, 154, 110, 0.76)),
    #47624a;
}

:root[data-rimbun-theme="sea-glass"] {
  --bg-radial: rgba(73, 146, 152, 0.15);
  --bg-start: #ecf4f4;
  --bg-end: #e1ebeb;
  --text-primary: #162126;
  --text-secondary: #52656a;
  --text-muted: #5b6f74;
  --text-soft: #476067;
  --text-strong: #1b2d33;
  --text-on-accent: #f5fcfd;
  --accent: #2c6f77;
  --accent-soft: #d1e7e8;
  --accent-hover: #e1f0f1;
  --accent-contrast: #174950;
  --surface-hero: linear-gradient(135deg, rgba(243, 251, 251, 0.98), rgba(202, 229, 228, 0.94));
  --surface-panel: rgba(248, 252, 252, 0.95);
  --surface-panel-strong: rgba(248, 252, 252, 0.99);
  --surface-topbar: rgba(244, 250, 250, 0.8);
  --surface-input: #ffffff;
  --surface-raised: #f5fbfb;
  --surface-toggle: #dbe9ea;
  --border-soft: rgba(22, 33, 38, 0.08);
  --border-medium: rgba(22, 33, 38, 0.12);
  --border-strong: rgba(22, 33, 38, 0.16);
  --shadow-soft: rgba(22, 33, 38, 0.12);
  --danger: #a54635;
  --danger-strong: #8c3729;
  --hero-gradient:
    linear-gradient(160deg, rgba(44, 111, 119, 0.9), rgba(95, 164, 164, 0.76)),
    #2c6f77;
}

:root[data-rimbun-theme="rose-evening"] {
  --bg-radial: rgba(162, 96, 82, 0.16);
  --bg-start: #f6ece8;
  --bg-end: #eee3df;
  --text-primary: #271715;
  --text-secondary: #71554f;
  --text-muted: #795c57;
  --text-soft: #644640;
  --text-strong: #321f1c;
  --text-on-accent: #fff7f4;
  --accent: #985245;
  --accent-soft: #efd4cb;
  --accent-hover: #f6e3dc;
  --accent-contrast: #643229;
  --surface-hero: linear-gradient(135deg, rgba(255, 246, 243, 0.98), rgba(236, 207, 198, 0.94));
  --surface-panel: rgba(255, 250, 248, 0.95);
  --surface-panel-strong: rgba(255, 250, 248, 0.99);
  --surface-topbar: rgba(255, 248, 245, 0.8);
  --surface-input: #ffffff;
  --surface-raised: #fff7f4;
  --surface-toggle: #f1dfd8;
  --border-soft: rgba(39, 23, 21, 0.08);
  --border-medium: rgba(39, 23, 21, 0.12);
  --border-strong: rgba(39, 23, 21, 0.16);
  --shadow-soft: rgba(39, 23, 21, 0.12);
  --danger: #ac3d3a;
  --danger-strong: #94312d;
  --hero-gradient:
    linear-gradient(160deg, rgba(152, 82, 69, 0.9), rgba(204, 129, 113, 0.76)),
    #985245;
}

:root[data-rimbun-theme="midnight-ink"] {
  color-scheme: dark;
  --bg-radial: rgba(85, 126, 193, 0.18);
  --bg-start: #111827;
  --bg-end: #0b1220;
  --text-primary: #ecf2ff;
  --text-secondary: #afbdd7;
  --text-muted: #91a4c5;
  --text-soft: #c4d0e6;
  --text-strong: #f6f8ff;
  --text-on-accent: #08111e;
  --accent: #8db4ff;
  --accent-soft: #243550;
  --accent-hover: #304668;
  --accent-contrast: #d7e5ff;
  --surface-hero: linear-gradient(135deg, rgba(25, 36, 60, 0.96), rgba(42, 60, 98, 0.92));
  --surface-panel: rgba(18, 27, 43, 0.94);
  --surface-panel-strong: rgba(18, 27, 43, 0.99);
  --surface-topbar: rgba(11, 18, 32, 0.84);
  --surface-input: #162235;
  --surface-raised: #132033;
  --surface-toggle: #21314b;
  --border-soft: rgba(196, 208, 230, 0.12);
  --border-medium: rgba(196, 208, 230, 0.18);
  --border-strong: rgba(196, 208, 230, 0.24);
  --shadow-soft: rgba(0, 0, 0, 0.3);
  --danger: #ff8a7d;
  --danger-strong: #ff6f61;
  --hero-gradient:
    linear-gradient(160deg, rgba(141, 180, 255, 0.86), rgba(100, 146, 235, 0.78)),
    #8db4ff;
}

:root[data-rimbun-theme="citrus-ledger"] {
  --bg-radial: rgba(177, 202, 70, 0.16);
  --bg-start: #f5f6df;
  --bg-end: #ecefcf;
  --text-primary: #202111;
  --text-secondary: #60653e;
  --text-muted: #6b7245;
  --text-soft: #535735;
  --text-strong: #282a16;
  --text-on-accent: #f9fff0;
  --accent: #7a8c1f;
  --accent-soft: #e2e9b8;
  --accent-hover: #ebf0c8;
  --accent-contrast: #49550d;
  --surface-hero: linear-gradient(135deg, rgba(252, 252, 239, 0.98), rgba(224, 234, 173, 0.94));
  --surface-panel: rgba(252, 253, 244, 0.94);
  --surface-panel-strong: rgba(252, 253, 244, 0.98);
  --surface-topbar: rgba(248, 250, 231, 0.82);
  --surface-input: #fffffb;
  --surface-raised: #fbfceb;
  --surface-toggle: #e7edbf;
  --border-soft: rgba(32, 33, 17, 0.08);
  --border-medium: rgba(32, 33, 17, 0.12);
  --border-strong: rgba(32, 33, 17, 0.16);
  --shadow-soft: rgba(32, 33, 17, 0.12);
  --danger: #a84b21;
  --danger-strong: #8a3917;
  --hero-gradient:
    linear-gradient(160deg, rgba(122, 140, 31, 0.9), rgba(183, 202, 70, 0.74)),
    #7a8c1f;
}

:root[data-rimbun-theme="violet-archive"] {
  --bg-radial: rgba(129, 93, 151, 0.16);
  --bg-start: #f1eaf3;
  --bg-end: #e7dfea;
  --text-primary: #241928;
  --text-secondary: #6a5574;
  --text-muted: #755f7f;
  --text-soft: #5d4867;
  --text-strong: #2d2032;
  --text-on-accent: #fbf4ff;
  --accent: #7a5290;
  --accent-soft: #e7d4ef;
  --accent-hover: #f0e1f5;
  --accent-contrast: #563267;
  --surface-hero: linear-gradient(135deg, rgba(252, 245, 255, 0.98), rgba(222, 198, 234, 0.94));
  --surface-panel: rgba(253, 249, 254, 0.95);
  --surface-panel-strong: rgba(253, 249, 254, 0.99);
  --surface-topbar: rgba(249, 244, 251, 0.82);
  --surface-input: #ffffff;
  --surface-raised: #fcf7fd;
  --surface-toggle: #ebdcf1;
  --border-soft: rgba(36, 25, 40, 0.08);
  --border-medium: rgba(36, 25, 40, 0.12);
  --border-strong: rgba(36, 25, 40, 0.16);
  --shadow-soft: rgba(36, 25, 40, 0.12);
  --danger: #b24461;
  --danger-strong: #943450;
  --hero-gradient:
    linear-gradient(160deg, rgba(122, 82, 144, 0.9), rgba(175, 132, 198, 0.76)),
    #7a5290;
}

:root[data-rimbun-theme="volcanic-clay"] {
  color-scheme: dark;
  --bg-radial: rgba(198, 94, 54, 0.16);
  --bg-start: #1e1614;
  --bg-end: #120e0d;
  --text-primary: #f3e8e1;
  --text-secondary: #c1aa9d;
  --text-muted: #ae9689;
  --text-soft: #d8c3b8;
  --text-strong: #fff4ee;
  --text-on-accent: #190d08;
  --accent: #d4744a;
  --accent-soft: #4a2b22;
  --accent-hover: #61372b;
  --accent-contrast: #ffd8c6;
  --surface-hero: linear-gradient(135deg, rgba(54, 31, 25, 0.96), rgba(97, 55, 43, 0.92));
  --surface-panel: rgba(33, 23, 20, 0.95);
  --surface-panel-strong: rgba(33, 23, 20, 0.99);
  --surface-topbar: rgba(18, 14, 13, 0.86);
  --surface-input: #241916;
  --surface-raised: #2b1d19;
  --surface-toggle: #3a2621;
  --border-soft: rgba(243, 232, 225, 0.12);
  --border-medium: rgba(243, 232, 225, 0.18);
  --border-strong: rgba(243, 232, 225, 0.24);
  --shadow-soft: rgba(0, 0, 0, 0.34);
  --danger: #ff8f72;
  --danger-strong: #ff7758;
  --hero-gradient:
    linear-gradient(160deg, rgba(212, 116, 74, 0.88), rgba(241, 157, 97, 0.72)),
    #d4744a;
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
  border-bottom: 1px solid var(--border-medium);
  background: var(--surface-topbar);
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
  background: color-mix(in srgb, var(--surface-input) 72%, transparent);
  cursor: pointer;
}

.identity {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  font-size: 0.95rem;
}

.identity small {
  color: var(--text-muted);
}

.account-chevron {
  color: var(--text-muted);
  font-size: 0.72rem;
}

.account-dropdown {
  position: absolute;
  right: 0;
  top: calc(100% + 0.55rem);
  min-width: 280px;
  max-height: calc(100vh - 5rem);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  padding: 0.8rem;
  border-radius: 1rem;
  background: var(--surface-panel-strong);
  box-shadow:
    0 18px 40px var(--shadow-soft),
    inset 0 0 0 1px var(--border-soft);
}

.account-summary {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  padding: 0.35rem 0.4rem 0.6rem;
  border-bottom: 1px solid var(--border-soft);
}

.account-list {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.account-list-label {
  padding: 0 0.4rem;
  color: var(--text-muted);
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.account-summary span {
  color: var(--text-muted);
}

.role-badge {
  display: inline-flex;
  width: fit-content;
  margin-top: 0.2rem;
  padding: 0.2rem 0.5rem;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--accent-contrast);
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
  color: var(--text-strong);
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
  color: var(--text-strong);
}

.menu-item:hover {
  background: var(--accent-hover);
}

.menu-link:hover {
  background: var(--accent-hover);
}

.menu-item.selected {
  background: var(--accent-soft);
}

.menu-item small {
  color: var(--text-muted);
}

.menu-link small {
  color: var(--text-muted);
}

.menu-item.danger {
  color: var(--danger-strong);
}

.topbar-link {
  color: var(--accent);
}
</style>
