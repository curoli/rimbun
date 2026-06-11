<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import { createSection, getDocument, updateSection } from "../api/documents";
import type { DocumentDetailResponse } from "../api/types";
import DocumentViewNav from "../components/DocumentViewNav.vue";
import SectionTree from "../components/SectionTree.vue";
import { useAuthStore } from "../stores/auth";

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const selectedSectionId = ref<string | null>(null);
const isLoadingDocument = ref(true);
const createSectionState = ref<"idle" | "creating">("idle");
const updateSectionState = ref<"idle" | "saving">("idle");
const error = ref<string | null>(null);
const createSectionError = ref<string | null>(null);
const updateSectionError = ref<string | null>(null);
const createSectionTitle = ref("");
const createSectionHasHeading = ref(true);
const createUnnamedSectionCount = ref(1);
const editSectionTitle = ref("");
const editSectionHasHeading = ref(true);
const editSectionHasOwnText = ref(true);
const editSectionParentId = ref<string>("root");
const editSectionPosition = ref(0);

const canManageSections = computed(() =>
  auth.user ? ["privileged", "admin"].includes(auth.user.role) : false,
);
const selectedSection = computed(
  () => documentData.value?.sections.find((section) => section.id === selectedSectionId.value) ?? null,
);
const siblingSections = computed(() => {
  if (!documentData.value || !selectedSection.value) {
    return [];
  }

  return documentData.value.sections
    .filter((section) => section.parent_id === selectedSection.value?.parent_id)
    .sort((a, b) => a.position - b.position || a.created_at.localeCompare(b.created_at));
});
const selectedSiblingIndex = computed(() =>
  siblingSections.value.findIndex((section) => section.id === selectedSection.value?.id),
);
const eligibleParentSections = computed(() => {
  if (!documentData.value || !selectedSection.value) {
    return documentData.value?.sections ?? [];
  }

  const selectedPathPrefix = `${selectedSection.value.path}/`;
  return documentData.value.sections.filter((section) => {
    if (section.id === selectedSection.value?.id) {
      return false;
    }
    return !section.path.startsWith(selectedPathPrefix);
  });
});
const canMoveUp = computed(() => selectedSiblingIndex.value > 0);
const canMoveDown = computed(
  () => selectedSiblingIndex.value >= 0 && selectedSiblingIndex.value < siblingSections.value.length - 1,
);
const canPromote = computed(() => {
  if (!selectedSection.value?.parent_id || !documentData.value) {
    return false;
  }
  return documentData.value.sections.some((section) => section.id === selectedSection.value?.parent_id);
});
const canDemote = computed(() => selectedSiblingIndex.value > 0);

function syncSectionForm() {
  if (!selectedSection.value) {
    editSectionTitle.value = "";
    editSectionHasHeading.value = true;
    editSectionHasOwnText.value = true;
    editSectionPosition.value = 0;
    editSectionParentId.value = "root";
    return;
  }

  editSectionTitle.value = selectedSection.value.title;
  editSectionHasHeading.value = selectedSection.value.has_heading;
  editSectionHasOwnText.value = selectedSection.value.has_own_text;
  editSectionParentId.value = selectedSection.value.parent_id ?? "root";
  editSectionPosition.value = selectedSection.value.position;
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
    if (!selectedSectionId.value || !data.sections.some((section) => section.id === selectedSectionId.value)) {
      selectedSectionId.value = data.sections[0]?.id ?? null;
    }
    syncSectionForm();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load document";
    if (error.value.toLowerCase().includes("authentication required")) {
      await router.push("/login");
    }
  } finally {
    isLoadingDocument.value = false;
  }
}

async function handleCreateSection() {
  if (!documentData.value) {
    return;
  }

  createSectionState.value = "creating";
  createSectionError.value = null;
  try {
    const siblings = documentData.value.sections.filter(
      (section) => section.parent_id === (selectedSectionId.value ?? null),
    );
    const created = await createSection(documentData.value.document.id, {
      parent_id: selectedSectionId.value,
      title: createSectionHasHeading.value ? createSectionTitle.value : "",
      has_heading: createSectionHasHeading.value,
      has_own_text: true,
      position: siblings.length,
    });
    createSectionTitle.value = "";
    createSectionHasHeading.value = true;
    await loadDocument();
    selectedSectionId.value = created.id;
  } catch (createError) {
    createSectionError.value =
      createError instanceof Error ? createError.message : "Failed to create section";
  } finally {
    createSectionState.value = "idle";
  }
}

