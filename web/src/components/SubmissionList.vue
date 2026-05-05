<script setup lang="ts">
import { computed } from "vue";

import type { ProjectionItemRecord, SubmissionRecord } from "../api/types";

const props = defineProps<{
  submissions: SubmissionRecord[];
  projection: ProjectionItemRecord[];
  preferredBaseSubmissionId: string | null;
}>();

const emit = defineEmits<{
  setBase: [submissionId: string];
}>();

const projectionRoles = computed(() => {
  const map = new Map<string, ProjectionItemRecord>();
  for (const item of props.projection) {
    map.set(item.submission_id, item);
  }
  return map;
});

const mainSubmission = computed(() =>
  props.submissions.find((submission) => projectionRoles.value.get(submission.id)?.role === "main") ?? null,
);

const principalAlternatives = computed(() =>
  props.submissions.filter(
    (submission) => projectionRoles.value.get(submission.id)?.role === "principal_alternative",
  ),
);

const otherSubmissions = computed(() =>
  props.submissions.filter((submission) => {
    const role = projectionRoles.value.get(submission.id)?.role;
    return role !== "main" && role !== "principal_alternative";
  }),
);

function roleLabel(submissionId: string) {
  const item = projectionRoles.value.get(submissionId);
  if (!item) {
    return "published";
  }
  if (item.role === "main") {
    return "main";
  }
  if (item.role === "principal_alternative") {
    return "alternative";
  }
  return "other";
}

function isPersonalBase(submissionId: string) {
  return submissionId === props.preferredBaseSubmissionId;
}

function authorLabel(submission: SubmissionRecord) {
  return `${submission.display_name} @${submission.username}`;
}
</script>

<template>
  <section class="submissions-panel">
    <div class="submissions-header">
      <h3>Published Versions</h3>
      <p>The system decides the global main version. You can override only your own reading base.</p>
    </div>

    <div v-if="mainSubmission" class="submission-group">
      <div class="group-heading">
        <h4>Main Version</h4>
        <p>System-selected default reading version.</p>
      </div>
      <article
        class="submission-card main-card"
        :class="{ selected: isPersonalBase(mainSubmission.id) }"
      >
        <div class="submission-meta">
          <div class="meta-stack">
            <span class="badge main-badge">{{ roleLabel(mainSubmission.id) }}</span>
            <strong>{{ authorLabel(mainSubmission) }}</strong>
          </div>
          <time>{{ new Date(mainSubmission.published_at).toLocaleString() }}</time>
        </div>
        <pre>{{ mainSubmission.markdown_content }}</pre>
        <button class="submission-action" @click="emit('setBase', mainSubmission.id)">
          {{ isPersonalBase(mainSubmission.id) ? "Using as personal base" : "Use as personal base" }}
        </button>
      </article>
    </div>

    <div v-if="principalAlternatives.length" class="submission-group">
      <div class="group-heading">
        <h4>Principal Alternatives</h4>
        <p>Competing versions that are closest to being shown alongside the main version.</p>
      </div>
      <article
        v-for="submission in principalAlternatives"
        :key="submission.id"
        class="submission-card"
        :class="{ selected: isPersonalBase(submission.id) }"
      >
        <div class="submission-meta">
          <div class="meta-stack">
            <span class="badge">{{ roleLabel(submission.id) }}</span>
            <strong>{{ authorLabel(submission) }}</strong>
          </div>
          <time>{{ new Date(submission.published_at).toLocaleString() }}</time>
        </div>
        <pre>{{ submission.markdown_content }}</pre>
        <button class="submission-action" @click="emit('setBase', submission.id)">
          {{ isPersonalBase(submission.id) ? "Using as personal base" : "Use as personal base" }}
        </button>
      </article>
    </div>

    <div v-if="otherSubmissions.length" class="submission-group">
      <div class="group-heading">
        <h4>Other Visible Versions</h4>
        <p>Published alternatives that are currently outside the principal set.</p>
      </div>
      <article
        v-for="submission in otherSubmissions"
        :key="submission.id"
        class="submission-card muted-card"
        :class="{ selected: isPersonalBase(submission.id) }"
      >
        <div class="submission-meta">
          <div class="meta-stack">
            <span class="badge">{{ roleLabel(submission.id) }}</span>
            <strong>{{ authorLabel(submission) }}</strong>
          </div>
          <time>{{ new Date(submission.published_at).toLocaleString() }}</time>
        </div>
        <pre>{{ submission.markdown_content }}</pre>
        <button class="submission-action" @click="emit('setBase', submission.id)">
          {{ isPersonalBase(submission.id) ? "Using as personal base" : "Use as personal base" }}
        </button>
      </article>
    </div>
  </section>
</template>

<style scoped>
.submissions-panel {
  display: flex;
  flex-direction: column;
  gap: 1.35rem;
}

.submissions-header h3,
.submissions-header p {
  margin: 0;
}

.submissions-header p {
  color: #705948;
  margin-top: 0.35rem;
}

.submission-group {
  display: flex;
  flex-direction: column;
  gap: 0.9rem;
}

.group-heading h4,
.group-heading p {
  margin: 0;
}

.group-heading p {
  color: #705948;
  margin-top: 0.3rem;
}

.submission-card {
  padding: 1rem;
  border-radius: 1.15rem;
  background: rgba(255, 252, 247, 0.9);
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.submission-card.selected {
  border-color: #c26b21;
  box-shadow: 0 0 0 1px rgba(194, 107, 33, 0.2);
}

.main-card {
  background: linear-gradient(180deg, rgba(255, 250, 243, 0.98), rgba(247, 232, 208, 0.98));
}

.muted-card {
  opacity: 0.9;
}

.submission-meta {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 1rem;
  margin-bottom: 0.75rem;
}

.meta-stack {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.meta-stack strong {
  font-size: 0.95rem;
  color: #2d1d12;
}

.badge {
  display: inline-flex;
  align-items: center;
  padding: 0.2rem 0.55rem;
  border-radius: 999px;
  background: #f1dcc4;
  color: #8e4b16;
  font-size: 0.78rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.main-badge {
  background: #d36b19;
  color: white;
}

pre {
  margin: 0;
  white-space: pre-wrap;
  font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
  font-size: 0.92rem;
  line-height: 1.45;
}

.submission-action {
  margin-top: 0.9rem;
  border: 0;
  border-radius: 0.85rem;
  padding: 0.7rem 1rem;
  background: #f4efe8;
  color: #442d1e;
  cursor: pointer;
}
</style>
