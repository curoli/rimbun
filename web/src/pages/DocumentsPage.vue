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
  auth.user ? ["privileged", "admin"].includes(auth.user.role) : false,
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
          Documents<span v-if="!isLoading && !error"> ({{ documents.length }})</span>
        </h1>
      </div>

      <p v-if="isLoading">Loading documents...</p>
      <p v-else-if="error" class="error">{{ error }}</p>
      <div v-else class="documents-grid">
        <RouterLink
          v-for="document in documents"
          :key="document.id"
          class="document-card"
          :to="`/documents/${document.id}`"
        >
          <span class="visibility">{{ document.visibility }}</span>
          <h2>{{ document.title }}</h2>
          <p>{{ document.slug }}</p>
        </RouterLink>
      </div>
    </section>

    <form v-if="canManageDocuments" class="create-form" @submit.prevent="handleCreateDocument">
      <div class="form-header">
        <div>
          <h2>Create New Document</h2>
          <p>Admins can create the document shell before sections and content are added.</p>
        </div>
        <button class="create-button" :disabled="createState === 'creating'">
          {{ createState === "creating" ? "Creating..." : "Create document" }}
        </button>
      </div>
      <div class="form-grid">
        <label>
          Title
          <input v-model="createForm.title" placeholder="Bandung Weather Notes" />
        </label>
        <label>
          Slug
          <input v-model="createForm.slug" placeholder="bandung-weather-notes" />
        </label>
        <label>
          Visibility
          <select v-model="createForm.visibility">
            <option value="authenticated">authenticated</option>
            <option value="public">public</option>
          </select>
        </label>
      </div>
    </form>

    <details class="about-note">
      <summary>What is Rimbun?</summary>
      <div class="about-copy">
        <p>
          Rimbun is a collaborative writing system for structured texts where competing published variants stay
          visible instead of disappearing into revision history.
        </p>
        <p>
          It lets readers browse a document through its current main text while still seeing where alternatives
          exist and how they differ.
        </p>
        <a class="repo-link" href="https://github.com/curoli/rimbun" target="_blank" rel="noreferrer">
          View the GitHub repository
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
  color: #8e4b16;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  font-size: 0.82rem;
}

.documents-panel {
  padding: 1.4rem;
  border-radius: 1.35rem;
  background: rgba(255, 252, 247, 0.94);
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.create-form {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.2rem;
  border-radius: 1.35rem;
  background: linear-gradient(180deg, rgba(252, 246, 238, 0.96), rgba(243, 230, 214, 0.94));
  border: 1px solid rgba(35, 24, 15, 0.08);
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
  color: #6b5646;
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
  color: #5b4331;
}

input,
select {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.8rem 0.9rem;
  background: #fff;
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

.create-button {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 1rem;
  background: #8e4b16;
  color: white;
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
  background: #fffaf4;
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.document-card h2,
.document-card p {
  margin: 0;
}

.document-card p {
  color: #6f5947;
}

.visibility {
  color: #8e4b16;
  text-transform: uppercase;
  font-size: 0.78rem;
  letter-spacing: 0.08em;
}

.error {
  color: #9d2a16;
}

.about-note {
  border-radius: 1rem;
  background: rgba(255, 252, 247, 0.88);
  border: 1px solid rgba(35, 24, 15, 0.08);
  padding: 0.95rem 1.1rem;
}

.about-note summary {
  cursor: pointer;
  font-weight: 600;
  color: #8e4b16;
}

.about-copy {
  margin-top: 0.85rem;
  max-width: 60ch;
  color: #5e4a3b;
}

.about-copy p {
  margin: 0 0 0.75rem;
}

.repo-link {
  color: #8e4b16;
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
