<script setup lang="ts">
import { computed, reactive } from "vue";

import type { CommentRecord, CommentTreeNode, SectionCompareDto, SubmissionSummaryDto } from "../api/types";
import CommentThreadNode from "./CommentThreadNode.vue";
import MarkdownContent from "./MarkdownContent.vue";

const props = defineProps<{
  compare: SectionCompareDto;
  canComment: boolean;
}>();

const emit = defineEmits<{
  createComment: [payload: {
    submissionId: string;
    parentCommentId: string | null;
    markdownContent: string;
  }];
}>();

const rootDrafts = reactive<Record<string, string>>({});

const submissions = computed(() => [props.compare.main_submission, ...props.compare.alternatives]);

function sortComments(left: CommentRecord, right: CommentRecord) {
  if (left.is_primary !== right.is_primary) {
    return left.is_primary ? -1 : 1;
  }
  return new Date(left.created_at).getTime() - new Date(right.created_at).getTime();
}

function commentsForSubmission(submissionId: string): CommentTreeNode[] {
  const relevant = props.compare.comments
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
    const parent = comment.parent_comment_id ? byId.get(comment.parent_comment_id) : null;
    if (parent) {
      parent.replies.push(node);
    } else {
      roots.push(node);
    }
  }
  return roots;
}

function submissionLabel(submission: SubmissionSummaryDto) {
  return `${submission.display_name} @${submission.username}`;
}

function submitRootComment(submissionId: string) {
  const markdownContent = (rootDrafts[submissionId] ?? "").trim();
  if (!markdownContent) {
    return;
  }
  emit("createComment", { submissionId, parentCommentId: null, markdownContent });
  rootDrafts[submissionId] = "";
}

function submitReply(
  submissionId: string,
  payload: { parentCommentId: string; markdownContent: string },
) {
  emit("createComment", { submissionId, ...payload });
}
</script>

<template>
  <div class="discussion-list">
    <section
      v-for="submission in submissions"
      :key="submission.submission_id"
      class="version-discussion"
    >
      <header class="version-heading">
        <span class="rank">{{ submission.rank }}</span>
        <div>
          <strong>{{ submissionLabel(submission) }}</strong>
          <time>{{ $date(submission.published_at) }}</time>
        </div>
        <span v-if="submission.support_percent !== null" class="support">
          {{ submission.support_percent.toFixed(0) }}%
        </span>
      </header>

      <section class="contribution-text">
        <span>{{ $t("Contribution") }}</span>
        <MarkdownContent class="contribution-markdown" :source="submission.markdown_content" />
      </section>

      <div v-if="commentsForSubmission(submission.submission_id).length" class="comment-list">
        <CommentThreadNode
          v-for="comment in commentsForSubmission(submission.submission_id)"
          :key="comment.id"
          :node="comment"
          :can-comment="canComment"
          @reply="submitReply(submission.submission_id, $event)"
        />
      </div>
      <p v-else class="empty-comments">{{ $t("No comments yet.") }}</p>

      <div v-if="canComment" class="new-comment-form">
        <textarea
          v-model="rootDrafts[submission.submission_id]"
          :placeholder="$t('Write a comment on this version.')"
          rows="4"
        />
        <button
          type="button"
          :disabled="!(rootDrafts[submission.submission_id] ?? '').trim()"
          @click="submitRootComment(submission.submission_id)"
        >
          {{ $t("Post comment") }}
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.discussion-list,
.comment-list {
  display: flex;
  flex-direction: column;
}

.contribution-text {
  margin-top: 0.9rem;
  padding: 0.85rem 0.95rem;
  border-radius: 0.8rem;
  background: var(--surface-panel);
}

.contribution-text > span {
  color: var(--text-muted);
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
}

.contribution-markdown {
  margin: 0.45rem 0 0;
}

.discussion-list {
  gap: 1rem;
}

.version-discussion {
  padding: 1rem;
  border: 1px solid var(--border-soft);
  border-radius: 1rem;
  background: var(--surface-raised);
}

.version-heading {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 0.75rem;
  align-items: start;
}

.version-heading > div {
  display: flex;
  flex-direction: column;
  gap: 0.2rem;
}

.version-heading time,
.empty-comments {
  color: var(--text-muted);
  font-size: 0.82rem;
}

.rank {
  color: var(--accent);
  font-weight: 700;
}

.support {
  padding: 0.25rem 0.5rem;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--accent-contrast);
  font-size: 0.75rem;
}

.comment-list {
  gap: 0.65rem;
  margin-top: 0.9rem;
}

.empty-comments {
  margin: 0.8rem 0 0;
  font-style: italic;
}

.new-comment-form {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.55rem;
  margin-top: 0.9rem;
}

textarea {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid var(--border-soft);
  border-radius: 0.8rem;
  padding: 0.75rem;
  background: var(--surface-input);
  color: var(--text-primary);
  font: inherit;
  resize: vertical;
}

.new-comment-form button {
  border: 0;
  border-radius: 999px;
  padding: 0.6rem 0.9rem;
  background: var(--accent);
  color: var(--text-on-accent);
  cursor: pointer;
}

.new-comment-form button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
