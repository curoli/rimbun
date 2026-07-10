<script setup lang="ts">
import { computed, reactive } from "vue";

import type { CommentRecord, ProjectionItemRecord, SubmissionRecord } from "../api/types";

type CommentTreeNode = CommentRecord & {
  replies: CommentTreeNode[];
};

const props = defineProps<{
  submissions: SubmissionRecord[];
  comments: CommentRecord[];
  projection: ProjectionItemRecord[];
  preferredBaseSubmissionId: string | null;
  currentUserId: string | null;
  canComment: boolean;
}>();

const emit = defineEmits<{
  setBase: [submissionId: string];
  createComment: [payload: {
    submissionId: string;
    parentCommentId: string | null;
    markdownContent: string;
    isPrimary?: boolean;
  }];
}>();

const rootDrafts = reactive<Record<string, string>>({});
const replyDrafts = reactive<Record<string, string>>({});
const replyTargets = reactive<Record<string, boolean>>({});

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

function commentAuthorLabel(comment: CommentRecord) {
  return `${comment.display_name} @${comment.username}`;
}

function sortComments(left: CommentRecord, right: CommentRecord) {
  if (left.is_primary !== right.is_primary) {
    return left.is_primary ? -1 : 1;
  }
  return new Date(left.created_at).getTime() - new Date(right.created_at).getTime();
}

function commentsForSubmission(submissionId: string): CommentTreeNode[] {
  const relevant = props.comments
    .filter((comment) => comment.submission_id === submissionId)
    .sort(sortComments);
  const byId = new Map<string, CommentTreeNode>();
  const roots: CommentTreeNode[] = [];

  for (const comment of relevant) {
    byId.set(comment.id, { ...comment, replies: [] });
  }
  for (const comment of relevant) {
    const node = byId.get(comment.id);
    if (!node) {
      continue;
    }
    if (comment.parent_comment_id) {
      const parent = byId.get(comment.parent_comment_id);
      if (parent) {
        parent.replies.push(node);
        continue;
      }
    }
    roots.push(node);
  }

  const sortTree = (nodes: CommentTreeNode[]) => {
    nodes.sort(sortComments);
    for (const node of nodes) {
      sortTree(node.replies);
    }
  };
  sortTree(roots);
  return roots;
}

function hasPrimaryComment(submissionId: string) {
  return props.comments.some(
    (comment) => comment.submission_id === submissionId && comment.is_primary && !comment.parent_comment_id,
  );
}

function canAddPrimaryComment(submission: SubmissionRecord) {
  return props.currentUserId === submission.user_id && !hasPrimaryComment(submission.id);
}

function submitRootComment(submissionId: string, isPrimary = false) {
  const markdownContent = (rootDrafts[`${submissionId}:${isPrimary ? "primary" : "root"}`] ?? "").trim();
  if (!markdownContent) {
    return;
  }
  emit("createComment", {
    submissionId,
    parentCommentId: null,
    markdownContent,
    isPrimary,
  });
  rootDrafts[`${submissionId}:${isPrimary ? "primary" : "root"}`] = "";
}

