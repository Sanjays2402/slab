//! Marketplace index data model.
//!
//! The marketplace index (`index.json`) is a curated list of plugins
//! published by the Slab maintainer. Each entry carries enough metadata
//! to render a card in the Browse tab, plus an Ed25519 signature over
//! the entry minus its `signature` field — verified in
//! [`crate::marketplace::verify`].
//!
//! This module is pure data — no I/O, no networking. HTTP fetching
//! lives in `fetch.rs` (Slice 3), the install pipeline in
//! `install.rs` (Slice 4).

use serde::{Deserialize, Serialize};

/// Top-level shape of `index.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Index {
    /// Bumped by maintainers when fields are added/removed. Slab
    /// refuses to load indices with a higher schema_version than it
    /// knows about, so old clients fail loudly instead of silently
    /// ignoring new fields.
    pub schema_version: u32,
    /// Identifies which baked-in public key to verify against. Slab
    /// currently knows about `"slab-maintainer-2026"`. Future key
    /// rotation adds new IDs without breaking old indices.
    pub signing_key_id: String,
    /// The plugins.
    pub plugins: Vec<IndexEntry>,
}

/// One entry in the marketplace index. The `signature` field is
/// Ed25519 over the canonical JSON of *everything else* in this
/// struct (i.e. the entry with `signature` removed). Canonicalization:
/// `serde_json::to_vec` of [`IndexEntryUnsigned`], which has a stable
/// field order matching the struct declaration order.
///
/// ## v2 discovery metadata (Workshop Marketplace, 2026-05-20)
///
/// `categories`, `tags`, `screenshots`, and `installs` were added in
/// `schema_version: 2`. All four are optional on the wire — they
/// serialize with `#[serde(default, skip_serializing_if = ...)]` so a
/// v2 entry with all four left empty produces bytes byte-identical to
/// a v1 entry. That preserves backward compatibility with v1-signed
/// indices: a signature produced against a v1 payload still verifies
/// against the same entry deserialized as v2 (because the canonical
/// payload — [`IndexEntryUnsigned`] — drops empty v2 fields too).
///
/// The plan originally proposed wrapping `IndexEntry` in a
/// `IndexEntryV2 { #[serde(flatten)] v1, .. }` struct. We chose direct
/// extension instead because (1) `skip_serializing_if` already gives us
/// wire-level backward compat, (2) the wrapper would force a giant
/// rename cascade through fetch.rs / install.rs / lib.rs / TS, and (3)
/// downstream code keeps a single canonical type to reason about.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    /// HTTPS URL of the plugin tarball (gzipped, .tar.gz).
    pub download_url: String,
    /// Hex-encoded SHA-256 of the tarball bytes. Cross-checked
    /// during install (Slice 4).
    pub sha256: String,
    /// Tarball size in bytes. Slab refuses to download anything
    /// larger than [`MAX_TARBALL_BYTES`] to avoid runaway pulls.
    pub size_bytes: u64,
    /// `>= 1.4.0`-style SemVer requirement against the Slab host
    /// version. (Mirrors the in-tree plugin manifest's `slab_compat`.)
    pub slab_compat: String,
    /// Base64-encoded Ed25519 signature over the entry minus this
    /// field. See [`IndexEntryUnsigned`].
    pub signature: String,
    /// **v2** — Curated category slugs (e.g. `"productivity"`,
    /// `"ai"`, `"developer"`). Powers the Browse-tab chip filters.
    /// Plain strings — the host doesn't enforce a closed enum; the
    /// maintainer signs whatever they want, the UI shows the union,
    /// the user filters. Empty by default; omitted from the wire form
    /// when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// **v2** — Free-form tags for full-text search. Lowercase,
    /// dash-separated by convention but unenforced. Empty by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// **v2** — HTTPS URLs to PNG/WebP/JPG screenshots. The Browse-tab
    /// detail drawer renders a carousel. Empty by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,
    /// **v2** — Approximate install count, snapshotted by the
    /// maintainer at index-publishing time. Not real-time; not a
    /// leaderboard — social proof only. Defaults to 0 (and is omitted
    /// from the wire form when 0).
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub installs: u64,
}

fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}

/// Canonical signing payload — the entry with `signature` removed.
/// Used both by the verifier and by the maintainer signing tool so
/// the two sides agree on the exact byte stream that gets signed.
///
/// Field order matters: serde_json serializes in struct declaration
/// order, and we rely on that for stable canonicalization. Do NOT
/// reorder fields here without also rotating the index format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntryUnsigned {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub download_url: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub slab_compat: String,
    /// **v2** — see [`IndexEntry::categories`]. Omitted when empty so
    /// the canonical payload is byte-identical to a v1 entry that
    /// doesn't carry any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
    /// **v2** — see [`IndexEntry::tags`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// **v2** — see [`IndexEntry::screenshots`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,
    /// **v2** — see [`IndexEntry::installs`].
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub installs: u64,
}

