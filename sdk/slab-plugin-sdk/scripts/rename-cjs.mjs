// Renames `.js` files emitted by `tsc -p tsconfig.cjs.json` to `.cjs`
// and rewrites internal `require()` paths + sourceMappingURL comments
// accordingly. Necessary because tsc doesn't honor a `.cjs`
// outFileExtension yet (TS issue #54573).
//
// The package.json `exports.require` field points at `.cjs` paths, so
// the renames here make those references valid post-build.

import { promises as fs } from "node:fs";
import { join, extname } from "node:path";

const ROOT = new URL("../dist/cjs/", import.meta.url).pathname;

async function walk(dir, predicate) {
  let out = [];
  const entries = await fs.readdir(dir, { withFileTypes: true });
  for (const e of entries) {
    const p = join(dir, e.name);
    if (e.isDirectory()) {
      out = out.concat(await walk(p, predicate));
    } else if (e.isFile() && predicate(p)) {
      out.push(p);
    }
  }
  return out;
}

async function main() {
  // Pass 1: rename .js files → .cjs
  const jsFiles = await walk(ROOT, (p) => p.endsWith(".js"));
  for (const src of jsFiles) {
    const dst = src.replace(/\.js$/, ".cjs");
    await fs.rename(src, dst);
  }

  // Pass 2: rename .js.map files → .cjs.map
  const mapFiles = await walk(ROOT, (p) => p.endsWith(".js.map"));
  for (const src of mapFiles) {
    const dst = src.replace(/\.js\.map$/, ".cjs.map");
    await fs.rename(src, dst);
  }

  // Pass 3: rewrite require(...) and sourceMappingURL refs in every
  // renamed .cjs file.
  const cjsFiles = await walk(ROOT, (p) => p.endsWith(".cjs"));
  for (const f of cjsFiles) {
    let body = await fs.readFile(f, "utf8");
    body = body.replace(/require\(["'](\.[^"']+)["']\)/g, (m, spec) => {
      const ext = extname(spec);
      if (ext === "") return `require("${spec}.cjs")`;
      if (ext === ".js") return `require("${spec.slice(0, -3)}.cjs")`;
      return m;
    });
    // sourceMappingURL inline comments: //# sourceMappingURL=foo.js.map
    body = body.replace(
      /(\/\/# sourceMappingURL=)([^\s]+)\.js\.map/g,
      "$1$2.cjs.map",
    );
    await fs.writeFile(f, body, "utf8");
  }

  // Pass 4: update each .cjs.map's `file:` field to reference the .cjs
  // file (not .js) so tooling that re-resolves the source-map → source
  // mapping uses the right basename.
  const updatedMaps = await walk(ROOT, (p) => p.endsWith(".cjs.map"));
  for (const f of updatedMaps) {
    let body = await fs.readFile(f, "utf8");
    try {
      const json = JSON.parse(body);
      if (typeof json.file === "string" && json.file.endsWith(".js")) {
        json.file = json.file.slice(0, -3) + ".cjs";
        body = JSON.stringify(json);
        await fs.writeFile(f, body, "utf8");
      }
    } catch {
      // Non-JSON map (shouldn't happen with tsc); leave it alone.
    }
  }

  process.stdout.write(
    `rename-cjs: ${jsFiles.length} .js→.cjs, ${mapFiles.length} .js.map→.cjs.map under ${ROOT}\n`,
  );
}

main().catch((e) => {
  process.stderr.write(`rename-cjs failed: ${e.stack ?? e}\n`);
  process.exit(1);
});
