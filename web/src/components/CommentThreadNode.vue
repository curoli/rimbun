<script setup lang="ts">
import { ref } from "vue";

import type { CommentTreeNode } from "../api/types";
import MarkdownContent from "./MarkdownContent.vue";

const props = defineProps<{
  node: CommentTreeNode;
  canComment: boolean;
  currentUserId: string | null;
  isAdmin: boolean;
}>();

const emit = defineEmits<{
  reply: [payload: { parentCommentId: string; markdownContent: string }];
  delete: [commentId: string];
}>();

const replyOpen = ref(false);
const replyDraft = ref("");

function submitReply() {
  const markdownContent = replyDraft.value.trim();
  if (!markdownContent) {
    return;
  }
  emit("reply", { parentCommentId: props.node.id, markdownContent });
  replyDraft.value = "";
  replyOpen.value = false;
}
</script>

<template>
  <article class="comment-node" :class="{ primary: node.is_primary }">
    <header>
      <strong>{{ node.display_name }} @{{ node.username }}</strong>
      <span v-if="node.is_primary" class="primary-badge">
        {{ $t("Author's main comment") }}
      </span>
      <time>{{ $date(node.created_at) }}</time>
      <button
        v-if="!node.deleted_at && (isAdmin || currentUserId === node.user_id)"
        type="button"
        class="delete-button"
        @click="emit('delete', node.id)"
      >
        {{ $t("Delete") }}
      </button>
    </header>
    <p v-if="node.deleted_at" class="deleted-comment">{{ $t("This comment was deleted.") }}</p>
    <MarkdownContent v-else class="comment-content" :source="node.markdown_content" />
    <button
      v-if="canComment"
      type="button"
      class="reply-button"
      @click="replyOpen = !replyOpen"
    >
      {{ replyOpen ? $t("Cancel reply") : $t("Reply") }}
    </button>
    <div v-if="replyOpen" class="reply-form">
      <textarea v-model="replyDraft" :placeholder="$t('Write a reply.')" rows="3" />
      <button type="button" :disabled="!replyDraft.trim()" @click="submitReply">
        {{ $t("Post reply") }}
      </button>
    </div>
    <div v-if="node.replies.length" class="replies">
      <CommentThreadNode
        v-for="reply in node.replies"
        :key="reply.id"
        :node="reply"
        :can-comment="canComment"
        :current-user-id="currentUserId"
        :is-admin="isAdmin"
        @reply="emit('reply', $event)"
        @delete="emit('delete', $event)"
      />
    </div>
  </article>
</template>

<style scoped>
.comment-node {
  padding: 0.85rem 0.95rem;
  border-left: 2px solid var(--border-medium);
  border-radius: 0 0.75rem 0.75rem 0;
  background: color-mix(in srgb, var(--surface-panel) 82%, transparent);
}

.comment-node.primary {
  border-left-color: var(--accent);
  background: color-mix(in srgb, var(--accent-soft) 34%, var(--surface-panel));
}

header {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0.4rem 0.65rem;
}

time {
  color: var(--text-muted);
  font-size: 0.78rem;
}

.primary-badge {
  padding: 0.18rem 0.42rem;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--accent-contrast);
  font-size: 0.7rem;
}

.comment-content {
  margin: 0.55rem 0 0;
}

.deleted-comment {
  margin: 0.55rem 0 0;
  color: var(--text-muted);
  font-style: italic;
}

.reply-button {
  margin-top: 0.55rem;
  border: 0;
  padding: 0;
  background: transparent;
  color: var(--accent);
  cursor: pointer;
}

.delete-button {
  margin-left: auto;
  border: 0;
  padding: 0;
  background: transparent;
  color: var(--danger);
  cursor: pointer;
  font: inherit;
  font-size: 0.78rem;
}

.reply-form {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 0.5rem;
  margin-top: 0.65rem;
}

textarea {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid var(--border-soft);
  border-radius: 0.7rem;
  padding: 0.7rem;
  background: var(--surface-input);
  color: var(--text-primary);
  font: inherit;
  resize: vertical;
}

.reply-form button {
  border: 0;
  border-radius: 999px;
  padding: 0.55rem 0.8rem;
  background: var(--accent);
  color: var(--text-on-accent);
  cursor: pointer;
}

.reply-form button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.replies {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
  margin: 0.7rem 0 0 1rem;
}
</style>
