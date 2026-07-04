<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import { getDocument, updateDocument } from "../api/documents";
import type { DocumentDetailResponse } from "../api/types";
import DocumentViewNav from "../components/DocumentViewNav.vue";
import { useAuthStore } from "../stores/auth";

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const isLoadingDocument = ref(true);
const saveState = ref<"idle" | "saving">("idle");
const error = ref<string | null>(null);

const form = reactive({
  title: "",
  slug: "",
  visibility: "authenticated" as "public" | "authenticated",
  paginationLevel: "none",
});

const canManageDocument = computed(() =>
  auth.user ? auth.user.role === "admin" : false,
);

function syncForm() {
  if (!documentData.value) {
    form.title = "";
    form.slug = "";
    form.visibility = "authenticated";
    form.paginationLevel = "none";
    return;
  }

  form.title = documentData.value.document.title;
  form.slug = documentData.value.document.slug;
  form.visibility = documentData.value.document.visibility as "public" | "authenticated";
  const rawPaginationLevel = documentData.value.document.markdown_policy?.pagination_level;
  form.paginationLevel =
    typeof rawPaginationLevel === "number" && rawPaginationLevel > 0
      ? String(rawPaginationLevel)
      : "none";
}

async function loadDocument() {
  const documentRef = route.params.documentRef;
  if (typeof documentRef !== "string") {
    return;
  }

  isLoadingDocument.value = true;
  error.value = null;
  try {
    const data = await getDocument(documentRef);
    documentData.value = data;
    if (documentRef !== data.document.slug) {
      await router.replace(`/documents/${data.document.slug}/settings`);
      return;
    }
    syncForm();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load document";
    if (error.value.toLowerCase().includes("authentication required")) {
      await router.push("/login");
    }
  } finally {
    isLoadingDocument.value = false;
  }
}

async function handleSave() {
  if (!documentData.value) {
    return;
  }

  saveState.value = "saving";
  error.value = null;
  try {
    const updated = await updateDocument(documentData.value.document.id, {
      title: form.title,
      slug: form.slug,
      visibility: form.visibility,
      markdown_policy: {
        ...documentData.value.document.markdown_policy,
        pagination_level: form.paginationLevel === "none" ? null : Number(form.paginationLevel),
      },
    });
    documentData.value = {
      ...documentData.value,
      document: updated,
    };
    if (route.params.documentRef !== updated.slug) {
      await router.replace(`/documents/${updated.slug}/settings`);
    }
    syncForm();
  } catch (saveError) {
    error.value = saveError instanceof Error ? saveError.message : "Failed to save document settings";
  } finally {
    saveState.value = "idle";
  }
}

watch(
  () => route.params.documentRef,
  () => {
    void loadDocument();
  },
);

onMounted(async () => {
  await auth.restoreSession();
  if (!canManageDocument.value) {
    await router.replace("/");
    return;
  }
  await loadDocument();
});
</script>

<template>
  <main class="document-page">
    <p v-if="isLoadingDocument">Loading document...</p>
    <p v-else-if="error && !documentData" class="error">{{ error }}</p>
    <template v-else-if="documentData">
      <section class="document-header">
        <div>
          <p class="eyebrow">{{ documentData.document.visibility }}</p>
          <h1>{{ documentData.document.title }}</h1>
        </div>
        <div class="document-header-meta">
          <p class="document-slug">{{ documentData.document.slug }}</p>
          <DocumentViewNav
            :document-ref="documentData.document.slug"
            :can-manage-outline="canManageDocument"
            active-view="settings"
          />
        </div>
      </section>

      <section class="settings-panel">
        <div class="panel-heading">
          <div>
            <h2>Document Settings</h2>
            <p>Update the document title, slug, visibility, and reader pagination.</p>
          </div>
          <button type="button" :disabled="saveState === 'saving'" @click="handleSave">
            {{ saveState === "saving" ? "Saving..." : "Save settings" }}
          </button>
        </div>

        <p v-if="error" class="error">{{ error }}</p>

        <div class="form-grid">
          <label>
            Title
            <input v-model="form.title" type="text" />
          </label>
          <label>
            Slug
            <input v-model="form.slug" type="text" />
          </label>
          <label>
            Visibility
            <select v-model="form.visibility">
              <option value="authenticated">authenticated</option>
              <option value="public">public</option>
            </select>
          </label>
          <label>
            Pagination level
            <select v-model="form.paginationLevel">
              <option value="none">No pagination</option>
              <option value="1">Level 1 sections</option>
              <option value="2">Level 2 sections</option>
              <option value="3">Level 3 sections</option>
              <option value="4">Level 4 sections</option>
            </select>
          </label>
        </div>
      </section>
    </template>
  </main>
</template>

<style scoped>
@import "./document-shared.css";

.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.25rem;
  border-radius: 1.25rem;
  background: var(--surface-panel);
  border: 1px solid var(--border-soft);
}

.panel-heading {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: flex-start;
}

.panel-heading h2,
.panel-heading p {
  margin: 0;
}

.panel-heading p {
  margin-top: 0.35rem;
  color: var(--text-secondary);
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 0.9rem;
}

label {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
  color: var(--text-soft);
}

input,
select,
button {
  font: inherit;
  border-radius: 0.95rem;
}

input,
select {
  border: 0;
  padding: 0.8rem 0.9rem;
  background: var(--surface-input);
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

button {
  border: 0;
  padding: 0.85rem 1rem;
  background: var(--accent);
  color: var(--text-on-accent);
  cursor: pointer;
}

.error {
  color: var(--danger);
  margin: 0;
}

@media (max-width: 900px) {
  .panel-heading {
    flex-direction: column;
  }

  .form-grid {
    grid-template-columns: 1fr;
  }
}
</style>
