<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";

import { updateSiteSettings } from "../api/siteSettings";
import { useAuthStore } from "../stores/auth";
import { useSiteStore } from "../stores/site";

const auth = useAuthStore();
const site = useSiteStore();
const router = useRouter();

const saveState = ref<"idle" | "saving">("idle");
const error = ref<string | null>(null);
const form = reactive({
  brand_name: "",
  browser_title: "",
});

const canManageSite = computed(() =>
  auth.user ? ["privileged", "admin"].includes(auth.user.role) : false,
);

function syncForm() {
  form.brand_name = site.brandName;
  form.browser_title = site.browserTitle;
}

async function handleSave() {
  saveState.value = "saving";
  error.value = null;
  try {
    const settings = await updateSiteSettings({
      brand_name: form.brand_name,
      browser_title: form.browser_title,
    });
    site.apply(settings);
    syncForm();
  } catch (saveError) {
    error.value = saveError instanceof Error ? saveError.message : "Failed to save site settings";
  } finally {
    saveState.value = "idle";
  }
}

onMounted(async () => {
  await auth.restoreSession();
  if (!canManageSite.value) {
    await router.replace("/");
    return;
  }
  await site.load();
  syncForm();
});
</script>

<template>
  <main class="site-settings-page">
    <section class="admin-header">
      <div>
        <p class="eyebrow">Admin</p>
        <h1>Site Settings</h1>
      </div>
      <p class="admin-copy">Change the site brand shown in the header and the browser page title.</p>
    </section>

    <section class="settings-panel">
      <p v-if="error" class="error">{{ error }}</p>

      <label>
        <span>Header brand</span>
        <input v-model="form.brand_name" type="text" />
      </label>

      <label>
        <span>Browser title</span>
        <input v-model="form.browser_title" type="text" />
      </label>

      <div class="action-row">
        <button type="button" :disabled="saveState === 'saving'" @click="handleSave">
          {{ saveState === "saving" ? "Saving..." : "Save settings" }}
        </button>
      </div>
    </section>
  </main>
</template>

<style scoped>
.site-settings-page {
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

.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  padding: 1.1rem;
  border-radius: 1rem;
  background: rgba(255, 252, 247, 0.94);
  border: 1px solid rgba(35, 24, 15, 0.08);
}

label {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  color: #5b4331;
}

input,
button {
  border-radius: 0.75rem;
  font: inherit;
}

input {
  border: 1px solid rgba(35, 24, 15, 0.14);
  padding: 0.7rem 0.85rem;
  background: white;
}

.action-row {
  display: flex;
  justify-content: flex-end;
}

button {
  border: 0;
  padding: 0.72rem 0.95rem;
  background: #8e4b16;
  color: white;
  cursor: pointer;
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
