import { apiRequest } from "./client";
import type { SiteSettings } from "./types";

export function getSiteSettings() {
  return apiRequest<SiteSettings>("/api/site-settings");
}

export function updateSiteSettings(payload: {
  brand_name: string;
  browser_title: string;
}) {
  return apiRequest<SiteSettings>("/api/site-settings", {
    method: "PATCH",
    bodyJson: payload,
  });
}
