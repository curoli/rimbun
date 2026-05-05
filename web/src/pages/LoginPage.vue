<script setup lang="ts">
import { reactive, ref } from "vue";
import { useRouter } from "vue-router";

import { useAuthStore } from "../stores/auth";

const auth = useAuthStore();
const router = useRouter();
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
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.auth-copy {
  background:
    linear-gradient(160deg, rgba(142, 75, 22, 0.9), rgba(196, 130, 49, 0.76)),
    #8e4b16;
  color: #fff8ef;
}

.auth-form-panel {
  background: rgba(255, 252, 247, 0.95);
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
  background: #f0e4d7;
}

.mode-toggle button {
  border: 0;
  border-radius: 999px;
  padding: 0.55rem 0.9rem;
  background: transparent;
  cursor: pointer;
}

.mode-toggle button.active {
  background: white;
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
  color: #563f2e;
}

input {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 1rem;
  background: #fff;
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

.auth-error {
  margin: 0;
  color: #9d2a16;
}

.submit {
  border: 0;
  border-radius: 1rem;
  padding: 0.9rem 1rem;
  background: #8e4b16;
  color: white;
  cursor: pointer;
}

@media (max-width: 820px) {
  .auth-panel {
    grid-template-columns: 1fr;
  }
}
</style>
