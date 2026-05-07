<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useRouter } from "vue-router";

import { listUsers } from "../api/users";
import type { User } from "../api/types";
import { useAuthStore } from "../stores/auth";

const auth = useAuthStore();
const router = useRouter();

const users = ref<User[]>([]);
const isLoading = ref(true);
const error = ref<string | null>(null);

const isAdmin = computed(() =>
  auth.user ? ["admin", "privileged"].includes(auth.user.role) : false,
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
        <p class="eyebrow">Admin</p>
        <h1>Users</h1>
      </div>
      <p class="admin-copy">
        Overview of all registered users and their current roles.
      </p>
    </section>

    <section class="admin-panel">
      <p v-if="isLoading">Loading users...</p>
      <p v-else-if="error" class="error">{{ error }}</p>
      <table v-else class="users-table">
        <thead>
          <tr>
            <th>Username</th>
            <th>Display name</th>
            <th>Email</th>
            <th>Role</th>
            <th>Created</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="user in users" :key="user.id">
            <td>@{{ user.username }}</td>
            <td>{{ user.display_name }}</td>
            <td>{{ user.email }}</td>
            <td>{{ user.role }}</td>
            <td>{{ new Date(user.created_at).toLocaleString() }}</td>
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
  background: linear-gradient(135deg, rgba(255, 248, 238, 0.98), rgba(235, 212, 184, 0.94));
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.eyebrow,
.admin-header h1,
.admin-copy {
  margin: 0;
}

.eyebrow {
  color: #8e4b16;
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
  color: #6f5947;
}

.admin-panel {
  padding: 1.25rem;
  border-radius: 1.25rem;
  background: rgba(255, 252, 247, 0.94);
  border: 1px solid rgba(35, 24, 15, 0.08);
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
  border-bottom: 1px solid rgba(35, 24, 15, 0.08);
}

.users-table th {
  color: #705948;
  font-size: 0.85rem;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.error {
  margin: 0;
  color: #9d2a16;
}

@media (max-width: 960px) {
  .admin-header {
    flex-direction: column;
  }
}
</style>
