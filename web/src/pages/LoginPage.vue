<script setup lang="ts">
import { reactive, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import { useAuthStore } from "../stores/auth";

const auth = useAuthStore();
const router = useRouter();
const route = useRoute();
const mode = ref<"login" | "register">("login");

const loginForm = reactive({
  identifier: "",
  password: "",
});

const registerForm = reactive({
  username: "",
  display_name: "",
  email: "",
  password: "",
});

async function submit() {
  if (mode.value === "login") {
    await auth.login({ ...loginForm });
  } else {
    await auth.register({ ...registerForm });
  }
  await router.push("/");
}

watch(
  () => route.query.mode,
  (rawMode) => {
    mode.value = rawMode === "register" ? "register" : "login";
  },
  { immediate: true },
);
</script>

<template>
  <main class="auth-layout">
    <section class="auth-panel">
      <div class="auth-copy">
        <p class="eyebrow">Rimbun MVP</p>
        <h1>Semantic versioning for prose.</h1>
        <p>
          Sign in to browse documents, switch between alternatives, and publish a new section version.
        </p>
      </div>

      <div class="auth-form-panel">
        <div class="mode-toggle">
          <button :class="{ active: mode === 'login' }" @click="mode = 'login'">Login</button>
          <button :class="{ active: mode === 'register' }" @click="mode = 'register'">Register</button>
        </div>

        <form class="auth-form" @submit.prevent="submit">
          <template v-if="mode === 'login'">
            <label>
              Identifier
              <input v-model="loginForm.identifier" placeholder="username or email" />
            </label>
            <label>
              Password
              <input v-model="loginForm.password" type="password" placeholder="password" />
            </label>
          </template>

          <template v-else>
            <label>
              Username
              <input v-model="registerForm.username" placeholder="username" />
            </label>
            <label>
              Display name
              <input v-model="registerForm.display_name" placeholder="display name" />
            </label>
            <label>
              Email
              <input v-model="registerForm.email" type="email" placeholder="email" />
            </label>
            <label>
              Password
              <input v-model="registerForm.password" type="password" placeholder="min. 8 characters" />
            </label>
          </template>

          <p v-if="auth.error" class="auth-error">{{ auth.error }}</p>
          <button class="submit" :disabled="auth.isLoading">
            {{ auth.isLoading ? "Working..." : mode === "login" ? "Login" : "Create account" }}
          </button>
        </form>
      </div>
    </section>
  </main>
</template>

<style scoped>
.auth-layout {
  min-height: 100vh;
  display: grid;
  place-items: center;
  padding: 2rem;
}

.auth-panel {
  width: min(980px, 100%);
  display: grid;
  grid-template-columns: 1.2fr 1fr;
  gap: 1.5rem;
}

.auth-copy,
.auth-form-panel {
  border-radius: 1.6rem;
  padding: 2rem;
  border: 1px solid var(--border-soft);
}

.auth-copy {
  background: var(--hero-gradient);
  color: var(--text-on-accent);
}

.auth-form-panel {
  background: var(--surface-panel);
}

.eyebrow {
  margin: 0 0 0.5rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  font-size: 0.82rem;
}

.auth-copy h1 {
  margin: 0 0 1rem;
  font-size: clamp(2rem, 4vw, 3.4rem);
  line-height: 0.96;
}

.mode-toggle {
  display: inline-flex;
  gap: 0.4rem;
  padding: 0.3rem;
  border-radius: 999px;
  background: var(--surface-toggle);
}

.mode-toggle button {
  border: 0;
  border-radius: 999px;
  padding: 0.55rem 0.9rem;
  background: transparent;
  cursor: pointer;
}

.mode-toggle button.active {
  background: var(--surface-input);
}

.auth-form {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  margin-top: 1.2rem;
}

label {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
  color: var(--text-soft);
}

input {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 1rem;
  background: var(--surface-input);
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

.auth-error {
  margin: 0;
  color: var(--danger);
}

.submit {
  border: 0;
  border-radius: 1rem;
  padding: 0.9rem 1rem;
  background: var(--accent);
  color: var(--text-on-accent);
  cursor: pointer;
}

@media (max-width: 820px) {
  .auth-panel {
    grid-template-columns: 1fr;
  }
}
</style>
