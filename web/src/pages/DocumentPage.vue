<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import {
  createSection,
  getDocument,
  getSectionView,
  publishSection,
  saveDraft,
  setPreferredBase,
  updateSection,
} from "../api/documents";
import type { DocumentDetailResponse, SectionViewResponse } from "../api/types";
import SectionEditor from "../components/SectionEditor.vue";
import SectionTree from "../components/SectionTree.vue";
import SubmissionList from "../components/SubmissionList.vue";
import { useAuthStore } from "../stores/auth";

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const sectionView = ref<SectionViewResponse | null>(null);
const selectedSectionId = ref<string | null>(null);
const draftContent = ref("");
const isLoadingDocument = ref(true);
const isLoadingSection = ref(false);
const saveState = ref<"idle" | "saving">("idle");
const publishState = ref<"idle" | "publishing">("idle");
const createSectionState = ref<"idle" | "creating">("idle");
const updateSectionState = ref<"idle" | "saving">("idle");
const error = ref<string | null>(null);
const createSectionError = ref<string | null>(null);
const updateSectionError = ref<string | null>(null);
const createSectionTitle = ref("");
const editSectionTitle = ref("");
const editSectionParentId = ref<string>("root");
const editSectionPosition = ref(0);

const selectedSection = computed(() => sectionView.value?.section ?? null);
const canManageSections = computed(() =>
  auth.user ? ["privileged", "admin"].includes(auth.user.role) : false,
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
const globalMainSubmission = computed(
  () =>
    sectionView.value?.active_submissions.find(
      (submission) =>
        sectionView.value?.projection.find((item) => item.submission_id === submission.id)?.role === "main",
    ) ?? null,
);
const personalBaseSubmission = computed(() => {
  if (!sectionView.value) {
    return null;
  }

  return (
    sectionView.value.active_submissions.find(
      (submission) => submission.id === sectionView.value?.preferred_base_submission_id,
    ) ?? globalMainSubmission.value
  );
});

function submissionLabel(submission: SectionViewResponse["active_submissions"][number]) {
  return `${submission.display_name} @${submission.username} • ${new Date(submission.published_at).toLocaleString()}`;
}

function syncSectionForm() {
  if (!selectedSection.value) {
    editSectionTitle.value = "";
    editSectionPosition.value = 0;
    return;
  }

  editSectionTitle.value = selectedSection.value.title;
  editSectionParentId.value = selectedSection.value.parent_id ?? "root";
  editSectionPosition.value = selectedSection.value.position;
}

function syncDraftFromView(view: SectionViewResponse | null) {
  if (!view) {
    draftContent.value = "";
    return;
  }

  if (view.draft) {
    draftContent.value = view.draft.markdown_content;
    return;
  }

  const preferred =
    view.active_submissions.find((submission) => submission.id === view.preferred_base_submission_id) ??
    view.active_submissions.find(
      (submission) => view.projection.find((item) => item.submission_id === submission.id)?.role === "main",
    ) ??
    view.active_submissions[0];

  draftContent.value = preferred?.markdown_content ?? "";
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
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load document";
  } finally {
    isLoadingDocument.value = false;
  }
}

async function loadSectionView() {
  if (!selectedSectionId.value) {
    sectionView.value = null;
    draftContent.value = "";
    return;
  }

  isLoadingSection.value = true;
  error.value = null;
  try {
    sectionView.value = await getSectionView(selectedSectionId.value);
    syncDraftFromView(sectionView.value);
    syncSectionForm();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load section";
    if (error.value.toLowerCase().includes("authentication required")) {
      await router.push("/login");
    }
  } finally {
    isLoadingSection.value = false;
  }
}

async function handleSaveDraft() {
  if (!selectedSectionId.value) {
    return;
  }

  saveState.value = "saving";
  error.value = null;
  try {
    await saveDraft(selectedSectionId.value, {
      base_submission_id: sectionView.value?.preferred_base_submission_id ?? null,
      markdown_content: draftContent.value,
    });
    await loadSectionView();
  } catch (saveError) {
    error.value = saveError instanceof Error ? saveError.message : "Failed to save draft";
  } finally {
    saveState.value = "idle";
  }
}

async function handlePublish() {
  if (!selectedSectionId.value) {
    return;
  }

  publishState.value = "publishing";
  error.value = null;
  try {
    await publishSection(selectedSectionId.value, {
      base_submission_id: sectionView.value?.preferred_base_submission_id ?? null,
      markdown_content: draftContent.value,
    });
    await loadSectionView();
  } catch (publishError) {
    error.value = publishError instanceof Error ? publishError.message : "Failed to publish section";
  } finally {
    publishState.value = "idle";
  }
}

