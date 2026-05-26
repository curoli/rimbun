<script setup lang="ts">
import { computed } from "vue";
import { RouterLink } from "vue-router";

const props = defineProps<{
  documentId: string;
  canManageOutline?: boolean;
  activeView: "reader" | "edit" | "outline";
  sectionId?: string | null;
}>();

const editTarget = computed(() => (props.sectionId ? `/sections/${props.sectionId}/edit` : null));
</script>

<template>
  <nav class="view-nav" aria-label="Document views">
    <RouterLink :class="{ active: activeView === 'reader' }" :to="`/documents/${documentId}`">
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
      :to="`/documents/${documentId}/outline`"
    >
      Outline
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
  background: rgba(255, 252, 247, 0.78);
  box-shadow: inset 0 0 0 1px rgba(35, 24, 15, 0.08);
}

.view-nav a {
  border-radius: 999px;
  padding: 0.65rem 0.95rem;
  color: #5f4737;
  text-decoration: none;
}

.view-nav a.active {
  background: #8e4b16;
  color: white;
}
</style>
