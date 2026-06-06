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
import { buildInlineDiff } from "../inline-diff";
import { buildSectionNumbers } from "../section-numbering";
import { useAuthStore } from "../stores/auth";

type SectionCompareItem = {
  section: SectionRecord;
  number: string;
  compare: SectionCompareDto | null;
  submissions: SubmissionSummaryDto[];
};

type VariantHighlight = {
  id: string;
  sourceText: string;
  start: number;
  end: number;
  variants: Array<{
    variant: ReturnType<typeof changedVariants>[number];
    submission: SubmissionSummaryDto | null;
    text: string;
  }>;
};

type ExcerptSegment = {
  text: string;
  changed: boolean;
};

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const sectionCompares = ref<Record<string, SectionCompareDto | null>>({});
const selectedSectionId = ref<string | null>(null);
const openVariantKey = ref<string | null>(null);
const dismissedVariantKey = ref<string | null>(null);
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

function markerKey(sectionId: string, highlightId: string) {
  return `${sectionId}:${highlightId}`;
}

function openVariantPanel(sectionId: string, highlightId: string) {
  const key = markerKey(sectionId, highlightId);
  dismissedVariantKey.value = null;
  openVariantKey.value = key;
}

function closeVariantPanel() {
  dismissedVariantKey.value = openVariantKey.value;
  openVariantKey.value = null;
}

function handleCloseVariantPanel(event: MouseEvent) {
  event.stopPropagation();
  closeVariantPanel();
}

function handleVariantMarkerClick(sectionId: string, highlightId: string, event: MouseEvent) {
  event.stopPropagation();
  if (isVariantPanelOpen(sectionId, highlightId)) {
    closeVariantPanel();
  } else {
    openVariantPanel(sectionId, highlightId);
  }
}

function isVariantPanelOpen(sectionId: string, highlightId: string) {
  return openVariantKey.value === markerKey(sectionId, highlightId);
}

function isVariantPanelDismissed(sectionId: string, highlightId: string) {
  return dismissedVariantKey.value === markerKey(sectionId, highlightId);
}

function clearDismissedVariantPanel(sectionId: string, highlightId: string) {
  if (isVariantPanelDismissed(sectionId, highlightId)) {
    dismissedVariantKey.value = null;
  }
}

function changedText(segments: ReturnType<typeof buildInlineDiff>["reference"], kind: "removed" | "added") {
  return segments
    .filter((segment) => segment.kind === kind)
    .map((segment) => segment.text)
    .join("")
    .trim();
}

function blockHighlights(item: SectionCompareItem, block: CompareBlockDto): VariantHighlight[] {
  const highlights = new Map<string, VariantHighlight>();

  for (const variant of changedVariants(block)) {
    const fallbackDiff =
      variant.reference_text && variant.text
        ? null
        : inlineDiff(block, variant.text);
    const sourceText =
      variant.reference_text?.trim()
      || (fallbackDiff ? changedText(fallbackDiff.reference, "removed") : "")
      || block.main_text.trim();
    const alternativeText =
      variant.text.trim()
      || (fallbackDiff ? changedText(fallbackDiff.alternative, "added") : "");
    const start = variant.reference_start ?? 0;
    const end = variant.reference_end ?? start + sourceText.length;
    if (!sourceText || !alternativeText || end <= start) {
      continue;
    }

    const key = `${start}:${end}`;
    const highlight = highlights.get(key) ?? {
      id: `${block.anchor.block_key}-${block.block_index}-${highlights.size}`,
      sourceText,
      start,
      end,
      variants: [],
    };
    highlight.variants.push({
      variant,
      submission: submissionById(item, variant.alternative_submission_id),
      text: alternativeText,
    });
    highlights.set(key, highlight);
  }

  return [...highlights.values()];
}

