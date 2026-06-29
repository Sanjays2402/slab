// Pure-helper tests for the Reader thumbnail-rail hover-zoom view-core.
//
// Style matches beaconCacheView.test.ts — no test runner, an inline
// `expect`. Run with:  tsx src/lib/readerThumbView.test.ts

import { clampFlyoutTop, shouldShowPreview, previewLabel } from "./readerThumbView";

let passed = 0;
let failed = 0;
function expect(cond: boolean, label: string) {
  if (cond) {
    passed++;
    // eslint-disable-next-line no-console
    console.log(`ok: ${label}`);
  } else {
    failed++;
    // eslint-disable-next-line no-console
    console.error(`FAIL: ${label}`);
  }
}

// --- clampFlyoutTop ---------------------------------------------------
{
  // Centred thumb mid-viewport: flyout centres on it.
  expect(clampFlyoutTop({ top: 400, height: 100 }, 200, 1000) === 350, "centre: 450 - 100");
  // Thumb near top: clamps to pad.
  expect(clampFlyoutTop({ top: 0, height: 100 }, 200, 1000) === 8, "top edge clamps to pad");
  // Thumb near bottom: clamps so bottom stays in.
  expect(clampFlyoutTop({ top: 960, height: 40 }, 200, 1000) === 792, "bottom clamps (1000-200-8)");
  // Flyout taller than viewport pins to pad.
  expect(clampFlyoutTop({ top: 400, height: 100 }, 2000, 1000) === 8, "oversized pins to top");
  // Custom pad respected.
  expect(clampFlyoutTop({ top: 0, height: 50 }, 100, 1000, 20) === 20, "custom pad");
  // Garbage -> pad.
  expect(clampFlyoutTop(null, 200, 1000) === 8, "null rect -> pad");
  expect(clampFlyoutTop({ top: NaN, height: 1 }, 200, 1000) === 8, "NaN rect -> pad");
}

// --- shouldShowPreview ------------------------------------------------
{
  expect(shouldShowPreview(3, 10, true) === true, "preview: in range, open, multi");
  expect(shouldShowPreview(3, 10, false) === false, "preview: closed rail -> no");
  expect(shouldShowPreview(1, 1, true) === false, "preview: single page -> no");
  expect(shouldShowPreview(0, 10, true) === false, "preview: page < 1 no");
  expect(shouldShowPreview(11, 10, true) === false, "preview: page > count no");
}

// --- previewLabel -----------------------------------------------------
{
  expect(previewLabel(4, 96) === "Page 4 of 96", "label: basic");
  expect(previewLabel(200, 96) === "Page 96 of 96", "label: clamps high");
  expect(previewLabel(0, 96) === "Page 1 of 96", "label: clamps low");
  expect(previewLabel(1, 0) === "", "label: no count -> empty");
}

// eslint-disable-next-line no-console
console.log(`\n${passed} passed, ${failed} failed`);
