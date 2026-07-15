<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";

import { listUsers, resetUserPassword } from "../api/users";
import type { User } from "../api/types";
import { useAuthStore } from "../stores/auth";

const auth = useAuthStore();
const router = useRouter();

const users = ref<User[]>([]);
const isLoading = ref(true);
const error = ref<string | null>(null);
const passwordDrafts = ref<Record<string, string>>({});
const resetStates = ref<Record<string, "idle" | "saving">>({});
const resetMessages = ref<Record<string, string>>({});

const isAdmin = computed(() =>
  auth.user ? auth.user.role === "admin" : false,
);

async function loadUsers() {
  isLoading.value = true;
  error.value = null;
  try {
    users.value = await listUsers();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load users";
  } finally {
    isLoading.value = false;
  }
}

async function handleResetPassword(userId: string) {
  const draft = passwordDrafts.value[userId]?.trim() ?? "";
  if (!draft) {
    resetMessages.value = {
      ...resetMessages.value,
      [userId]: "Enter a new password first.",
    };
    return;
  }

  resetStates.value = {
    ...resetStates.value,
    [userId]: "saving",
  };
  resetMessages.value = {
    ...resetMessages.value,
    [userId]: "",
  };

  try {
    await resetUserPassword(userId, { new_password: draft });
    passwordDrafts.value = {
      ...passwordDrafts.value,
      [userId]: "",
    };
    resetMessages.value = {
      ...resetMessages.value,
      [userId]: "Password reset.",
    };
  } catch (resetError) {
    resetMessages.value = {
      ...resetMessages.value,
      [userId]: resetError instanceof Error ? resetError.message : "Failed to reset password",
    };
  } finally {
    resetStates.value = {
      ...resetStates.value,
      [userId]: "idle",
    };
  }
}

onMounted(async () => {
  await auth.restoreSession();
  if (!isAdmin.value) {
    await router.replace("/");
    return;
  }
  await loadUsers();
});
</script>

<template>
  <main class="admin-users-page">
    <section class="admin-header">
      <div>
        <p class="eyebrow">{{ $t("Admin") }}</p>
        <h1>{{ $t("Users") }}</h1>
      </div>
      <p class="admin-copy">
        {{ $t("Overview of all registered users and their current roles.") }}
      </p>
    </section>

    <section class="admin-panel">
      <p v-if="isLoading">{{ $t("Loading users...") }}</p>
      <p v-else-if="error" class="error">{{ $t(error) }}</p>
      <table v-else class="users-table">
        <thead>
          <tr>
            <th>{{ $t("Username") }}</th>
            <th>{{ $t("Display name") }}</th>
            <th>{{ $t("Email") }}</th>
            <th>{{ $t("Role") }}</th>
            <th>{{ $t("Created") }}</th>
            <th>{{ $t("Password reset") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="user in users" :key="user.id">
            <td>@{{ user.username }}</td>
            <td>{{ user.display_name }}</td>
            <td>{{ user.email }}</td>
            <td>{{ user.role }}</td>
            <td>{{ $date(user.created_at) }}</td>
            <td class="password-reset-cell">
              <div class="password-reset-controls">
                <input
                  v-model="passwordDrafts[user.id]"
                  type="password"
                  :placeholder="$t('New password')"
                />
                <button
                  type="button"
                  :disabled="resetStates[user.id] === 'saving' || !(passwordDrafts[user.id] ?? '').trim()"
                  @click="handleResetPassword(user.id)"
                >
                  {{ resetStates[user.id] === "saving" ? $t("Resetting...") : $t("Reset password") }}
                </button>
              </div>
              <p v-if="resetMessages[user.id]" class="reset-message">{{ $t(resetMessages[user.id]) }}</p>
            </td>
          </tr>
        </tbody>
      </table>
    </section>
  </main>
</template>

<style scoped>
.admin-users-page {
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.admin-header {
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
.admin-header h1,
.admin-copy {
  margin: 0;
}

.eyebrow {
  color: var(--accent);
  text-transform: uppercase;
  font-size: 0.82rem;
  letter-spacing: 0.08em;
  margin-bottom: 0.35rem;
}

.admin-header h1 {
  font-size: clamp(2rem, 4vw, 3rem);
  line-height: 0.95;
}

.admin-copy {
  max-width: 34ch;
  color: var(--text-secondary);
}

.admin-panel {
  padding: 1.25rem;
  border-radius: 1.25rem;
  background: var(--surface-panel);
  border: 1px solid var(--border-soft);
  overflow-x: auto;
}

.users-table {
  width: 100%;
  border-collapse: collapse;
}

.users-table th,
.users-table td {
  padding: 0.85rem 0.75rem;
  text-align: left;
  border-bottom: 1px solid var(--border-soft);
}

.password-reset-cell {
  min-width: 18rem;
}

.password-reset-controls {
  display: flex;
  gap: 0.5rem;
  align-items: center;
}

.password-reset-controls input,
.password-reset-controls button {
  font: inherit;
  border-radius: 0.8rem;
}

.password-reset-controls input {
  flex: 1;
  min-width: 0;
  border: 0;
  padding: 0.7rem 0.8rem;
  background: var(--surface-input);
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

.password-reset-controls button {
  border: 0;
  padding: 0.72rem 0.9rem;
  background: var(--accent);
  color: var(--text-on-accent);
  cursor: pointer;
  white-space: nowrap;
}

.password-reset-controls button:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.reset-message {
  margin: 0.45rem 0 0;
  color: var(--text-secondary);
  font-size: 0.85rem;
}

.users-table th {
  color: var(--text-muted);
  font-size: 0.85rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.error {
  margin: 0;
  color: var(--danger);
}

@media (max-width: 960px) {
  .admin-header {
    flex-direction: column;
  }
}
</style>