async function handleCreateUnnamedSections() {
  if (!documentData.value) {
    return;
  }

  createSectionState.value = "creating";
  createSectionError.value = null;
  try {
    const parentId = selectedSectionId.value ?? null;
    let nextPosition = documentData.value.sections.filter((section) => section.parent_id === parentId).length;
    let lastCreatedId: string | null = null;

    for (let index = 0; index < createUnnamedSectionCount.value; index += 1) {
      const created = await createSection(documentData.value.document.id, {
        parent_id: parentId,
        title: "",
        has_heading: false,
        has_own_text: true,
        position: nextPosition,
      });
      nextPosition += 1;
      lastCreatedId = created.id;
    }

    await loadDocument();
    if (lastCreatedId) {
      selectedSectionId.value = lastCreatedId;
    }
  } catch (createError) {
    createSectionError.value =
      createError instanceof Error ? createError.message : "Failed to create unnamed sections";
  } finally {
    createSectionState.value = "idle";
  }
}

async function handleUpdateSection() {
  if (!selectedSection.value) {
    return;
  }

  updateSectionState.value = "saving";
  updateSectionError.value = null;
  try {
    await updateSection(selectedSection.value.id, {
      title: editSectionHasHeading.value ? editSectionTitle.value : "",
      has_heading: editSectionHasHeading.value,
      has_own_text: editSectionHasOwnText.value,
      parent_id: editSectionParentId.value === "root" ? null : editSectionParentId.value,
      position: editSectionPosition.value,
    });
    const keepSelectedId = selectedSection.value.id;
    await loadDocument();
    selectedSectionId.value = keepSelectedId;
  } catch (updateError) {
    updateSectionError.value =
      updateError instanceof Error ? updateError.message : "Failed to update section";
  } finally {
    updateSectionState.value = "idle";
  }
}

async function applyStructureUpdate(parentId: string | null, position: number) {
  if (!selectedSection.value) {
    return;
  }

  updateSectionState.value = "saving";
  updateSectionError.value = null;
  try {
    await updateSection(selectedSection.value.id, {
      title: editSectionHasHeading.value ? editSectionTitle.value : "",
      has_heading: editSectionHasHeading.value,
      has_own_text: editSectionHasOwnText.value,
      parent_id: parentId,
      position,
    });
    const keepSelectedId = selectedSection.value.id;
    await loadDocument();
    selectedSectionId.value = keepSelectedId;
  } catch (updateError) {
    updateSectionError.value =
      updateError instanceof Error ? updateError.message : "Failed to update section";
  } finally {
    updateSectionState.value = "idle";
  }
}

async function handleMoveUp() {
  if (!selectedSection.value || !canMoveUp.value) {
    return;
  }
  await applyStructureUpdate(selectedSection.value.parent_id, selectedSection.value.position - 1);
}

async function handleMoveDown() {
  if (!selectedSection.value || !canMoveDown.value) {
    return;
  }
  await applyStructureUpdate(selectedSection.value.parent_id, selectedSection.value.position + 1);
}

async function handlePromote() {
  if (!selectedSection.value?.parent_id || !documentData.value) {
    return;
  }

  const parent = documentData.value.sections.find((section) => section.id === selectedSection.value?.parent_id);
  if (!parent) {
    return;
  }

  await applyStructureUpdate(parent.parent_id, parent.position + 1);
}

async function handleDemote() {
  if (!selectedSection.value || !canDemote.value) {
    return;
  }

  const newParent = siblingSections.value[selectedSiblingIndex.value - 1];
  if (!newParent) {
    return;
  }

  const cousinCount =
    documentData.value?.sections.filter((section) => section.parent_id === newParent.id).length ?? 0;
  await applyStructureUpdate(newParent.id, cousinCount);
}

watch(selectedSectionId, syncSectionForm);

watch(
  () => route.params.id,
  () => {
    selectedSectionId.value = null;
    void loadDocument();
  },
);

