import { defineStore } from "pinia";

import * as siteSettingsApi from "../api/siteSettings";
import type { SiteSettings } from "../api/types";

type SiteState = {
  settings: SiteSettings | null;
  isLoading: boolean;
  hydrated: boolean;
  error: string | null;
};

const DEFAULT_SETTINGS: SiteSettings = {
  brand_name: "Rimbun",
  browser_title: "Rimbun",
  updated_at: "",
};

function applyDocumentTitle(settings: SiteSettings | null) {
  if (typeof document === "undefined") {
    return;
  }
  document.title = settings?.browser_title || DEFAULT_SETTINGS.browser_title;
}

export const useSiteStore = defineStore("site", {
  state: (): SiteState => ({
    settings: null,
    isLoading: false,
    hydrated: false,
    error: null,
  }),
  getters: {
    brandName(state) {
      return state.settings?.brand_name || DEFAULT_SETTINGS.brand_name;
    },
    browserTitle(state) {
      return state.settings?.browser_title || DEFAULT_SETTINGS.browser_title;
    },
  },
  actions: {
    async load() {
      if (this.isLoading) {
        return;
      }
      this.isLoading = true;
      this.error = null;
      try {
        this.settings = await siteSettingsApi.getSiteSettings();
        this.hydrated = true;
        applyDocumentTitle(this.settings);
      } catch (error) {
        this.error = error instanceof Error ? error.message : "Failed to load site settings";
        this.hydrated = true;
        if (!this.settings) {
          this.settings = DEFAULT_SETTINGS;
        }
        applyDocumentTitle(this.settings);
      } finally {
        this.isLoading = false;
      }
    },
    apply(settings: SiteSettings) {
      this.settings = settings;
      this.hydrated = true;
      applyDocumentTitle(settings);
    },
  },
});
