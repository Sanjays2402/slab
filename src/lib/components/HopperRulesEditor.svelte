<script lang="ts">
  // HopperRulesEditor — v3.21.0 "Hopper Conditions" wow surface.
  //
  // For a given watch, lets the user write an ordered chain of routing
  // rules. Each rule says "IF this predicate matches, THEN route the
  // file through this recipe / output dir / rename pattern instead of
  // the watch defaults". First match wins; non-matches fall through.
  //
  // The wow is the right-hand live preview pane: shows up to five
  // candidate filenames (last log rows + a user-typed test filename)
  // and renders green/grey chips per rule indicating which would match.
  // Updates within ~150ms of any edit — the test runs entirely in Rust
  // against the in-flight, unsaved rule list, so users see "yes this
  // catches my tax PDFs" before clicking Save.
  //
  // Auto-save is debounced 600ms after the last edit; an explicit
  // "Saved ✓" toast confirms persistence.
  //
  // Persisted as JSON in `watches.rules_json`. See
  // `src-tauri/src/pdf/hopper/rules.rs` and `cmds.rs` for the backend.

  import { onMount } from "svelte";
  import {
    slabHopperGetRules,
    slabHopperSetRules,
    slabHopperTestRules,
    slabHopperListRuns,
    PREDICATE_KINDS,
    predicateLabel,
    emptyPredicate,
    emptyAction,
    formatPredicate,
    basename,
    type Rule,
    type RulePredicate,
    type RuleTestResult,
  } from "$lib/hopper";
  import HopperBackfillPanel from "./HopperBackfillPanel.svelte";

  // -------------------------------------------------------------------
  // Props
  // -------------------------------------------------------------------

  interface Props {
    watchId: number;
    watchSource: string;
    watchOutput: string;
    watchRecipeId: string | null;
  }

  let { watchId, watchSource, watchOutput, watchRecipeId }: Props = $props();

  // -------------------------------------------------------------------
  // State
  // -------------------------------------------------------------------

  let rules = $state<Rule[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let savedAt = $state<number | null>(null);
  let errorMsg = $state<string | null>(null);

  /** Files the preview pane evaluates against. Seeded from the
   *  watch's recent run log (so users see *their* files) plus a
   *  user-typed test filename for what-if scenarios. */
  let candidateFiles = $state<string[]>([]);
  let testFilename = $state("invoice_acme_2026-05-24.pdf");

  /** matchMatrix[fileIdx] = per-rule preview rows + final routing. */
  let matchMatrix = $state<
    {
      file: string;
      perRule: { matched: boolean }[];
      resolved: RuleTestResult;
    }[]
  >([]);
  let previewing = $state(false);

  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let previewTimer: ReturnType<typeof setTimeout> | null = null;

  /** v3.22.0 Hopper Loop — when true, the BackfillPanel overlay is
   *  mounted. Driven by the "Test on this folder" button below and by
   *  the public `openBackfill()` API used by the command palette. */
  let backfillOpen = $state(false);

  /** Public-ish entry point — used by the command palette /
   *  Cmd+Shift+B to open the backfill panel from anywhere. */
  export function openBackfill(): void {
    backfillOpen = true;
  }

  // -------------------------------------------------------------------
  // Load
  // -------------------------------------------------------------------

  onMount(async () => {
    try {
      rules = await slabHopperGetRules(watchId);
      // Seed preview files from this watch's recent runs.
      try {
        const recent = await slabHopperListRuns(20);
        const mine = recent
          .filter((r) => r.watch_id === watchId)
          .map((r) => basename(r.input_path));
        // De-dupe and cap at 4 (test filename is the 5th slot).
        candidateFiles = Array.from(new Set(mine)).slice(0, 4);
      } catch {
        candidateFiles = [];
      }
    } catch (e) {
      errorMsg = `Failed to load rules: ${String(e)}`;
    } finally {
      loading = false;
    }
    schedulePreview(0);
  });

  // -------------------------------------------------------------------
  // Derived: all filenames the preview should evaluate.
  // -------------------------------------------------------------------

  function allPreviewFiles(): string[] {
    const set = new Set<string>();
    for (const f of candidateFiles) if (f) set.add(f);
    if (testFilename.trim()) set.add(testFilename.trim());
    return Array.from(set).slice(0, 5);
  }

  // -------------------------------------------------------------------
  // Live preview — runs every rule against every file via the Rust
  // test endpoint (uses the in-flight unsaved `rules` array).
  // -------------------------------------------------------------------

  async function recomputePreview() {
    previewing = true;
    try {
      const files = allPreviewFiles();
      const next: typeof matchMatrix = [];
      for (const file of files) {
        // Per-rule individual evaluation: ask the server for each
        // single-rule chain, so we know which specific rule(s) matched.
        const perRule: { matched: boolean }[] = [];
        for (const r of rules) {
          const single = await slabHopperTestRules(watchId, file, {
            candidateRules: [r],
          });
          perRule.push({ matched: single.matched_index !== null });
        }
        // Then ask for the resolved routing (first-match-wins over full chain).
        const resolved = await slabHopperTestRules(watchId, file, {
          candidateRules: rules,
        });
        next.push({ file, perRule, resolved });
      }
      matchMatrix = next;
    } catch (e) {
      console.warn("hopper rules preview failed", e);
    } finally {
      previewing = false;
    }
  }

  function schedulePreview(delay = 150) {
    if (previewTimer) clearTimeout(previewTimer);
    previewTimer = setTimeout(recomputePreview, delay);
  }

  // -------------------------------------------------------------------
  // Save (debounced)
  // -------------------------------------------------------------------

  function scheduleSave() {
    schedulePreview();
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      saving = true;
      try {
        await slabHopperSetRules(watchId, rules);
        savedAt = Date.now();
      } catch (e) {
        errorMsg = `Save failed: ${String(e)}`;
      } finally {
        saving = false;
      }
    }, 600);
  }

  // -------------------------------------------------------------------
  // Rule mutations
  // -------------------------------------------------------------------

  function addRule() {
    rules = [
      ...rules,
      {
        name: `Rule ${rules.length + 1}`,
        predicate: { kind: "filename-glob", pattern: "*.pdf" },
        action: emptyAction(),
      },
    ];
    scheduleSave();
  }

  function removeRule(i: number) {
    rules = rules.filter((_, j) => j !== i);
    scheduleSave();
  }

  function moveUp(i: number) {
    if (i === 0) return;
    const next = rules.slice();
    [next[i - 1], next[i]] = [next[i], next[i - 1]];
    rules = next;
    scheduleSave();
  }

  function moveDown(i: number) {
    if (i >= rules.length - 1) return;
    const next = rules.slice();
    [next[i], next[i + 1]] = [next[i + 1], next[i]];
    rules = next;
    scheduleSave();
  }

  function setPredicateKind(i: number, kind: RulePredicate["kind"]) {
    rules[i].predicate = emptyPredicate(kind);
    rules = rules;
    scheduleSave();
  }

  function setNeedles(i: number, joined: string) {
    const p = rules[i].predicate;
    if (p.kind === "text-contains-all") {
      p.needles = joined
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      rules = rules;
      scheduleSave();
    }
  }

  // -------------------------------------------------------------------
  // Display helpers
  // -------------------------------------------------------------------

  function ruleNameAt(idx: number): string {
    return rules[idx]?.name?.trim() || `Rule ${idx + 1}`;
  }

  function savedLabel(): string {
    if (saving) return "Saving…";
    if (!savedAt) return "";
    const ago = Math.floor((Date.now() - savedAt) / 1000);
    if (ago < 2) return "Saved ✓";
    if (ago < 60) return `Saved ${ago}s ago`;
    return "Saved";
  }
