// Slab i18n — v1.2.0 "Glass II" Slice 5.
//
// Tiny, zero-deps i18n surface. Pure resolver function + Svelte store
// for live re-renders. JSON bundles are loaded eagerly because they're
// small (~3 KB each) and the cost of waiting for a network/disk round
// trip every time someone toggles language is worse than the extra
// 12 KB in the bundle.
//
// Design notes:
//   - Flat dotted-key shape, not nested. Cheaper resolver, no path
//     traversal, JSON stays grep-able. Linear / Stripe both do this.
//   - English (`en`) is the canonical fallback. Every key the app
//     uses MUST exist in `en.json`. Missing keys in other bundles
//     fall back to English; missing in English falls back to the
//     key string itself (so a developer typo is visible at runtime).
//   - The `t()` function is for one-shot lookups (logs, toasts).
//     Reactive UI should use `$tStore("key")` so a locale switch
//     re-renders the component.
//   - `<html lang>` + `<html dir>` are flipped synchronously inside
//     `applyLocaleToHtml` so screen readers + CSS `:dir(rtl)` see
//     the change immediately.

import { writable, get, derived } from "svelte/store";
import en from "./i18n/en.json";
import es from "./i18n/es.json";
import fr from "./i18n/fr.json";
import ar from "./i18n/ar.json";
import { pluginsStore, loadPluginLocaleBundle, currentPlugins } from "$lib/plugins";

export type LocaleId = "en" | "es" | "fr" | "ar";
export type Bundle = Record<string, string>;

export const LOCALES: { id: LocaleId; label: string; dir: "ltr" | "rtl" }[] = [
	{ id: "en", label: "English", dir: "ltr" },
	{ id: "es", label: "Español", dir: "ltr" },
	{ id: "fr", label: "Français", dir: "ltr" },
	{ id: "ar", label: "العربية", dir: "rtl" },
];

const BUNDLES: Record<LocaleId, Bundle> = {
	en: en as Bundle,
	es: es as Bundle,
	fr: fr as Bundle,
	ar: ar as Bundle,
};

const STORAGE_KEY = "slab.i18n.locale";
const VALID: ReadonlySet<LocaleId> = new Set<LocaleId>(["en", "es", "fr", "ar"]);

function loadLocale(): LocaleId {
	if (typeof localStorage === "undefined") return "en";
	const raw = localStorage.getItem(STORAGE_KEY);
	if (raw && (VALID as Set<string>).has(raw)) return raw as LocaleId;
	// First launch: best-effort guess from navigator.
	if (typeof navigator !== "undefined" && navigator.language) {
		const short = navigator.language.slice(0, 2).toLowerCase() as LocaleId;
		if (VALID.has(short)) return short;
	}
	return "en";
}

function persistLocale(v: LocaleId): void {
	if (typeof localStorage === "undefined") return;
	try {
		localStorage.setItem(STORAGE_KEY, v);
	} catch {
		// quota-exceeded etc — silently ignore, locale stays in-memory.
	}
}

/** Reactive current locale. Components can subscribe (or use `$locale`). */
export const locale = writable<LocaleId>(loadLocale());
locale.subscribe(persistLocale);

/**
 * Pure resolver — given a bundle, a key, and optional `{var}` template
 * variables, return the rendered string. Falls back to English bundle
 * then to the key itself. Exported for unit testing.
 */
export function resolve(
	bundle: Bundle,
	key: string,
	vars?: Record<string, string | number>,
): string {
	let raw = bundle[key];
	if (raw == null) raw = BUNDLES.en[key];
	if (raw == null) raw = key;
	if (vars) {
		for (const [k, v] of Object.entries(vars)) {
			raw = raw.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
		}
	}
	return raw;
}

/**
 * One-shot translate — reads the current locale from the store and
 * resolves the key. Use for logs / toasts / anywhere outside a
 * reactive Svelte component. For reactive UI, use `tStore`.
 */
export function t(key: string, vars?: Record<string, string | number>): string {
	const id = get(locale);
	return resolve(BUNDLES[id] ?? BUNDLES.en, key, vars);
}

