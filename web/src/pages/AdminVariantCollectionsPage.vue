<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";

import {
  createVariantCollection,
  createVariantEntry,
  deleteTestRun,
  deleteVariantCollection,
  deleteVariantEntry,
  listVariantCollections,
  runVariantCollection,
  updateVariantCollection,
  updateVariantEntry,
} from "../api/adminVariants";
import type { VariantCollectionDetail, VariantEntryRecord } from "../api/types";
import { useAuthStore } from "../stores/auth";

const auth = useAuthStore();
const router = useRouter();

const collections = ref<VariantCollectionDetail[]>([]);
const selectedCollectionId = ref<string | null>(null);
const isLoading = ref(true);
const isSaving = ref(false);
const error = ref<string | null>(null);

const createCollectionName = ref("");
const createCollectionDescription = ref("");

const collectionName = ref("");
const collectionDescription = ref("");

const entryId = ref<string | null>(null);
const entryPosition = ref(0);
const entryLabel = ref("");
const entryUsernameHint = ref("");
const entryMarkdown = ref("");

const isAdmin = computed(() =>
  auth.user ? ["admin", "privileged"].includes(auth.user.role) : false,
);

const selectedCollection = computed(
  () => collections.value.find((item) => item.collection.id === selectedCollectionId.value) ?? null,
);

function sortCollections(items: VariantCollectionDetail[]) {
  return [...items].sort(
    (left, right) =>
      left.collection.created_at.localeCompare(right.collection.created_at) ||
      left.collection.name.localeCompare(right.collection.name),
  );
}

function resetEntryForm(entry?: VariantEntryRecord) {
  entryId.value = entry?.id ?? null;
  entryPosition.value = entry?.position ?? ((selectedCollection.value?.entries.length ?? 0));
  entryLabel.value = entry?.label ?? "";
  entryUsernameHint.value = entry?.username_hint ?? "";
  entryMarkdown.value = entry?.markdown_content ?? "";
}

async function loadCollections() {
  isLoading.value = true;
  error.value = null;
  try {
    const data = sortCollections(await listVariantCollections());
    collections.value = data;
    selectedCollectionId.value =
      selectedCollectionId.value && data.some((item) => item.collection.id === selectedCollectionId.value)
        ? selectedCollectionId.value
        : data[0]?.collection.id ?? null;
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load variant collections";
  } finally {
    isLoading.value = false;
  }
}

async function handleCreateCollection() {
  if (!createCollectionName.value.trim()) {
    return;
  }
  isSaving.value = true;
  error.value = null;
  try {
    await createVariantCollection({
      name: createCollectionName.value.trim(),
      description: createCollectionDescription.value.trim(),
    });
    createCollectionName.value = "";
    createCollectionDescription.value = "";
    await loadCollections();
  } catch (saveError) {
    error.value = saveError instanceof Error ? saveError.message : "Failed to create collection";
  } finally {
    isSaving.value = false;
  }
}

async function handleSaveCollection() {
  if (!selectedCollection.value || !collectionName.value.trim()) {
    return;
  }
  isSaving.value = true;
  error.value = null;
  try {
    await updateVariantCollection(selectedCollection.value.collection.id, {
      name: collectionName.value.trim(),
      description: collectionDescription.value.trim(),
    });
    await loadCollections();
  } catch (saveError) {
    error.value = saveError instanceof Error ? saveError.message : "Failed to save collection";
  } finally {
    isSaving.value = false;
  }
}

async function handleDeleteCollection() {
  if (!selectedCollection.value) {
    return;
  }
  isSaving.value = true;
  error.value = null;
  try {
    await deleteVariantCollection(selectedCollection.value.collection.id);
    selectedCollectionId.value = null;
    await loadCollections();
  } catch (deleteError) {
    error.value = deleteError instanceof Error ? deleteError.message : "Failed to delete collection";
  } finally {
    isSaving.value = false;
  }
}

async function handleSaveEntry() {
  if (!selectedCollection.value || !entryLabel.value.trim() || !entryMarkdown.value.trim()) {
    return;
  }
  isSaving.value = true;
  error.value = null;
  try {
    const payload = {
      position: entryPosition.value,
      label: entryLabel.value.trim(),
      username_hint: entryUsernameHint.value.trim() || null,
      markdown_content: entryMarkdown.value,
    };
    if (entryId.value) {
      await updateVariantEntry(entryId.value, payload);
    } else {
      await createVariantEntry(selectedCollection.value.collection.id, payload);
    }
    await loadCollections();
    resetEntryForm();
  } catch (saveError) {
    error.value = saveError instanceof Error ? saveError.message : "Failed to save variant";
  } finally {
    isSaving.value = false;
  }
}