</script>

<section class="rules-editor" data-tour="hopper-rules">
  <header class="head">
    <div>
      <h3>Routing rules</h3>
      <p class="sub">
        First match wins. Files that match no rule fall through to this
        watch's defaults
        {#if watchRecipeId}<span class="kbd">{watchRecipeId}</span>{/if}
        → <code>{basename(watchOutput) || watchOutput}</code>.
      </p>
    </div>
    <div class="head-actions">
      {#if savedLabel()}
        <span class="saved" class:err={errorMsg}>{savedLabel()}</span>
      {/if}
      <button
        class="ghost"
        onclick={() => (backfillOpen = true)}
        title="Apply these rules to PDFs already in this folder (Cmd+Shift+B)"
      >
        Test on this folder…
      </button>
      <button class="primary" onclick={addRule}>+ Add rule</button>
    </div>
  </header>

  {#if errorMsg}
    <div class="error">{errorMsg}</div>
  {/if}

  <div class="split">
    <!-- ============== LEFT: rule chain ============== -->
    <ol class="rules">
      {#if loading}
        <li class="empty"><span class="spinner" /> Loading rules…</li>
      {:else if rules.length === 0}
        <li class="empty">
          <div class="empty-icon">🎯</div>
          <div>
            <strong>No rules yet.</strong>
            All files in <code>{basename(watchSource) || watchSource}</code>
            run the watch defaults.
            <br />
            Add a rule to route, say, tax PDFs to a different folder.
          </div>
        </li>
      {:else}
        {#each rules as rule, i (i)}
          <li class="rule">
            <div class="rule-head">
              <div class="reorder">
                <button
                  class="rbtn"
                  disabled={i === 0}
                  onclick={() => moveUp(i)}
                  title="Move up">↑</button
                >
                <button
                  class="rbtn"
                  disabled={i === rules.length - 1}
                  onclick={() => moveDown(i)}
                  title="Move down">↓</button
                >
              </div>
              <span class="prio">#{i + 1}</span>
              <input
                class="name"
                bind:value={rule.name}
                oninput={scheduleSave}
                placeholder="Rule name"
              />
              <button
                class="del"
                onclick={() => removeRule(i)}
                title="Delete this rule">×</button
              >
            </div>

            <div class="rule-body">
              <div class="row">
                <label>IF</label>
                <select
                  value={rule.predicate.kind}
                  onchange={(e) =>
                    setPredicateKind(
                      i,
                      (e.currentTarget as HTMLSelectElement)
                        .value as RulePredicate["kind"],
                    )}
                >
                  {#each PREDICATE_KINDS as k (k)}
                    <option value={k}>{predicateLabel(k)}</option>
                  {/each}
                </select>

                {#if rule.predicate.kind === "filename-glob" || rule.predicate.kind === "filename-regex"}
                  <input
                    class="grow mono"
                    bind:value={rule.predicate.pattern}
                    oninput={scheduleSave}
                    placeholder={rule.predicate.kind === "filename-glob"
                      ? "tax_*.pdf"
                      : "receipt|invoice"}
                  />
                {:else if rule.predicate.kind === "text-contains-all"}
                  <input
                    class="grow mono"
                    value={rule.predicate.needles.join(", ")}
                    oninput={(e) =>
                      setNeedles(
                        i,
                        (e.currentTarget as HTMLInputElement).value,
                      )}
                    placeholder="invoice, due date, total"
                  />
                {:else if rule.predicate.kind === "page-count-between"}
                  <input
                    type="number"
                    min="1"
                    bind:value={rule.predicate.min}
                    oninput={scheduleSave}
                  />
                  <span class="dash">to</span>
                  <input
                    type="number"
                    min="1"
                    bind:value={rule.predicate.max}
                    oninput={scheduleSave}
                  />
                {:else if rule.predicate.kind === "size-over"}
                  <input
                    type="number"
                    min="0"
                    step="1000"
                    bind:value={rule.predicate.bytes}
                    oninput={scheduleSave}
                  />
                  <span class="dash">bytes</span>
                {/if}
              </div>

              <div class="row">
                <label>THEN</label>
                <input
                  class="grow"
                  bind:value={rule.action.recipe_id}
                  oninput={scheduleSave}
                  placeholder="recipe id (blank = inherit)"
                />
                <input
                  class="grow"
                  bind:value={rule.action.output_dir}
                  oninput={scheduleSave}
                  placeholder="→ output dir (blank = inherit)"
                />
                <input
                  class="grow mono"
                  bind:value={rule.action.rename_pattern}
                  oninput={scheduleSave}
                  placeholder="{`{date}_{ai_title}.pdf`}"
                />
              </div>
              <div class="summary">{formatPredicate(rule.predicate)}</div>
            </div>
          </li>
        {/each}
      {/if}
    </ol>

    <!-- ============== RIGHT: live preview ============== -->
    <aside class="preview">
      <div class="preview-head">
        <h4>Live preview</h4>
        {#if previewing}<span class="dot"></span>{/if}
      </div>
      <p class="hint">
        Watching the last few files in
        <code>{basename(watchSource) || watchSource}</code>. Edits
        re-evaluate instantly.
      </p>

      <label class="testlabel">
        What-if filename
        <input
          class="mono"
          bind:value={testFilename}
          oninput={() => schedulePreview()}
          placeholder="test_filename.pdf"
        />
      </label>

      {#if matchMatrix.length === 0}
        <div class="preview-empty">
          {rules.length === 0
            ? "Add a rule to see how it routes."
            : "No candidate files yet — type one above."}
        </div>
      {:else}
        <ul class="matrix">
          {#each matchMatrix as row (row.file)}
            <li class="mrow">
              <div class="mfile mono">{row.file}</div>
              <div class="chips">
                {#each row.perRule as cell, idx (idx)}
                  <span
                    class="chip"
                    class:on={cell.matched}
                    class:win={row.resolved.matched_index === idx}
                    title={cell.matched
                      ? `Matches: ${ruleNameAt(idx)}`
                      : `Skipped: ${ruleNameAt(idx)}`}
                  >
                    {cell.matched ? "✓" : "·"}
                    <span class="chip-label">{ruleNameAt(idx)}</span>
                  </span>
                {/each}
              </div>
              <div class="dest">
                <span class="arrow">→</span>
                {#if row.resolved.matched_rule}
                  <span class="badge win"
                    >{row.resolved.matched_rule}</span
                  >
                {:else}
                  <span class="badge default">default</span>
                {/if}
                <code class="outdir"
                  >{basename(row.resolved.output_dir) ||
                    row.resolved.output_dir}</code
                >
                {#if row.resolved.recipe_id}
                  <span class="badge recipe">{row.resolved.recipe_id}</span>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>
  </div>
</section>

{#if backfillOpen}
  <HopperBackfillPanel
    {watchId}
    watchSource={watchSource}
    onClose={() => (backfillOpen = false)}
  />
{/if}

<style>
  .rules-editor {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 14px;
    padding: 16px 18px;
    backdrop-filter: blur(24px) saturate(140%);
    margin: 12px 0;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 12px;
  }
  .head h3 {
    margin: 0;
    font-size: 14px;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--text-strong, #fff);
  }
  .sub {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-muted, #999);
  }
  .head-actions {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .saved {
    font-size: 11px;
    color: rgba(120, 230, 160, 0.9);
    font-variant-numeric: tabular-nums;
  }
  .saved.err {
    color: rgb(240, 120, 120);
  }
  .primary {
    background: rgba(110, 165, 255, 0.18);
    border: 1px solid rgba(110, 165, 255, 0.4);
    color: rgb(180, 210, 255);
    border-radius: 8px;
    padding: 6px 12px;
    font-size: 12px;
    cursor: pointer;
  }
  .primary:hover {
    background: rgba(110, 165, 255, 0.28);
  }
  .error {
    background: rgba(240, 80, 80, 0.12);
    border: 1px solid rgba(240, 80, 80, 0.3);
    color: rgb(255, 180, 180);
    padding: 6px 10px;
    border-radius: 8px;
    margin-bottom: 10px;
    font-size: 12px;
  }

  .split {
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(280px, 1fr);
    gap: 16px;
  }
  @media (max-width: 1100px) {
    .split {
      grid-template-columns: 1fr;
    }
  }

  .rules {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .empty {
    display: flex;
    gap: 12px;
    padding: 14px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px dashed rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    color: var(--text-muted, #aaa);
    font-size: 13px;
  }
  .empty-icon {
    font-size: 22px;
  }
  .empty code {
    color: rgb(180, 210, 255);
  }

  .rule {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 10px;
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rule-head {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .reorder {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .rbtn {
    background: none;
    border: none;
    color: var(--text-muted, #888);
    cursor: pointer;
    font-size: 10px;
    line-height: 1;
    padding: 1px 4px;
  }
  .rbtn:disabled {
    opacity: 0.2;
    cursor: default;
  }
  .prio {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: rgba(110, 165, 255, 0.8);
    width: 28px;
  }
  .name {
    flex: 1;
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--text-strong, #fff);
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 13px;
  }
  .del {
    background: none;
    border: none;
    color: rgba(240, 120, 120, 0.7);
    cursor: pointer;
    font-size: 18px;
    width: 22px;
    height: 22px;
    border-radius: 4px;
  }
  .del:hover {
    background: rgba(240, 80, 80, 0.15);
    color: rgb(255, 180, 180);
  }

  .rule-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px 0 2px 36px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
  .row label {
    font-family: ui-monospace, monospace;
    font-size: 10px;
    color: var(--text-muted, #888);
    width: 30px;
    letter-spacing: 0.05em;
  }
  .row input,
  .row select {
    background: rgba(0, 0, 0, 0.18);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--text-strong, #fff);
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 12px;
  }
  .row input[type="number"] {
    width: 86px;
  }
  .grow {
    flex: 1 1 120px;
    min-width: 0;
  }
  .mono {
    font-family: ui-monospace, "SF Mono", monospace;
  }
  .dash {
    color: var(--text-muted, #888);
    font-size: 11px;
  }
  .summary {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    color: rgba(180, 210, 255, 0.6);
    padding-left: 30px;
  }

  /* ---------- preview pane ---------- */
  .preview {
    background: rgba(0, 0, 0, 0.18);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 10px;
    padding: 12px;
    align-self: start;
  }
  .preview-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 4px;
  }
  .preview-head h4 {
    margin: 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-strong, #fff);
  }
  .preview .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: rgb(120, 230, 160);
    animation: pulse 1s ease-in-out infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 1;
    }
  }
  .hint {
    font-size: 11px;
    color: var(--text-muted, #999);
    margin: 0 0 10px;
  }
  .hint code {
    color: rgb(180, 210, 255);
    font-size: 11px;
  }
  .testlabel {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 10px;
    color: var(--text-muted, #888);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: 10px;
  }
  .testlabel input {
    background: rgba(0, 0, 0, 0.25);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: var(--text-strong, #fff);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 12px;
  }
  .preview-empty {
    color: var(--text-muted, #777);
    font-size: 12px;
    font-style: italic;
    padding: 10px 0;
  }
  .matrix {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .mrow {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.05);
    border-radius: 8px;
  }
  .mfile {
    font-size: 12px;
    color: rgb(220, 230, 240);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 10px;
    background: rgba(160, 160, 160, 0.12);
    color: var(--text-muted, #888);
    border: 1px solid rgba(160, 160, 160, 0.18);
    transition: all 0.15s ease;
  }
  .chip.on {
    background: rgba(80, 200, 120, 0.22);
    color: rgb(140, 240, 180);
    border-color: rgba(80, 200, 120, 0.4);
  }
  .chip.win {
    background: rgba(80, 200, 120, 0.42);
    color: #fff;
    border-color: rgb(120, 230, 160);
    box-shadow: 0 0 0 2px rgba(80, 200, 120, 0.15);
  }
  .chip-label {
    font-family: ui-monospace, monospace;
    font-size: 10px;
  }
  .dest {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    font-size: 11px;
  }
  .arrow {
    color: var(--text-muted, #888);
  }
  .badge {
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
  }
  .badge.win {
    background: rgba(80, 200, 120, 0.2);
    color: rgb(140, 240, 180);
  }
  .badge.default {
    background: rgba(160, 160, 160, 0.18);
    color: var(--text-muted, #aaa);
  }
  .badge.recipe {
    background: rgba(255, 200, 100, 0.18);
    color: rgb(255, 220, 150);
  }
  .outdir {
    color: rgb(180, 210, 255);
    font-size: 11px;
  }
  .kbd {
    font-family: ui-monospace, monospace;
    font-size: 11px;
    padding: 1px 5px;
    border-radius: 4px;
    background: rgba(255, 200, 100, 0.18);
    color: rgb(255, 220, 150);
    margin: 0 2px;
  }
  .spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(255, 255, 255, 0.2);
    border-top-color: rgb(180, 210, 255);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
