<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { RouterLink } from "vue-router";

import { createDocument, listDocuments } from "../api/documents";
import type { DocumentRecord } from "../api/types";
import { useAuthStore } from "../stores/auth";

const auth = useAuthStore();
const documents = ref<DocumentRecord[]>([]);
const isLoading = ref(true);
const error = ref<string | null>(null);
const createState = ref<"idle" | "creating">("idle");
const createForm = reactive({
  slug: "",
  title: "",
  visibility: "authenticated" as "public" | "authenticated",
});

const canManageDocuments = computed(() =>
  auth.user ? auth.user.role === "admin" : false,
);

async function loadDocuments() {
  isLoading.value = true;
  error.value = null;
  try {
    documents.value = await listDocuments();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load documents";
  } finally {
    isLoading.value = false;
  }
}

async function handleCreateDocument() {
  createState.value = "creating";
  error.value = null;
  try {
    const created = await createDocument({
      slug: createForm.slug,
      title: createForm.title,
      visibility: createForm.visibility,
      markdown_policy: {},
    });
    createForm.slug = "";
    createForm.title = "";
    createForm.visibility = "authenticated";
    await loadDocuments();
    documents.value = [created, ...documents.value.filter((item) => item.id !== created.id)];
  } catch (createError) {
    error.value = createError instanceof Error ? createError.message : "Failed to create document";
  } finally {
    createState.value = "idle";
  }
}

onMounted(() => {
  void loadDocuments();
});
</script>

<template>
  <main class="documents-page">
    <section class="documents-panel">
      <div class="panel-heading">
        <h1>
          {{ $t("Documents") }}<span v-if="!isLoading && !error"> ({{ documents.length }})</span>
        </h1>
      </div>

      <p v-if="isLoading">{{ $t("Loading documents...") }}</p>
      <p v-else-if="error" class="error">{{ $t(error) }}</p>
      <div v-else class="documents-grid">
        <RouterLink
          v-for="document in documents"
          :key="document.id"
          class="document-card"
          :to="`/documents/${document.slug}`"
        >
          <span class="visibility">{{ $t(document.visibility) }}</span>
          <h2>{{ document.title }}</h2>
          <p>{{ document.slug }}</p>
        </RouterLink>
      </div>
    </section>

    <form v-if="canManageDocuments" class="create-form" @submit.prevent="handleCreateDocument">
      <div class="form-header">
        <div>
          <h2>{{ $t("Create New Document") }}</h2>
          <p>{{ $t("Admins can create the document shell before sections and content are added.") }}</p>
        </div>
        <button class="create-button" :disabled="createState === 'creating'">
          {{ createState === "creating" ? $t("Creating...") : $t("Create document") }}
        </button>
      </div>
      <div class="form-grid">
        <label>
          {{ $t("Title") }}
          <input v-model="createForm.title" placeholder="Bandung Weather Notes" />
        </label>
        <label>
          {{ $t("Slug") }}
          <input v-model="createForm.slug" placeholder="bandung-weather-notes" />
        </label>
        <label>
          {{ $t("Visibility") }}
          <select v-model="createForm.visibility">
            <option value="authenticated">{{ $t("authenticated") }}</option>
            <option value="public">{{ $t("public") }}</option>
          </select>
        </label>
      </div>
    </form>

    <details class="about-note">
      <summary>{{ $t("What is Rimbun?") }}</summary>
      <div class="about-copy">
        <p>
          {{ $t("Rimbun is a collaborative writing system for structured texts where competing published variants stay visible instead of disappearing into revision history.") }}
        </p>
        <p>
          {{ $t("It lets readers browse a document through its current main text while still seeing where alternatives exist and how they differ.") }}
        </p>
        <a class="repo-link" href="https://github.com/curoli/rimbun" target="_blank" rel="noreferrer">
          {{ $t("View the GitHub repository") }}
        </a>
      </div>
    </details>
  </main>
</template>

<style scoped>
.documents-page {
  padding: 2rem;
  display: flex;
  flex-direction: column;
  gap: 1.75rem;
}

.eyebrow {
  margin: 0 0 0.5rem;
  color: var(--accent);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  font-size: 0.82rem;
}

.documents-panel {
  padding: 1.4rem;
  border-radius: 1.35rem;
  background: var(--surface-panel);
  border: 1px solid var(--border-soft);
}

.create-form {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.2rem;
  border-radius: 1.35rem;
  background: linear-gradient(180deg, color-mix(in srgb, var(--surface-panel) 88%, var(--surface-input)), color-mix(in srgb, var(--accent-soft) 40%, var(--surface-panel)));
  border: 1px solid var(--border-soft);
}

.form-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
}

.form-header h2,
.form-header p {
  margin: 0;
}

.form-header p {
  color: var(--text-secondary);
  margin-top: 0.35rem;
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 0.9rem;
}

label {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  color: var(--text-soft);
}

input,
select {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.8rem 0.9rem;
  background: var(--surface-input);
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

.create-button {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 1rem;
  background: var(--accent);
  color: var(--text-on-accent);
  cursor: pointer;
}

.documents-grid {
  margin-top: 1rem;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 1rem;
}

.document-card {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
  padding: 1.2rem;
  border-radius: 1.2rem;
  background: var(--surface-raised);
  border: 1px solid var(--border-soft);
}

.document-card h2,
.document-card p {
  margin: 0;
}

.document-card p {
  color: var(--text-secondary);
}

.visibility {
  color: var(--accent);
  text-transform: uppercase;
  font-size: 0.78rem;
  letter-spacing: 0.08em;
}

.error {
  color: var(--danger);
}

.about-note {
  border-radius: 1rem;
  background: color-mix(in srgb, var(--surface-panel) 94%, transparent);
  border: 1px solid var(--border-soft);
  padding: 0.95rem 1.1rem;
}

.about-note summary {
  cursor: pointer;
  font-weight: 600;
  color: var(--accent);
}

.about-copy {
  margin-top: 0.85rem;
  max-width: 60ch;
  color: var(--text-soft);
}

.about-copy p {
  margin: 0 0 0.75rem;
}

.repo-link {
  color: var(--accent);
  text-decoration: underline;
  text-underline-offset: 0.16em;
}

@media (max-width: 820px) {
  .form-grid {
    grid-template-columns: 1fr;
  }

  .form-header {
    flex-direction: column;
  }
}
</style>
