<script setup lang="ts">
import { currentLanguage, selectLanguage, t, type UiLanguage } from "../i18n";

function handleChange(event: Event) {
  selectLanguage((event.target as HTMLSelectElement).value as UiLanguage);
}
</script>

<template>
  <label class="language-selector">
    <span>{{ t("Language") }}</span>
    <select :value="currentLanguage" @change="handleChange">
      <option value="de">🇩🇪 {{ t("German") }}</option>
      <option value="en">🇬🇧 {{ t("English") }}</option>
    </select>
  </label>
</template>

<style scoped>
.language-selector {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  color: var(--text-secondary);
  font-size: 0.82rem;
}

.language-selector select {
  width: auto;
  border: 1px solid var(--border-soft);
  border-radius: 999px;
  padding: 0.42rem 1.8rem 0.42rem 0.7rem;
  background: var(--surface-panel-strong);
  color: var(--text-primary);
}

@media (max-width: 620px) {
  .language-selector span {
    display: none;
  }
}
</style>
