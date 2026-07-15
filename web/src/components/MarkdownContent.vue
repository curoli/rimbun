<script setup lang="ts">
import DOMPurify from "dompurify";
import { marked } from "marked";
import { computed } from "vue";

const props = defineProps<{
  source: string;
}>();

const renderedHtml = computed(() =>
  DOMPurify.sanitize(marked.parse(props.source, { async: false }) as string),
);
</script>

<template>
  <div class="markdown-content" v-html="renderedHtml" />
</template>

<style scoped>
.markdown-content {
  overflow-wrap: anywhere;
  line-height: 1.6;
}

.markdown-content :deep(> :first-child) {
  margin-top: 0;
}

.markdown-content :deep(> :last-child) {
  margin-bottom: 0;
}

.markdown-content :deep(h1),
.markdown-content :deep(h2),
.markdown-content :deep(h3),
.markdown-content :deep(h4),
.markdown-content :deep(h5),
.markdown-content :deep(h6) {
  margin: 1.1em 0 0.45em;
  line-height: 1.2;
}

.markdown-content :deep(h1) {
  font-size: 1.45rem;
}

.markdown-content :deep(h2) {
  font-size: 1.28rem;
}

.markdown-content :deep(h3) {
  font-size: 1.12rem;
}

.markdown-content :deep(p),
.markdown-content :deep(ul),
.markdown-content :deep(ol),
.markdown-content :deep(blockquote),
.markdown-content :deep(pre) {
  margin: 0.65em 0;
}

.markdown-content :deep(ul),
.markdown-content :deep(ol) {
  padding-left: 1.5rem;
}

.markdown-content :deep(blockquote) {
  padding-left: 0.85rem;
  border-left: 3px solid var(--border-medium);
  color: var(--text-secondary);
}

.markdown-content :deep(code) {
  padding: 0.12em 0.3em;
  border-radius: 0.3rem;
  background: var(--accent-hover);
  font-family: "IBM Plex Mono", monospace;
  font-size: 0.9em;
}

.markdown-content :deep(pre) {
  overflow-x: auto;
  padding: 0.8rem;
  border-radius: 0.65rem;
  background: var(--accent-hover);
}

.markdown-content :deep(pre code) {
  padding: 0;
  background: transparent;
}

.markdown-content :deep(a) {
  color: var(--accent);
  text-underline-offset: 0.14em;
}

.markdown-content :deep(img) {
  max-width: 100%;
  height: auto;
}

.markdown-content :deep(table) {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  border-collapse: collapse;
}

.markdown-content :deep(th),
.markdown-content :deep(td) {
  padding: 0.45rem 0.6rem;
  border: 1px solid var(--border-medium);
}
</style>
