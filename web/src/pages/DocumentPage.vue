<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";

import { getDocument, getSectionView } from "../api/documents";
import type { DocumentDetailResponse, SectionRecord, SectionViewResponse } from "../api/types";
import DocumentViewNav from "../components/DocumentViewNav.vue";
import SectionTree from "../components/SectionTree.vue";
import { useAuthStore } from "../stores/auth";

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const sectionViews = ref<Record<string, SectionViewResponse>>({});
const selectedSectionId = ref<string | null>(null);
const isLoadingDocument = ref(true);
const isLoadingReader = ref(false);
const error = ref<string | null>(null);

const canManageOutline = computed(() =>
  auth.user ? ["privileged", "admin"].includes(auth.user.role) : false,
);

const orderedSections = computed(() => {
  const sections = documentData.value?.sections ?? [];
  const byParent = new Map<string | null, SectionRecord[]>();

  for (const section of sections) {
    const group = byParent.get(section.parent_id) ?? [];
    group.push(section);
    byParent.set(section.parent_id, group);
  }

  for (const group of byParent.values()) {
    group.sort((a, b) => a.position - b.position || a.created_at.localeCompare(b.created_at));
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

const sectionNumbers = computed(() => {
  const byParent = new Map<string | null, SectionRecord[]>();

  for (const section of orderedSections.value) {
    const group = byParent.get(section.parent_id) ?? [];
    group.push(section);
    byParent.set(section.parent_id, group);
  }

  for (const group of byParent.values()) {
    group.sort((a, b) => a.position - b.position || a.created_at.localeCompare(b.created_at));
  }

  const result = new Map<string, string>();

  function visit(parentId: string | null, prefix: number[]) {
    const children = byParent.get(parentId) ?? [];
    children.forEach((child, index) => {
      const nextPrefix = [...prefix, index + 1];
      result.set(child.id, nextPrefix.join("."));
      visit(child.id, nextPrefix);
    });
  }

  visit(null, []);
  return result;
});

const readerSections = computed(() =>
  orderedSections.value.map((section) => {
    const view = sectionViews.value[section.id];
    const mainSubmission =
      view?.active_submissions.find(
        (submission) => view.projection.find((item) => item.submission_id === submission.id)?.role === "main",
      ) ?? view?.active_submissions[0] ?? null;
    const alternativeCount =
      view?.projection.filter((item) => item.role === "principal_alternative").length ?? 0;

    return {
      section,
      number: sectionNumbers.value.get(section.id) ?? "",
      mainSubmission,
      alternativeCount,
    };
  }),
);

function submissionLabel(submission: SectionViewResponse["active_submissions"][number]) {
  return `${submission.display_name} @${submission.username} • ${new Date(submission.published_at).toLocaleString()}`;
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
    selectedSectionId.value = selectedSectionId.value && data.sections.some((s) => s.id === selectedSectionId.value)
      ? selectedSectionId.value
      : data.sections[0]?.id ?? null;
    await loadSectionViews(data.sections.map((section) => section.id));
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load document";
    if (error.value.toLowerCase().includes("authentication required")) {
      await router.push("/login");
    }
  } finally {
    isLoadingDocument.value = false;
  }
}

async function loadSectionViews(sectionIds: string[]) {
  isLoadingReader.value = true;
  try {
    const entries = await Promise.all(
      sectionIds.map(async (sectionId) => [sectionId, await getSectionView(sectionId)] as const),
    );
    sectionViews.value = Object.fromEntries(entries);
  } finally {
    isLoadingReader.value = false;
  }
}

async function handleSelectSection(sectionId: string) {
  selectedSectionId.value = sectionId;
  await nextTick();
  document.getElementById(`reader-section-${sectionId}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

watch(
  () => route.params.id,
  () => {
    selectedSectionId.value = null;
    void loadDocument();
  },
);

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
        <div class="document-header-meta">
          <p class="document-slug">{{ documentData.document.slug }}</p>
          <DocumentViewNav
            :document-id="documentData.document.id"
            :can-manage-outline="canManageOutline"
            :section-id="selectedSectionId"
            active-view="reader"
          />
        </div>
      </section>

      <section class="document-layout">
        <SectionTree
          :sections="documentData.sections"
          :active-section-id="selectedSectionId"
          @select="handleSelectSection"
        />

        <section class="reader-panel">
          <p v-if="isLoadingReader">Loading document text...</p>
          <div v-else-if="readerSections.length" class="reader-sections">
            <section
              v-for="item in readerSections"
              :id="`reader-section-${item.section.id}`"
              :key="item.section.id"
              class="reader-section"
              :class="{ active: item.section.id === selectedSectionId }"
            >
              <div class="reader-section-heading">
                <h2>
                  <span class="reader-section-number">{{ item.number }}</span>
                  {{ item.section.title }}
                </h2>
                <div class="reader-section-meta">
                  <span v-if="item.mainSubmission">
                    {{ submissionLabel(item.mainSubmission) }}
                  </span>
                  <span v-else>No published version yet</span>
                  <span v-if="item.alternativeCount" class="reader-badge">
                    {{ item.alternativeCount }} alternative{{ item.alternativeCount === 1 ? "" : "s" }}
                  </span>
                </div>
              </div>

              <div v-if="item.mainSubmission" class="reader-markdown">
                {{ item.mainSubmission.markdown_content }}
              </div>
              <p v-else class="reader-empty">
                No published content exists for this section yet.
              </p>

              <div class="reader-actions">
                <RouterLink :to="`/documents/${documentData.document.id}/compare`">Compare alternatives</RouterLink>
                <RouterLink :to="`/sections/${item.section.id}/edit`">Edit this section</RouterLink>
              </div>
            </section>
          </div>
          <p v-else class="reader-empty">This document has no sections yet.</p>
        </section>
      </section>
    </template>
  </main>
</template>

<style scoped>
@import "./document-shared.css";

.reader-panel {
  display: flex;
  flex-direction: column;
  gap: 1.5rem;
  padding: 0.25rem 0;
}

.reader-empty,
.reader-section-heading h2 {
  margin: 0;
}

.reader-sections {
  display: flex;
  flex-direction: column;
  gap: 2rem;
}

.reader-section {
  display: flex;
  flex-direction: column;
  gap: 0.95rem;
  padding-top: 2rem;
  border-top: 1px solid rgba(35, 24, 15, 0.1);
}

.reader-section:first-child {
  padding-top: 0;
  border-top: 0;
}

.reader-section.active {
  scroll-margin-top: 5rem;
}

.reader-section-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0.65rem;
  color: #6f5947;
  font-size: 0.88rem;
}

.reader-section-heading {
  display: flex;
  flex-direction: column;
  gap: 0.45rem;
}

.reader-section-heading h2 {
  font-size: clamp(1.25rem, 2vw, 1.7rem);
  line-height: 1.08;
}

.reader-section-number {
  margin-right: 0.55rem;
  color: #8e4b16;
  font-variant-numeric: tabular-nums;
}

.reader-badge {
  padding: 0.35rem 0.6rem;
  border-radius: 999px;
  background: #f1dcc4;
  color: #5f3b1c;
}

.reader-markdown {
  white-space: pre-wrap;
  line-height: 1.72;
  color: #2d1d12;
  font-size: 1.02rem;
}

.reader-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.9rem;
}

.reader-actions a {
  color: #8e4b16;
  text-decoration: none;
}

.error {
  color: #9d2a16;
}

@media (max-width: 960px) {
}
</style>
