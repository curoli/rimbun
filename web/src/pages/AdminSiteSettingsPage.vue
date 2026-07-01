<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useRouter } from "vue-router";

import { updateSiteSettings } from "../api/siteSettings";
import { SITE_COLOR_SCHEMES } from "../site-theme";
import { useAuthStore } from "../stores/auth";
import { useSiteStore } from "../stores/site";

const auth = useAuthStore();
const site = useSiteStore();
const router = useRouter();

const saveState = ref<"idle" | "saving">("idle");
const error = ref<string | null>(null);
const form = reactive<{
  brand_name: string;
  browser_title: string;
  color_scheme: string;
}>({
  brand_name: "",
  browser_title: "",
  color_scheme: SITE_COLOR_SCHEMES[0].value,
});

const canManageSite = computed(() =>
  auth.user ? auth.user.role === "admin" : false,
);

function syncForm() {
  form.brand_name = site.brandName;
  form.browser_title = site.browserTitle;
  form.color_scheme = site.colorScheme;
}

async function handleSave() {
  saveState.value = "saving";
  error.value = null;
  try {
    const settings = await updateSiteSettings({
      brand_name: form.brand_name,
      browser_title: form.browser_title,
      color_scheme: form.color_scheme,
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
      <p class="admin-copy">
        Change the site brand shown in the header, the browser page title, and the active color scheme.
      </p>
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

      <label>
        <span>Color scheme</span>
        <select v-model="form.color_scheme">
          <option v-for="scheme in SITE_COLOR_SCHEMES" :key="scheme.value" :value="scheme.value">
            {{ scheme.label }}: {{ scheme.description }}
          </option>
        </select>
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

.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  padding: 1.1rem;
  border-radius: 1rem;
  background: var(--surface-panel);
  border: 1px solid var(--border-soft);
}

label {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  color: var(--text-secondary);
}

input,
select,
button {
  border-radius: 0.75rem;
  font: inherit;
}

input,
select {
  border: 1px solid var(--border-strong);
  padding: 0.7rem 0.85rem;
  background: var(--surface-input);
}

.action-row {
  display: flex;
  justify-content: flex-end;
}

button {
  border: 0;
  padding: 0.72rem 0.95rem;
  background: var(--accent);
  color: var(--text-on-accent);
  cursor: pointer;
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
