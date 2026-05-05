<script setup lang="ts">
defineProps<{
  title: string;
  content: string;
  saveState: string;
  publishState: string;
  error: string | null;
  canEdit: boolean;
  globalMainLabel: string;
  personalBaseLabel: string;
}>();

const emit = defineEmits<{
  "update:content": [value: string];
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
      <div class="editor-actions">
        <button class="ghost" :disabled="!canEdit || saveState === 'saving'" @click="emit('save')">
          {{ saveState === "saving" ? "Saving..." : "Save Draft" }}
        </button>
        <button class="solid" :disabled="!canEdit || publishState === 'publishing'" @click="emit('publish')">
          {{ publishState === "publishing" ? "Publishing..." : "Publish" }}
        </button>
      </div>
    </div>
    <div class="editor-status-grid">
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
    <textarea
      :value="content"
      :disabled="!canEdit"
      placeholder="Write the section content in Markdown."
      @input="emit('update:content', ($event.target as HTMLTextAreaElement).value)"
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
  background: linear-gradient(180deg, rgba(255, 251, 245, 0.98), rgba(249, 241, 231, 0.96));
  border: 1px solid rgba(35, 24, 15, 0.08);
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
  color: #8e4b16;
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
  background: rgba(255, 255, 255, 0.7);
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

.status-card span {
  color: #705948;
  font-size: 0.78rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.status-card strong {
  font-size: 1rem;
  color: #2d1d12;
}

.status-card.accent {
  background: #f1dcc4;
}

.ghost,
.solid {
  border: 0;
  border-radius: 0.85rem;
  padding: 0.75rem 1rem;
  cursor: pointer;
}

.ghost {
  background: #efe4d6;
  color: #4d3322;
}

.solid {
  background: #8e4b16;
  color: white;
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
  background: rgba(255, 255, 255, 0.85);
  color: #271b12;
  font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
  font-size: 0.96rem;
  line-height: 1.55;
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

.error {
  margin: 0;
  color: #9d2a16;
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
