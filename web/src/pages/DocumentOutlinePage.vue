<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import { createSection, deleteSection, getDocument, updateSection } from "../api/documents";
import type { DocumentDetailResponse } from "../api/types";
import DocumentViewNav from "../components/DocumentViewNav.vue";
import SectionTree from "../components/SectionTree.vue";
import { useAuthStore } from "../stores/auth";
import { t } from "../i18n";

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const selectedSectionId = ref<string | null>(null);
const isLoadingDocument = ref(true);
const createSectionState = ref<"idle" | "creating">("idle");
const updateSectionState = ref<"idle" | "saving">("idle");
const deleteSectionState = ref<"idle" | "deleting">("idle");
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
  auth.user ? auth.user.role === "admin" : false,
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
const selectedDescendantCount = computed(() => {
  if (!documentData.value || !selectedSection.value) {
    return 0;
  }
  const pathPrefix = `${selectedSection.value.path}/`;
  return documentData.value.sections.filter((section) => section.path.startsWith(pathPrefix)).length;
});
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
      await router.replace(`/documents/${data.document.slug}/outline`);
      return;
    }
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

async function handleDeleteSection() {
  const section = selectedSection.value;
  if (!section || deleteSectionState.value === "deleting") {
    return;
  }

  const confirmation = selectedDescendantCount.value > 0
    ? t("Delete this section and all subsections ({count})? All associated contributions, drafts, and comments will be deleted.", {
        count: selectedDescendantCount.value,
      })
    : t("Delete this section? All associated contributions, drafts, and comments will be deleted.");
  if (!window.confirm(confirmation)) {
    return;
  }

  deleteSectionState.value = "deleting";
  updateSectionError.value = null;
  const nextSelection = section.parent_id;
  try {
    await deleteSection(section.id);
    selectedSectionId.value = nextSelection;
    await loadDocument();
  } catch (deleteError) {
    updateSectionError.value =
      deleteError instanceof Error ? deleteError.message : "Failed to delete section";
  } finally {
    deleteSectionState.value = "idle";
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
  () => route.params.documentRef,
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
    <p v-if="isLoadingDocument">{{ $t("Loading document...") }}</p>
    <p v-else-if="error && !documentData" class="error">{{ $t(error) }}</p>
    <template v-else-if="documentData">
      <section class="document-header">
        <div>
          <p class="eyebrow">{{ $t(documentData.document.visibility) }}</p>
          <h1>{{ documentData.document.title }}</h1>
        </div>
        <div class="document-header-meta">
          <p class="document-slug">{{ documentData.document.slug }}</p>
          <DocumentViewNav
            :document-ref="documentData.document.slug"
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
              <p class="eyebrow">{{ $t("Outline Edit") }}</p>
              <h2>{{ $t("Create Child Section") }}</h2>
              <p class="section-copy">
                {{ $t("New sections are attached below the currently selected section. Select nothing to create a root section.") }}
              </p>
            </div>
            <div class="section-create-controls">
              <input
                v-model="createSectionTitle"
                :disabled="!createSectionHasHeading"
                :placeholder="createSectionHasHeading ? $t('New subsection title') : $t('This section will have no heading')"
              />
              <label class="toggle-inline">
                <input v-model="createSectionHasHeading" type="checkbox" />
                <span>{{ $t("Has heading") }}</span>
              </label>
              <button :disabled="createSectionState === 'creating' || (createSectionHasHeading && !createSectionTitle.trim())">
                {{ createSectionState === "creating" ? $t("Creating...") : $t("Add section") }}
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
                {{ createSectionState === "creating" ? $t("Creating...") : $t("Add unnamed subsections") }}
              </button>
            </div>
            <p v-if="createSectionError" class="error">{{ $t(createSectionError) }}</p>
          </form>

          <form
            v-if="selectedSection"
            class="section-admin-form"
            @submit.prevent="handleUpdateSection"
          >
            <div>
              <p class="eyebrow">{{ $t("Selected Section") }}</p>
              <h2>{{ $t("Edit Outline Placement") }}</h2>
              <p class="section-copy">
                {{ $t("This view changes only hierarchy, order, and headings. Section text is edited separately.") }}
              </p>
            </div>
            <div class="section-move-actions">
              <button type="button" :disabled="updateSectionState === 'saving' || !canMoveUp" @click="handleMoveUp">
                {{ $t("Move up") }}
              </button>
              <button
                type="button"
                :disabled="updateSectionState === 'saving' || !canMoveDown"
                @click="handleMoveDown"
              >
                {{ $t("Move down") }}
              </button>
              <button
                type="button"
                :disabled="updateSectionState === 'saving' || !canPromote"
                @click="handlePromote"
              >
                {{ $t("Promote") }}
              </button>
              <button
                type="button"
                :disabled="updateSectionState === 'saving' || !canDemote"
                @click="handleDemote"
              >
                {{ $t("Demote") }}
              </button>
            </div>
            <div class="section-edit-grid">
              <label>
                {{ $t("Title") }}
                <input
                  v-model="editSectionTitle"
                  :disabled="!editSectionHasHeading"
                  :placeholder="editSectionHasHeading ? $t('Section title') : $t('This section has no heading')"
                />
              </label>
              <div class="checkbox-row">
                <span>{{ $t("Heading") }}</span>
                <label class="checkbox-inline">
                  <input v-model="editSectionHasHeading" type="checkbox" />
                  <span>{{ $t("This section has a heading") }}</span>
                </label>
              </div>
              <div class="checkbox-row">
                <span>{{ $t("Content") }}</span>
                <label class="checkbox-inline">
                  <input v-model="editSectionHasOwnText" type="checkbox" />
                  <span>{{ $t("This section has its own text") }}</span>
                </label>
              </div>
              <label>
                {{ $t("Parent") }}
                <select v-model="editSectionParentId">
                  <option value="root">{{ $t("root") }}</option>
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
                {{ $t("Position") }}
                <input v-model.number="editSectionPosition" type="number" min="0" />
              </label>
              <button class="action-button" :disabled="updateSectionState === 'saving' || !editSectionTitle.trim()">
                {{ updateSectionState === "saving" ? $t("Saving...") : $t("Save section") }}
              </button>
            </div>
            <div class="danger-zone">
              <div>
                <strong>{{ $t("Delete section") }}</strong>
                <p>
                  {{
                    selectedDescendantCount > 0
                      ? $t("This also deletes all subsections ({count}) and their content.", { count: selectedDescendantCount })
                      : $t("This permanently deletes the section and its content.")
                  }}
                </p>
              </div>
              <button
                type="button"
                class="delete-button"
                :disabled="deleteSectionState === 'deleting' || updateSectionState === 'saving'"
                @click="handleDeleteSection"
              >
                {{ deleteSectionState === "deleting" ? $t("Deleting...") : $t("Delete section") }}
              </button>
            </div>
            <p v-if="updateSectionError" class="error">{{ $t(updateSectionError) }}</p>
          </form>

          <p v-else class="empty-note">{{ $t("Select a section to edit the outline.") }}</p>
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
  background: var(--surface-panel);
  border: 1px solid var(--border-soft);
}

.section-copy,
.empty-note,
.section-admin-form h2 {
  margin: 0;
}

.section-copy {
  margin-top: 0.35rem;
  color: var(--text-secondary);
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
  background: var(--surface-input);
  box-shadow: inset 0 0 0 1px var(--border-soft);
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
  background: var(--accent);
  color: var(--text-on-accent);
}

.secondary-button {
  background: var(--accent-hover);
  color: var(--accent-contrast);
}

.section-move-actions button {
  background: var(--accent-hover);
  color: var(--accent-contrast);
}

.action-button {
  background: #4e6f3a;
  color: var(--text-on-accent);
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
  color: var(--text-secondary);
}

.checkbox-row {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  color: var(--text-secondary);
}

.checkbox-inline {
  display: flex;
  align-items: center;
  gap: 0.6rem;
  min-height: 3rem;
  padding: 0.85rem 0.95rem;
  border-radius: 0.95rem;
  background: var(--surface-input);
  box-shadow: inset 0 0 0 1px var(--border-soft);
  color: var(--text-strong);
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
  background: var(--surface-input);
  box-shadow: inset 0 0 0 1px var(--border-soft);
  color: var(--text-strong);
}

.error {
  color: var(--danger);
}

.danger-zone {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 1rem;
  padding: 1rem;
  border: 1px solid color-mix(in srgb, var(--danger) 35%, var(--border-soft));
  border-radius: 0.95rem;
  background: color-mix(in srgb, var(--danger) 7%, var(--surface-input));
}

.danger-zone p {
  margin: 0.25rem 0 0;
  color: var(--text-secondary);
}

.delete-button {
  flex: 0 0 auto;
  border: 0;
  border-radius: 0.85rem;
  padding: 0.75rem 1rem;
  background: var(--danger);
  color: white;
  cursor: pointer;
}

.delete-button:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

@media (max-width: 960px) {
  .section-create-controls,
  .section-edit-grid {
    flex-direction: column;
    display: flex;
  }


  .danger-zone {
    align-items: stretch;
    flex-direction: column;
  }
}
</style>
