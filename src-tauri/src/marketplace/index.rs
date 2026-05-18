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
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

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
        }
    }

    #[test]
    fn roundtrip_index() {
        let idx = Index {
            schema_version: 1,
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
        // A realistic on-the-wire index snippet.
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
    }
}
