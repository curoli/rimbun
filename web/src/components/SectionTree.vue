<script setup lang="ts">
import { computed } from "vue";

import type { SectionRecord } from "../api/types";

const props = defineProps<{
  sections: SectionRecord[];
  activeSectionId: string | null;
}>();

const emit = defineEmits<{
  select: [sectionId: string];
}>();

function depthFor(path: string) {
  return path.split("/").length - 1;
}

const orderedSections = computed(() => {
  const byParent = new Map<string | null, SectionRecord[]>();

  for (const section of props.sections) {
    const group = byParent.get(section.parent_id) ?? [];
    group.push(section);
    byParent.set(section.parent_id, group);
  }

  for (const group of byParent.values()) {
    group.sort((a, b) => a.position - b.position || a.created_at.localeCompare(b.created_at));
  }

  const result: SectionRecord[] = [];

  function visit(parentId: string | null) {
    const children = byParent.get(parentId) ?? [];
    for (const child of children) {
      result.push(child);
      visit(child.id);
    }
  }

  visit(null);
  return result;
});
</script>

<template>
  <aside class="tree-panel">
    <div class="tree-header">
      <p>Sections</p>
      <span>{{ sections.length }}</span>
    </div>
    <button
      v-for="section in orderedSections"
      :key="section.id"
      class="tree-row"
      :class="{ active: section.id === activeSectionId }"
      :style="{ paddingLeft: `${1 + depthFor(section.path) * 1.1}rem` }"
      @click="emit('select', section.id)"
    >
      {{ section.title }}
    </button>
  </aside>
</template>

<style scoped>
.tree-panel {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  padding: 1rem;
  border-radius: 1.25rem;
  background: rgba(255, 253, 250, 0.92);
  border: 1px solid rgba(35, 24, 15, 0.08);
}

.tree-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  color: #6f5947;
  font-size: 0.92rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.tree-header p {
  margin: 0;
}

.tree-row {
  text-align: left;
  border: 0;
  border-radius: 0.9rem;
  padding: 0.8rem 1rem;
  background: transparent;
  color: #2d1d12;
  cursor: pointer;
  transition: background 140ms ease, transform 140ms ease;
}

.tree-row:hover,
.tree-row.active {
  background: #f1dcc4;
  transform: translateX(2px);
}
</style>
