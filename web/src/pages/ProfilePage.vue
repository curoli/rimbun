<script setup lang="ts">
import { onMounted, reactive, ref, watch } from "vue";
import { useRouter } from "vue-router";

import * as authApi from "../api/auth";
import { useAuthStore } from "../stores/auth";

const auth = useAuthStore();
const router = useRouter();

const profileForm = reactive({
  username: "",
  email: "",
  display_name: "",
  role: "",
});

const passwordForm = reactive({
  current_password: "",
  new_password: "",
  confirm_password: "",
});

const profileState = ref<"idle" | "saving">("idle");
const passwordState = ref<"idle" | "saving">("idle");
const profileError = ref<string | null>(null);
const passwordError = ref<string | null>(null);
const passwordSuccess = ref<string | null>(null);

function syncFromUser() {
  if (!auth.user) {
    profileForm.username = "";
    profileForm.email = "";
    profileForm.display_name = "";
    profileForm.role = "";
    return;
  }

  profileForm.username = auth.user.username;
  profileForm.email = auth.user.email;
  profileForm.display_name = auth.user.display_name;
  profileForm.role = auth.user.role;
}

async function handleUpdateProfile() {
  profileState.value = "saving";
  profileError.value = null;
  try {
    const user = await authApi.updateProfile({
      display_name: profileForm.display_name,
    });
    auth.updateCurrentUser(user);
    syncFromUser();
  } catch (error) {
    profileError.value = error instanceof Error ? error.message : "Failed to update profile";
  } finally {
    profileState.value = "idle";
  }
}

async function handleChangePassword() {
  passwordState.value = "saving";
  passwordError.value = null;
  passwordSuccess.value = null;

  if (passwordForm.new_password !== passwordForm.confirm_password) {
    passwordError.value = "new password and confirmation must match";
    passwordState.value = "idle";
    return;
  }

  try {
    await authApi.changePassword({
      current_password: passwordForm.current_password,
      new_password: passwordForm.new_password,
    });
    passwordForm.current_password = "";
    passwordForm.new_password = "";
    passwordForm.confirm_password = "";
    passwordSuccess.value = "Password updated.";
  } catch (error) {
    passwordError.value = error instanceof Error ? error.message : "Failed to change password";
  } finally {
    passwordState.value = "idle";
  }
}

watch(
  () => auth.user,
  () => {
    syncFromUser();
  },
  { immediate: true },
);

onMounted(async () => {
  await auth.restoreSession();
  if (!auth.user) {
    await router.push("/login");
  }
});
</script>

<template>
  <main class="profile-page">
    <section class="profile-header">
      <div>
        <p class="eyebrow">{{ $t("Profile") }}</p>
        <h1>{{ $t("Your account") }}</h1>
      </div>
      <p class="profile-copy">
        {{ $t("Review your account details and update your display name or password.") }}
      </p>
    </section>

    <section class="profile-grid">
      <form class="profile-card" @submit.prevent="handleUpdateProfile">
        <div>
          <h2>{{ $t("Profile details") }}</h2>
          <p class="card-copy">{{ $t("Username, email, and role are currently read-only. You can change your display name.") }}</p>
        </div>

        <label>
          {{ $t("Username") }}
          <input v-model="profileForm.username" disabled />
        </label>

        <label>
          {{ $t("Email") }}
          <input v-model="profileForm.email" disabled />
        </label>

        <label>
          {{ $t("Role") }}
          <input v-model="profileForm.role" disabled />
        </label>

        <label>
          {{ $t("Display name") }}
          <input v-model="profileForm.display_name" />
        </label>

        <p v-if="profileError" class="error">{{ $t(profileError) }}</p>
        <button class="solid" :disabled="profileState === 'saving' || !profileForm.display_name.trim()">
          {{ profileState === "saving" ? $t("Saving...") : $t("Save profile") }}
        </button>
      </form>

      <form class="profile-card" @submit.prevent="handleChangePassword">
        <div>
          <h2>{{ $t("Password") }}</h2>
          <p class="card-copy">{{ $t("Change your password by entering the current one and confirming the new one.") }}</p>
        </div>

        <label>
          {{ $t("Current password") }}
          <input v-model="passwordForm.current_password" type="password" />
        </label>

        <label>
          {{ $t("New password") }}
          <input v-model="passwordForm.new_password" type="password" />
        </label>

        <label>
          {{ $t("Confirm new password") }}
          <input v-model="passwordForm.confirm_password" type="password" />
        </label>

        <p v-if="passwordError" class="error">{{ $t(passwordError) }}</p>
        <p v-if="passwordSuccess" class="success">{{ $t(passwordSuccess) }}</p>
        <button
          class="solid"
          :disabled="
            passwordState === 'saving' ||
            !passwordForm.current_password ||
            !passwordForm.new_password ||
            !passwordForm.confirm_password
          "
        >
          {{ passwordState === "saving" ? $t("Saving...") : $t("Change password") }}
        </button>
      </form>
    </section>
  </main>
</template>

<style scoped>
.profile-page {
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.profile-header {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: flex-start;
  padding: 1.6rem;
  border-radius: 1.5rem;
  background: var(--surface-hero);
  border: 1px solid var(--border-soft);
}

.eyebrow,
.profile-header h1,
.profile-copy,
.profile-card h2,
.card-copy {
  margin: 0;
}

.eyebrow {
  color: var(--accent);
  text-transform: uppercase;
  font-size: 0.82rem;
  letter-spacing: 0.08em;
  margin-bottom: 0.35rem;
}

.profile-header h1 {
  font-size: clamp(2rem, 4vw, 3rem);
  line-height: 0.95;
}

.profile-copy,
.card-copy {
  color: var(--text-secondary);
}

.profile-copy {
  max-width: 34ch;
}

.profile-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1.25rem;
}

.profile-card {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.25rem;
  border-radius: 1.25rem;
  background: var(--surface-panel);
  border: 1px solid var(--border-soft);
}

label {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  color: var(--text-soft);
}

input {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 0.95rem;
  background: var(--surface-input);
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

input:disabled {
  color: var(--text-muted);
  background: var(--accent-hover);
}

.solid {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 1rem;
  background: var(--accent);
  color: var(--text-on-accent);
  cursor: pointer;
}

.solid:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.error {
  margin: 0;
  color: var(--danger);
}

.success {
  margin: 0;
  color: #2f6f3e;
}

@media (max-width: 960px) {
  .profile-header,
  .profile-grid {
    display: flex;
    flex-direction: column;
  }
}
</style>