function mainSegments(item: SectionCompareItem, block: CompareBlockDto) {
  const highlights = blockHighlights(item, block);
  if (!highlights.length) {
    return [{ text: block.main_text, highlight: null as VariantHighlight | null }];
  }

  const segments: Array<{ text: string; highlight: VariantHighlight | null }> = [];
  const boundaries = new Set<number>([0, block.main_text.length]);
  for (const highlight of highlights) {
    boundaries.add(highlight.start);
    boundaries.add(highlight.end);
  }
  const orderedBoundaries = [...boundaries].sort((left, right) => left - right);

  for (let index = 0; index < orderedBoundaries.length - 1; index += 1) {
    const start = orderedBoundaries[index];
    const end = orderedBoundaries[index + 1];
    if (end <= start) {
      continue;
    }

    const text = block.main_text.slice(start, end);
    const overlapping = highlights.filter((highlight) => highlight.start < end && highlight.end > start);
    if (!overlapping.length) {
      segments.push({ text, highlight: null });
      continue;
    }

    const combinedHighlight: VariantHighlight = {
      id: `${block.anchor.block_key}-${block.block_index}-${start}-${end}`,
      sourceText: text,
      start,
      end,
      variants: dedupeHighlightVariants(overlapping.flatMap((highlight) => highlight.variants)),
    };
    segments.push({ text, highlight: combinedHighlight });
  }

  return segments;
}

