<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import { getDocument, getSectionCompare } from "../api/documents";
import type {
  CompareBlockDto,
  DocumentDetailResponse,
  SectionCompareDto,
  SectionRecord,
  SubmissionSummaryDto,
} from "../api/types";
import DocumentViewNav from "../components/DocumentViewNav.vue";
import SectionTree from "../components/SectionTree.vue";
import { buildInlineDiff, type DiffSegment } from "../inline-diff";
import { buildSectionNumbers } from "../section-numbering";
import { useAuthStore } from "../stores/auth";

type SectionCompareItem = {
  section: SectionRecord;
  number: string;
  compare: SectionCompareDto | null;
  submissions: SubmissionSummaryDto[];
};

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const sectionCompares = ref<Record<string, SectionCompareDto>>({});
const selectedSectionId = ref<string | null>(null);
const isLoadingDocument = ref(true);
const isLoadingCompares = ref(false);
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

const sectionNumbers = computed(() => buildSectionNumbers(orderedSections.value));

const compareSections = computed<SectionCompareItem[]>(() =>
  orderedSections.value.map((section) => {
    const compare = sectionCompares.value[section.id] ?? null;
    return {
      section,
      number: compare?.section_number ?? sectionNumbers.value.get(section.id)?.full ?? "",
      compare,
      submissions: compare ? [compare.main_submission, ...compare.alternatives] : [],
    };
  }),
);

function submissionLabel(submission: SubmissionSummaryDto) {
  return `${submission.display_name} @${submission.username} • ${new Date(submission.published_at).toLocaleString()}`;
}

function supportLabel(score: number | null) {
  return score === null ? "n/a" : `${score.toFixed(0)}%`;
}

function changedVariants(block: CompareBlockDto) {
  return block.variants.filter((variant) => variant.kind === "changed");
}

function submissionById(item: SectionCompareItem, submissionId: string) {
  return item.submissions.find((submission) => submission.submission_id === submissionId) ?? null;
}

function blockLabel(block: CompareBlockDto) {
  return block.block_kind.replaceAll("_", " ");
}

function inlineDiff(block: CompareBlockDto, alternativeText: string) {
  return buildInlineDiff(block.main_text, alternativeText);
}

function segmentClass(kind: DiffSegment["kind"]) {
  return {
    unchanged: kind === "unchanged",
    removed: kind === "removed",
    added: kind === "added",
  };
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
    selectedSectionId.value =
      selectedSectionId.value && data.sections.some((section) => section.id === selectedSectionId.value)
        ? selectedSectionId.value
        : data.sections[0]?.id ?? null;
    await loadSectionCompares(data.sections.map((section) => section.id));
  } catch (loadError) {
    error.value = loadError instanceof Error ? loadError.message : "Failed to load document";
    if (error.value.toLowerCase().includes("authentication required")) {
      await router.push("/login");
    }
  } finally {
    isLoadingDocument.value = false;
  }
}

async function loadSectionCompares(sectionIds: string[]) {
  isLoadingCompares.value = true;
  try {
    const entries = await Promise.all(
      sectionIds.map(async (sectionId) => [sectionId, await getSectionCompare(sectionId)] as const),
    );
    sectionCompares.value = Object.fromEntries(entries);
  } finally {
    isLoadingCompares.value = false;
  }
}