/**
 * Reactive translate — a derived store that re-emits a `t(key, vars?)`
 * function every time the locale changes. In a component:
 *
 *   ```svelte
 *   <h1>{$tStore("settings.title")}</h1>
 *   ```
 */
export const tStore = derived(locale, ($l) => {
	const bundle = BUNDLES[$l] ?? BUNDLES.en;
	return (key: string, vars?: Record<string, string | number>) =>
		resolve(bundle, key, vars);
});

/** Flip `<html lang>` + `<html dir>` to match the given locale. */
export function applyLocaleToHtml(id: LocaleId): void {
	if (typeof document === "undefined") return;
	const def = LOCALES.find((l) => l.id === id) ?? LOCALES[0];
	document.documentElement.setAttribute("lang", def.id);
	document.documentElement.setAttribute("dir", def.dir);
}

/** Set the active locale + persist + apply to <html>. */
export function setLocale(id: LocaleId): void {
	locale.set(id);
	applyLocaleToHtml(id);
}

/**
 * Merge a plugin-contributed bundle into the in-memory bundle for `id`.
 * Logs an info line on every key that overrides a built-in entry so plugin
 * authors notice clashes during dev. Idempotent — calling with the same
 * bundle is harmless; the last writer wins. After mutation, re-emits the
 * `locale` store so anything subscribed to `$tStore` repaints.
 */
export function mergePluginBundle(id: LocaleId, bundle: Bundle, pluginId: string): void {
	const target = BUNDLES[id];
	if (!target) return;
	let changed = false;
	for (const [key, value] of Object.entries(bundle)) {
		if (typeof value !== "string") continue;
		if (key in target && target[key] !== value) {
			console.info(`[slab i18n] plugin ${pluginId} overrides "${key}" in ${id}`);
		}
		if (target[key] !== value) {
			target[key] = value;
			changed = true;
		}
	}
	if (changed) {
		// Force `tStore` to re-emit so any component watching `$tStore` repaints.
		locale.set(get(locale));
	}
}

/**
 * Boot the i18n system. Called once from +layout.svelte's `onMount`
 * alongside `bootTheme()`. Idempotent. Synchronously applies the
 * persisted locale so there's no flash of the wrong language. Also
 * subscribes to the plugin snapshot (populated by
 * +layout.svelte's `refreshPlugins()` call) to merge plugin locales
 * whenever the snapshot changes or the active locale flips.
 */
export function bootI18n(): void {
	applyLocaleToHtml(get(locale));

	// Track which (plugin_id, bundle) pairs we've already merged so we
	// don't refetch on every store emit. Resets on locale switch via the
	// `locale.subscribe` below.
	let lastSeen = new Set<string>();
	let lastLocale: LocaleId = get(locale);

	pluginsStore.subscribe(async (snap) => {
		const id = get(locale);
		const targets = snap.locales.filter((l) => l.locale === id);
		const seen = new Set(targets.map((l) => `${l.plugin_id}::${l.bundle}`));
		// Skip if nothing relevant changed.
		if (seen.size === lastSeen.size && [...seen].every((k) => lastSeen.has(k))) return;
		lastSeen = seen;
		for (const t of targets) {
			try {
				const bundle = await loadPluginLocaleBundle(t.plugin_id, t.locale);
				mergePluginBundle(id, bundle, t.plugin_id);
			} catch (e) {
				console.warn(`[slab i18n] failed to load bundle from ${t.plugin_id}`, e);
			}
		}
	});

	// Re-merge on locale switch.
	locale.subscribe((id) => {
		if (id === lastLocale) return;
		lastLocale = id;
		lastSeen = new Set();
		applyLocaleToHtml(id);
		const snap = currentPlugins();
		const targets = snap.locales.filter((l) => l.locale === id);
		for (const t of targets) {
			loadPluginLocaleBundle(t.plugin_id, t.locale)
				.then((b) => {
					mergePluginBundle(id, b, t.plugin_id);
					lastSeen.add(`${t.plugin_id}::${t.bundle}`);
				})
				.catch((e) =>
					console.warn(`[slab i18n] reload bundle on locale switch failed`, e),
				);
		}
	});
}
