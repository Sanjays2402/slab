<script lang="ts">
  // ClauseGroup — recursive Svelte component that renders one node of
  // the nested AND/OR/NOT smart-collection rule tree. v3.34.0 "Atlas Smart+".
  //
  // Each instance renders:
  //   • a combinator pill (AND ↔ OR)
  //   • one row per clause (with NOT toggle, value picker, ✕ delete)
  //   • nested <ClauseGroup> when a clause is itself a group
  //   • footer: + Rule / + Group buttons
  //
  // The whole tree is two-way bound back to the parent via $bindable so
  // the live-preview pane in SmartCollectionBuilder re-runs on every keystroke.

  import ClauseGroup from "./ClauseGroup.svelte";
  import type {
    FilterClause,
    FilterCombinator,
    FilterGroup,
    FolderRecord,
    TagRecord,
  } from "$lib/library";

  type Props = {
    group: FilterGroup;
    tags: TagRecord[];
    folders: FolderRecord[];
    /** Nesting depth — drives the left-border accent color and indent. */
    depth?: number;
    onChange: (g: FilterGroup) => void;
    onDelete?: () => void;
  };

  let { group, tags, folders, depth = 0, onChange, onDelete }: Props =
    $props();

  // ---------- Combinator toggle ----------
  function setCombinator(c: FilterCombinator) {
    onChange({ ...group, combinator: c });
  }

  // ---------- Add / remove clauses ----------
  function addRule() {
    const clause: FilterClause = { type: "title_contains", value: "" };
    onChange({ ...group, clauses: [...group.clauses, clause] });
  }

  function addGroup() {
    const inner: FilterClause = {
      type: "group",
      combinator: group.combinator === "and" ? "or" : "and",
      clauses: [],
    };
    onChange({ ...group, clauses: [...group.clauses, inner] });
  }

  function updateClauseAt(i: number, next: FilterClause) {
    const out = [...group.clauses];
    out[i] = next;
    onChange({ ...group, clauses: out });
  }

  function removeClauseAt(i: number) {
    onChange({ ...group, clauses: group.clauses.filter((_, j) => j !== i) });
  }

  // ---------- Per-clause type / NOT helpers ----------
  type ClauseKind = "tag" | "folder" | "title";
  function kindOf(c: FilterClause): ClauseKind {
    if (c.type === "tag" || c.type === "not_tag") return "tag";
    if (c.type === "folder" || c.type === "not_folder") return "folder";
    return "title";
  }
  function isNot(c: FilterClause): boolean {
    return (
      c.type === "not_tag" ||
      c.type === "not_folder" ||
      c.type === "title_not_contains"
    );
  }
  function toggleNot(c: FilterClause): FilterClause {
    switch (c.type) {
      case "tag":
        return { type: "not_tag", id: c.id };
      case "not_tag":
        return { type: "tag", id: c.id };
      case "folder":
        return { type: "not_folder", id: c.id };
      case "not_folder":
        return { type: "folder", id: c.id };
      case "title_contains":
        return { type: "title_not_contains", value: c.value };
      case "title_not_contains":
        return { type: "title_contains", value: c.value };
      default:
        return c;
    }
  }
  function switchKind(c: FilterClause, kind: ClauseKind): FilterClause {
    const negated = isNot(c);
    if (kind === "tag") {
      const id = tags[0]?.id ?? 0;
      return negated ? { type: "not_tag", id } : { type: "tag", id };
    }
    if (kind === "folder") {
      const id = folders[0]?.id ?? 0;
      return negated ? { type: "not_folder", id } : { type: "folder", id };
    }
    return negated
      ? { type: "title_not_contains", value: "" }
      : { type: "title_contains", value: "" };
  }

  function setClauseId(i: number, id: number) {
    const c = group.clauses[i];
    if (c.type === "tag" || c.type === "not_tag") {
      updateClauseAt(i, { ...c, id });
    } else if (c.type === "folder" || c.type === "not_folder") {
      updateClauseAt(i, { ...c, id });
    }
  }
  function setClauseValue(i: number, value: string) {
    const c = group.clauses[i];
    if (c.type === "title_contains" || c.type === "title_not_contains") {
      updateClauseAt(i, { ...c, value });
    }
  }

  // depth-based accent (cycles violet → sky → emerald → amber)
  const ACCENTS = ["#a78bfa", "#7cc4ff", "#34d399", "#fbbf24"];
  const accent = $derived(ACCENTS[depth % ACCENTS.length]);
</script>