async function handleSelectSection(sectionId: string) {
  selectedSectionId.value = sectionId;
  await nextTick();
  document.getElementById(`compare-section-${sectionId}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
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
            active-view="compare"
          />
        </div>
      </section>

      <section class="document-layout">
        <SectionTree
          :sections="documentData.sections"
          :active-section-id="selectedSectionId"
          @select="handleSelectSection"
        />

        <section class="compare-panel">
          <div class="compare-panel-header">
            <div>
              <p class="eyebrow">Compare</p>
              <h2>Aligned variants</h2>
            </div>
            <p class="compare-copy">
              Shared wording stays plain. Differences are marked directly inside the main text and each alternative.
            </p>
          </div>

          <p v-if="isLoadingCompares">Loading comparisons...</p>
          <div v-else class="compare-sections">
            <article
              v-for="item in compareSections"
              :id="`compare-section-${item.section.id}`"
              :key="item.section.id"
              class="compare-section"
              :class="{ active: item.section.id === selectedSectionId }"
            >
              <header class="compare-section-header">
                <div class="section-heading">
                  <h3>
                    <span class="section-number">{{ item.number }}</span>
                    {{ item.section.title }}
                  </h3>
                  <p v-if="item.compare" class="main-meta">
                    Main: {{ submissionLabel(item.compare.main_submission) }}
                  </p>
                </div>
                <RouterLink class="edit-link" :to="`/sections/${item.section.id}/edit`">
                  Edit this section
                </RouterLink>
              </header>

              <div v-if="item.submissions.length" class="rank-strip">
                <span
                  v-for="submission in item.submissions"
                  :key="submission.submission_id"
                  class="rank-pill"
                  :class="{ main: submission.submission_id === item.compare?.main_submission.submission_id }"
                >
                  <span class="rank-marker">{{ submission.rank }}</span>
                  <span>{{ submission.display_name }} @{{ submission.username }}</span>
                  <span v-if="submission.support_percent !== null" class="support-pill">
                    {{ supportLabel(submission.support_percent) }}
                  </span>
                </span>
              </div>

              <div v-if="item.compare?.blocks.length" class="block-list">
                <article
                  v-for="block in item.compare.blocks"
                  :key="`${item.section.id}-${block.anchor.block_key}-${block.block_index}`"
                  class="block-card"
                >
                  <div v-if="changedVariants(block).length" class="variant-list">
                    <article
                      v-for="variant in changedVariants(block)"
                      :key="`${block.anchor.block_key}-${variant.alternative_submission_id}-${variant.alternative_index}`"
                      class="variant-card"
                    >
                      <header class="variant-header">
                        <strong class="variant-meta">
                          <span class="rank-marker">
                            {{
                              submissionById(item, variant.alternative_submission_id)?.rank !== undefined
                                ? submissionById(item, variant.alternative_submission_id)!.rank
                                : "?"
                            }}
                          </span>
                          <span>
                            {{
                              submissionById(item, variant.alternative_submission_id)
                                ? submissionLabel(submissionById(item, variant.alternative_submission_id)!)
                                : "Unknown alternative"
                            }}
                          </span>
                        </strong>
                        <span
                          v-if="submissionById(item, variant.alternative_submission_id)?.support_percent !== null"
                          class="support-pill"
                        >
                          {{ supportLabel(submissionById(item, variant.alternative_submission_id)!.support_percent) }}
                        </span>
                      </header>

                      <div class="comparison-columns">
                        <section class="diff-pane main-pane">
                          <header class="diff-pane-header">
                            <span class="block-kind">{{ blockLabel(block) }}</span>
                            <span class="diff-label">Main</span>
                          </header>
                          <p class="diff-text">
                            <template
                              v-for="(segment, index) in inlineDiff(block, variant.text).reference"
                              :key="`main-${block.block_index}-${variant.alternative_submission_id}-${index}`"
                            >
                              <span class="diff-segment" :class="segmentClass(segment.kind)">{{ segment.text }}</span>
                            </template>
                          </p>
                        </section>

                        <section class="diff-pane alt-pane">
                          <header class="diff-pane-header">
                            <span class="block-kind">{{ blockLabel(block) }}</span>
                            <span class="diff-label">Alternative</span>
                          </header>
                          <p class="diff-text">
                            <template
                              v-for="(segment, index) in inlineDiff(block, variant.text).alternative"
                              :key="`alt-${block.block_index}-${variant.alternative_submission_id}-${index}`"
                            >
                              <span class="diff-segment" :class="segmentClass(segment.kind)">{{ segment.text }}</span>
                            </template>
                          </p>
                        </section>
                      </div>
                    </article>
                  </div>
                  <div v-else class="all-equal-note">
                    <span class="block-kind">{{ blockLabel(block) }}</span>
                    <span>All visible alternatives match this block.</span>
                  </div>
                </article>
              </div>
              <p v-else class="empty-note">No published version yet.</p>
            </article>
          </div>
        </section>
      </section>
    </template>
  </main>
</template>

<style scoped>
@import "./document-shared.css";

.compare-panel,
.compare-section,
.block-card,
.variant-card,
.diff-pane {
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.compare-panel {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.25rem;
  border-radius: 1.25rem;
  background: rgba(255, 252, 247, 0.94);
}

.compare-panel-header,
.compare-section-header,
.variant-header {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: flex-start;
}

.compare-panel-header h2,
.compare-copy,
.compare-section-header h3,
.main-meta,
.empty-note {
  margin: 0;
}

.compare-copy,
.main-meta {
  color: #6f5947;
}

.compare-sections,
.block-list,
.variant-list {
  display: flex;
  flex-direction: column;
}

.compare-sections {
  gap: 1rem;
}

.compare-section {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1rem;
  border-radius: 1rem;
  background: #fffdf9;
}

.compare-section.active {
  box-shadow: inset 0 0 0 2px rgba(142, 75, 22, 0.22);
}

.section-heading {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.edit-link {
  color: #8e4b16;
  text-decoration: none;
}

.section-number {
  margin-right: 0.55rem;
  color: #8e4b16;
  font-variant-numeric: tabular-nums;
}

.rank-strip {
  display: flex;
  flex-wrap: wrap;
  gap: 0.6rem;
}

.rank-pill,
.support-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  border-radius: 999px;
  font-size: 0.78rem;
  white-space: nowrap;
}

.rank-pill {
  padding: 0.45rem 0.75rem;
  background: rgba(142, 75, 22, 0.08);
  color: #58351a;
}

.rank-pill.main {
  background: rgba(142, 75, 22, 0.18);
}

.support-pill {
  padding: 0.32rem 0.62rem;
  background: rgba(142, 75, 22, 0.1);
  color: #8e4b16;
}

.rank-marker {
  min-width: 1.35rem;
  color: #8e4b16;
  font-variant-numeric: tabular-nums;
}

.block-list {
  gap: 0.9rem;
}

.block-card,
.variant-card,
.diff-pane {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 0.95rem;
  border-radius: 1rem;
  background: white;
}

.block-kind {
  text-transform: capitalize;
  color: #8e4b16;
  font-size: 0.8rem;
  letter-spacing: 0.04em;
}

.variant-list {
  gap: 0.75rem;
}

.variant-card {
  background: #fff8ef;
}

.comparison-columns {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.75rem;
}

.diff-pane {
  background: rgba(255, 255, 255, 0.78);
}

.main-pane {
  background: #fffdf9;
}

.alt-pane {
  background: #fff9f1;
}

.diff-pane-header {
  display: flex;
  justify-content: space-between;
  gap: 0.75rem;
  align-items: baseline;
}

.diff-label,
.all-equal-note {
  color: #6f5947;
  font-size: 0.84rem;
}

.all-equal-note {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: baseline;
}

.variant-meta {
  display: flex;
  align-items: baseline;
  gap: 0.55rem;
  font-weight: 600;
}

.diff-text {
  margin: 0;
  white-space: pre-wrap;
  line-height: 1.55;
  font-size: 0.98rem;
}

.diff-segment {
  white-space: pre-wrap;
}

.diff-segment.removed {
  background: rgba(192, 74, 53, 0.14);
  color: #8e2f23;
  text-decoration: line-through;
}

.diff-segment.added {
  background: rgba(82, 131, 61, 0.16);
  color: #335f28;
}

.diff-segment.unchanged {
  color: #22150d;
}

@media (max-width: 960px) {
  .compare-panel-header,
  .compare-section-header,
  .variant-header {
    flex-direction: column;
  }

  .comparison-columns {
    grid-template-columns: 1fr;
  }
}
</style>
