<script setup lang="ts">
import { computed, ref } from "vue";

import type { SectionRecord } from "../api/types";
import { buildSectionNumbers } from "../section-numbering";

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

const collapsedSectionIds = ref(new Set<string>());

const childCounts = computed(() => {
  const counts = new Map<string, number>();
  for (const section of props.sections) {
    if (section.parent_id) {
      counts.set(section.parent_id, (counts.get(section.parent_id) ?? 0) + 1);
    }
  }
  return counts;
});

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
      if (!collapsedSectionIds.value.has(child.id)) {
        visit(child.id);
      }
    }
  }

  visit(null);
  return result;
});

const sectionNumbers = computed(() => buildSectionNumbers(props.sections));

function hasChildren(sectionId: string) {
  return (childCounts.value.get(sectionId) ?? 0) > 0;
}

function isCollapsed(sectionId: string) {
  return collapsedSectionIds.value.has(sectionId);
}

function handleSectionClick(section: SectionRecord) {
  emit("select", section.id);

  if (!hasChildren(section.id)) {
    return;
  }

  const next = new Set(collapsedSectionIds.value);
  if (next.has(section.id)) {
    next.delete(section.id);
  } else {
    next.add(section.id);
  }
  collapsedSectionIds.value = next;
}

function sectionLabel(section: SectionRecord) {
  return section.has_heading ? section.title : "(no heading)";
}
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
      @click="handleSectionClick(section)"
    >
      <span class="tree-toggle" :class="{ hidden: !hasChildren(section.id) }">
        {{ isCollapsed(section.id) ? ">" : "v" }}
      </span>
      <span class="tree-number">{{ sectionNumbers.get(section.id)?.full }}</span>
      <span class="tree-title" :class="{ muted: !section.has_heading }">{{ sectionLabel(section) }}</span>
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
  background: color-mix(in srgb, var(--surface-raised) 94%, transparent);
  border: 1px solid var(--border-soft);
}

.tree-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  color: var(--text-secondary);
  font-size: 0.92rem;
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.tree-header p {
  margin: 0;
}

.tree-row {
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
  text-align: left;
  border: 0;
  border-radius: 0.9rem;
  padding: 0.8rem 1rem;
  background: transparent;
  color: var(--text-strong);
  cursor: pointer;
  transition: background 140ms ease, transform 140ms ease;
}

.tree-row:hover,
.tree-row.active {
  background: var(--accent-soft);
  transform: translateX(2px);
}

.tree-toggle {
  width: 0.8rem;
  color: var(--accent);
  font-size: 1rem;
  line-height: 1;
}

.tree-toggle.hidden {
  visibility: hidden;
}

.tree-number {
  min-width: 1ch;
  color: var(--accent);
  font-variant-numeric: tabular-nums;
}

.tree-title {
  min-width: 0;
}

.tree-title.muted {
  color: #7a6759;
  font-style: italic;
}
</style>