async function handleDeleteEntry() {
  if (!entryId.value) {
    return;
  }
  isSaving.value = true;
  error.value = null;
  try {
    await deleteVariantEntry(entryId.value);
    await loadCollections();
    resetEntryForm();
  } catch (deleteError) {
    error.value = deleteError instanceof Error ? deleteError.message : "Failed to delete variant";
  } finally {
    isSaving.value = false;
  }
}

async function handleRunCollection() {
  if (!selectedCollection.value) {
    return;
  }
  isSaving.value = true;
  error.value = null;
  try {
    const result = await runVariantCollection(selectedCollection.value.collection.id);
    await loadCollections();
    await router.push(`/documents/${result.document.id}`);
  } catch (runError) {
    error.value = runError instanceof Error ? runError.message : "Failed to create test document";
  } finally {
    isSaving.value = false;
  }
}

async function handleDeleteRun(runId: string) {
  isSaving.value = true;
  error.value = null;
  try {
    await deleteTestRun(runId);
    await loadCollections();
  } catch (deleteError) {
    error.value = deleteError instanceof Error ? deleteError.message : "Failed to delete test run";
  } finally {
    isSaving.value = false;
  }
}

watch(selectedCollection, (collection) => {
  collectionName.value = collection?.collection.name ?? "";
  collectionDescription.value = collection?.collection.description ?? "";
  resetEntryForm();
});

onMounted(async () => {
  await auth.restoreSession();
  if (!isAdmin.value) {
    await router.replace("/");
    return;
  }
  await loadCollections();
});
</script>

<template>
  <main class="variant-lab-page">
    <section class="admin-header">
      <div>
        <p class="eyebrow">Admin</p>
        <h1>Variant Collections</h1>
      </div>
      <p class="admin-copy">Reusable text variants for generating test documents and test users.</p>
    </section>

    <p v-if="error" class="error">{{ error }}</p>

    <section class="lab-layout">
      <aside class="collection-sidebar">
        <form class="sidebar-panel" @submit.prevent="handleCreateCollection">
          <h2>New Collection</h2>
          <label>
            <span>Name</span>
            <input v-model="createCollectionName" type="text" />
          </label>
          <label>
            <span>Description</span>
            <textarea v-model="createCollectionDescription" rows="3" />
          </label>
          <button type="submit" :disabled="isSaving">Create</button>
        </form>

        <section class="sidebar-panel">
          <div class="panel-heading">
            <h2>Collections</h2>
            <span>{{ collections.length }}</span>
          </div>
          <p v-if="isLoading">Loading collections...</p>
          <div v-else class="collection-list">
            <button
              v-for="item in collections"
              :key="item.collection.id"
              type="button"
              class="collection-button"
              :class="{ active: item.collection.id === selectedCollectionId }"
              @click="selectedCollectionId = item.collection.id"
            >
              <strong>{{ item.collection.name }}</strong>
              <small>{{ item.entries.length }} variants</small>
            </button>
          </div>
        </section>
      </aside>

      <section v-if="selectedCollection" class="workspace">
        <section class="workspace-panel">
          <div class="panel-heading">
            <h2>Collection</h2>
            <div class="action-row">
              <button type="button" class="danger" :disabled="isSaving" @click="handleDeleteCollection">Delete</button>
              <button type="button" :disabled="isSaving" @click="handleRunCollection">Create test document</button>
            </div>
          </div>

          <label>
            <span>Name</span>
            <input v-model="collectionName" type="text" />
          </label>
          <label>
            <span>Description</span>
            <textarea v-model="collectionDescription" rows="3" />
          </label>
          <button type="button" :disabled="isSaving" @click="handleSaveCollection">Save collection</button>
        </section>

        <section class="workspace-panel">
          <div class="panel-heading">
            <h2>Variants</h2>
            <span>{{ selectedCollection.entries.length }}</span>
          </div>

          <div class="entry-list">
            <button
              v-for="entry in selectedCollection.entries"
              :key="entry.id"
              type="button"
              class="entry-button"
              :class="{ active: entry.id === entryId }"
              @click="resetEntryForm(entry)"
            >
              <strong>{{ entry.position + 1 }}. {{ entry.label }}</strong>
              <small>{{ entry.username_hint || "auto username" }}</small>
            </button>
          </div>

          <div class="entry-editor">
            <label>
              <span>Position</span>
              <input v-model.number="entryPosition" type="number" min="0" />
            </label>
            <label>
              <span>Label</span>
              <input v-model="entryLabel" type="text" />
            </label>
            <label>
              <span>Username hint</span>
              <input v-model="entryUsernameHint" type="text" />
            </label>
            <label>
              <span>Markdown</span>
              <textarea v-model="entryMarkdown" rows="10" />
            </label>
            <div class="action-row">
              <button type="button" class="secondary" :disabled="isSaving" @click="resetEntryForm()">New variant</button>
              <button type="button" class="danger" :disabled="isSaving || !entryId" @click="handleDeleteEntry">Delete</button>
              <button type="button" :disabled="isSaving" @click="handleSaveEntry">
                {{ entryId ? "Save variant" : "Add variant" }}
              </button>
            </div>
          </div>
        </section>

        <section class="workspace-panel">
          <div class="panel-heading">
            <h2>Test Runs</h2>
            <span>{{ selectedCollection.runs.length }}</span>
          </div>

          <div v-if="selectedCollection.runs.length" class="run-list">
            <article v-for="run in selectedCollection.runs" :key="run.id" class="run-card">
              <div>
                <strong>{{ new Date(run.created_at).toLocaleString() }}</strong>
                <small>{{ run.status }}</small>
              </div>
              <div class="action-row">
                <RouterLink
                  v-if="run.document_id"
                  class="link-button"
                  :to="`/documents/${run.document_id}`"
                >
                  Open document
                </RouterLink>
                <button
                  v-if="run.status === 'active'"
                  type="button"
                  class="danger"
                  :disabled="isSaving"
                  @click="handleDeleteRun(run.id)"
                >
                  Delete run
                </button>
              </div>
            </article>
          </div>
          <p v-else>No test runs yet.</p>
        </section>
      </section>
    </section>
  </main>
