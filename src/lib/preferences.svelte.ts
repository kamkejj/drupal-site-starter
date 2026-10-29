import { getCurrentWindow } from "@tauri-apps/api/window";

export type Mode = "light" | "dark";

export const appearances = { system: "Match system", light: "Light", dark: "Dark" } as const;
export type Appearance = keyof typeof appearances;

/** Each id has a matching `[data-theme]` block in app.css. */
export const themes = {
  latte: { label: "Latte", mode: "light" },
  frappe: { label: "Frappé", mode: "dark" },
  macchiato: { label: "Macchiato", mode: "dark" },
  mocha: { label: "Mocha", mode: "dark" },
} as const satisfies Record<string, { label: string; mode: Mode }>;
export type ThemeId = keyof typeof themes;

/** Ordered smallest to largest, so the zoom shortcuts can step through them. Each has a `[data-text-size]` rule in app.css. */
export const textSizes = { small: "Small", default: "Default", large: "Large", larger: "Larger" } as const;
export type TextSize = keyof typeof textSizes;

export type Preferences = {
  appearance: Appearance;
  themes: Record<Mode, ThemeId>;
  textSize: TextSize;
};

const KEY = "preferences";
const defaults: Preferences = { appearance: "system", themes: { light: "latte", dark: "mocha" }, textSize: "default" };

const oneOf = <T extends string>(table: Record<T, unknown>, v: unknown, fallback: T): T =>
  typeof v === "string" && v in table ? (v as T) : fallback;
const themeFor = (mode: Mode, v: unknown): ThemeId => {
  const id = oneOf(themes, v, defaults.themes[mode]);
  return themes[id].mode === mode ? id : defaults.themes[mode];
};

function load(): Preferences {
  let raw: Partial<Preferences> = {};
  try {
    raw = JSON.parse(localStorage.getItem(KEY) ?? "{}");
  } catch {}
  return {
    appearance: oneOf(appearances, raw.appearance, defaults.appearance),
    themes: { light: themeFor("light", raw.themes?.light), dark: themeFor("dark", raw.themes?.dark) },
    textSize: oneOf(textSizes, raw.textSize, defaults.textSize),
  };
}

export const preferences = $state(load());

const prefersDark = matchMedia("(prefers-color-scheme: dark)");
let systemMode = $state<Mode>(prefersDark.matches ? "dark" : "light");
prefersDark.addEventListener("change", (e) => (systemMode = e.matches ? "dark" : "light"));

export const themesFor = (mode: Mode) =>
  (Object.keys(themes) as ThemeId[]).filter((id) => themes[id].mode === mode);

/** Steps the text size by one, or back to the default for 0, clamped at both ends. */
export function stepTextSize(step: -1 | 0 | 1) {
  const sizes = Object.keys(textSizes) as TextSize[];
  const i = sizes.indexOf(preferences.textSize);
  preferences.textSize = step === 0 ? "default" : sizes[Math.min(sizes.length - 1, Math.max(0, i + step))];
}

/** Call inside an effect. The pre-paint script in app.html applies the same rules before this module loads. */
export function applyPreferences() {
  localStorage.setItem(KEY, JSON.stringify(preferences));
  const mode = preferences.appearance === "system" ? systemMode : preferences.appearance;
  const html = document.documentElement;
  html.dataset.theme = preferences.themes[mode];
  html.dataset.textSize = preferences.textSize;
  if ("__TAURI_INTERNALS__" in window) void getCurrentWindow().setTheme(preferences.appearance === "system" ? null : preferences.appearance);
}
