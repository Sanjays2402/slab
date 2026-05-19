//! Workshop (v2.0.0 Slice 7) — host-side fetch executor for `slab.fetch`.
//!
//! A single, process-wide [`reqwest::Client`] (rustls TLS, 30 s
//! default timeout, 10-redirect chain, cookies disabled because the
//! `cookies` reqwest feature isn't enabled) plus an async
//! [`do_fetch`] helper that turns a [`FetchRequest`] into a
//! [`FetchResponse`]. The actor's recv loop calls [`do_fetch`] via
//! `tokio::runtime::Handle::block_on` from a non-tokio worker
//! thread, so the request lifecycle is decoupled from the JS
//! interpreter's single-threaded execution model.
//!
//! ## Why a shared client?
//!
//! `reqwest::Client::builder().build()` is expensive — DNS resolver
//! setup, rustls config, connection pool. Per-request construction
//! would defeat connection pooling and TLS session reuse. We expose
//! a [`shared_client`] accessor that returns an `Arc<Client>` so
//! every fetch in the process shares one pool.
//!
//! ## Why NOT `once_cell::sync::Lazy`?
//!
//! Slab doesn't take `once_cell` as a direct dependency (and the
//! `Lazy` API isn't needed once `std::sync::OnceLock` stabilised in
//! Rust 1.70). `OnceLock` gives us identical semantics (build once,
//! cheap clones thereafter) without growing the dep graph.
//!
//! ## Why a worker-local pending-fetch map?
//!
//! `rquickjs::Persistent<Function>` holds a `*mut JSRuntime` and is
//! therefore `!Send`. We CANNOT send a `Persistent<Function>` across
//! the [`crate::plugins::runtime::actor::RuntimeCmd`] channel — the
//! channel's `RuntimeCmd` MUST stay `Send + Clone` because Tauri's
//! [`tauri::State`] container requires its contents be `Send`.
//!
//! Solution: stash the `(resolve, reject)` Persistent pair in a
//! worker-local [`PendingFetches`] map at the JS-binding callsite
//! (which already runs on the worker thread via `ctx.with`),
//! keyed by a monotonic `u64` request id. Send only the lightweight
//! `{ request_id, request }` payload over the channel. The recv
//! loop pops the entry back out by id and settles the Promise.
//!
//! ## Safety / robustness
//!
//! - Bodies are capped at [`MAX_BODY_BYTES`] (16 MiB). Both the
//!   `Content-Length` hint (fast deny before download) and the
//!   actual bytes-read length are checked.
//! - `file://` and other non-`http(s)` schemes are rejected by
//!   [`extract_host`] BEFORE the capability gate even runs — defence
//!   in depth against a plugin discovering a future hole in the gate.
//! - All error paths funnel through a stringified `Err(String)`. We
//!   intentionally drop the `reqwest::Error` chain because plugin
//!   authors don't need the source backtrace (and exposing it could
//!   leak internal host details). The top-level message is enough
//!   for diagnostics.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use rquickjs::{Ctx, Function, Object, Persistent, Result, Value};

use super::actor::{FetchRequest, FetchResponse};

/// Hard cap on response body size, in bytes. 16 MiB matches the
/// per-plugin heap cap so a single response can't OOM the actor's
/// QuickJS runtime. Plugins that need larger downloads should split
/// into ranged requests (or wait for Slice 7b streaming).
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// PendingFetches — worker-local resolve/reject callback registry.
// ---------------------------------------------------------------------------

/// The two `Persistent<Function>` callbacks for a single outstanding
/// `slab.fetch` Promise. `(resolve, reject)`.
pub type PendingCallbacks = (Persistent<Function<'static>>, Persistent<Function<'static>>);

/// Stash of outstanding fetch callbacks, keyed by request id.
///
/// **Always accessed from the worker thread** — the wrapper
/// [`SharedPendingFetches`] is only `Arc<Mutex<_>>`-shaped so the
/// JS-binding closure (which captures by clone) can share storage
/// with [`run_actor`]'s recv loop on the same thread. It is NEVER
/// sent across threads.
#[derive(Default)]
pub struct PendingFetches {
    by_id: HashMap<u64, PendingCallbacks>,
}

