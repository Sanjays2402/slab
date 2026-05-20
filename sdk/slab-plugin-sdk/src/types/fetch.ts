/**
 * @slab/plugin-sdk — `slab.fetch` types.
 *
 * Host-mediated HTTP from inside the plugin sandbox. The shape
 * deliberately mirrors the web Fetch API at the surface level — same
 * verbs, same Response shape, same "only network failures reject /
 * 4xx-5xx resolve with ok=false" semantics.
 *
 * Surface ground truth:
 *   - JS binding: `src-tauri/src/plugins/runtime/slab_global.rs:470-501`
 *   - Backend: `src-tauri/src/plugins/runtime/fetch.rs`
 *
 * **Capability gating:**
 *
 *   - `NetCap === "none"` → every call throws synchronously
 *   - `NetCap === "specific"` → host must be in `net_allow_hosts`
 *     (Throws sync on mismatch — this is a security gate, not a
 *     transient error.)
 *   - `NetCap === "any"` → all hosts permitted (use sparingly; the
 *     consent modal warns the user loudly about this choice).
 *
 * **Deliberate omissions** (NOT shipping in this surface):
 *
 *   - Streaming bodies — bodies always arrive as one string
 *   - `AbortSignal` — use `timeoutMs` instead
 *   - `credentials` — cookies disabled host-side
 *   - `cache` / `referrer` — no equivalents
 *
 * Plugins that need any of those should ship their own platform-
 * specific bindings (e.g. via the `slab.beacon.registerAiProvider`
 * route for LLM streaming, or a sidecar binary registered as a
 * `pdf_action`).
 */

/**
 * Init bag accepted by `slab.fetch(url, init?)`.
 */
export interface SlabFetchInit {
  /**
   * HTTP method. Defaults to `"GET"`. Method strings are
   * case-sensitive at the host boundary.
   */
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS";
  /** Request headers. Both keys and values must be strings. */
  headers?: Record<string, string>;
  /**
   * Request body. Use `null` or omit for body-less requests.
   * Strings are sent as-is; for binary upload, base64-encode and set
   * an appropriate `Content-Type` / `Content-Transfer-Encoding`.
   */
  body?: string | null;
  /**
   * Hard timeout in milliseconds. Defaults to 30_000. Clamped to
   * `[1, 120_000]` host-side; out-of-range values are silently
   * clamped (no rejection) — the host doc explains why.
   */
  timeoutMs?: number;
}

/**
 * Web-Fetch-shaped Response object resolved by `slab.fetch(...)`.
 *
 * Only network-layer failures (DNS, timeout, capability gate)
 * REJECT the Promise. Every HTTP response — including 4xx and 5xx —
 * RESOLVES; check `ok` (true iff `status in 200..=299`) to
 * differentiate.
 */
export interface SlabFetchResponse {
  /** `true` iff `status in 200..=299`. */
  ok: boolean;
  /** HTTP status code (e.g. 200, 404, 503). */
  status: number;
  /** Reason phrase ("OK", "Not Found"). May be empty. */
  statusText: string;
  /** Final URL after following any redirects (up to 10). */
  url: string;
  /** Response headers (case-insensitive keys preserved as-sent). */
  headers: Record<string, string>;
  /**
   * Response body decoded as UTF-8. Binary bodies arrive as
   * replacement-character-tolerated strings; for true binary data
   * use base64 + a custom encoding header.
   */
  body: string;
}

/**
 * The shape of `slab.fetch`. The function form (not an object with
 * a `.fetch` method) mirrors the web Fetch API ergonomics.
 */
export type SlabFetch = (
  url: string,
  init?: SlabFetchInit,
) => Promise<SlabFetchResponse>;
