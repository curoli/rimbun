export const DEFAULT_SITE_COLOR_SCHEME = "amber-dawn";

export const SITE_COLOR_SCHEMES = [
  {
    value: "amber-dawn",
    label: "Amber Dawn",
    description: "Warm paper, copper accents, close to the current look.",
  },
  {
    value: "forest-paper",
    label: "Forest Paper",
    description: "Moss and parchment tones with a calmer editorial feel.",
  },
  {
    value: "sea-glass",
    label: "Sea Glass",
    description: "Cool teal surfaces and cleaner contrast.",
  },
  {
    value: "rose-evening",
    label: "Rose Evening",
    description: "Muted brick and blush tones with softer warmth.",
  },
] as const;

export type SiteColorScheme = (typeof SITE_COLOR_SCHEMES)[number]["value"];

export function isSiteColorScheme(value: string): value is SiteColorScheme {
  return SITE_COLOR_SCHEMES.some((scheme) => scheme.value === value);
}
