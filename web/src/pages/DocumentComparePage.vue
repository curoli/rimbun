<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

import { getDocument, getSectionView } from "../api/documents";
import type {
  DocumentDetailResponse,
  ProjectionItemRecord,
  SectionRecord,
  SectionViewResponse,
  SubmissionRecord,
} from "../api/types";
import DocumentViewNav from "../components/DocumentViewNav.vue";
import SectionTree from "../components/SectionTree.vue";
import { buildSectionNumbers } from "../section-numbering";
import { useAuthStore } from "../stores/auth";

type RankedSubmission = {
  submission: SubmissionRecord;
  projection: ProjectionItemRecord;
};

const route = useRoute();
const router = useRouter();
const auth = useAuthStore();

const documentData = ref<DocumentDetailResponse | null>(null);
const sectionViews = ref<Record<string, SectionViewResponse>>({});
const selectedSectionId = ref<string | null>(null);
const isLoadingDocument = ref(true);
const isLoadingViews = ref(false);
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

const compareSections = computed(() =>
  orderedSections.value.map((section) => {
    const view = sectionViews.value[section.id];
    if (!view) {
      return {
        section,
        number: sectionNumbers.value.get(section.id)?.full ?? "",
        ranked: [] as RankedSubmission[],
      };
    }

    const submissionById = new Map(view.active_submissions.map((submission) => [submission.id, submission]));
    const ranked = view.projection
      .map((projection) => {
        const submission = submissionById.get(projection.submission_id);
        return submission ? { submission, projection } : null;
      })
      .filter((entry): entry is RankedSubmission => entry !== null)
      .sort((left, right) => left.projection.rank - right.projection.rank);

    return {
      section,
      number: sectionNumbers.value.get(section.id)?.full ?? "",
      ranked,
    };
  }),
);

const sectionNumbers = computed(() => buildSectionNumbers(orderedSections.value));

function submissionLabel(submission: SubmissionRecord) {
  return `${submission.display_name} @${submission.username} • ${new Date(submission.published_at).toLocaleString()}`;
}

function supportLabel(score: number | null) {
  return score === null ? "n/a" : `${score.toFixed(0)}%`;
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
  isLoadingViews.value = true;
  try {
    const entries = await Promise.all(
      sectionIds.map(async (sectionId) => [sectionId, await getSectionView(sectionId)] as const),
    );
    sectionViews.value = Object.fromEntries(entries);
  } finally {
    isLoadingViews.value = false;
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
              <h2>Ranked versions</h2>
            </div>
            <p class="compare-copy">
              Versions are ordered by the current `popsam` result. Support is shown from the candidate's best round result.
            </p>
          </div>

          <p v-if="isLoadingViews">Loading comparisons...</p>
          <div v-else class="compare-sections">
            <article
              v-for="item in compareSections"
              :id="`compare-section-${item.section.id}`"
              :key="item.section.id"
              class="compare-section"
              :class="{ active: item.section.id === selectedSectionId }"
            >
              <header class="compare-section-header">
                <h3>
                  <span class="section-number">{{ item.number }}</span>
                  {{ item.section.title }}
                </h3>
                <RouterLink class="edit-link" :to="`/sections/${item.section.id}/edit`">
                  Edit this section
                </RouterLink>
              </header>

              <div v-if="item.ranked.length" class="ranked-list">
                <article
                  v-for="entry in item.ranked"
                  :key="entry.submission.id"
                  class="ranked-card"
                  :class="{ main: entry.projection.role === 'main' }"
                >
                  <div class="card-header">
                    <strong class="variant-meta">
                      <span class="rank-marker">{{ entry.projection.rank + 1 }}</span>
                      <span>{{ submissionLabel(entry.submission) }}</span>
                    </strong>
                    <span class="support-pill">{{ supportLabel(entry.projection.score) }}</span>
                  </div>
                  <pre>{{ entry.submission.markdown_content }}</pre>
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
.ranked-card {
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
.card-header {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: flex-start;
}

.compare-panel-header h2,
.compare-copy,
.compare-section-header h3,
.empty-note {
  margin: 0;
}

.compare-copy {
  max-width: 42ch;
  color: #6f5947;
}

.compare-sections,
.ranked-list {
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

.edit-link {
  color: #8e4b16;
  text-decoration: none;
}

.section-number {
  margin-right: 0.55rem;
  color: #8e4b16;
  font-variant-numeric: tabular-nums;
}

.ranked-list {
  gap: 0.85rem;
}

.ranked-card {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 0.95rem;
  border-radius: 1rem;
  background: white;
}

.ranked-card.main {
  background: #fff6eb;
}

.variant-meta {
  display: flex;
  align-items: baseline;
  gap: 0.55rem;
  font-weight: 600;
}

.rank-marker {
  min-width: 1.5rem;
  color: #8e4b16;
  font-variant-numeric: tabular-nums;
}

.support-pill {
  display: inline-flex;
  align-items: center;
  border-radius: 999px;
  padding: 0.38rem 0.7rem;
  background: rgba(142, 75, 22, 0.1);
  color: #8e4b16;
  font-size: 0.78rem;
  white-space: nowrap;
}

pre {
  margin: 0;
  white-space: pre-wrap;
  font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
  line-height: 1.55;
}

@media (max-width: 960px) {
  .compare-panel-header,
  .compare-section-header,
  .card-header {
    flex-direction: column;
  }
}
</style>