impl PendingFetches {
    /// Insert a `(resolve, reject)` pair. Returns the same id that
    /// was passed in — convenience for chained calls.
    pub fn insert(&mut self, id: u64, callbacks: PendingCallbacks) -> u64 {
        self.by_id.insert(id, callbacks);
        id
    }

    /// Remove and return the pair for `id`, if any. Called by the
    /// recv loop when settling the matching `RuntimeCmd::Fetch`.
    pub fn take(&mut self, id: u64) -> Option<PendingCallbacks> {
        self.by_id.remove(&id)
    }

    /// Number of outstanding fetches. Test/diagnostic surface only.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Whether the table is empty. Pair to [`Self::len`] — clippy
    /// flags any public `len` without a corresponding `is_empty`.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// Sharable handle around [`PendingFetches`]. See type-level doc on
/// `PendingFetches` for the threading rules.
pub type SharedPendingFetches = Arc<Mutex<PendingFetches>>;

/// Convenience helper for the actor's recv loop: lock the table
/// and pop the entry for `id`, swallowing any poison-error and
/// logging it. Kept as a free function (not a trait method) so
/// callers don't need to bring an extra trait into scope.
pub fn take_pending(shared: &SharedPendingFetches, id: u64) -> Option<PendingCallbacks> {
    match shared.lock() {
        Ok(mut g) => g.take(id),
        Err(_) => {
            eprintln!("[plugin fetch] pending-fetch mutex poisoned");
            None
        }
    }
}

/// Build a fresh empty [`SharedPendingFetches`]. The `Arc<Mutex<_>>`
/// shell allows the JS-binding closure (running on the same worker
/// thread) to keep its own clone of the handle.
//
// The `Mutex` wraps `!Send + !Sync` data (Persistents) but since
// the Mutex itself never crosses threads the warning is spurious —
// hence the allow attribute. Same pattern as `lifecycle::new_shared`.
#[allow(clippy::arc_with_non_send_sync)]
pub fn new_shared_pending() -> SharedPendingFetches {
    Arc::new(Mutex::new(PendingFetches::default()))
}

// ---------------------------------------------------------------------------
// Shared reqwest client.
// ---------------------------------------------------------------------------

/// Build the process-wide [`reqwest::Client`]. Called exactly once
/// per process by the [`OnceLock`] below.
fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Slab/", env!("CARGO_PKG_VERSION"), " (+plugins)"))
        // We allow http:// (not just https://) because plugin
        // authors will hit `http://localhost:NNNN` against local
        // dev tools (Ollama, local LLM proxies, etc). The capability
        // gate already enforces the host allow-list separately.
        .https_only(false)
        // 10 hops matches the web Fetch default and prevents
        // redirect loops from pinning the actor thread.
        .redirect(reqwest::redirect::Policy::limited(10))
        // Default 30 s for callers who omit `init.timeoutMs`. The
        // per-request override (set on the RequestBuilder below)
        // takes precedence.
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("reqwest client build (default config should never fail)")
    // NB: cookies are disabled implicitly — we don't enable the
    // `cookies` feature in Cargo.toml, so reqwest never builds a
    // cookie jar. Plugins must pass an explicit `Authorization`
    // header if they need auth; no ambient credentials are ever
    // forwarded.
}

/// Backing storage for the lazy singleton. `Arc<Client>` so callers
/// can take cheap clones without holding the `OnceLock` borrow.
static CLIENT: OnceLock<Arc<reqwest::Client>> = OnceLock::new();

/// Process-wide shared client. First call builds it; subsequent
/// calls are a single atomic load + `Arc::clone`.
pub fn shared_client() -> Arc<reqwest::Client> {
    Arc::clone(CLIENT.get_or_init(|| Arc::new(build_client())))
}

/// Parse `raw_url`, reject non-`http(s)` schemes, return the host
/// in lowercase ASCII. Used by the JS-side capability gate so the
/// allow-list check is always against the normalised authority.
///
/// We deliberately do NOT accept `ws://`, `file://`, `data:`,
/// `blob:`, etc. — plugins can shell-out via future capabilities
/// if they need non-HTTP transport.
///
/// Uses `reqwest::Url` (re-export of the `url` crate) so we don't
/// have to declare a direct dep on `url`.
pub fn extract_host(raw_url: &str) -> std::result::Result<String, String> {
    let parsed = reqwest::Url::parse(raw_url).map_err(|e| format!("invalid url: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!("scheme '{other}' not allowed (use http or https)"));
        }
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "url has no host".to_string())?;
    Ok(host.to_lowercase())
}

