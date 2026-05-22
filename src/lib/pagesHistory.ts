// pagesHistory.ts — 50-deep undo/redo command stack for the Pages (Visual)
// panel. Implements the Arranger (v2.1.2, issue #26) acceptance criterion
// "every page operation is undoable with Cmd/Ctrl+Z, up to 50 deep".
//
// The store is intentionally tiny and framework-agnostic — the panel layers
// imperative side-effects (animation, persistence) on top via `push`.
//
// PageOp must stay byte-compatible with the Rust enum in
// `src-tauri/src/pdf/pages_undo.rs::PageOp` so a future tick can ship a
// `slab_apply_page_ops` command that takes the same shape across the IPC
// boundary verbatim.

import { writable, type Writable } from "svelte/store";

export type PageOp =
  | { kind: "delete"; at: number }
  | { kind: "duplicate"; from: number; to: number }
  | { kind: "rotate"; at: number; degrees: 90 | 180 | 270 }
  | { kind: "reorder"; order: number[] }
  | {
      kind: "insert_blank";
      at: number;
      count: number;
      width: number;
      height: number;
    }
  | { kind: "insert_pdf"; at: number; path: string }
  | { kind: "insert_image"; at: number; path: string };

export interface HistorySnapshot {
  ops: PageOp[];
  /** Index of the next un-applied slot; 0 means nothing applied yet. */
  cursor: number;
  canUndo: boolean;
  canRedo: boolean;
  /** Human label for the next undo target (e.g. "Rotate page"). */
  undoLabel: string | null;
  /** Human label for the next redo target. */
  redoLabel: string | null;
}

export interface PagesHistory {
  store: Writable<HistorySnapshot>;
  push(op: PageOp): void;
  undo(): PageOp | null;
  redo(): PageOp | null;
  snapshot(): HistorySnapshot;
  clear(): void;
  /** Currently-applied ops (the part of the stack before the cursor). */
  applied(): PageOp[];
}

export const DEFAULT_CAP = 50;

/** Human-readable label used in undo-stack tooltips on the frontend. */
export function labelOf(op: PageOp): string {
  switch (op.kind) {
    case "delete":
      return "Delete page";
    case "duplicate":
      return "Duplicate page";
    case "rotate":
      return "Rotate page";
    case "reorder":
      return "Reorder pages";
    case "insert_blank":
      return "Insert blank page";
    case "insert_pdf":
      return "Insert PDF";
    case "insert_image":
      return "Insert image";
  }
}

export function createPagesHistory(
  { cap = DEFAULT_CAP }: { cap?: number } = {},
): PagesHistory {
  let ops: PageOp[] = [];
  let cursor = 0;

  function snapshot(): HistorySnapshot {
    const undoLabel = cursor > 0 ? labelOf(ops[cursor - 1]) : null;
    const redoLabel = cursor < ops.length ? labelOf(ops[cursor]) : null;
    return {
      ops: ops.slice(),
      cursor,
      canUndo: cursor > 0,
      canRedo: cursor < ops.length,
      undoLabel,
      redoLabel,
    };
  }

  const store = writable<HistorySnapshot>(snapshot());
  function commit() {
    store.set(snapshot());
  }

  return {
    store,
    push(op) {
      // Truncate redo branch — pushing after an undo collapses the
      // future history (matches the Linear / VS Code / Figma model).
      ops = ops.slice(0, cursor);
      ops.push(op);
      // FIFO evict if over cap.
      if (ops.length > cap) {
        ops = ops.slice(ops.length - cap);
      }
      cursor = ops.length;
      commit();
    },
    undo() {
      if (cursor === 0) return null;
      cursor -= 1;
      const op = ops[cursor];
      commit();
      return op;
    },
    redo() {
      if (cursor >= ops.length) return null;
      const op = ops[cursor];
      cursor += 1;
      commit();
      return op;
    },
    snapshot,
    clear() {
      ops = [];
      cursor = 0;
      commit();
    },
    applied() {
      return ops.slice(0, cursor);
    },
  };
}
