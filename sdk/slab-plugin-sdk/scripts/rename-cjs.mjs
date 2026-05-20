// Renames `.js` files emitted by `tsc -p tsconfig.cjs.json` to `.cjs`
// and rewrites internal `require()` paths accordingly. Necessary because
// tsc doesn't honor a `.cjs` outFileExtension yet (TS issue #54573).
//
// The package.json `exports.require` field points at `.cjs` paths, so the
// renames here make those references valid post-build.

import { promises as fs } from "node:fs";
import { join, dirname, basename, extname } from "node:path";

const ROOT = new URL("../dist/cjs/", import.meta.url).pathname;

/** @returns {Promise<string[]>} */
async function walk(dir) {
  let out = [];
  const entries = await fs.readdir(dir, { withFileTypes: true });
  for (const e of entries) {
    const p = join(dir, e.name);
    if (e.isDirectory()) {
      out = out.concat(await walk(p));
    } else if (e.isFile() && p.endsWith(".js")) {
      out.push(p);
    }
  }
  return out;
}

async function main() {
  const files = await walk(ROOT);
  // First pass: rename .js → .cjs
  const renamed = new Map();
  for (const src of files) {
    const dst = src.replace(/\.js$/, ".cjs");
    await fs.rename(src, dst);
    renamed.set(src, dst);
  }
  // Second pass: rewrite require("./foo") to require("./foo.cjs") for
  // every internal require. External requires (no leading ./ or ../)
  // are left alone.
  for (const dst of renamed.values()) {
    let body = await fs.readFile(dst, "utf8");
    body = body.replace(
      /require\(["'](\.[^"']+)["']\)/g,
      (m, spec) => {
        // If spec has no extension, append .cjs. If it ends in .js,
        // swap to .cjs. Anything else (e.g. .json) is left alone.
        const ext = extname(spec);
        if (ext === "") return `require("${spec}.cjs")`;
        if (ext === ".js") return `require("${spec.slice(0, -3)}.cjs")`;
        return m;
      },
    );
    await fs.writeFile(dst, body, "utf8");
  }
  // Source maps: rename .js.map → .cjs.map and update inline refs.
  const maps = (await walk(ROOT)).filter((p) => p.endsWith(".js.map"));
  for (const src of maps) {
    const dst = src.replace(/\.js\.map$/, ".cjs.map");
    await fs.rename(src, dst);
  }
  process.stdout.write(
    `rename-cjs: ${renamed.size} file(s) renamed under ${ROOT}\n`,
  );
  // basename/dirname imported but only used implicitly via path manipulations
  void basename;
  void dirname;
}

main().catch((e) => {
  process.stderr.write(`rename-cjs failed: ${e.stack ?? e}\n`);
  process.exit(1);
});
