/// <reference types="vite/client" />

import type { formatDate, t } from "./i18n";

declare module "vue" {
  interface ComponentCustomProperties {
    $t: typeof t;
    $date: typeof formatDate;
  }
}

export {};