// ---------------------------------------------------------------------------
// do_fetch — actual HTTP execution.
// ---------------------------------------------------------------------------

/// Execute a `FetchRequest` against the shared client. Bounded by
/// `request.timeout_ms` and [`MAX_BODY_BYTES`]. Errors stringify
/// the `reqwest::Error` chain into a single human-readable message
/// suitable for surfacing as `(await fetch(...)).catch(e =>
/// e.message)`.
pub async fn do_fetch(request: FetchRequest) -> std::result::Result<FetchResponse, String> {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

    let client = shared_client();

    let method = reqwest::Method::from_bytes(request.method.as_bytes())
        .map_err(|e| format!("invalid method '{}': {e}", request.method))?;

    // Validate URL again at the host. Belt-and-braces — the JS
    // binding already called extract_host, but if a future caller
    // hands us a FetchRequest directly we want the same guarantees.
    // (Cheap: url parsing is microseconds.)
    let _host = extract_host(&request.url)?;

    let mut headers = HeaderMap::new();
    for (k, v) in &request.headers {
        let name = HeaderName::from_bytes(k.as_bytes())
            .map_err(|e| format!("invalid header name '{k}': {e}"))?;
        let val =
            HeaderValue::from_str(v).map_err(|e| format!("invalid header value for '{k}': {e}"))?;
        headers.insert(name, val);
    }

    let mut builder = client
        .request(method, &request.url)
        .timeout(Duration::from_millis(request.timeout_ms))
        .headers(headers);
    if let Some(body) = request.body {
        // Outbound body cap matches the inbound one. A hostile
        // plugin shouldn't be able to spam a server (or starve our
        // memory) with a giant upload.
        if body.len() > MAX_BODY_BYTES {
            return Err(format!(
                "request body too large: {} bytes > {MAX_BODY_BYTES} cap",
                body.len()
            ));
        }
        builder = builder.body(body);
    }

    let resp = builder.send().await.map_err(|e| format!("network: {e}"))?;
    let status = resp.status();
    let status_text = status.canonical_reason().unwrap_or("").to_string();
    let final_url = resp.url().to_string();
    let mut resp_headers: Vec<(String, String)> = Vec::with_capacity(resp.headers().len());
    for (k, v) in resp.headers() {
        // Lowercase header names so plugins can `resp.headers["x-foo"]`
        // without worrying about case (matches web Fetch's Headers
        // API behaviour). Skip non-UTF-8 values — they're virtually
        // never seen in practice and `to_str()` errors hard otherwise.
        let v_str = v.to_str().unwrap_or("").to_string();
        resp_headers.push((k.as_str().to_lowercase(), v_str));
    }

    // Pre-flight: if Content-Length is present and over cap, reject
    // BEFORE the body streams in. Saves bandwidth + RAM.
    if let Some(cl) = resp.content_length() {
        if cl as usize > MAX_BODY_BYTES {
            return Err(format!(
                "response body too large: {cl} bytes > {MAX_BODY_BYTES} cap"
            ));
        }
    }
    let body_bytes = resp.bytes().await.map_err(|e| format!("body: {e}"))?;
    if body_bytes.len() > MAX_BODY_BYTES {
        return Err(format!(
            "response body too large: {} bytes > {MAX_BODY_BYTES} cap",
            body_bytes.len()
        ));
    }

    Ok(FetchResponse {
        status: status.as_u16(),
        status_text,
        url: final_url,
        headers: resp_headers,
        body: body_bytes.to_vec(),
        ok: status.is_success(),
    })
}

// ---------------------------------------------------------------------------
// response_to_js — build the JS Response-like object.
// ---------------------------------------------------------------------------

