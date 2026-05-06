<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";

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
    const main =
      view?.active_submissions.find(
        (submission) => view.projection.find((item) => item.submission_id === submission.id)?.role === "main",
      ) ?? view?.active_submissions[0] ?? null;
    const alternatives =
      view?.active_submissions.filter(
        (submission) =>
          view.projection.find((item) => item.submission_id === submission.id)?.role === "principal_alternative",
      ) ?? [];

    return { section, main, alternatives };
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
              <p class="eyebrow">Compare View</p>
              <h2>Main version and principal alternatives</h2>
            </div>
            <p class="compare-copy">
              This first compare slice is section-based: it shows the global main version next to the principal alternatives.
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
                <div>
                  <span class="section-kicker">Section</span>
                  <h3>{{ item.section.title }}</h3>
                </div>
                <RouterLink class="edit-link" :to="`/sections/${item.section.id}/edit`">
                  Edit this section
                </RouterLink>
              </header>

              <div class="compare-grid">
                <section class="compare-card main">
                  <p class="card-label">Global Main Version</p>
                  <strong v-if="item.main">{{ submissionLabel(item.main) }}</strong>
                  <p v-else class="empty-note">No published version yet.</p>
                  <pre v-if="item.main">{{ item.main.markdown_content }}</pre>
                </section>

                <section class="compare-card">
                  <p class="card-label">Principal Alternatives</p>
                  <div v-if="item.alternatives.length" class="alternatives">
                    <article v-for="alternative in item.alternatives" :key="alternative.id" class="alternative-card">
                      <strong>{{ submissionLabel(alternative) }}</strong>
                      <pre>{{ alternative.markdown_content }}</pre>
                    </article>
                  </div>
                  <p v-else class="empty-note">No principal alternatives yet.</p>
                </section>
              </div>
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
.compare-card,
.alternative-card {
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
.compare-section-header {
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
  max-width: 38ch;
  color: #6f5947;
}

.compare-sections {
  display: flex;
  flex-direction: column;
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

.section-kicker,
.card-label {
  color: #8e4b16;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  font-size: 0.76rem;
}

.edit-link {
  color: #8e4b16;
  text-decoration: none;
}

.compare-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1rem;
}

.compare-card,
.alternative-card {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  padding: 0.95rem;
  border-radius: 1rem;
  background: white;
}

.compare-card.main {
  background: #fff6eb;
}

.alternatives {
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
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
  .compare-grid {
    display: flex;
    flex-direction: column;
  }
}
</style>
