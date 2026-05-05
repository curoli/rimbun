<script setup lang="ts">
import { computed, onMounted } from "vue";
import { RouterLink, RouterView, useRoute } from "vue-router";

import { useAuthStore } from "./stores/auth";

const auth = useAuthStore();
const route = useRoute();

const isAuthPage = computed(() => route.path.startsWith("/login"));

onMounted(() => {
  void auth.restoreSession();
});
</script>

<template>
  <div class="app-shell">
    <header v-if="!isAuthPage" class="topbar">
      <RouterLink class="brand" to="/">Rimbun</RouterLink>
      <div class="topbar-meta">
        <span v-if="auth.user" class="identity">
          {{ auth.user.display_name }}
          <small>@{{ auth.user.username }}</small>
        </span>
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

.identity {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  font-size: 0.95rem;
}

.identity small {
  color: #705948;
}

.topbar-link {
  color: #8e4b16;
}
</style>