/// Translate a [`FetchResponse`] into a JS plain object matching
/// the minimal web-Fetch `Response` surface that plugins expect:
///
/// ```js
/// {
///   status: 200,
///   statusText: "OK",
///   url: "https://api.example.com/v1/x",
///   ok: true,
///   headers: { "content-type": "application/json", ... },
///   text(): string,
///   json(): any,
/// }
/// ```
///
/// `text()` and `json()` are eager (synchronous) by design: the
/// body is already fully buffered host-side, so there's no point
/// pretending it's async. Plugin authors who write `await
/// resp.text()` get the right answer because `await` on a
/// non-Promise resolves to the value as-is.
pub fn response_to_js<'js>(ctx: &Ctx<'js>, resp: &FetchResponse) -> Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("status", resp.status)?;
    obj.set("statusText", resp.status_text.clone())?;
    obj.set("url", resp.url.clone())?;
    obj.set("ok", resp.ok)?;

    // Headers as a plain object. We accept that this means
    // duplicate-name headers collapse to the last value — that
    // matches what 99% of plugin code expects and avoids forcing
    // every caller to handle the array form. Authors who genuinely
    // need set-cookie etc. can wait for the Headers proxy in 7b.
    let headers = Object::new(ctx.clone())?;
    for (k, v) in &resp.headers {
        headers.set(k.as_str(), v.clone())?;
    }
    obj.set("headers", headers)?;

    // text() — UTF-8 lossy decode of the body bytes. Cheap clone
    // of the Vec into the closure so the function can be called
    // multiple times (the web Fetch API technically forbids
    // re-reading; we're lenient).
    let body_for_text = resp.body.clone();
    let text_fn = Function::new(ctx.clone(), move || -> String {
        String::from_utf8_lossy(&body_for_text).into_owned()
    })?;
    obj.set("text", text_fn)?;

    // json() — UTF-8 decode + JSON.parse via the QuickJS globals.
    // We route through the JS-side `JSON.parse` rather than
    // serde_json + rquickjs::IntoJs because the former gives us
    // identical semantics to what the plugin would get from
    // `JSON.parse(await resp.text())` and handles oddities like
    // `Infinity` consistently.
    //
    // Lifetime note: we bind the closure to the outer `'js`
    // explicitly because rquickjs's `IntoJsFunc` impl works for
    // any single closure-lifetime, and using two independent `_`s
    // (one on Ctx, one on Value) confuses inference into demanding
    // `'js: 'static`.
    let body_for_json = resp.body.clone();
    let json_fn = Function::new(ctx.clone(), move |ctx: Ctx<'js>| -> Result<Value<'js>> {
        let s = match std::str::from_utf8(&body_for_json) {
            Ok(s) => s,
            Err(_) => {
                // Mirror what the browser does: invalid UTF-8 in
                // the body throws a SyntaxError when json() is
                // called. We surface it via `Exception::throw_type`
                // which becomes a JS `TypeError`; close enough
                // for a sandboxed v1.
                return Err(rquickjs::Exception::throw_type(
                    &ctx,
                    "fetch response body is not valid UTF-8",
                ));
            }
        };
        let globals = ctx.globals();
        let json: Object = globals
            .get("JSON")
            .map_err(|_| rquickjs::Exception::throw_internal(&ctx, "JSON global missing"))?;
        let parse: Function = json
            .get("parse")
            .map_err(|_| rquickjs::Exception::throw_internal(&ctx, "JSON.parse missing"))?;
        parse.call((s.to_string(),))
    })?;
    obj.set("json", json_fn)?;

    Ok(obj.into_value())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- shared_client -----------------------------------------------------

    #[test]
    fn shared_client_is_reused_across_calls() {
        let a = shared_client();
        let b = shared_client();
        // Same Arc pointer => same underlying Client.
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn shared_client_builds_without_panic() {
        let c = shared_client();
        // Construct a request builder against an arbitrary URL.
        // We never `send()` it — this is purely a smoke test
        // confirming the client's builder paths work.
        let _req = c.get("https://example.com/");
    }

    // ---- extract_host ------------------------------------------------------

    #[test]
    fn extract_host_returns_lowercase_host_for_https() {
        assert_eq!(
            extract_host("https://API.EXAMPLE.com/v1/x").unwrap(),
            "api.example.com"
        );
    }

    #[test]
    fn extract_host_returns_lowercase_host_for_http() {
        assert_eq!(
            extract_host("http://Cake.LOCAL/info").unwrap(),
            "cake.local"
        );
    }

    #[test]
    fn extract_host_rejects_file_scheme() {
        let e = extract_host("file:///etc/passwd").unwrap_err();
        assert!(e.contains("scheme"), "got: {e}");
        assert!(e.contains("file"), "got: {e}");
    }

    #[test]
    fn extract_host_rejects_ws_scheme() {
        let e = extract_host("ws://example.com/socket").unwrap_err();
        assert!(e.contains("ws"), "got: {e}");
    }

    #[test]
    fn extract_host_rejects_data_scheme() {
        let e = extract_host("data:text/plain,hello").unwrap_err();
        // url crate may classify data: as without a host; either
        // path is a rejection.
        assert!(!e.is_empty());
    }

    #[test]
    fn extract_host_rejects_malformed_url() {
        let e = extract_host("not a url").unwrap_err();
        assert!(e.contains("invalid url"), "got: {e}");
    }

    #[test]
    fn extract_host_accepts_localhost_http_for_dev() {
        assert_eq!(
            extract_host("http://localhost:5173/api").unwrap(),
            "localhost"
        );
    }

    #[test]
    fn extract_host_preserves_ipv6_brackets_stripped() {
        // url::Url normalises IPv6 hosts to bracketed form for the
        // display; host_str() returns the raw address. We just want
        // to confirm we don't crash.
        let h = extract_host("http://[::1]:8080/").unwrap();
        assert!(h.contains("::1") || h.contains(":"), "got: {h}");
    }

    #[test]
    fn extract_host_normalises_ipv4_authority() {
        assert_eq!(
            extract_host("https://127.0.0.1:3000/v1").unwrap(),
            "127.0.0.1"
        );
    }

    // ---- PendingFetches ----------------------------------------------------

    #[test]
    fn pending_fetches_default_is_empty() {
        let p = PendingFetches::default();
        assert_eq!(p.len(), 0);
    }

    #[test]
    fn pending_fetches_take_missing_returns_none() {
        let mut p = PendingFetches::default();
        assert!(p.take(42).is_none());
    }

    #[test]
    fn new_shared_pending_returns_independent_arcs() {
        let a = new_shared_pending();
        let b = new_shared_pending();
        assert!(!Arc::ptr_eq(&a, &b));
        assert_eq!(a.lock().unwrap().len(), 0);
        assert_eq!(b.lock().unwrap().len(), 0);
    }

    #[test]
    fn shared_pending_take_returns_none_on_empty() {
        let p = new_shared_pending();
        assert!(take_pending(&p, 1).is_none());
    }

    // ---- do_fetch input validation (offline) -------------------------------
    //
    // Tests that actually go over the wire live in `fetch_tests.rs`
    // (Task 7.6, mockito-backed). These tests stay offline and pure.

    #[tokio::test]
    async fn do_fetch_rejects_bad_method() {
        let req = FetchRequest {
            method: "WHAT IS THIS".to_string(),
            url: "http://localhost:1/x".to_string(),
            headers: vec![],
            body: None,
            timeout_ms: 100,
        };
        let err = do_fetch(req).await.unwrap_err();
        assert!(err.contains("invalid method"), "got: {err}");
    }

    #[tokio::test]
    async fn do_fetch_rejects_file_scheme_url() {
        let req = FetchRequest {
            method: "GET".to_string(),
            url: "file:///etc/passwd".to_string(),
            headers: vec![],
            body: None,
            timeout_ms: 100,
        };
        let err = do_fetch(req).await.unwrap_err();
        assert!(err.contains("scheme"), "got: {err}");
    }

    #[tokio::test]
    async fn do_fetch_rejects_oversized_outbound_body() {
        // Build a 17 MiB Vec; do_fetch should bail before issuing
        // the request. We use `vec![0u8; ...]` rather than a real
        // allocation pattern because the test only validates the
        // length check, not real allocation.
        let body = vec![0u8; MAX_BODY_BYTES + 1];
        let req = FetchRequest {
            method: "POST".to_string(),
            url: "http://localhost:1/x".to_string(),
            headers: vec![],
            body: Some(body),
            timeout_ms: 100,
        };
        let err = do_fetch(req).await.unwrap_err();
        assert!(err.contains("body too large"), "got: {err}");
    }

    // ---- Constants sanity --------------------------------------------------

    #[test]
    fn max_body_bytes_is_16_mib() {
        assert_eq!(MAX_BODY_BYTES, 16 * 1024 * 1024);
    }
}
