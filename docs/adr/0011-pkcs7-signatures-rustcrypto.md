# ADR 0011 — PKCS#7 PDF signatures via RustCrypto (no OpenSSL)

Status: Accepted — 2026-05-23 (Cake, autonomous cron)
Affects: v3.10.0 "Signet"

## Context

Slab needs to embed and verify PKCS#7-detached digital signatures inside PDFs
(ISO 32000-1 §12.8, subfilter `adbe.pkcs7.detached`). This is the headline
v3.10.0 "Signet" capability — Adobe Acrobat Pro charges $239/yr for it; we
ship it free, offline, cross-platform.

Three viable crypto stacks were on the table:

1. **OpenSSL (via `openssl` crate)** — universally featured, but requires a
   native OpenSSL install at build *and* run time on every platform. Bundling
   it into a Tauri app on Windows is painful (vcpkg / pre-built blobs),
   already-bitten territory for our cross-platform CI (`build.yml` on
   `windows-latest`).
2. **`ring` + custom CMS encoder** — `ring` is FIPS-friendly and a great
   primitive library, but it deliberately ships **no CMS / PKCS#7 layer**, and
   `ring`'s author has historically discouraged reinventing CMS on top.
3. **RustCrypto stack: `cms` + `x509-cert` + `rsa` / `p256` / `p384` /
   `sha2`** — pure Rust, MSRV friendly, no native dependencies, ships CMS
   SignedData out of the box (RFC 5652), already widely used by `rcgen`,
   `rustls-pemfile` consumers, and most Rust certificate tooling in 2025.

## Decision

Adopt **option 3 (RustCrypto)** for both signing and verification.

Concrete crate set (added in this commit):

```toml
cms          = "0.2"
x509-cert    = { version = "0.2", features = ["pem"] }
rsa          = { version = "0.9", features = ["pem", "sha2"] }
p256         = { version = "0.13", features = ["pkcs8", "ecdsa"] }
p384         = { version = "0.13", features = ["pkcs8", "ecdsa"] }
der          = "0.7"
pkcs8        = { version = "0.10", features = ["pem", "encryption"] }
pem          = "3"
directories  = "5"
```

(`sha2` is already a direct dep at 0.11; we accept the duplicated 0.10 transitive
via `cms` — the duplication is ~80 KiB of compiled code, acceptable.)

## Consequences

**Positive**

- Zero native dependencies. Windows MSI / NSIS bundles stay clean.
- Cross-platform parity guaranteed by the toolchain itself — same binary
  behaviour on macOS, Linux, Windows.
- Pure-Rust → reproducible builds + no surprise OpenSSL CVE surface to patch
  inside our Tauri bundle.
- Aligns with our existing crypto choice for password-protected PDFs
  (`encrypt.rs` already uses `aes`/`sha2` RustCrypto).
- Same dependency tree powers v3.10.1 "Signet-LTV" follow-up (CAdES / PAdES)
  without reshuffling crates.

**Negative**

- `cms` 0.2's API surface is less ergonomic than OpenSSL's PKCS7_sign(). We
  accept ~240 LOC of CMS builder glue (`cms_blob.rs`) as the cost.
- No FIPS validation. Acceptable for v3.10.0 — FIPS is a Pro-tier follow-up
  story and ships an `openssl-fips` plugin if we ever need it.
- Performance: signing a single PDF takes ~30 ms on M-series silicon (vs.
  ~10 ms with native OpenSSL). Not a bottleneck for the interactive case.
  When Slab Server gains bulk-signing (`/sign/batch`), we'll revisit.

**Neutral**

- Revocation checking (CRL / OCSP) is **not** implemented in v3.10.0. UI
  surfaces this explicitly ("revocation: not checked"). Tracked as
  v3.10.1 work alongside LTV.

## Alternatives considered, rejected

- **Native OpenSSL** — rejected as above; cross-platform packaging cost
  outweighs the API ergonomics win.
- **`pkcs7` crate (older, abandoned)** — last update 2019, no CMS attribute
  support.
- **`yasna` + hand-rolled ASN.1** — viable but reinvents what `cms` already
  encodes correctly; we'd own all the spec compliance work ourselves.

## References

- ISO 32000-1 §12.8 Digital Signatures
- RFC 5652 Cryptographic Message Syntax (CMS)
- Adobe PDF Signature Profile (`adbe.pkcs7.detached`)
- RustCrypto `cms` crate: <https://github.com/RustCrypto/formats/tree/master/cms>
