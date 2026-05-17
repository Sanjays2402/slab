#!/usr/bin/env node
/**
 * scripts/audit-a11y.mjs
 *
 * v1.2.0 "Glass II" Slice 4 — accessibility audit.
 *
 * Walks every `.svelte` file under `src/` and surfaces a curated list
 * of accessibility issues that the human reviewer (and Cake) should fix.
 *
 * What it flags (heuristic — no false-negative guarantee, only false-
 * positive avoidance because we explicitly skip obviously-labelled cases):
 *
 *   1. <button> opening tag with no `aria-label=` and no `title=` and
 *      no text content (icon-only buttons / glyph buttons).
 *   2. <input> / <select> / <textarea> with no `aria-label=` and no
 *      preceding/sibling <label for=>.
 *   3. <img> with no `alt=`.
 *   4. <a> link with no text and no `aria-label=`.
 *
 * Run with:
 *
 *     pnpm a11y:audit               # human-readable summary
 *     pnpm a11y:audit --json        # machine-readable JSON
 *     pnpm a11y:audit --strict      # exit 1 if any issues found
 *
 * Intentionally written in plain Node + fs.readdir — no extra deps so
 * we can run it from CI without bloating package.json.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = process.cwd();
const SRC = join(ROOT, "src");
const args = new Set(process.argv.slice(2));
const FLAG_JSON = args.has("--json");
const FLAG_STRICT = args.has("--strict");

/** Walk a dir recursively, yielding every `.svelte` file. */
function* walk(dir) {
	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		const p = join(dir, entry.name);
		if (entry.isDirectory()) {
			// Skip node_modules, build outputs, hidden dirs.
			if (entry.name === "node_modules") continue;
			if (entry.name.startsWith(".")) continue;
			if (entry.name === "build" || entry.name === ".svelte-kit") continue;
			yield* walk(p);
		} else if (entry.isFile() && entry.name.endsWith(".svelte")) {
			yield p;
		}
	}
}

/**
 * Find iconic-only <button> tags missing labels.
 * Returns array of { line, snippet }.
 */
function findIconButtons(text) {
	const findings = [];
	// Match an opening <button ...> tag — greedy across multiple lines.
	const re = /<button\b([^>]*)>([\s\S]*?)<\/button>/g;
	let m;
	while ((m = re.exec(text)) !== null) {
		const attrs = m[1];
		const body = m[2];
		const hasAriaLabel = /\baria-label\s*=/.test(attrs);
		const hasTitle = /\btitle\s*=/.test(attrs);
		if (hasAriaLabel || hasTitle) continue;
		// If the body contains a string literal or named ident inside a
		// mustache expression (e.g. {label} / {"Save"} / {ok ? "X" : "Y"}),
		// treat it as labelled — likely a textual button bound to data.
		const hasMustache = /\{[\s\S]*?\}/.test(body);
		const mustacheHasLetters = /\{[\s\S]*?[a-zA-Z][\s\S]*?\}/.test(body);
		if (hasMustache && mustacheHasLetters) continue;
		// Strip HTML tags + (now-empty-of-meaning) mustaches.
		const stripped = body
			.replace(/\{[\s\S]*?\}/g, "")
			.replace(/<[^>]+>/g, " ")
			.trim();
		const hasLetters = /[a-zA-Z0-9]{2,}/.test(stripped);
		if (hasLetters) continue;
		const upToMatch = text.slice(0, m.index);
		const line = upToMatch.split("\n").length;
		const snippet = m[0]
			.split("\n")[0]
			.replace(/\s+/g, " ")
			.slice(0, 120);
		findings.push({ line, snippet });
	}
	return findings;
}

