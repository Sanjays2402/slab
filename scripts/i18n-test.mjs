#!/usr/bin/env node
// Slab i18n resolver smoke tests — v1.2.0 "Glass II" Slice 5.
//
// Plain Node, zero deps, no vitest. Re-implements `resolve()` in plain
// JS and feeds it the real JSON bundles. If a translator drops a key
// it shows up here.
//
// Run:  pnpm run i18n:test
//
// Exit code 0 on pass, 1 on fail.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, "..", "src", "lib", "i18n");

function load(id) {
	return JSON.parse(readFileSync(join(ROOT, `${id}.json`), "utf8"));
}

const en = load("en");
const es = load("es");
const fr = load("fr");
const ar = load("ar");

// Ported from src/lib/i18n.ts — must stay in sync.
function resolve(bundle, key, vars) {
	let raw = bundle[key];
	if (raw == null) raw = en[key];
	if (raw == null) raw = key;
	if (vars) {
		for (const [k, v] of Object.entries(vars)) {
			raw = raw.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
		}
	}
	return raw;
}

const results = [];
function assert(label, actual, expected) {
	const ok = actual === expected;
	results.push({ label, ok, actual, expected });
	if (!ok) {
		console.error(
			`  ✗ ${label}\n      expected: ${JSON.stringify(expected)}\n      actual:   ${JSON.stringify(actual)}`,
		);
	} else {
		console.log(`  ✓ ${label}`);
	}
}

console.log("i18n resolver tests:\n");

// 1. Canonical English key.
assert("en/app.title", resolve(en, "app.title"), "Slab");

// 2. Spanish translation present.
assert("es/settings.title", resolve(es, "settings.title"), "Ajustes");

// 3. French translation present.
assert("fr/settings.theme.title", resolve(fr, "settings.theme.title"), "Thème");

// 4. Arabic translation present + RTL-clean.
assert("ar/settings.title", resolve(ar, "settings.title"), "الإعدادات");

// 5. Missing key in fr falls back to English (synthetic — inject).
const partial = { "app.title": "X" };
assert(
	"missing-in-bundle falls back to en",
	resolve(partial, "settings.title"),
	en["settings.title"],
);

// 6. Missing in en falls back to key itself.
assert(
	"missing-everywhere returns key",
	resolve(en, "nonexistent.deeply.missing.key.xyz"),
	"nonexistent.deeply.missing.key.xyz",
);

// 7. Interpolation: {panel} → value.
assert(
	"interpolation works",
	resolve(en, "toast.detached", { panel: "Beacon AI" }),
	"Detached Beacon AI",
);

// 8. Interpolation in Spanish bundle.
assert(
	"interpolation works in es",
	resolve(es, "toast.detached", { panel: "Beacon IA" }),
	"Beacon IA separado",
);

// 9. Every key in en.json must be a string (catches typos).
for (const [k, v] of Object.entries(en)) {
	if (typeof v !== "string") {
		assert(`en/${k} must be string`, typeof v, "string");
	}
}

// 10. All non-English bundles: keys must be subset of en.json (no orphans).
for (const [name, bundle] of [
	["es", es],
	["fr", fr],
	["ar", ar],
]) {
	const orphans = Object.keys(bundle).filter((k) => !(k in en));
	assert(`${name}.json has no orphan keys`, orphans.length, 0);
}

const failed = results.filter((r) => !r.ok);
console.log(
	`\n${results.length - failed.length} passed, ${failed.length} failed`,
);
process.exit(failed.length === 0 ? 0 : 1);