async function handleSetBase(submissionId: string) {
  if (!selectedSectionId.value) {
    return;
  }

  try {
    await setPreferredBase(selectedSectionId.value, submissionId);
    await loadSectionView();
  } catch (setBaseError) {
    error.value = setBaseError instanceof Error ? setBaseError.message : "Failed to set personal base";
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
      title: createSectionTitle.value,
      position: siblings.length,
    });
    createSectionTitle.value = "";
    await loadDocument();
    selectedSectionId.value = created.id;
  } catch (createError) {
    createSectionError.value =
      createError instanceof Error ? createError.message : "Failed to create section";
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
      title: editSectionTitle.value,
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
      title: editSectionTitle.value,
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

watch(
  () => route.params.id,
  () => {
    selectedSectionId.value = null;
    void loadDocument();
  },
);

watch(selectedSectionId, () => {
  void loadSectionView();
});

onMounted(async () => {
  await auth.restoreSession();
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
        <p class="document-slug">{{ documentData.document.slug }}</p>
      </section>

      <section class="document-layout">
        <SectionTree
          :sections="documentData.sections"
          :active-section-id="selectedSectionId"
          @select="selectedSectionId = $event"
        />

        <div class="document-main">
          <form v-if="canManageSections" class="section-admin-form" @submit.prevent="handleCreateSection">
            <div>
              <p class="eyebrow">Structure</p>
              <h2>Create Child Section</h2>
              <p class="section-create-copy">
                New sections are attached below the currently selected section. Select nothing to create a root section.
              </p>
            </div>
            <div class="section-create-controls">
              <input v-model="createSectionTitle" placeholder="New subsection title" />
              <button :disabled="createSectionState === 'creating' || !createSectionTitle.trim()">
                {{ createSectionState === "creating" ? "Creating..." : "Add section" }}
              </button>
            </div>
            <p v-if="createSectionError" class="error">{{ createSectionError }}</p>
          </form>

          <form
            v-if="canManageSections && selectedSection"
            class="section-admin-form"
            @submit.prevent="handleUpdateSection"
          >
            <div>
              <p class="eyebrow">Selected Section</p>
              <h2>Edit Title and Position</h2>
              <p class="section-create-copy">
                Update the currently selected section without changing the document hierarchy.
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
                <input v-model="editSectionTitle" placeholder="Section title" />
              </label>
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
              <button
                class="section-edit-button"
                :disabled="updateSectionState === 'saving' || !editSectionTitle.trim()"
              >
                {{ updateSectionState === "saving" ? "Saving..." : "Save section" }}
              </button>
            </div>
            <p v-if="updateSectionError" class="error">{{ updateSectionError }}</p>
          </form>

          <p v-if="isLoadingSection">Loading section...</p>
          <template v-else-if="selectedSection && sectionView">
            <SectionEditor
              :title="selectedSection.title"
              :content="draftContent"
              :save-state="saveState"
              :publish-state="publishState"
              :error="error"
              :can-edit="Boolean(auth.user)"
              :global-main-label="
                globalMainSubmission
                  ? submissionLabel(globalMainSubmission)
                  : 'No global main version yet'
              "
              :personal-base-label="
                personalBaseSubmission
                  ? submissionLabel(personalBaseSubmission)
                  : auth.user
                    ? 'Not set yet'
                    : 'Login required'
              "
              @update:content="draftContent = $event"
              @save="handleSaveDraft"
              @publish="handlePublish"
            />

            <SubmissionList
              :submissions="sectionView.active_submissions"
              :projection="sectionView.projection"
              :preferred-base-submission-id="sectionView.preferred_base_submission_id"
              @set-base="handleSetBase"
            />
          </template>
          <p v-else>Select a section to begin.</p>
        </div>
      </section>
    </template>
  </main>
</template>

<style scoped>
.document-page {
  padding: 1.5rem;
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
}

.document-header {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: end;
  padding: 1.6rem;
  border-radius: 1.5rem;
  background: linear-gradient(135deg, rgba(255, 248, 238, 0.98), rgba(235, 212, 184, 0.94));
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.document-header h1,
.document-slug,
.eyebrow {
  margin: 0;
}

.eyebrow {
  color: #8e4b16;
  text-transform: uppercase;
  font-size: 0.82rem;
  letter-spacing: 0.08em;
  margin-bottom: 0.35rem;
}

.document-header h1 {
  font-size: clamp(2rem, 4vw, 3rem);
  line-height: 0.95;
}

.document-slug {
  color: #6f5947;
}

.document-layout {
  display: grid;
  grid-template-columns: 280px minmax(0, 1fr);
  gap: 1.25rem;
  align-items: start;
}

.document-main {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.section-admin-form {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
  padding: 1.2rem;
  border-radius: 1.2rem;
  background: rgba(255, 252, 247, 0.94);
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.section-admin-form h2,
.section-create-copy {
  margin: 0;
}

.section-create-copy {
  margin-top: 0.35rem;
  color: #6f5947;
}

.section-create-controls {
  display: flex;
  gap: 0.85rem;
}

.section-create-controls input {
  flex: 1;
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 0.95rem;
  background: white;
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

.section-create-controls button {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 1rem;
  background: #8e4b16;
  color: white;
  cursor: pointer;
}

.section-create-controls button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.section-edit-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) 120px 180px;
  gap: 0.85rem;
  align-items: end;
}

.section-move-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.section-move-actions button {
  border: 0;
  border-radius: 999px;
  padding: 0.7rem 0.95rem;
  background: #efe4d6;
  color: #4d3322;
  cursor: pointer;
}

.section-move-actions button:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.section-edit-grid label {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  color: #6f5947;
}

.section-edit-grid input,
.section-edit-grid select {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 0.95rem;
  background: white;
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

.section-edit-button {
  border: 0;
  border-radius: 0.95rem;
  padding: 0.85rem 1rem;
  background: #4e6f3a;
  color: white;
  cursor: pointer;
}

.section-edit-button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.error {
  color: #9d2a16;
}

@media (max-width: 960px) {
  .document-layout {
    grid-template-columns: 1fr;
  }

  .document-header {
    flex-direction: column;
    align-items: flex-start;
  }

  .section-create-controls {
    flex-direction: column;
  }

  .section-edit-grid {
    grid-template-columns: 1fr;
  }
}
</style>