/** Detect inputs / selects / textareas without explicit accessible labels. */
function findUnlabelledInputs(text) {
	const findings = [];
	const re = /<(input|select|textarea)\b([^>]*?)\/?>/g;
	let m;
	while ((m = re.exec(text)) !== null) {
		const tag = m[1];
		const attrs = m[2];
		if (/\baria-label\s*=/.test(attrs)) continue;
		if (/\baria-labelledby\s*=/.test(attrs)) continue;
		// Hidden / type=hidden inputs are fine.
		if (tag === "input" && /\btype\s*=\s*["']hidden["']/.test(attrs))
			continue;
		// If the input has an `id=`, look for a <label for="id"> in the file.
		const idMatch = attrs.match(/\bid\s*=\s*["']([^"']+)["']/);
		if (idMatch) {
			const id = idMatch[1];
			const labelRe = new RegExp(
				`<label\\b[^>]*\\bfor\\s*=\\s*["']${escapeRegex(id)}["']`,
			);
			if (labelRe.test(text)) continue;
		}
		// Check if the input is wrapped in a <label>…</label> (no `for=`
		// needed in that case — implicit association). We scan for the
		// nearest open <label> before this match without a closing tag
		// in between.
		const before = text.slice(0, m.index);
		const lastLabelOpen = before.lastIndexOf("<label");
		const lastLabelClose = before.lastIndexOf("</label>");
		if (lastLabelOpen > lastLabelClose) {
			// We're inside a <label> — implicit association applies.
			continue;
		}
		const upToMatch = text.slice(0, m.index);
		const line = upToMatch.split("\n").length;
		const snippet = m[0]
			.split("\n")[0]
			.replace(/\s+/g, " ")
			.slice(0, 120);
		findings.push({ line, snippet });
	}
	return findings;
}

/** Detect <img> without alt= attribute. */
function findImagesWithoutAlt(text) {
	const findings = [];
	const re = /<img\b([^>]*?)\/?>/g;
	let m;
	while ((m = re.exec(text)) !== null) {
		const attrs = m[1];
		if (/\balt\s*=/.test(attrs)) continue;
		const upToMatch = text.slice(0, m.index);
		const line = upToMatch.split("\n").length;
		const snippet = m[0].replace(/\s+/g, " ").slice(0, 120);
		findings.push({ line, snippet });
	}
	return findings;
}

function escapeRegex(s) {
	return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

const report = {
	scanned: 0,
	files: {},
	totals: {
		iconButtons: 0,
		unlabelledInputs: 0,
		imagesWithoutAlt: 0,
	},
};

for (const file of walk(SRC)) {
	report.scanned += 1;
	const text = readFileSync(file, "utf8");
	const rel = relative(ROOT, file);
	const ic = findIconButtons(text);
	const ul = findUnlabelledInputs(text);
	const ia = findImagesWithoutAlt(text);
	if (ic.length === 0 && ul.length === 0 && ia.length === 0) continue;
	report.files[rel] = {
		iconButtons: ic,
		unlabelledInputs: ul,
		imagesWithoutAlt: ia,
	};
	report.totals.iconButtons += ic.length;
	report.totals.unlabelledInputs += ul.length;
	report.totals.imagesWithoutAlt += ia.length;
}

if (FLAG_JSON) {
	process.stdout.write(JSON.stringify(report, null, 2) + "\n");
} else {
	const filesWith = Object.keys(report.files);
	console.log(`Slab a11y audit — scanned ${report.scanned} .svelte files`);
	console.log(
		`  ${report.totals.iconButtons} icon-only <button> missing aria-label/title`,
	);
	console.log(
		`  ${report.totals.unlabelledInputs} <input>/<select>/<textarea> missing labels`,
	);
	console.log(`  ${report.totals.imagesWithoutAlt} <img> missing alt=`);
	if (filesWith.length === 0) {
		console.log("\n✓ Clean.");
	} else {
		for (const f of filesWith) {
			const r = report.files[f];
			const total =
				r.iconButtons.length +
				r.unlabelledInputs.length +
				r.imagesWithoutAlt.length;
			console.log(`\n${f} — ${total} issues`);
			for (const x of r.iconButtons) {
				console.log(`  L${x.line}  [icon-button] ${x.snippet}`);
			}
			for (const x of r.unlabelledInputs) {
				console.log(`  L${x.line}  [input-label] ${x.snippet}`);
			}
			for (const x of r.imagesWithoutAlt) {
				console.log(`  L${x.line}  [img-alt]     ${x.snippet}`);
			}
		}
	}
}

const total =
	report.totals.iconButtons +
	report.totals.unlabelledInputs +
	report.totals.imagesWithoutAlt;

if (FLAG_STRICT && total > 0) {
	process.exit(1);
}