</template>

<style scoped>
.variant-lab-page {
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
.admin-copy,
.panel-heading h2,
.workspace-panel p,
.variant-lab-page label span {
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

.lab-layout {
  display: grid;
  grid-template-columns: minmax(18rem, 22rem) minmax(0, 1fr);
  gap: 1rem;
}

.collection-sidebar,
.workspace {
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.sidebar-panel,
.workspace-panel {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
  padding: 1.1rem;
  border-radius: 1rem;
  background: rgba(255, 252, 247, 0.94);
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.panel-heading,
.action-row,
.run-card {
  display: flex;
  justify-content: space-between;
  gap: 0.75rem;
  align-items: center;
}

.panel-heading span,
.collection-button small,
.entry-button small,
.run-card small {
  color: #6f5947;
}

.collection-list,
.entry-list,
.run-list {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
}

.collection-button,
.entry-button {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
  align-items: flex-start;
  text-align: left;
  border: 1px solid rgba(35, 24, 15, 0.08);
  border-radius: 0.75rem;
  padding: 0.85rem;
  background: white;
  cursor: pointer;
}

.collection-button.active,
.entry-button.active {
  background: #f1dcc4;
  border-color: rgba(142, 75, 22, 0.22);
}

.entry-editor,
label {
  display: flex;
  flex-direction: column;
  gap: 0.4rem;
}

input,
textarea,
button,
.link-button {
  border-radius: 0.75rem;
}

input,
textarea {
  border: 1px solid rgba(35, 24, 15, 0.14);
  padding: 0.7rem 0.85rem;
  background: white;
}

button,
.link-button {
  border: 0;
  padding: 0.72rem 0.95rem;
  background: #8e4b16;
  color: white;
  cursor: pointer;
  text-decoration: none;
}

button.secondary {
  background: #eadac8;
  color: #58351a;
}

button.danger {
  background: #9d2a16;
}

.run-card {
  padding: 0.85rem;
  border-radius: 0.75rem;
  border: 1px solid rgba(35, 24, 15, 0.08);
  background: white;
}

.error {
  margin: 0;
  color: #9d2a16;
}

@media (max-width: 1100px) {
  .lab-layout {
    grid-template-columns: 1fr;
  }
}

@media (max-width: 960px) {
  .admin-header,
  .panel-heading,
  .action-row,
  .run-card {
    flex-direction: column;
    align-items: stretch;
  }
}
</style>