function submitReply(submissionId: string, parentCommentId: string) {
  const markdownContent = (replyDrafts[parentCommentId] ?? "").trim();
  if (!markdownContent) {
    return;
  }
  emit("createComment", {
    submissionId,
    parentCommentId,
    markdownContent,
  });
  replyDrafts[parentCommentId] = "";
  replyTargets[parentCommentId] = false;
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
        <div class="comment-thread">
          <template v-for="comment in commentsForSubmission(mainSubmission.id)" :key="comment.id">
            <div class="comment-card" :class="{ primary: comment.is_primary }">
              <div class="comment-meta">
                <strong>{{ commentAuthorLabel(comment) }}</strong>
                <div class="comment-meta-right">
                  <span v-if="comment.is_primary" class="badge primary-badge">primary comment</span>
                  <time>{{ new Date(comment.created_at).toLocaleString() }}</time>
                </div>
              </div>
              <pre>{{ comment.markdown_content }}</pre>
              <button v-if="canComment" class="reply-toggle" @click="replyTargets[comment.id] = !replyTargets[comment.id]">
                {{ replyTargets[comment.id] ? "Cancel reply" : "Reply" }}
              </button>
              <div v-if="replyTargets[comment.id] && canComment" class="comment-form nested">
                <textarea v-model="replyDrafts[comment.id]" placeholder="Write a reply." />
                <button class="submission-action" @click="submitReply(mainSubmission.id, comment.id)">Post reply</button>
              </div>
              <div v-if="comment.replies.length" class="reply-list">
                <div v-for="reply in comment.replies" :key="reply.id" class="comment-card reply-card">
                  <div class="comment-meta">
                    <strong>{{ commentAuthorLabel(reply) }}</strong>
                    <time>{{ new Date(reply.created_at).toLocaleString() }}</time>
                  </div>
                  <pre>{{ reply.markdown_content }}</pre>
                </div>
              </div>
            </div>
          </template>
        </div>
        <div v-if="canAddPrimaryComment(mainSubmission)" class="comment-form">
          <h5>Add primary comment</h5>
          <textarea v-model="rootDrafts[`${mainSubmission.id}:primary`]" placeholder="Optional author note for this version." />
          <button class="submission-action" @click="submitRootComment(mainSubmission.id, true)">Post primary comment</button>
        </div>
        <div v-if="canComment" class="comment-form">
          <h5>Add comment</h5>
          <textarea v-model="rootDrafts[`${mainSubmission.id}:root`]" placeholder="Write a comment on this contribution." />
          <button class="submission-action" @click="submitRootComment(mainSubmission.id)">Post comment</button>
        </div>
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
        <div class="comment-thread">
          <template v-for="comment in commentsForSubmission(submission.id)" :key="comment.id">
            <div class="comment-card" :class="{ primary: comment.is_primary }">
              <div class="comment-meta">
                <strong>{{ commentAuthorLabel(comment) }}</strong>
                <div class="comment-meta-right">
                  <span v-if="comment.is_primary" class="badge primary-badge">primary comment</span>
                  <time>{{ new Date(comment.created_at).toLocaleString() }}</time>
                </div>
              </div>
              <pre>{{ comment.markdown_content }}</pre>
              <button v-if="canComment" class="reply-toggle" @click="replyTargets[comment.id] = !replyTargets[comment.id]">
                {{ replyTargets[comment.id] ? "Cancel reply" : "Reply" }}
              </button>
              <div v-if="replyTargets[comment.id] && canComment" class="comment-form nested">
                <textarea v-model="replyDrafts[comment.id]" placeholder="Write a reply." />
                <button class="submission-action" @click="submitReply(submission.id, comment.id)">Post reply</button>
              </div>
              <div v-if="comment.replies.length" class="reply-list">
                <div v-for="reply in comment.replies" :key="reply.id" class="comment-card reply-card">
                  <div class="comment-meta">
                    <strong>{{ commentAuthorLabel(reply) }}</strong>
                    <time>{{ new Date(reply.created_at).toLocaleString() }}</time>
                  </div>
                  <pre>{{ reply.markdown_content }}</pre>
                </div>
              </div>
            </div>
          </template>
        </div>
        <div v-if="canAddPrimaryComment(submission)" class="comment-form">
          <h5>Add primary comment</h5>
          <textarea v-model="rootDrafts[`${submission.id}:primary`]" placeholder="Optional author note for this version." />
          <button class="submission-action" @click="submitRootComment(submission.id, true)">Post primary comment</button>
        </div>
        <div v-if="canComment" class="comment-form">
          <h5>Add comment</h5>
          <textarea v-model="rootDrafts[`${submission.id}:root`]" placeholder="Write a comment on this contribution." />
          <button class="submission-action" @click="submitRootComment(submission.id)">Post comment</button>
        </div>
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
        <div class="comment-thread">
          <template v-for="comment in commentsForSubmission(submission.id)" :key="comment.id">
            <div class="comment-card" :class="{ primary: comment.is_primary }">
              <div class="comment-meta">
                <strong>{{ commentAuthorLabel(comment) }}</strong>
                <div class="comment-meta-right">
                  <span v-if="comment.is_primary" class="badge primary-badge">primary comment</span>
                  <time>{{ new Date(comment.created_at).toLocaleString() }}</time>
                </div>
              </div>
              <pre>{{ comment.markdown_content }}</pre>
              <button v-if="canComment" class="reply-toggle" @click="replyTargets[comment.id] = !replyTargets[comment.id]">
                {{ replyTargets[comment.id] ? "Cancel reply" : "Reply" }}
              </button>
              <div v-if="replyTargets[comment.id] && canComment" class="comment-form nested">
                <textarea v-model="replyDrafts[comment.id]" placeholder="Write a reply." />
                <button class="submission-action" @click="submitReply(submission.id, comment.id)">Post reply</button>
              </div>
              <div v-if="comment.replies.length" class="reply-list">
                <div v-for="reply in comment.replies" :key="reply.id" class="comment-card reply-card">
                  <div class="comment-meta">
                    <strong>{{ commentAuthorLabel(reply) }}</strong>
                    <time>{{ new Date(reply.created_at).toLocaleString() }}</time>
                  </div>
                  <pre>{{ reply.markdown_content }}</pre>
                </div>
              </div>
            </div>
          </template>
        </div>
        <div v-if="canAddPrimaryComment(submission)" class="comment-form">
          <h5>Add primary comment</h5>
          <textarea v-model="rootDrafts[`${submission.id}:primary`]" placeholder="Optional author note for this version." />
          <button class="submission-action" @click="submitRootComment(submission.id, true)">Post primary comment</button>
        </div>
        <div v-if="canComment" class="comment-form">
          <h5>Add comment</h5>
          <textarea v-model="rootDrafts[`${submission.id}:root`]" placeholder="Write a comment on this contribution." />
          <button class="submission-action" @click="submitRootComment(submission.id)">Post comment</button>
        </div>
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
  color: var(--text-muted);
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
  color: var(--text-muted);
  margin-top: 0.3rem;
}

