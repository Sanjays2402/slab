import { readdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { join } from "node:path";

function discover(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? discover(path) : entry.name.endsWith(".test.ts") ? [path] : [];
  });
}
const files = discover("src/lib").sort();
if (!files.length) throw new Error("No unit test files found.");
let failures = 0;
for (const file of files) {
  const result = spawnSync(process.execPath, ["--import", "tsx", file], {
    encoding: "utf8", timeout: 30000,
    env: { ...process.env, TSX_TSCONFIG_PATH: "tsconfig.tests.json" },
  });
  const output = (result.stdout || "") + (result.stderr || "");
  // Several legacy assertion helpers log FAIL without setting an exit code.
  // Treat those assertions as failures too, rather than accepting a false pass.
  const failed = result.status !== 0 || /^\s*FAIL[: ]/m.test(output);
  console.log(`${failed ? "FAIL" : "PASS"} ${file}`);
  if (failed) { failures++; console.error(output, result.error || ""); }
}
console.log(`${files.length - failures}/${files.length} unit test files passed.`);
process.exitCode = failures ? 1 : 0;