<div class="group" style="--accent: {accent}">
  <header class="group-header">
    <div class="combinator">
      <button
        type="button"
        class="pill"
        class:active={group.combinator === "and"}
        onclick={() => setCombinator("and")}
        title="Match ALL conditions"
      >
        AND
      </button>
      <button
        type="button"
        class="pill or"
        class:active={group.combinator === "or"}
        onclick={() => setCombinator("or")}
        title="Match ANY condition"
      >
        OR
      </button>
    </div>
    <span class="hint">
      {group.combinator === "and" ? "Match every rule below" : "Match any rule below"}
    </span>
    {#if onDelete}
      <button
        type="button"
        class="delete-group"
        onclick={onDelete}
        title="Delete group"
        aria-label="Delete group"
      >
        ✕
      </button>
    {/if}
  </header>

  <div class="clauses">
    {#if group.clauses.length === 0}
      <div class="empty">
        No rules yet — click <strong>+ Rule</strong> or <strong>+ Group</strong> below.
      </div>
    {/if}

    {#each group.clauses as clause, i (i)}
      {#if clause.type === "group"}
        <div class="nested">
          <ClauseGroup
            group={{ combinator: clause.combinator, clauses: clause.clauses }}
            {tags}
            {folders}
            depth={depth + 1}
            onChange={(next) =>
              updateClauseAt(i, {
                type: "group",
                combinator: next.combinator,
                clauses: next.clauses,
              })}
            onDelete={() => removeClauseAt(i)}
          />
        </div>
      {:else}
        <div class="row" class:negated={isNot(clause)}>
          <button
            type="button"
            class="not-toggle"
            class:on={isNot(clause)}
            onclick={() => updateClauseAt(i, toggleNot(clause))}
            title={isNot(clause) ? "Remove NOT" : "Negate this rule"}
          >
            NOT
          </button>

          <select
            value={kindOf(clause)}
            onchange={(e) =>
              updateClauseAt(
                i,
                switchKind(clause, e.currentTarget.value as ClauseKind),
              )}
            aria-label="Rule type"
          >
            <option value="tag">tag</option>
            <option value="folder">folder</option>
            <option value="title">title</option>
          </select>

          <span class="op">
            {#if kindOf(clause) === "title"}
              {isNot(clause) ? "does not contain" : "contains"}
            {:else}
              {isNot(clause) ? "is not" : "is"}
            {/if}
          </span>

          {#if clause.type === "tag" || clause.type === "not_tag"}
            <select
              value={clause.id}
              onchange={(e) => setClauseId(i, Number(e.currentTarget.value))}
              aria-label="Tag"
            >
              {#each tags as t (t.id)}
                <option value={t.id}>{t.name}</option>
              {/each}
            </select>
          {:else if clause.type === "folder" || clause.type === "not_folder"}
            <select
              value={clause.id}
              onchange={(e) => setClauseId(i, Number(e.currentTarget.value))}
              aria-label="Folder"
            >
              {#each folders as f (f.id)}
                <option value={f.id}>{f.path}</option>
              {/each}
            </select>
          {:else if clause.type === "title_contains" || clause.type === "title_not_contains"}
            <input
              type="text"
              value={clause.value}
              placeholder="text…"
              oninput={(e) => setClauseValue(i, e.currentTarget.value)}
              aria-label="Title contains text"
            />
          {/if}

          <button
            type="button"
            class="delete-row"
            onclick={() => removeClauseAt(i)}
            title="Delete rule"
            aria-label="Delete rule"
          >
            ✕
          </button>
        </div>
      {/if}
    {/each}
  </div>

  <footer class="group-footer">
    <button type="button" class="add" onclick={addRule}>+ Rule</button>
    <button type="button" class="add" onclick={addGroup}>+ Group</button>
  </footer>
</div>

<style>
  .group {
    position: relative;
    border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
    border-left: 3px solid var(--accent);
    border-radius: 10px;
    padding: 10px 12px 12px 12px;
    background: color-mix(in srgb, var(--accent) 5%, transparent);
    backdrop-filter: blur(8px) saturate(140%);
    margin-bottom: 8px;
  }

  .group-header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }

  .combinator {
    display: inline-flex;
    border-radius: 999px;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .pill {
    border: 0;
    padding: 3px 10px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    background: transparent;
    color: color-mix(in srgb, var(--accent) 75%, #fff 25%);
    cursor: pointer;
    transition: background 120ms ease, color 120ms ease;
  }
  .pill.active {
    background: var(--accent);
    color: #0b0b14;
  }
  .pill.or.active {
    /* slight teal-shift for OR so AND vs OR is instantly readable */
    background: color-mix(in srgb, var(--accent) 70%, #34d399 30%);
  }
  .hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.55);
    flex: 1;
  }

  .delete-group,
  .delete-row {
    background: transparent;
    border: 0;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 6px;
    font-size: 12px;
    transition: color 120ms ease, background 120ms ease;
  }
  .delete-group:hover,
  .delete-row:hover {
    color: #fb7185;
    background: rgba(251, 113, 133, 0.12);
  }

  .clauses {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .empty {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.4);
    padding: 8px 4px;
    font-style: italic;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.06);
  }
  .row.negated {
    background: rgba(251, 113, 133, 0.08);
    border-color: rgba(251, 113, 133, 0.25);
  }
  .row.negated .op {
    color: #fb7185;
  }

  .not-toggle {
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: rgba(255, 255, 255, 0.4);
    border-radius: 6px;
    padding: 2px 6px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    cursor: pointer;
  }
  .not-toggle.on {
    background: #fb7185;
    border-color: #fb7185;
    color: #1a0a10;
  }

  select,
  input[type="text"] {
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: #fff;
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 12px;
    font-family: inherit;
  }
  input[type="text"] {
    flex: 1;
    min-width: 80px;
  }
  select:focus,
  input[type="text"]:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent);
  }

  .op {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.5);
    white-space: nowrap;
  }

  .nested {
    margin: 4px 0 4px 8px;
  }

  .group-footer {
    display: flex;
    gap: 8px;
    margin-top: 10px;
    padding-top: 8px;
    border-top: 1px dashed rgba(255, 255, 255, 0.08);
  }
  .add {
    background: rgba(255, 255, 255, 0.04);
    border: 1px dashed rgba(255, 255, 255, 0.18);
    color: rgba(255, 255, 255, 0.7);
    border-radius: 6px;
    padding: 4px 10px;
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: all 120ms ease;
  }
  .add:hover {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
