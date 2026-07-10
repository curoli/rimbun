<script setup lang="ts">
defineProps<{
  title: string;
  content: string;
  mainComment: string;
  hasOwnText: boolean;
  saveState: string;
  publishState: string;
  error: string | null;
  canEdit: boolean;
  globalMainLabel: string;
  personalBaseLabel: string;
}>();

const emit = defineEmits<{
  "update:content": [value: string];
  "update:mainComment": [value: string];
  save: [];
  publish: [];
}>();
</script>

<template>
  <section class="editor-panel">
    <div class="editor-header">
      <div>
        <p class="eyebrow">Draft Editor</p>
        <h2>{{ title }}</h2>
      </div>
      <div v-if="hasOwnText" class="editor-actions">
        <button class="ghost" :disabled="!canEdit || saveState === 'saving'" @click="emit('save')">
          {{ saveState === "saving" ? "Saving..." : "Save Draft" }}
        </button>
        <button class="solid" :disabled="!canEdit || publishState === 'publishing'" @click="emit('publish')">
          {{ publishState === "publishing" ? "Publishing..." : "Publish" }}
        </button>
      </div>
    </div>
    <div v-if="hasOwnText" class="editor-status-grid">
      <div class="status-card">
        <span>Global Main Version</span>
        <strong>{{ globalMainLabel }}</strong>
      </div>
      <div class="status-card accent">
        <span>Personal Reading Base</span>
        <strong>{{ personalBaseLabel }}</strong>
      </div>
    </div>
    <p v-if="error" class="error">{{ error }}</p>
    <p v-if="!hasOwnText" class="structure-note">
      This section is configured as structure-only. Its content comes only from its subsections.
    </p>
    <textarea
      v-else
      :value="content"
      :disabled="!canEdit"
      placeholder="Write the section content in Markdown."
      @input="emit('update:content', ($event.target as HTMLTextAreaElement).value)"
    />
    <textarea
      v-if="hasOwnText"
      class="comment-textarea"
      :value="mainComment"
      :disabled="!canEdit"
      placeholder="Optional main comment for this contribution."
      @input="emit('update:mainComment', ($event.target as HTMLTextAreaElement).value)"
    />
  </section>
</template>

<style scoped>
.editor-panel {
  display: flex;
  flex-direction: column;
  gap: 1rem;
  padding: 1.3rem;
  border-radius: 1.4rem;
  background: linear-gradient(180deg, color-mix(in srgb, var(--surface-input) 92%, var(--surface-panel)), color-mix(in srgb, var(--accent-soft) 30%, var(--surface-panel)));
  border: 1px solid var(--border-soft);
}

.editor-header {
  display: flex;
  justify-content: space-between;
  gap: 1rem;
  align-items: flex-start;
}

.editor-header h2,
.eyebrow {
  margin: 0;
}

.eyebrow {
  color: var(--accent);
  font-size: 0.8rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  margin-bottom: 0.35rem;
}

.editor-actions {
  display: flex;
  gap: 0.75rem;
}

.editor-status-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.85rem;
}

.status-card {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
  padding: 0.95rem 1rem;
  border-radius: 1rem;
  background: color-mix(in srgb, var(--surface-input) 70%, transparent);
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

.status-card span {
  color: var(--text-muted);
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.status-card strong {
  font-size: 1rem;
  color: var(--text-strong);
}

.status-card.accent {
  background: var(--accent-soft);
}

.ghost,
.solid {
  border: 0;
  border-radius: 0.85rem;
  padding: 0.75rem 1rem;
  cursor: pointer;
}

.ghost {
  background: var(--accent-hover);
  color: var(--accent-contrast);
}

.solid {
  background: var(--accent);
  color: var(--text-on-accent);
}

.ghost:disabled,
.solid:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

textarea {
  min-height: 22rem;
  width: 100%;
  resize: vertical;
  border: 0;
  outline: none;
  border-radius: 1rem;
  padding: 1rem;
  background: color-mix(in srgb, var(--surface-input) 85%, transparent);
  color: var(--text-strong);
  font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
  font-size: 0.96rem;
  line-height: 1.55;
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

.comment-textarea {
  min-height: 8rem;
}

.error {
  margin: 0;
  color: var(--danger);
}

.structure-note {
  margin: 0;
  padding: 1rem 1.05rem;
  border-radius: 1rem;
  background: color-mix(in srgb, var(--surface-input) 72%, transparent);
  box-shadow: inset 0 0 0 1px var(--border-soft);
  color: var(--text-soft);
}

@media (max-width: 760px) {
  .editor-header,
  .editor-status-grid {
    grid-template-columns: 1fr;
    display: grid;
  }

  .editor-actions {
    justify-content: flex-start;
    flex-wrap: wrap;
  }
}
</style>
