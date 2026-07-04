<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";

const props = defineProps<{
  documentRef: string;
  canManageOutline?: boolean;
  activeView: "reader" | "edit" | "outline" | "settings";
  sectionId?: string | null;
}>();

const editTarget = computed(() => (props.sectionId ? `/sections/${props.sectionId}/edit` : null));
</script>

<template>
  <nav class="view-nav" aria-label="Document views">
    <RouterLink :class="{ active: activeView === 'reader' }" :to="`/documents/${documentRef}`">
      Read
    </RouterLink>
    <RouterLink
      v-if="editTarget"
      :class="{ active: activeView === 'edit' }"
      :to="editTarget"
    >
      Edit section
    </RouterLink>
    <RouterLink
      v-if="canManageOutline"
      :class="{ active: activeView === 'outline' }"
      :to="`/documents/${documentRef}/outline`"
    >
      Outline
    </RouterLink>
    <RouterLink
      v-if="canManageOutline"
      :class="{ active: activeView === 'settings' }"
      :to="`/documents/${documentRef}/settings`"
    >
      Settings
    </RouterLink>
  </nav>
</template>

<style scoped>
.view-nav {
  display: flex;
  flex-wrap: wrap;
  gap: 0.55rem;
  padding: 0.4rem;
  border-radius: 999px;
  background: color-mix(in srgb, var(--surface-panel) 84%, transparent);
  box-shadow: inset 0 0 0 1px var(--border-soft);
}

.view-nav a {
  border-radius: 999px;
  padding: 0.65rem 0.95rem;
  color: var(--text-soft);
  text-decoration: none;
}

.view-nav a.active {
  background: var(--accent);
  color: var(--text-on-accent);
}
</style>