.submission-card {
  padding: 1rem;
  border-radius: 1.15rem;
  background: color-mix(in srgb, var(--surface-panel) 96%, transparent);
  border: 1px solid var(--border-soft);
}

.submission-card.selected {
  border-color: #c26b21;
  box-shadow: 0 0 0 1px rgba(194, 107, 33, 0.2);
}

.main-card {
  background: linear-gradient(180deg, color-mix(in srgb, var(--surface-input) 92%, var(--surface-panel)), color-mix(in srgb, var(--accent-soft) 34%, var(--surface-panel)));
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
  color: var(--text-strong);
}

.badge {
  display: inline-flex;
  align-items: center;
  padding: 0.2rem 0.55rem;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--accent);
  font-size: 0.78rem;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

.main-badge {
  background: #d36b19;
  color: var(--text-on-accent);
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
  color: var(--text-strong);
  cursor: pointer;
}

.comment-thread,
.reply-list {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  margin: 0.85rem 0;
}

.comment-card {
  padding: 0.8rem 0.9rem;
  border-radius: 0.95rem;
  background: color-mix(in srgb, var(--surface-input) 78%, transparent);
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

.comment-card.primary {
  background: color-mix(in srgb, var(--accent-soft) 50%, var(--surface-input));
}

.reply-card {
  margin-left: 1rem;
}

.comment-meta {
  display: flex;
  justify-content: space-between;
  gap: 0.75rem;
  margin-bottom: 0.55rem;
  align-items: flex-start;
}

.comment-meta-right {
  display: flex;
  gap: 0.6rem;
  align-items: center;
  flex-wrap: wrap;
}

.primary-badge {
  background: color-mix(in srgb, var(--accent) 18%, var(--surface-input));
}

.comment-form {
  display: flex;
  flex-direction: column;
  gap: 0.65rem;
  margin: 0.8rem 0;
}

.comment-form h5 {
  margin: 0;
}

.comment-form textarea {
  min-height: 6rem;
  border: 0;
  outline: none;
  border-radius: 0.9rem;
  padding: 0.85rem 0.95rem;
  resize: vertical;
  background: color-mix(in srgb, var(--surface-input) 82%, transparent);
  box-shadow: inset 0 0 0 1px var(--border-soft);
  color: var(--text-strong);
  font: inherit;
}

.comment-form.nested {
  margin-left: 1rem;
}

.reply-toggle {
  border: 0;
  background: transparent;
  color: var(--accent);
  padding: 0;
  cursor: pointer;
  font: inherit;
}
</style>
