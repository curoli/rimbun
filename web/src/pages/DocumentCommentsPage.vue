<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";

import { createSubmissionComment, getDocument, getSectionCompare } from "../api/documents";
import type { DocumentDetailResponse, SectionCompareDto, SectionRecord } from "../api/types";
import CommentDiscussion from "../components/CommentDiscussion.vue";
import DocumentViewNav from "../components/DocumentViewNav.vue";
import SectionTree from "../components/SectionTree.vue";
import { buildSectionNumbers } from "../section-numbering";
import { useAuthStore } from "../stores/auth";

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const sectionCompares = ref<Record<string, SectionCompareDto | null>>({});
const selectedSectionId = ref<string | null>(null);
const isLoading = ref(true);
const isPosting = ref(false);
const error = ref<string | null>(null);

const canManageOutline = computed(() => auth.user?.role === "admin");

const orderedSections = computed(() => {
  const sections = documentData.value?.sections ?? [];
  const byParent = new Map<string | null, SectionRecord[]>();
  for (const section of sections) {
    const group = byParent.get(section.parent_id) ?? [];
    group.push(section);
    byParent.set(section.parent_id, group);
  }
  for (const group of byParent.values()) {
    group.sort((left, right) => left.position - right.position || left.created_at.localeCompare(right.created_at));
  }

  const result: SectionRecord[] = [];
  function visit(parentId: string | null) {
    for (const section of byParent.get(parentId) ?? []) {
      result.push(section);
      visit(section.id);
    }
  }
  visit(null);
  return result;
});

const sectionNumbers = computed(() => buildSectionNumbers(orderedSections.value));
const selectedSection = computed(
  () => orderedSections.value.find((section) => section.id === selectedSectionId.value) ?? null,
);
const selectedCompare = computed(() =>
  selectedSectionId.value ? sectionCompares.value[selectedSectionId.value] ?? null : null,
);
const selectedNumber = computed(() =>
  selectedSectionId.value ? sectionNumbers.value.get(selectedSectionId.value)?.full ?? "" : "",
);

function isMissingMainSubmission(error: unknown) {
  return error instanceof Error && error.message.includes("no published main submission");
}

async function loadCompare(sectionId: string) {
  try {
    return await getSectionCompare(sectionId);
  } catch (loadError) {
    if (isMissingMainSubmission(loadError)) {
      return null;
    }
    throw loadError;
  }
}

function chooseInitialSection() {
  const requested = typeof route.query.section === "string" ? route.query.section : null;
  if (requested && orderedSections.value.some((section) => section.id === requested)) {
    selectedSectionId.value = requested;
    return;
  }

  selectedSectionId.value =
    orderedSections.value.find((section) => sectionCompares.value[section.id]?.comments.length)?.id ??
    orderedSections.value.find((section) => sectionCompares.value[section.id])?.id ??
    orderedSections.value[0]?.id ??
    null;
}

async function loadDocument() {
  const documentRef = route.params.documentRef;
  if (typeof documentRef !== "string") {
    return;
  }

  isLoading.value = true;
  error.value = null;
  try {
    const data = await getDocument(documentRef);
    documentData.value = data;
    if (documentRef !== data.document.slug) {
      await router.replace({
        path: `/documents/${data.document.slug}/comments`,
        query: route.query,
      });
      return;
    }

    const entries = await Promise.all(
      data.sections
        .filter((section) => section.has_own_text)
        .map(async (section) => [section.id, await loadCompare(section.id)] as const),
    );
    sectionCompares.value = Object.fromEntries(entries);
    chooseInitialSection();
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load comments";
  } finally {
    isLoading.value = false;
  }
}

async function handleSelectSection(sectionId: string) {
  selectedSectionId.value = sectionId;
  await router.replace({ query: { ...route.query, section: sectionId } });
}

async function handleCreateComment(payload: {
  submissionId: string;
  parentCommentId: string | null;
  markdownContent: string;
}) {
  const sectionId = selectedSectionId.value;
  if (!sectionId || isPosting.value) {
    return;
  }

  isPosting.value = true;
  error.value = null;
  try {
    await createSubmissionComment(payload.submissionId, {
      parent_comment_id: payload.parentCommentId,
      markdown_content: payload.markdownContent,
    });
    sectionCompares.value = {
      ...sectionCompares.value,
      [sectionId]: await loadCompare(sectionId),
    };
  } catch (postError) {
    error.value = postError instanceof Error ? postError.message : "Failed to create comment";
  } finally {
    isPosting.value = false;
  }
}

watch(
  () => route.query.section,
  (sectionId) => {
    if (typeof sectionId === "string" && orderedSections.value.some((section) => section.id === sectionId)) {
      selectedSectionId.value = sectionId;
    }
  },
);

watch(
  () => route.params.documentRef,
  () => void loadDocument(),
);

onMounted(async () => {
  await auth.restoreSession();
  await loadDocument();
});
</script>

<template>
  <main class="document-page">
    <p v-if="isLoading">{{ $t("Loading comments...") }}</p>
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
            :can-manage-outline="canManageOutline"
            :section-id="selectedSection?.has_own_text ? selectedSectionId : null"
            active-view="comments"
          />
        </div>
      </section>

      <section class="document-layout">
        <SectionTree
          :sections="documentData.sections"
          :active-section-id="selectedSectionId"
          @select="handleSelectSection"
        />

        <section class="comments-panel">
          <p v-if="error" class="error">{{ $t(error) }}</p>
          <template v-if="selectedSection">
            <header class="comments-heading">
              <h2>
                <span v-if="selectedNumber" class="section-number">{{ selectedNumber }}</span>
                <template v-if="selectedSection.has_heading">{{ selectedSection.title }}</template>
                <template v-else>{{ $t("Comments") }}</template>
              </h2>
              <p>{{ $t("Read and discuss the published versions of this section.") }}</p>
            </header>

            <p v-if="!auth.user" class="login-note">
              <RouterLink to="/login">{{ $t("Log in") }}</RouterLink>
              {{ $t("to write a comment.") }}
            </p>

            <CommentDiscussion
              v-if="selectedCompare"
              :compare="selectedCompare"
              :can-comment="Boolean(auth.user) && !isPosting"
              @create-comment="handleCreateComment"
            />
            <p v-else-if="selectedSection.has_own_text" class="empty-note">
              {{ $t("No published version yet.") }}
            </p>
            <p v-else class="empty-note">
              {{ $t("This section has no own text. Only its subsections contribute content.") }}
            </p>
          </template>
        </section>
      </section>
    </template>
  </main>
</template>

<style scoped>
@import "./document-shared.css";

.comments-panel {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-width: 0;
  padding: 1.25rem;
  border-radius: 1.25rem;
  background: var(--surface-panel);
}

.comments-heading h2,
.comments-heading p,
.login-note,
.empty-note {
  margin: 0;
}

.comments-heading p,
.login-note,
.empty-note {
  color: var(--text-secondary);
}

.comments-heading p {
  margin-top: 0.35rem;
}

.section-number {
  margin-right: 0.55rem;
  color: var(--accent);
}

.login-note {
  padding: 0.75rem 0.85rem;
  border-radius: 0.75rem;
  background: var(--accent-hover);
}

.login-note a {
  color: var(--accent);
  font-weight: 600;
}

.empty-note {
  padding: 1rem;
  border: 1px dashed var(--border-medium);
  border-radius: 0.8rem;
  font-style: italic;
}

@media (max-width: 900px) {
  .comments-panel {
    padding: 1rem;
  }
}
</style>
