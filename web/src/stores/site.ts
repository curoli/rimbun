import { defineStore } from "pinia";

import * as siteSettingsApi from "../api/siteSettings";
import type { SiteSettings } from "../api/types";
import { DEFAULT_SITE_COLOR_SCHEME, isSiteColorScheme } from "../site-theme";

type SiteState = {
  settings: SiteSettings | null;
  isLoading: boolean;
  hydrated: boolean;
  error: string | null;
};

const DEFAULT_SETTINGS: SiteSettings = {
  brand_name: "Rimbun",
  browser_title: "Rimbun",
  color_scheme: DEFAULT_SITE_COLOR_SCHEME,
  updated_at: "",
};

function applySiteSettings(settings: SiteSettings | null) {
  if (typeof document === "undefined") {
    return;
  }
  document.title = settings?.browser_title || DEFAULT_SETTINGS.browser_title;
  const theme = settings?.color_scheme;
  document.documentElement.dataset.rimbunTheme =
    theme && isSiteColorScheme(theme) ? theme : DEFAULT_SITE_COLOR_SCHEME;
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
    colorScheme(state) {
      const value = state.settings?.color_scheme;
      return value && isSiteColorScheme(value) ? value : DEFAULT_SITE_COLOR_SCHEME;
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
        applySiteSettings(this.settings);
      } catch (error) {
        this.error = error instanceof Error ? error.message : "Failed to load site settings";
        this.hydrated = true;
        if (!this.settings) {
          this.settings = DEFAULT_SETTINGS;
        }
        applySiteSettings(this.settings);
      } finally {
        this.isLoading = false;
      }
    },
    apply(settings: SiteSettings) {
      this.settings = settings;
      this.hydrated = true;
      applySiteSettings(settings);
    },
  },
});
