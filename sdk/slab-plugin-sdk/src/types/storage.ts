/**
 * @slab/plugin-sdk — `slab.storage.*` types.
 *
 * Per-plugin sqlite-backed key/value store. Plugins use this for
 * persistent state (counters, settings, cached responses, etc.).
 *
 * Surface ground truth:
 *   - JS shape: `src-tauri/src/plugins/runtime/slab_global.rs:159-175`
 *   - Backing store: `src-tauri/src/plugins/storage.rs`
 *
 * **Security model — read this twice:**
 *
 * Per-plugin scoping is enforced *in SQL* — every WHERE clause pins
 * `plugin_id = ?`. There is NO declared-capability gate on this
 * surface (intentional design choice — see the module-level doc on
 * `plugins::storage` in src-tauri for the rationale). Your plugin
 * literally cannot see or modify other plugins' keys; do not write
 * code that "tries to be careful" about scoping, because scoping is
 * already the security boundary.
 *
 * **Quotas (compile-time enforced, hard limits):**
 *
 *   - 8 MiB total per plugin (`MAX_PLUGIN_BYTES`)
 *   - 1 MiB per value (`MAX_VALUE_BYTES`)
 *   - 64 KiB per key (`MAX_KEY_BYTES`)
 *
 * Exceeding any of them rejects the Promise with a clear error
 * message naming the limit. Overwriting an existing key correctly
 * accounts for the previous value's bytes (no double-counting).
 *
 * **Why Promises for sync ops?**
 *
 * Sqlite KV is microseconds — the host could expose a sync surface.
 * The async wrapper is deliberate: it leaves room to swap the
 * backing store for an actual async one (network KV, IndexedDB-via-
 * webview, etc.) without breaking the public JS contract.
 */

/**
 * Result of `slab.storage.usage()` — current quota consumption for
 * this plugin's slice of the store.
 */
export interface StorageUsage {
  /** Total bytes used across all values (excluding keys + overhead). */
  bytes: number;
  /** Number of keys currently stored. */
  keys: number;
}

/**
 * The shape of `slab.storage`. Every method is per-plugin scoped
 * automatically; calls from one plugin never see another's data.
 */
export interface StorageSurface {
  /**
   * Look up `key`. Resolves with the stored string or `null` if the
   * key is absent. Distinguish "missing" from "stored empty string"
   * by inspecting the resolved value.
   */
  get(key: string): Promise<string | null>;
  /**
   * Store `value` under `key`. Overwrites any prior value.
   *
   * Rejects with:
   *   - `"slab.storage.set: KeyTooLong(...)"` — key > 64 KiB
   *   - `"slab.storage.set: ValueTooLarge(...)"` — value > 1 MiB
   *   - `"slab.storage.set: QuotaExceeded(...)"` — total > 8 MiB
   */
  set(key: string, value: string): Promise<void>;
  /**
   * Delete `key`. Resolves with `true` if a row existed and was
   * removed, `false` if no such key. Idempotent.
   */
  remove(key: string): Promise<boolean>;
  /**
   * List every key this plugin currently owns, sorted
   * lexicographically. Resolves with an empty array if nothing is
   * stored.
   */
  list(): Promise<string[]>;
  /**
   * Drop every key this plugin currently owns. Resolves once the
   * delete is committed. No-op if the slice is already empty.
   */
  clear(): Promise<void>;
  /**
   * Report current quota consumption. See `StorageUsage`.
   */
  usage(): Promise<StorageUsage>;
}
