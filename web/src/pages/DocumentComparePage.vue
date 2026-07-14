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

type ReaderPage = {
  id: string;
  title: string;
  sectionIds: string[];
  sections: SectionCompareItem[];
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

type MainSegment = {
  text: string;
  highlight: VariantHighlight | null;
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
const currentPageIndex = ref(0);
const openVariantKey = ref<string | null>(null);
const dismissedVariantKey = ref<string | null>(null);
const isLoadingDocument = ref(true);
const isLoadingCompares = ref(false);
const error = ref<string | null>(null);

const canManageOutline = computed(() =>
  auth.user ? auth.user.role === "admin" : false,
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

const paginationLevel = computed(() => {
  const rawLevel = documentData.value?.document.markdown_policy?.pagination_level;
  return typeof rawLevel === "number" && rawLevel > 0 ? rawLevel : null;
});

function sectionLevel(section: SectionRecord) {
  return section.path.split("/").length;
}

const readerPages = computed<ReaderPage[]>(() => {
  const items = compareSections.value;
  const level = paginationLevel.value;
  if (!items.length) {
    return [];
  }

  if (!level) {
    return [
      {
        id: "all",
        title: documentData.value?.document.title ?? "Document",
        sectionIds: items.map((item) => item.section.id),
        sections: items,
      },
    ];
  }

  const pages: ReaderPage[] = [];
  let pendingPrefix: SectionCompareItem[] = [];
  let currentPage: ReaderPage | null = null;

  for (const item of items) {
    const currentLevel = sectionLevel(item.section);
    if (currentLevel === level) {
      if (currentPage) {
        pages.push(currentPage);
      }
      currentPage = {
        id: item.section.id,
        title: item.section.has_heading
          ? `${item.number} ${item.section.title}`.trim()
          : item.number || "Untitled page",
        sectionIds: [...pendingPrefix.map((entry) => entry.section.id), item.section.id],
        sections: [...pendingPrefix, item],
      };
      pendingPrefix = [];
      continue;
    }

    if (currentPage && currentLevel > level) {
      currentPage.sections.push(item);
      currentPage.sectionIds.push(item.section.id);
      continue;
    }

    pendingPrefix.push(item);
  }

  if (currentPage) {
    pages.push(currentPage);
  }

  if (!pages.length) {
    return [
      {
        id: "all",
        title: documentData.value?.document.title ?? "Document",
        sectionIds: items.map((item) => item.section.id),
        sections: items,
      },
    ];
  }

  return pages;
});

const currentPage = computed(() => readerPages.value[currentPageIndex.value] ?? null);
const visibleCompareSections = computed(() => currentPage.value?.sections ?? compareSections.value);

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

function sectionDisplayTitle(section: SectionRecord) {
  return section.has_heading ? section.title : "";
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
      || "";
    const alternativeText =
      variant.text.trim()
      || (fallbackDiff ? changedText(fallbackDiff.alternative, "added") : "");
    const start = variant.reference_start ?? 0;
    const end = variant.reference_end ?? start + sourceText.length;
    if (!alternativeText || end < start) {
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

function insertionHighlightsAt(highlights: VariantHighlight[], position: number) {
  return highlights.filter((highlight) => highlight.start === position && highlight.end === position);
}

function makeInsertionSegment(
  block: CompareBlockDto,
  start: number,
  end: number,
  highlights: VariantHighlight[],
): MainSegment {
  return {
    text: "",
    highlight: {
      id: `${block.anchor.block_key}-${block.block_index}-${start}-${end}-insert`,
      sourceText: "",
      start,
      end,
      variants: dedupeHighlightVariants(highlights.flatMap((highlight) => highlight.variants)),
    },
  };
}

function mainSegments(item: SectionCompareItem, block: CompareBlockDto): MainSegment[] {
  const highlights = blockHighlights(item, block);
  if (!highlights.length) {
    return [{ text: block.main_text, highlight: null as VariantHighlight | null }];
  }

  const rangeHighlights = highlights.filter((highlight) => highlight.end > highlight.start);
  const insertionHighlights = highlights.filter((highlight) => highlight.end === highlight.start);
  const segments: MainSegment[] = [];
  const boundaries = new Set<number>([0, block.main_text.length]);
  for (const highlight of rangeHighlights) {
    boundaries.add(highlight.start);
    boundaries.add(highlight.end);
  }
  const orderedBoundaries = [...boundaries].sort((left, right) => left - right);

  for (const insertion of insertionHighlightsAt(insertionHighlights, 0)) {
    segments.push(makeInsertionSegment(block, insertion.start, insertion.end, [insertion]));
  }

  for (let index = 0; index < orderedBoundaries.length - 1; index += 1) {
    const start = orderedBoundaries[index];
    const end = orderedBoundaries[index + 1];
    if (end <= start) {
      continue;
    }

    const text = block.main_text.slice(start, end);
    const overlapping = rangeHighlights.filter((highlight) => highlight.start < end && highlight.end > start);
    if (!overlapping.length) {
      segments.push({ text, highlight: null });
    } else {
      const combinedHighlight: VariantHighlight = {
        id: `${block.anchor.block_key}-${block.block_index}-${start}-${end}`,
        sourceText: text,
        start,
        end,
        variants: dedupeHighlightVariants(overlapping.flatMap((highlight) => highlight.variants)),
      };
      segments.push({ text, highlight: combinedHighlight });
    }

    const insertionAtEnd = insertionHighlightsAt(insertionHighlights, end);
    if (insertionAtEnd.length) {
      segments.push(makeInsertionSegment(block, end, end, insertionAtEnd));
    }
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
      await router.replace(`/documents/${data.document.slug}`);
      return;
    }
    selectedSectionId.value =
      selectedSectionId.value && data.sections.some((section) => section.id === selectedSectionId.value)
        ? selectedSectionId.value
        : data.sections[0]?.id ?? null;
    await loadSectionCompares(data.sections.map((section) => section.id));
    currentPageIndex.value = 0;
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

        try {
          return [sectionId, await getSectionCompare(sectionId)] as const;
        } catch (compareError) {
          if (
            compareError instanceof Error
            && compareError.message.toLowerCase().includes("no published main submission")
          ) {
            return [sectionId, null] as const;
          }
          throw compareError;
        }
      }),
    );
    sectionCompares.value = Object.fromEntries(entries);
  } finally {
    isLoadingCompares.value = false;
  }
}

async function handleSelectSection(sectionId: string) {
  selectedSectionId.value = sectionId;
  const pageIndex = readerPages.value.findIndex((page) => page.sectionIds.includes(sectionId));
  if (pageIndex >= 0) {
    currentPageIndex.value = pageIndex;
  }
  await nextTick();
  document.getElementById(`compare-section-${sectionId}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function setCurrentPage(index: number) {
  if (index < 0 || index >= readerPages.value.length) {
    return;
  }

  currentPageIndex.value = index;
  const firstSectionId = readerPages.value[index]?.sectionIds[0] ?? null;
  if (firstSectionId) {
    selectedSectionId.value = firstSectionId;
  }
}

watch(
  () => route.params.documentRef,
  () => {
    selectedSectionId.value = null;
    void loadDocument();
  },
);

onMounted(async () => {
  await auth.restoreSession();
  await loadDocument();
});

watch(readerPages, (pages) => {
  if (!pages.length) {
    currentPageIndex.value = 0;
    return;
  }

  if (currentPageIndex.value >= pages.length) {
    currentPageIndex.value = pages.length - 1;
  }

  if (selectedSectionId.value && pages[currentPageIndex.value]?.sectionIds.includes(selectedSectionId.value)) {
    return;
  }

  const pageIndex = selectedSectionId.value
    ? pages.findIndex((page) => page.sectionIds.includes(selectedSectionId.value as string))
    : -1;

  if (pageIndex >= 0) {
    currentPageIndex.value = pageIndex;
    return;
  }

  selectedSectionId.value = pages[currentPageIndex.value]?.sectionIds[0] ?? null;
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
            :document-ref="documentData.document.slug"
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
            <nav v-if="readerPages.length > 1" class="page-nav">
              <button type="button" :disabled="currentPageIndex === 0" @click="setCurrentPage(currentPageIndex - 1)">
                Previous page
              </button>
              <div class="page-pills">
                <button
                  v-for="(page, pageIndex) in readerPages"
                  :key="page.id"
                  type="button"
                  class="page-pill"
                  :class="{ active: pageIndex === currentPageIndex }"
                  @click="setCurrentPage(pageIndex)"
                >
                  {{ page.title }}
                </button>
              </div>
              <button
                type="button"
                :disabled="currentPageIndex >= readerPages.length - 1"
                @click="setCurrentPage(currentPageIndex + 1)"
              >
                Next page
              </button>
            </nav>
            <article
              v-for="item in visibleCompareSections"
              :id="`compare-section-${item.section.id}`"
              :key="item.section.id"
              class="compare-section"
              :class="{ active: item.section.id === selectedSectionId }"
            >
              <header class="compare-section-header">
                <div class="section-heading">
                  <h3 v-if="item.section.has_heading">
                    <span class="section-number">{{ item.number }}</span>
                    {{ sectionDisplayTitle(item.section) }}
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
                          :class="{ insertion: segment.highlight.start === segment.highlight.end }"
                          type="button"
                          :aria-expanded="isVariantPanelOpen(item.section.id, segment.highlight.id)"
                          @click="handleVariantMarkerClick(item.section.id, segment.highlight.id, $event)"
                          @keyup.esc="closeVariantPanel"
                        >
                          {{ segment.text || "+" }}
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
  border: 1px solid var(--border-soft);
}

.compare-panel {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.25rem;
  border-radius: 1.25rem;
  background: var(--surface-panel);
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

.page-nav {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 0.85rem;
  align-items: center;
  padding-bottom: 0.25rem;
  border-bottom: 1px solid var(--border-soft);
}

.page-nav > button,
.page-pill {
  border: 0;
  border-radius: 999px;
  padding: 0.7rem 0.95rem;
  cursor: pointer;
  font: inherit;
}

.page-nav > button {
  background: var(--accent-hover);
  color: var(--accent-contrast);
}

.page-nav > button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.page-pills {
  display: flex;
  gap: 0.6rem;
  overflow-x: auto;
  padding-bottom: 0.2rem;
}

.page-pill {
  background: color-mix(in srgb, var(--surface-input) 72%, transparent);
  color: var(--accent-contrast);
  white-space: nowrap;
}

.page-pill.active {
  background: var(--accent);
  color: var(--text-on-accent);
}

.compare-section {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding-top: 2rem;
  border-top: 1px solid color-mix(in srgb, var(--border-medium) 84%, transparent);
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
  color: var(--text-secondary);
  font-size: 0.88rem;
}

.reader-badge {
  padding: 0.35rem 0.6rem;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--text-secondary);
  font-size: 0.88rem;
}

.edit-link {
  color: var(--accent);
  text-decoration: none;
}

.section-number {
  margin-right: 0.55rem;
  color: var(--accent);
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
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  color: var(--accent);
}

.rank-marker {
  min-width: 1.35rem;
  color: var(--accent);
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
  background: linear-gradient(90deg, color-mix(in srgb, var(--accent) 8%, transparent), transparent);
}

.main-block-text {
  margin: 0;
  white-space: pre-wrap;
  font-family: inherit;
  line-height: 1.65;
  color: var(--text-strong);
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
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  color: var(--accent);
  font: inherit;
  text-decoration: underline;
  text-decoration-thickness: 0.12em;
  text-underline-offset: 0.16em;
  text-decoration-color: color-mix(in srgb, var(--accent) 42%, transparent);
  cursor: pointer;
}

.inline-variant-marker:hover,
.inline-variant-wrap.open .inline-variant-marker {
  background: color-mix(in srgb, var(--accent) 24%, transparent);
}

.inline-variant-marker.insertion {
  padding: 0.04rem 0.34rem;
  border-radius: 999px;
  text-decoration: none;
  font-weight: 700;
  line-height: 1;
}

.inline-variant-marker:focus-visible {
  outline: 2px solid color-mix(in srgb, var(--accent) 50%, transparent);
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
  border: 1px solid var(--border-medium);
  border-radius: 0.8rem;
  background-color: var(--surface-raised);
  box-shadow: 0 1.5rem 3rem rgba(35, 24, 15, 0.18);
  color: var(--text-strong);
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
  border-bottom: 1px solid var(--border-soft);
}

.popover-close {
  display: inline-grid;
  width: 1.8rem;
  height: 1.8rem;
  place-items: center;
  border: 1px solid var(--border-medium);
  border-radius: 999px;
  background: var(--surface-input);
  color: var(--text-secondary);
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
  color: var(--accent);
  font-size: 0.8rem;
}

.variant-card {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  padding: 0.8rem;
  border-radius: 0.55rem;
  background: var(--surface-raised);
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
  color: var(--text-strong);
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
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  color: inherit;
}

@media (max-width: 960px) {
  .page-nav {
    grid-template-columns: 1fr;
  }

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
