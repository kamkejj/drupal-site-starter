import { stepTextSize } from "./preferences.svelte";

export const appSettings = $state({ open: false });

const isMac = navigator.userAgent.includes("Mac");
export const modifier = isMac ? { label: "⌘", aria: "Meta" } : { label: "Ctrl+", aria: "Control" };

/** `keys` are the KeyboardEvent.key values that trigger a shortcut; `label` is how it's shown. */
export const shortcuts = [
  { keys: [","], label: ",", description: "Open App settings", run: () => (appSettings.open = true) },
  { keys: ["=", "+"], label: "+", description: "Larger text", run: () => stepTextSize(1) },
  { keys: ["-"], label: "−", description: "Smaller text", run: () => stepTextSize(-1) },
  { keys: ["0"], label: "0", description: "Default text size", run: () => stepTextSize(0) },
];

export function handleShortcut(e: KeyboardEvent) {
  const modified = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
  const shortcut = modified && !e.altKey ? shortcuts.find((s) => s.keys.includes(e.key)) : undefined;
  if (!shortcut) return;
  e.preventDefault();
  shortcut.run();
}