function dedupeHighlightVariants(variants: VariantHighlight["variants"]) {
  const seen = new Set<string>();
  return variants.filter((entry) => {
    const key = `${entry.variant.alternative_submission_id}:${entry.variant.reference_start ?? "none"}:${entry.variant.reference_end ?? "none"}:${entry.text}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}

function variantChangesForSubmission(block: CompareBlockDto, submissionId: string) {
  return changedVariants(block)
    .filter(
      (variant) =>
        variant.alternative_submission_id === submissionId
        && variant.reference_start !== null
        && variant.reference_end !== null,
    )
    .map((variant) => ({
      start: variant.reference_start as number,
      end: variant.reference_end as number,
      replacement: variant.text,
    }))
    .sort((left, right) => left.start - right.start || left.end - right.end);
}

function mergeNearbyChanges(
  changes: Array<{ start: number; end: number; replacement: string }>,
  gapPadding: number,
) {
  if (!changes.length) {
    return [];
  }

  const clusters: Array<Array<{ start: number; end: number; replacement: string }>> = [];
  let currentCluster = [changes[0]];

  for (let index = 1; index < changes.length; index += 1) {
    const current = changes[index];
    const previous = currentCluster[currentCluster.length - 1];
    if (current.start - previous.end <= gapPadding) {
      currentCluster.push(current);
    } else {
      clusters.push(currentCluster);
      currentCluster = [current];
    }
  }

  clusters.push(currentCluster);
  return clusters;
}

function variantExcerpt(
  block: CompareBlockDto,
  variant: ReturnType<typeof changedVariants>[number],
  padding = 7,
) {
  const currentStart = variant.reference_start ?? 0;
  const currentEnd = variant.reference_end ?? currentStart;
  const changes = variantChangesForSubmission(block, variant.alternative_submission_id);
  const clusters = mergeNearbyChanges(changes, padding);
  const cluster = clusters.find((candidate) =>
    candidate.some((change) => change.start === currentStart && change.end === currentEnd),
  ) ?? [{ start: currentStart, end: currentEnd, replacement: variant.text }];

  const clusterStart = cluster[0].start;
  const clusterEnd = cluster[cluster.length - 1].end;
  const excerptStart = Math.max(0, clusterStart - padding);
  const excerptEnd = Math.min(block.main_text.length, clusterEnd + padding);
  const segments: ExcerptSegment[] = [];
  let cursor = excerptStart;

  for (const change of cluster) {
    if (change.start > cursor) {
      segments.push({
        text: block.main_text.slice(cursor, change.start),
        changed: false,
      });
    }
    segments.push({
      text: change.replacement,
      changed: true,
    });
    cursor = change.end;
  }

  if (cursor < excerptEnd) {
    segments.push({
      text: block.main_text.slice(cursor, excerptEnd),
      changed: false,
    });
  }

  return {
    leadingEllipsis: excerptStart > 0,
    trailingEllipsis: excerptEnd < block.main_text.length,
    segments,
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
      sectionIds.map(async (sectionId) => {
        const section = documentData.value?.sections.find((item) => item.id === sectionId) ?? null;
        if (!section?.has_own_text) {
          return [sectionId, null] as const;
        }
        return [sectionId, await getSectionCompare(sectionId)] as const;
      }),
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

        <section class="compare-panel">
          <p v-if="isLoadingCompares">Loading document text...</p>
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
                  <div class="section-meta">
                    <span v-if="item.compare">
                      {{ submissionLabel(item.compare.main_submission) }}
                    </span>
                    <span v-if="item.compare?.alternatives.length" class="reader-badge">
                      {{ item.compare.alternatives.length }} alternative{{ item.compare.alternatives.length === 1 ? "" : "s" }}
                    </span>
                  </div>
                </div>
                <RouterLink class="edit-link" :to="`/sections/${item.section.id}/edit`">
                  Edit this section
                </RouterLink>
              </header>

              <div v-if="item.compare?.blocks.length" class="block-list">
                <section
                  v-for="block in item.compare.blocks"
                  :key="`${item.section.id}-${block.anchor.block_key}-${block.block_index}`"
                  class="block-card"
                  :class="{ changed: changedVariants(block).length > 0 }"
                >
                  <p class="main-block-text">
                    <template
                      v-for="(segment, segmentIndex) in mainSegments(item, block)"
                      :key="`${block.anchor.block_key}-${block.block_index}-${segmentIndex}`"
                    >
                      <span v-if="!segment.highlight">{{ segment.text }}</span>
                      <span
                        v-else
                        class="inline-variant-wrap"
                        :class="{
                          open: isVariantPanelOpen(item.section.id, segment.highlight.id),
                          dismissed: isVariantPanelDismissed(item.section.id, segment.highlight.id),
                        }"
                        @mouseleave="clearDismissedVariantPanel(item.section.id, segment.highlight.id)"
                      >
                        <button
                          class="inline-variant-marker"
                          type="button"
                          :aria-expanded="isVariantPanelOpen(item.section.id, segment.highlight.id)"
                          @click="handleVariantMarkerClick(item.section.id, segment.highlight.id, $event)"
                          @keyup.esc="closeVariantPanel"
                        >
                          {{ segment.text }}
                        </button>

                        <aside class="variant-popover">
                          <header class="popover-header">
                            <span class="block-kind">{{ blockLabel(block) }}</span>
                            <strong>
                              {{ segment.highlight.variants.length }} variant{{ segment.highlight.variants.length === 1 ? "" : "s" }}
                            </strong>
                            <button
                              class="popover-close"
                              type="button"
                              aria-label="Close variants"
                              @click="handleCloseVariantPanel"
                            >
                              x
                            </button>
                          </header>

                          <article
                            v-for="entry in segment.highlight.variants"
                            :key="`${segment.highlight.id}-${entry.variant.alternative_submission_id}-${entry.variant.alternative_index}`"
                            class="variant-card"
                          >
                            <header class="variant-header">
                              <strong class="variant-meta">
                                <span class="rank-marker">{{ entry.submission?.rank ?? "?" }}</span>
                                <span>
                                  {{ entry.submission ? submissionLabel(entry.submission) : "Unknown alternative" }}
                                </span>
                              </strong>
                              <span v-if="entry.submission?.support_percent !== null" class="support-pill">
                                {{ supportLabel(entry.submission!.support_percent) }}
                              </span>
                            </header>

                            <p class="variant-replacement">
                              <template v-if="variantExcerpt(block, entry.variant).leadingEllipsis">...</template>
                              <template
                                v-for="(excerptSegment, excerptIndex) in variantExcerpt(block, entry.variant).segments"
                                :key="`${segment.highlight.id}-${entry.variant.alternative_submission_id}-${excerptIndex}`"
                              >
                                <mark v-if="excerptSegment.changed" class="variant-delta">{{ excerptSegment.text }}</mark>
                                <template v-else>{{ excerptSegment.text }}</template>
                              </template>
                              <template v-if="variantExcerpt(block, entry.variant).trailingEllipsis">...</template>
                            </p>
                          </article>
                        </aside>
                      </span>
                    </template>
                  </p>
                </section>
              </div>
              <p v-else-if="item.section.has_own_text" class="empty-note">No published version yet.</p>
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
.variant-card {
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

.compare-section-header,
.variant-header {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: flex-start;
}

.compare-section-header h3,
.empty-note {
  margin: 0;
}

.compare-sections,
.block-list {
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
  padding-top: 2rem;
  border-top: 1px solid rgba(35, 24, 15, 0.1);
}

.compare-section:first-child {
  padding-top: 0;
  border-top: 0;
}

.compare-section.active {
  scroll-margin-top: 5rem;
}

.section-heading {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.section-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 0.65rem;
  min-height: 1.8rem;
  color: #6f5947;
  font-size: 0.88rem;
}

.reader-badge {
  padding: 0.35rem 0.6rem;
  border-radius: 999px;
  background: #f1dcc4;
  color: #6f5947;
  font-size: 0.88rem;
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

.support-pill {
  display: inline-flex;
  align-items: center;
  gap: 0.45rem;
  border-radius: 999px;
  font-size: 0.78rem;
  white-space: nowrap;
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
  gap: 0.45rem;
}

.block-card {
  position: relative;
  padding: 0.2rem 0;
  border-radius: 0.4rem;
}

.block-card.changed {
  background: linear-gradient(90deg, rgba(142, 75, 22, 0.08), rgba(142, 75, 22, 0));
}

.main-block-text {
  margin: 0;
  white-space: pre-wrap;
  font-family: inherit;
  line-height: 1.65;
  color: #22150d;
}

.inline-variant-wrap {
  position: relative;
  display: inline-block;
  z-index: 1;
}

.inline-variant-wrap.open,
.inline-variant-wrap:not(.dismissed):hover,
.inline-variant-wrap:focus-within {
  z-index: 1000;
}

.inline-variant-marker {
  display: inline;
  border: 0;
  border-radius: 0.25rem;
  padding: 0.04rem 0.16rem;
  background: rgba(142, 75, 22, 0.16);
  color: #8e4b16;
  font: inherit;
  text-decoration: underline;
  text-decoration-thickness: 0.12em;
  text-underline-offset: 0.16em;
  text-decoration-color: rgba(142, 75, 22, 0.42);
  cursor: pointer;
}

.inline-variant-marker:hover,
.inline-variant-wrap.open .inline-variant-marker {
  background: rgba(142, 75, 22, 0.24);
}

.inline-variant-marker:focus-visible {
  outline: 2px solid rgba(142, 75, 22, 0.5);
  outline-offset: 3px;
}

.variant-popover {
  position: absolute;
  top: 2.45rem;
  left: 0;
  display: none;
  width: min(44rem, calc(100vw - 4rem));
  max-height: min(70vh, 44rem);
  overflow: auto;
  padding: 0.9rem;
  border: 1px solid rgba(35, 24, 15, 0.12);
  border-radius: 0.8rem;
  background-color: #fffdf9;
  box-shadow: 0 1.5rem 3rem rgba(35, 24, 15, 0.18);
  color: #22150d;
  opacity: 1;
  z-index: 1001;
}

.inline-variant-wrap.open .variant-popover,
.inline-variant-wrap:not(.open):not(.dismissed):hover .variant-popover,
.inline-variant-wrap:not(.open):not(.dismissed) .inline-variant-marker:focus + .variant-popover,
.inline-variant-wrap:not(.open):not(.dismissed) .variant-popover:hover,
.inline-variant-wrap:not(.open):not(.dismissed) .variant-popover:focus-within {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.popover-header {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: baseline;
  padding-bottom: 0.4rem;
  border-bottom: 1px solid rgba(35, 24, 15, 0.08);
}

.popover-close {
  display: inline-grid;
  width: 1.8rem;
  height: 1.8rem;
  place-items: center;
  border: 1px solid rgba(35, 24, 15, 0.12);
  border-radius: 999px;
  background: white;
  color: #6f5947;
  cursor: pointer;
  font: inherit;
  line-height: 1;
}

.popover-close:hover,
.popover-close:focus-visible {
  color: #8e2f23;
  border-color: rgba(142, 47, 35, 0.24);
}

.block-kind {
  text-transform: capitalize;
  color: #8e4b16;
  font-size: 0.8rem;
}

.variant-card {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  padding: 0.8rem;
  border-radius: 0.55rem;
  background: #fff8ef;
}

.variant-meta {
  display: flex;
  align-items: baseline;
  gap: 0.55rem;
  font-weight: 600;
}

.variant-replacement {
  margin: 0;
  white-space: pre-wrap;
  color: #22150d;
  line-height: 1.5;
}

.variant-replacement::before {
  content: "-> ";
  color: #335f28;
  font-weight: 700;
}

.variant-delta {
  padding: 0 0.14rem;
  border-radius: 0.22rem;
  background: rgba(142, 75, 22, 0.16);
  color: inherit;
}

@media (max-width: 960px) {
  .compare-panel-header,
  .compare-section-header,
  .variant-header {
    flex-direction: column;
  }

  .variant-popover {
    position: fixed;
    inset: auto 1rem 1rem 1rem;
    width: auto;
    max-height: 72vh;
  }
}
</style>