impl IndexEntry {
    /// Strip `signature` and return the canonical signing payload.
    pub fn to_unsigned(&self) -> IndexEntryUnsigned {
        IndexEntryUnsigned {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            description: self.description.clone(),
            author: self.author.clone(),
            download_url: self.download_url.clone(),
            sha256: self.sha256.clone(),
            size_bytes: self.size_bytes,
            slab_compat: self.slab_compat.clone(),
            categories: self.categories.clone(),
            tags: self.tags.clone(),
            screenshots: self.screenshots.clone(),
            installs: self.installs,
        }
    }
}

/// Maximum tarball size we'll allow. 5 MiB. Larger plugins almost
/// certainly indicate bundled binaries or media that shouldn't ship
/// via the marketplace.
pub const MAX_TARBALL_BYTES: u64 = 5 * 1024 * 1024;

/// Schema version this build of Slab understands. Bumped whenever
/// `Index` or `IndexEntry` gain/lose fields. Slab refuses to load
/// indices reporting a higher `schema_version`.
///
/// **v2 (2026-05-20, Workshop Marketplace):** added `categories`,
/// `tags`, `screenshots`, `installs` to `IndexEntry`. All optional —
/// v1-signed indices still verify because the canonical payload omits
/// empty v2 fields entirely.
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> IndexEntry {
        IndexEntry {
            id: "com.example.hello".into(),
            name: "Hello".into(),
            version: "0.1.0".into(),
            description: "Demo plugin".into(),
            author: "Sanjay".into(),
            download_url: "https://example.com/hello.tar.gz".into(),
            sha256: "deadbeef".repeat(8),
            size_bytes: 1024,
            slab_compat: ">=1.4.0".into(),
            signature: "AAAA".into(),
            categories: Vec::new(),
            tags: Vec::new(),
            screenshots: Vec::new(),
            installs: 0,
        }
    }

    fn sample_entry_v2() -> IndexEntry {
        IndexEntry {
            categories: vec!["productivity".into(), "ai".into()],
            tags: vec!["pdf".into(), "ollama".into()],
            screenshots: vec!["https://example.com/s1.png".into()],
            installs: 1234,
            ..sample_entry()
        }
    }

    #[test]
    fn roundtrip_index() {
        let idx = Index {
            schema_version: 2,
            signing_key_id: "slab-maintainer-2026".into(),
            plugins: vec![sample_entry()],
        };
        let s = serde_json::to_string(&idx).unwrap();
        let back: Index = serde_json::from_str(&s).unwrap();
        assert_eq!(idx, back);
    }

    #[test]
    fn to_unsigned_strips_signature() {
        let e = sample_entry();
        let u = e.to_unsigned();
        let canon = serde_json::to_string(&u).unwrap();
        assert!(
            !canon.contains("signature"),
            "canonical payload must not include signature field"
        );
        // sanity: all real fields present
        assert!(canon.contains("\"id\""));
        assert!(canon.contains("\"sha256\""));
        assert!(canon.contains("\"slab_compat\""));
    }

    #[test]
    fn unsigned_field_order_is_stable() {
        // Two structurally identical entries must produce byte-identical
        // canonical payloads — we depend on this for sig verification.
        let a = sample_entry();
        let b = sample_entry();
        let ca = serde_json::to_vec(&a.to_unsigned()).unwrap();
        let cb = serde_json::to_vec(&b.to_unsigned()).unwrap();
        assert_eq!(ca, cb);
    }

    #[test]
    fn deserialize_real_world_index_json() {
        // A realistic on-the-wire index snippet. v1 shape — must still
        // deserialize cleanly into a v2 IndexEntry (with empty v2
        // fields, default 0 install count).
        let s = r#"{
            "schema_version": 1,
            "signing_key_id": "slab-maintainer-2026",
            "plugins": [
                {
                    "id": "com.example.hello",
                    "name": "Hello Slab",
                    "version": "0.1.0",
                    "description": "Demo plugin showing every contribution kind.",
                    "author": "Sanjay",
                    "download_url": "https://github.com/Sanjays2402/slab-plugins/releases/download/v0.1.0/hello-slab-0.1.0.tar.gz",
                    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "size_bytes": 4096,
                    "slab_compat": ">=1.4.0",
                    "signature": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
                }
            ]
        }"#;
        let idx: Index = serde_json::from_str(s).unwrap();
        assert_eq!(idx.schema_version, 1);
        assert_eq!(idx.plugins.len(), 1);
        assert_eq!(idx.plugins[0].id, "com.example.hello");
        assert!(idx.plugins[0].categories.is_empty());
        assert!(idx.plugins[0].tags.is_empty());
        assert!(idx.plugins[0].screenshots.is_empty());
        assert_eq!(idx.plugins[0].installs, 0);
    }

    // ---------------- v2 (Workshop Marketplace) tests ----------------

    /// Pins the current schema version so a future bump fails this
    /// test loudly and forces a CHANGELOG entry + bump audit.
    #[test]
    fn current_schema_version_is_two() {
        assert_eq!(CURRENT_SCHEMA_VERSION, 2);
    }

    /// A v2 entry with all four new fields populated must round-trip
    /// through JSON.
    #[test]
    fn v2_entry_round_trips_with_new_fields() {
        let entry = sample_entry_v2();
        let json = serde_json::to_string(&entry).expect("serialize");
        let back: IndexEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(entry, back);
    }

    /// **Backward-compat invariant.** A v2 entry with all v2 fields
    /// empty must serialize to bytes byte-identical to a v1 entry —
    /// `categories`, `tags`, `screenshots`, `installs` keys are
    /// omitted from the JSON output entirely.
    #[test]
    fn v2_empty_fields_omit_from_wire_form() {
        let entry = sample_entry();
        let json = serde_json::to_string(&entry).expect("serialize");
        assert!(
            !json.contains("categories"),
            "v2 categories field must be omitted when empty"
        );
        assert!(
            !json.contains("tags"),
            "v2 tags field must be omitted when empty"
        );
        assert!(
            !json.contains("screenshots"),
            "v2 screenshots field must be omitted when empty"
        );
        assert!(
            !json.contains("installs"),
            "v2 installs field must be omitted when zero"
        );
    }

    /// **Signing-payload invariant.** Two signing payloads for the
    /// same v1-shaped entry — one constructed before v2, one after —
    /// must be byte-identical. We verify this transitively: a
    /// `to_unsigned()` of a v2-shaped entry with all v2 fields empty
    /// must produce the SAME JSON as the v1 canonical schema would
    /// have produced.
    #[test]
    fn v2_to_unsigned_byte_identical_to_v1_when_v2_fields_empty() {
        let entry = sample_entry();
        let unsigned = entry.to_unsigned();
        let canon = serde_json::to_vec(&unsigned).expect("canonicalize");
        let canon_str = std::str::from_utf8(&canon).expect("utf8");
        // The canonical form must look exactly like a v1 entry would
        // have looked — no v2 keys at all. This is the property we
        // rely on for v1-signed indices to keep verifying under v2
        // semantics.
        assert!(!canon_str.contains("categories"));
        assert!(!canon_str.contains("tags"));
        assert!(!canon_str.contains("screenshots"));
        assert!(!canon_str.contains("installs"));
        // And the v1 fields must still be there.
        assert!(canon_str.contains("\"id\""));
        assert!(canon_str.contains("\"sha256\""));
        assert!(canon_str.contains("\"slab_compat\""));
    }

    /// When v2 fields are populated, the signing payload contains
    /// them in declaration order, AFTER the v1 fields. This is the
    /// stable canonical order maintainers sign against.
    #[test]
    fn v2_signing_payload_orders_new_fields_after_v1() {
        let entry = sample_entry_v2();
        let unsigned = entry.to_unsigned();
        let canon = serde_json::to_vec(&unsigned).expect("canonicalize");
        let s = std::str::from_utf8(&canon).expect("utf8");
        let id_pos = s.find("\"id\"").expect("id present");
        let slab_compat_pos = s.find("\"slab_compat\"").expect("slab_compat present");
        let categories_pos = s.find("\"categories\"").expect("categories present");
        let tags_pos = s.find("\"tags\"").expect("tags present");
        let screenshots_pos = s.find("\"screenshots\"").expect("screenshots present");
        let installs_pos = s.find("\"installs\"").expect("installs present");
        // v1 fields precede v2 fields.
        assert!(
            slab_compat_pos < categories_pos,
            "v1 slab_compat must precede v2 categories in canonical form"
        );
        // v2 fields appear in declaration order.
        assert!(id_pos < categories_pos);
        assert!(categories_pos < tags_pos);
        assert!(tags_pos < screenshots_pos);
        assert!(screenshots_pos < installs_pos);
    }

    /// A v2 index with the new fields populated must deserialize
    /// cleanly into the live struct.
    #[test]
    fn deserialize_v2_index_with_categories_and_installs() {
        let s = r#"{
            "schema_version": 2,
            "signing_key_id": "slab-maintainer-2026",
            "plugins": [
                {
                    "id": "com.example.hello",
                    "name": "Hello",
                    "version": "0.1.0",
                    "description": "Demo",
                    "author": "Sanjay",
                    "download_url": "https://example.com/hello.tar.gz",
                    "sha256": "0000",
                    "size_bytes": 4096,
                    "slab_compat": ">=2.0.0",
                    "signature": "AAAA",
                    "categories": ["productivity", "ai"],
                    "tags": ["pdf"],
                    "screenshots": ["https://example.com/s1.png"],
                    "installs": 4242
                }
            ]
        }"#;
        let idx: Index = serde_json::from_str(s).unwrap();
        assert_eq!(idx.schema_version, 2);
        assert_eq!(idx.plugins[0].categories, vec!["productivity", "ai"]);
        assert_eq!(idx.plugins[0].tags, vec!["pdf"]);
        assert_eq!(idx.plugins[0].installs, 4242);
    }
}
