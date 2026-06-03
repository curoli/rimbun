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
});

const canManageDocument = computed(() =>
  auth.user ? ["privileged", "admin"].includes(auth.user.role) : false,
);

function syncForm() {
  if (!documentData.value) {
    form.title = "";
    form.slug = "";
    form.visibility = "authenticated";
    return;
  }

  form.title = documentData.value.document.title;
  form.slug = documentData.value.document.slug;
  form.visibility = documentData.value.document.visibility as "public" | "authenticated";
}

async function loadDocument() {
  const id = route.params.id;
  if (typeof id !== "string") {
    return;
  }

  isLoadingDocument.value = true;
  error.value = null;
  try {
    const data = await getDocument(id);
    documentData.value = data;
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
      markdown_policy: documentData.value.document.markdown_policy,
    });
    documentData.value = {
      ...documentData.value,
      document: updated,
    };
    syncForm();
  } catch (saveError) {
    error.value = saveError instanceof Error ? saveError.message : "Failed to save document settings";
  } finally {
    saveState.value = "idle";
  }
}

watch(
  () => route.params.id,
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
            :document-id="documentData.document.id"
            :can-manage-outline="canManageDocument"
            active-view="settings"
          />
        </div>
      </section>

      <section class="settings-panel">
        <div class="panel-heading">
          <div>
            <h2>Document Settings</h2>
            <p>Update the document title, slug, and visibility.</p>
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
  background: rgba(255, 252, 247, 0.94);
  border: 1px solid rgba(35, 24, 15, 0.08);
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
  color: #6b5646;
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
select,
button {
  font: inherit;
  border-radius: 0.95rem;
}

input,
select {
  border: 0;
  padding: 0.8rem 0.9rem;
  background: #fff;
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

button {
  border: 0;
  padding: 0.85rem 1rem;
  background: #8e4b16;
  color: white;
  cursor: pointer;
}

.error {
  color: #9d2a16;
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