onMounted(async () => {
  await auth.restoreSession();
  if (!canManageSections.value) {
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
            :can-manage-outline="canManageSections"
            :section-id="selectedSectionId"
            active-view="outline"
          />
        </div>
      </section>

      <section class="document-layout">
        <SectionTree
          :sections="documentData.sections"
          :active-section-id="selectedSectionId"
          @select="selectedSectionId = $event"
        />

        <div class="document-main">
          <form class="section-admin-form" @submit.prevent="handleCreateSection">
            <div>
              <p class="eyebrow">Outline Edit</p>
              <h2>Create Child Section</h2>
              <p class="section-copy">
                New sections are attached below the currently selected section. Select nothing to create a root section.
              </p>
            </div>
            <div class="section-create-controls">
              <input
                v-model="createSectionTitle"
                :disabled="!createSectionHasHeading"
                :placeholder="createSectionHasHeading ? 'New subsection title' : 'This section will have no heading'"
              />
              <label class="toggle-inline">
                <input v-model="createSectionHasHeading" type="checkbox" />
                <span>Has heading</span>
              </label>
              <button :disabled="createSectionState === 'creating' || (createSectionHasHeading && !createSectionTitle.trim())">
                {{ createSectionState === "creating" ? "Creating..." : "Add section" }}
              </button>
            </div>
            <div class="section-create-controls secondary">
              <input v-model.number="createUnnamedSectionCount" type="number" min="1" />
              <button
                type="button"
                class="secondary-button"
                :disabled="createSectionState === 'creating' || createUnnamedSectionCount < 1"
                @click="handleCreateUnnamedSections"
              >
                {{ createSectionState === "creating" ? "Creating..." : "Add unnamed subsections" }}
              </button>
            </div>
            <p v-if="createSectionError" class="error">{{ createSectionError }}</p>
          </form>

          <form
            v-if="selectedSection"
            class="section-admin-form"
            @submit.prevent="handleUpdateSection"
          >
            <div>
              <p class="eyebrow">Selected Section</p>
              <h2>Edit Outline Placement</h2>
              <p class="section-copy">
                This view changes only hierarchy, order, and headings. Section text is edited separately.
              </p>
            </div>
            <div class="section-move-actions">
              <button type="button" :disabled="updateSectionState === 'saving' || !canMoveUp" @click="handleMoveUp">
                Move up
              </button>
              <button
                type="button"
                :disabled="updateSectionState === 'saving' || !canMoveDown"
                @click="handleMoveDown"
              >
                Move down
              </button>
              <button
                type="button"
                :disabled="updateSectionState === 'saving' || !canPromote"
                @click="handlePromote"
              >
                Promote
              </button>
              <button
                type="button"
                :disabled="updateSectionState === 'saving' || !canDemote"
                @click="handleDemote"
              >
                Demote
              </button>
            </div>
            <div class="section-edit-grid">
              <label>
                Title
                <input
                  v-model="editSectionTitle"
                  :disabled="!editSectionHasHeading"
                  :placeholder="editSectionHasHeading ? 'Section title' : 'This section has no heading'"
                />
              </label>
              <div class="checkbox-row">
                <span>Heading</span>
                <label class="checkbox-inline">
                  <input v-model="editSectionHasHeading" type="checkbox" />
                  <span>This section has a heading</span>
                </label>
              </div>
              <div class="checkbox-row">
                <span>Content</span>
                <label class="checkbox-inline">
                  <input v-model="editSectionHasOwnText" type="checkbox" />
                  <span>This section has its own text</span>
                </label>
              </div>
              <label>
                Parent
                <select v-model="editSectionParentId">
                  <option value="root">root</option>
                  <option
                    v-for="section in eligibleParentSections"
                    :key="section.id"
                    :value="section.id"
                  >
                    {{ section.title }}
                  </option>
                </select>
              </label>
              <label>
                Position
                <input v-model.number="editSectionPosition" type="number" min="0" />
              </label>
              <button class="action-button" :disabled="updateSectionState === 'saving' || !editSectionTitle.trim()">
                {{ updateSectionState === "saving" ? "Saving..." : "Save section" }}
              </button>
            </div>
            <p v-if="updateSectionError" class="error">{{ updateSectionError }}</p>
          </form>

          <p v-else class="empty-note">Select a section to edit the outline.</p>
        </div>
      </section>
    </template>
  </main>
</template>

<style scoped>
@import "./document-shared.css";

.section-admin-form {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  padding: 1.2rem;
  border-radius: 1.2rem;
  background: rgba(255, 252, 247, 0.94);
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.section-copy,
.empty-note,
.section-admin-form h2 {
  margin: 0;
}

.section-copy {
  margin-top: 0.35rem;
  color: #6f5947;
}

.section-create-controls,
.section-move-actions {
  display: flex;
  gap: 0.85rem;
  flex-wrap: wrap;
}

.section-create-controls.secondary {
  align-items: center;
}

.section-create-controls input,
.section-edit-grid input,
.section-edit-grid select {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 0.95rem;
  background: white;
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

.section-create-controls input {
  flex: 1;
}

.section-create-controls button,
.section-move-actions button,
.action-button {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 1rem;
  cursor: pointer;
}

.section-create-controls button {
  background: #8e4b16;
  color: white;
}

.secondary-button {
  background: #efe4d6;
  color: #4d3322;
}

.section-move-actions button {
  background: #efe4d6;
  color: #4d3322;
}

.action-button {
  background: #4e6f3a;
  color: white;
}

.section-edit-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr)) 120px 180px;
  gap: 0.85rem;
  align-items: end;
}

.section-edit-grid label {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  color: #6f5947;
}

.checkbox-row {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  color: #6f5947;
}

.checkbox-inline {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  min-height: 3rem;
  padding: 0.85rem 0.95rem;
  border-radius: 0.95rem;
  background: white;
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
  color: #2d1d12;
}

.checkbox-inline input {
  margin: 0;
}

.toggle-inline {
  display: flex;
  align-items: center;
  gap: 0.55rem;
  padding: 0.85rem 0.95rem;
  border-radius: 0.95rem;
  background: white;
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
  color: #2d1d12;
}

.error {
  color: #9d2a16;
}

@media (max-width: 960px) {
  .section-create-controls,
  .section-edit-grid {
    flex-direction: column;
    display: flex;
  }
}
</style>
