// Pure-helper tests for the update check.
//
// Run with:
//   node --import tsx src/lib/updates.test.ts

import { autoCheckDue, isSafeReleaseUrl, AUTO_CHECK_INTERVAL_MS } from "./updates";

let failed = 0;
function check(name: string, ok: boolean) {
  if (!ok) { failed++; console.log(`FAIL: ${name}`); } else console.log(`ok: ${name}`);
}

const now = 1_800_000_000_000;
check("due when never checked", autoCheckDue(now, null));
check("not due right after a check", !autoCheckDue(now, now - 1000));
check("not due just under 24h", !autoCheckDue(now, now - AUTO_CHECK_INTERVAL_MS + 1));
check("due at 24h", autoCheckDue(now, now - AUTO_CHECK_INTERVAL_MS));
check("due when clock went backwards", autoCheckDue(now, now + 5000));
check("due when last is NaN", autoCheckDue(now, Number.NaN));

check("accepts Slab release URL", isSafeReleaseUrl("https://github.com/Sanjays2402/slab/releases/tag/v3.41.1"));
check("rejects other host", !isSafeReleaseUrl("https://evil.example/Sanjays2402/slab/"));
check("rejects sibling repo", !isSafeReleaseUrl("https://github.com/Sanjays2402/slab-evil/x"));
check("rejects http", !isSafeReleaseUrl("http://github.com/Sanjays2402/slab/x"));

if (failed) { console.log(`${failed} failed`); process.exit(1); }
