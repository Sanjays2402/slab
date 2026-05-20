/**
 * @slab/plugin-sdk — `slab.document.*` types.
 *
 * The `slab.document` surface exposes the lifecycle of the active
 * PDF in the reader. Plugins can:
 *
 *   - Subscribe to open/close events via `onOpen` / `onClose`
 *   - Snapshot the current active document via `getActive`
 *
 * Surface ground truth:
 *   - JS shape: `src-tauri/src/plugins/runtime/slab_global.rs:142-157`
 *   - Actor body: `src-tauri/src/plugins/runtime/actor.rs` (Slice 6.5)
 *
 * The handlers fire from the actor's event loop. Handlers run inside
 * the plugin's QuickJS runtime — no host-thread blocking, no shared
 * mutable state. Throwing inside a handler is logged host-side but
 * doesn't poison the actor: subsequent events still dispatch.
 *
 * **Lifecycle quirks worth knowing:**
 *
 *   - `getActive()` returns the doc that's open at the moment of
 *     the call. Inside an `onOpen` handler it returns the doc that
 *     just opened (host sets it BEFORE dispatch). Inside an
 *     `onClose` handler it returns `null` (host clears it BEFORE
 *     dispatch). This is intuitive but worth pinning.
 *   - `onOpen` and `onClose` are only available inside the long-lived
 *     per-plugin runtime (i.e. after the plugin is enabled). They
 *     throw a clear error if called from ephemeral evaluation paths.
 */

/**
 * Snapshot of the active PDF in the reader, or `null` when no doc
 * is open.
 */
export interface ActiveDocument {
  /** Absolute filesystem path to the PDF on disk. */
  path: string;
  /**
   * Pretty name derived from the file stem. Roughly:
   *   - `/x/y/Report.pdf` → `"Report"`
   *   - `/x/archive.tar.gz` → `"archive.tar"`
   *   - `/x/.bashrc` → `".bashrc"` (dotfiles keep their dot)
   *   - `/` → `""`
   */
  name: string;
}

/**
 * Document event handler signature. Sync or async. Async handlers'
 * promises are awaited by the actor; subsequent events still fire
 * concurrently because the actor uses a fresh interrupt deadline per
 * batch.
 */
export type DocumentHandler = (
  doc: ActiveDocument,
) => void | Promise<void>;

/**
 * The shape of `slab.document`. All methods are always present on
 * the surface; the inner behaviour varies based on the runtime
 * context (long-lived vs ephemeral).
 */
export interface DocumentSurface {
  /**
   * Register a handler fired whenever the user opens a PDF.
   *
   * The handler runs inside the per-plugin runtime; the host
   * snapshots `active_doc` BEFORE dispatch so `getActive()` inside
   * the handler returns the doc that just opened.
   *
   * Throws if called outside the per-plugin runtime (e.g. from an
   * `execute_script` ephemeral evaluation).
   */
  onOpen(handler: DocumentHandler): void;
  /**
   * Register a handler fired whenever the active PDF closes (also
   * fired on tab close + app exit). Inside the handler `getActive()`
   * returns `null` because the host clears `active_doc` BEFORE
   * dispatch.
   *
   * Same throw-on-ephemeral behaviour as `onOpen`.
   */
  onClose(handler: DocumentHandler): void;
  /**
   * Snapshot the active document right now, or `null` if no PDF
   * is open. Safe to call from any context (returns `null` on
   * ephemeral paths instead of throwing).
   */
  getActive(): ActiveDocument | null;
}
