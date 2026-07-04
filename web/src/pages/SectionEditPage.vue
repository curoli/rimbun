<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import { getDocument, getSectionView, publishSection, saveDraft, setPreferredBase } from "../api/documents";
import type { DocumentDetailResponse, SectionViewResponse } from "../api/types";
import DocumentViewNav from "../components/DocumentViewNav.vue";
import SectionEditor from "../components/SectionEditor.vue";
import SubmissionList from "../components/SubmissionList.vue";
import { buildSectionNumbers } from "../section-numbering";
import { useAuthStore } from "../stores/auth";

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const sectionView = ref<SectionViewResponse | null>(null);
const draftContent = ref("");
const isLoadingSection = ref(true);
const saveState = ref<"idle" | "saving">("idle");
const publishState = ref<"idle" | "publishing">("idle");
const error = ref<string | null>(null);

const canManageOutline = computed(() =>
  auth.user ? auth.user.role === "admin" : false,
);
const selectedSection = computed(() => sectionView.value?.section ?? null);
const sectionNumber = computed(() => {
  const section = selectedSection.value;
  const sections = documentData.value?.sections;
  if (!section || !sections) {
    return "";
  }

  return buildSectionNumbers(sections).get(section.id)?.full ?? "";
});
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

const sectionHeadingLabel = computed(() =>
  selectedSection.value?.has_heading ? selectedSection.value.title : "No heading",
);

function submissionLabel(submission: SectionViewResponse["active_submissions"][number]) {
  return `${submission.display_name} @${submission.username} • ${new Date(submission.published_at).toLocaleString()}`;
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

async function loadSectionView() {
  const id = route.params.id;
  if (typeof id !== "string") {
    return;
  }

  isLoadingSection.value = true;
  error.value = null;
  try {
    const view = await getSectionView(id);
    sectionView.value = view;
    documentData.value = await getDocument(view.section.document_id);
    syncDraftFromView(sectionView.value);
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
  const sectionId = selectedSection.value?.id;
  if (!sectionId) {
    return;
  }

  saveState.value = "saving";
  error.value = null;
  try {
    await saveDraft(sectionId, {
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
  const sectionId = selectedSection.value?.id;
  if (!sectionId) {
    return;
  }

  publishState.value = "publishing";
  error.value = null;
  try {
    await publishSection(sectionId, {
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
  const sectionId = selectedSection.value?.id;
  if (!sectionId) {
    return;
  }

  try {
    await setPreferredBase(sectionId, submissionId);
    await loadSectionView();
  } catch (setBaseError) {
    error.value = setBaseError instanceof Error ? setBaseError.message : "Failed to set personal base";
  }
}

watch(
  () => route.params.id,
  () => {
    void loadSectionView();
  },
);

onMounted(async () => {
  await auth.restoreSession();
  await loadSectionView();
});
</script>

<template>
  <main class="document-page">
    <p v-if="isLoadingSection">Loading section...</p>
    <p v-else-if="error && !sectionView" class="error">{{ error }}</p>
    <template v-else-if="selectedSection && sectionView">
      <section class="document-header">
        <div>
          <p class="eyebrow">Section Edit</p>
          <h1>
            <span v-if="selectedSection.has_heading && sectionNumber" class="section-number">{{ sectionNumber }}</span>
            {{ sectionHeadingLabel }}
          </h1>
        </div>
        <div class="document-header-meta">
          <p class="document-slug">Section workspace</p>
          <DocumentViewNav
            :document-ref="documentData?.document.slug ?? selectedSection.document_id"
            :can-manage-outline="canManageOutline"
            :section-id="selectedSection.id"
            active-view="edit"
          />
        </div>
      </section>

      <div class="document-main">
        <SectionEditor
          :title="sectionHeadingLabel"
          :content="draftContent"
          :has-own-text="selectedSection.has_own_text"
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
          v-if="selectedSection.has_own_text"
          :submissions="sectionView.active_submissions"
          :projection="sectionView.projection"
          :preferred-base-submission-id="sectionView.preferred_base_submission_id"
          @set-base="handleSetBase"
        />
        <p v-else class="empty-note">This section has no own text. Only its subsections contribute content.</p>
      </div>
    </template>
  </main>
</template>

<style scoped>
@import "./document-shared.css";

.document-main {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.error {
  color: var(--danger);
}

.empty-note {
  margin: 0;
  padding: 1rem 1.05rem;
  border-radius: 1rem;
  background: var(--surface-panel);
  color: var(--text-soft);
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

.section-number {
  margin-right: 0.55rem;
  color: var(--accent);
  font-variant-numeric: tabular-nums;
}
</style>
