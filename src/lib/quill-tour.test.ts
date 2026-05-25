// Smoke tests for the v3.29.0 "Forms Tour" store. Same lightweight
// inline-expect style as `quill.test.ts` (no jest/vitest dep).
//
// Run with:
//   node --import tsx src/lib/quill-tour.test.ts
//
// We provide a tiny in-memory localStorage stub so the persistence path is
// covered even outside a browser. Without this, `loadCompleted()` returns
// `true` on SSR and the auto-start logic is never exercised.

// --- localStorage stub (must run BEFORE importing the module) ---------------
const store = new Map<string, string>();
const fakeLocalStorage = {
  getItem(k: string) {
    return store.has(k) ? (store.get(k) as string) : null;
  },
  setItem(k: string, v: string) {
    store.set(k, String(v));
  },
  removeItem(k: string) {
    store.delete(k);
  },
  clear() {
    store.clear();
  },
  key(_i: number) {
    return null;
  },
  get length() {
    return store.size;
  },
};
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(globalThis as any).window = (globalThis as any).window ?? {};
// eslint-disable-next-line @typescript-eslint/no-explicit-any
(globalThis as any).window.localStorage = fakeLocalStorage;

import {
  TOUR_STEPS,
  startTour,
  nextStep,
  prevStep,
  gotoStep,
  skipTour,
  finishTour,
  replayTour,
  shouldAutoStart,
  _resetForTests,
  _snapshot,
} from "./quill-tour";

function expect(cond: boolean, label: string): void {
  if (!cond) {
    // eslint-disable-next-line no-console
    console.error("FAIL:", label);
    if (typeof process !== "undefined") process.exitCode = 1;
  } else {
    // eslint-disable-next-line no-console
    console.log("ok  ", label);
  }
}

// --- starts dismissed and uncompleted ---------------------------------------
_resetForTests();
{
  const s = _snapshot();
  expect(s.step === null, "starts with no active step");
  expect(s.completed === false, "starts uncompleted after reset");
  expect(shouldAutoStart() === true, "first-time visitors auto-start");
}

// --- TOUR_STEPS shape -------------------------------------------------------
{
  expect(TOUR_STEPS.length === 5, "five tour steps shipped");
  for (let i = 0; i < TOUR_STEPS.length; i++) {
    const st = TOUR_STEPS[i];
    expect(typeof st.title === "string" && st.title.length > 0, `step ${i} has title`);
    expect(typeof st.body === "string" && st.body.length > 0, `step ${i} has body`);
    expect(st.body.length <= 220, `step ${i} body fits coachmark`);
    expect(
      ["top", "bottom", "left", "right", "center"].includes(st.placement),
      `step ${i} placement valid`,
    );
  }
}

// --- startTour + nextStep ---------------------------------------------------
_resetForTests();
{
  startTour();
  expect(_snapshot().step === 0, "startTour lands on step 0");
  nextStep();
  expect(_snapshot().step === 1, "nextStep advances");
  nextStep();
  nextStep();
  nextStep();
  expect(_snapshot().step === 4, "nextStep stops at last step before finish");
  nextStep(); // last → finish
  expect(_snapshot().step === null, "finish dismisses tour");
  expect(_snapshot().completed === true, "finish marks completed");
  expect(shouldAutoStart() === false, "completed → no auto-start next time");
}

// --- prevStep clamps at 0 ---------------------------------------------------
_resetForTests();
{
  startTour();
  prevStep();
  expect(_snapshot().step === 0, "prevStep clamps at 0");
  nextStep();
  prevStep();
  expect(_snapshot().step === 0, "prevStep walks back");
}

// --- gotoStep bounds --------------------------------------------------------
_resetForTests();
{
  startTour();
  gotoStep(3);
  expect(_snapshot().step === 3, "gotoStep jumps to index");
  gotoStep(99);
  expect(_snapshot().step === 3, "gotoStep ignores out-of-bounds");
  gotoStep(-1);
  expect(_snapshot().step === 3, "gotoStep ignores negative");
}

// --- skipTour ---------------------------------------------------------------
_resetForTests();
{
  startTour();
  nextStep();
  skipTour();
  expect(_snapshot().step === null, "skipTour dismisses");
  expect(_snapshot().completed === true, "skipTour marks completed");
}

// --- replayTour ignores completed flag --------------------------------------
_resetForTests();
{
  finishTour(); // user has seen it before
  expect(_snapshot().completed === true, "primed completed=true");
  replayTour();
  expect(_snapshot().step === 0, "replayTour opens even if completed");
  expect(_snapshot().completed === true, "replayTour does not un-complete");
}

// --- persistence across module state ----------------------------------------
_resetForTests();
{
  finishTour();
  // Simulate a page reload by clearing in-memory but keeping localStorage.
  // The exported `shouldAutoStart` reads from the live store so we can't
  // re-import easily — assert via the stored flag directly instead.
  expect(
    fakeLocalStorage.getItem("slab.quill.tour.completed.v1") === "1",
    "finishTour writes localStorage flag",
  );
}

// --- noop guards ------------------------------------------------------------
_resetForTests();
{
  // nextStep/prevStep when dismissed should be inert
  nextStep();
  expect(_snapshot().step === null, "nextStep is noop when dismissed");
  prevStep();
  expect(_snapshot().step === null, "prevStep is noop when dismissed");
}
