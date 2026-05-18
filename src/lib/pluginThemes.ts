// Plugin theme injector — v1.3.0 "Foundry" Slice 9.
//
// Owns the runtime `<style>` tag for the currently-active plugin theme.
// Only ONE plugin theme can be active at a time (the same way a single
// built-in theme is active); switching themes swaps the tag's contents.
//
// The Rust side already enforces path-traversal safety on
// `slab_plugins_read_asset`; this module trusts whatever string it gets
// back and writes it verbatim into a `<style>` tag's textContent.
// `textContent` (not `innerHTML`) ensures the browser parses it as CSS
// rather than HTML, so a hostile plugin can't inject script tags.

import { readPluginAsset } from "$lib/plugins";

const STYLE_TAG_ID = "slab-plugin-theme";

/** Activate the given plugin theme. Replaces any previously-active one. */
export async function applyPluginTheme(
  pluginId: string,
  themeId: string,
  cssRelative: string,
): Promise<void> {
  if (typeof document === "undefined") return;
  let tag = document.getElementById(STYLE_TAG_ID) as HTMLStyleElement | null;
  if (!tag) {
    tag = document.createElement("style");
    tag.id = STYLE_TAG_ID;
    document.head.appendChild(tag);
  }
  tag.setAttribute("data-plugin-id", pluginId);
  tag.setAttribute("data-theme-id", themeId);
  const css = await readPluginAsset(pluginId, cssRelative);
  tag.textContent = css;
}

/** Remove the plugin theme tag (called when user picks a built-in theme). */
export function clearPluginTheme(): void {
  if (typeof document === "undefined") return;
  const tag = document.getElementById(STYLE_TAG_ID);
  tag?.remove();
}

/** Read the currently-active plugin theme's IDs, or null. */
export function currentPluginTheme(): { pluginId: string; themeId: string } | null {
  if (typeof document === "undefined") return null;
  const tag = document.getElementById(STYLE_TAG_ID);
  if (!tag) return null;
  const pluginId = tag.getAttribute("data-plugin-id");
  const themeId = tag.getAttribute("data-theme-id");
  if (!pluginId || !themeId) return null;
  return { pluginId, themeId };
}
