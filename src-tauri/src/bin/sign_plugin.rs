// slab-sign-plugin — Slab maintainer signing tool (v1.4.0 Bench).
//
// Reads a plugin tarball + the maintainer Ed25519 private key, computes
// the tarball's SHA-256, builds a canonical IndexEntry payload, and
// signs it. Output is a JSON object (the signed IndexEntry) ready to
// paste into `index.json` in the slab-plugins repo.
//
// Usage:
//   cargo run --bin slab-sign-plugin -- \
//     --tarball ./hello-slab-0.1.0.tar.gz \
//     --id com.example.hello \
//     --name "Hello Slab" \
//     --version 0.1.0 \
//     --description "Demo plugin" \
//     --author Sanjay \
//     --download-url https://github.com/.../hello-slab-0.1.0.tar.gz \
//     --slab-compat ">=1.4.0" \
//     [--key ~/.slab-maintainer-key]
//
// Auxiliary commands:
//   --print-public-key            Print the maintainer public key (hex)
//                                 derived from the private key on disk
//                                 and exit. Useful for cross-checking
//                                 the baked-in MAINTAINER_PUBLIC_KEY.
//   --print-fixture-signature     Re-sign the verifier's test fixture
//                                 and print the resulting base64
//                                 signature. Used to refresh the
//                                 regression test if the key rotates.
//
// The private key file is expected to be a base64-encoded 32-byte raw
// Ed25519 seed (one line of base64; surrounding comment lines starting
// with '#' are stripped). This is the format `slab-sign-plugin
// --generate-key` (when added in a future slice) would emit.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use slab_lib::marketplace::index::{IndexEntry, IndexEntryUnsigned};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const DEFAULT_KEY_PATH: &str = "~/.slab-maintainer-key";

#[derive(Default, Debug)]
struct Args {
    tarball: Option<PathBuf>,
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    author: Option<String>,
    download_url: Option<String>,
    slab_compat: Option<String>,
    key_path: Option<PathBuf>,
    print_public_key: bool,
    print_fixture_signature: bool,
    help: bool,
}

fn main() -> ExitCode {
    let argv: Vec<String> = env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            print_usage();
            return ExitCode::from(2);
        }
    };
    if args.help {
        print_usage();
        return ExitCode::SUCCESS;
    }

    let key_path = resolve_key_path(args.key_path.clone());
    let signing_key = match load_signing_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            eprintln!(
                "error: failed to load signing key from {}: {e}",
                key_path.display()
            );
            return ExitCode::FAILURE;
        }
    };

    if args.print_public_key {
        let pk = signing_key.verifying_key().to_bytes();
        println!("hex:     {}", hex_lower(&pk));
        println!("base64:  {}", B64.encode(pk));
        println!("array:   {}", as_rust_array(&pk));
        return ExitCode::SUCCESS;
    }

    if args.print_fixture_signature {
        // Mirrors the test fixture in marketplace::verify::tests::fixture_entry.
        // Keep these field values in lockstep with that fixture.
        let unsigned = IndexEntryUnsigned {
            id: "com.example.hello".into(),
            name: "Hello".into(),
            version: "0.1.0".into(),
            description: "Demo".into(),
            author: "Sanjay".into(),
            download_url: "https://example.com/hello.tar.gz".into(),
            sha256: "deadbeef".repeat(8),
            size_bytes: 1024,
            slab_compat: ">=1.4.0".into(),
        };
        let canonical = match serde_json::to_vec(&unsigned) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: failed to serialize fixture: {e}");
                return ExitCode::FAILURE;
            }
        };
        let sig = signing_key.sign(&canonical);
        println!("{}", B64.encode(sig.to_bytes()));
        return ExitCode::SUCCESS;
    }

    // Real signing path — all metadata flags required.
    let tarball = match args.tarball {
        Some(t) => t,
        None => {
            eprintln!("error: --tarball is required");
            print_usage();
            return ExitCode::from(2);
        }
    };
    let id = required(args.id, "--id");
    let name = required(args.name, "--name");
    let version = required(args.version, "--version");
    let description = required(args.description, "--description");
    let author = required(args.author, "--author");
    let download_url = required(args.download_url, "--download-url");
    let slab_compat = required(args.slab_compat, "--slab-compat");

    let (id, name, version, description, author, download_url, slab_compat) = match (
        id,
        name,
        version,
        description,
        author,
        download_url,
        slab_compat,
    ) {
        (Ok(a), Ok(b), Ok(c), Ok(d), Ok(e), Ok(f), Ok(g)) => (a, b, c, d, e, f, g),
        (a, b, c, d, e, f, g) => {
            for err in [a, b, c, d, e, f, g]
                .iter()
                .filter_map(|r| r.as_ref().err())
            {
                eprintln!("error: missing {err}");
            }
            print_usage();
            return ExitCode::from(2);
        }
    };

    let tarball_bytes = match fs::read(&tarball) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: cannot read tarball {}: {e}", tarball.display());
            return ExitCode::FAILURE;
        }
    };
    let size_bytes = tarball_bytes.len() as u64;
    let sha256 = sha256_hex(&tarball_bytes);

    let unsigned = IndexEntryUnsigned {
        id: id.clone(),
        name: name.clone(),
        version: version.clone(),
        description: description.clone(),
        author: author.clone(),
        download_url: download_url.clone(),
        sha256: sha256.clone(),
        size_bytes,
        slab_compat: slab_compat.clone(),
    };
    let canonical = match serde_json::to_vec(&unsigned) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: canonical serialization failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let sig = signing_key.sign(&canonical);
    let signature = B64.encode(sig.to_bytes());

    let signed = IndexEntry {
        id,
        name,
        version,
        description,
        author,
        download_url,
        sha256,
        size_bytes,
        slab_compat,
        signature,
    };
    match serde_json::to_string_pretty(&signed) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: pretty-print failed: {e}");
            ExitCode::FAILURE
        }
    }
}

fn required(v: Option<String>, flag: &'static str) -> Result<String, &'static str> {
    v.ok_or(flag)
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut args = Args::default();
    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        match a.as_str() {
            "--help" | "-h" => {
                args.help = true;
            }
            "--print-public-key" => {
                args.print_public_key = true;
            }
            "--print-fixture-signature" => {
                args.print_fixture_signature = true;
            }
            "--tarball" => {
                args.tarball = Some(PathBuf::from(next_value(argv, &mut i, a)?));
            }
            "--id" => args.id = Some(next_value(argv, &mut i, a)?),
            "--name" => args.name = Some(next_value(argv, &mut i, a)?),
            "--version" => args.version = Some(next_value(argv, &mut i, a)?),
            "--description" => args.description = Some(next_value(argv, &mut i, a)?),
            "--author" => args.author = Some(next_value(argv, &mut i, a)?),
            "--download-url" => args.download_url = Some(next_value(argv, &mut i, a)?),
            "--slab-compat" => args.slab_compat = Some(next_value(argv, &mut i, a)?),
            "--key" => args.key_path = Some(PathBuf::from(next_value(argv, &mut i, a)?)),
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }
    Ok(args)
}

fn next_value(argv: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    argv.get(*i)
        .cloned()
        .ok_or_else(|| format!("flag {flag} requires a value"))
}

fn resolve_key_path(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(p) = explicit {
        return p;
    }
    expand_tilde(DEFAULT_KEY_PATH)
}

fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(p)
}

fn load_signing_key(path: &Path) -> Result<SigningKey, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    // Strip blank lines and `#` comments; take the first non-empty line.
    let b64 = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .ok_or_else(|| "key file is empty or only contains comments".to_string())?;
    let bytes = B64
        .decode(b64)
        .map_err(|e| format!("base64 decode failed: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!(
            "key must be 32 bytes (raw Ed25519 seed), got {}",
            bytes.len()
        ));
    }
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| "internal: failed to convert 32-byte vec to array".to_string())?;
    Ok(SigningKey::from_bytes(&arr))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex_lower(&h.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn as_rust_array(bytes: &[u8]) -> String {
    let parts: Vec<String> = bytes.iter().map(|b| format!("0x{b:02x}")).collect();
    let chunks: Vec<String> = parts.chunks(8).map(|c| c.join(", ")).collect();
    format!("[{}]", chunks.join(", "))
}

fn print_usage() {
    eprintln!(
        "\
slab-sign-plugin — sign a plugin tarball into a marketplace IndexEntry

USAGE:
  slab-sign-plugin --tarball <path> --id <id> --name <name> \\
                   --version <version> --description <text> \\
                   --author <name> --download-url <url> \\
                   --slab-compat <semver-req> [--key <path>]

UTILITIES:
  slab-sign-plugin --print-public-key           Print maintainer pubkey
  slab-sign-plugin --print-fixture-signature    Re-sign verifier fixture
  slab-sign-plugin --help                       This message

Key file format: base64-encoded 32-byte Ed25519 seed (lines starting
with '#' ignored). Default path: ~/.slab-maintainer-key.

Emits JSON on stdout — paste into the `plugins` array of `index.json`."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_key_file(b64_seed: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "# test key").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "{b64_seed}").unwrap();
        f
    }

    #[test]
    fn parses_full_argv() {
        let argv: Vec<String> = [
            "--tarball",
            "/tmp/x.tgz",
            "--id",
            "com.example.x",
            "--name",
            "X",
            "--version",
            "1.0.0",
            "--description",
            "desc",
            "--author",
            "auth",
            "--download-url",
            "https://x/y.tgz",
            "--slab-compat",
            ">=1.4.0",
            "--key",
            "/tmp/k",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let a = parse_args(&argv).unwrap();
        assert_eq!(a.tarball, Some(PathBuf::from("/tmp/x.tgz")));
        assert_eq!(a.id.as_deref(), Some("com.example.x"));
        assert_eq!(a.slab_compat.as_deref(), Some(">=1.4.0"));
        assert_eq!(a.key_path, Some(PathBuf::from("/tmp/k")));
        assert!(!a.print_public_key);
        assert!(!a.help);
    }

    #[test]
    fn parses_print_public_key_flag() {
        let argv: Vec<String> = ["--print-public-key"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let a = parse_args(&argv).unwrap();
        assert!(a.print_public_key);
    }

    #[test]
    fn rejects_unknown_argument() {
        let argv: Vec<String> = ["--bogus"].iter().map(|s| s.to_string()).collect();
        let e = parse_args(&argv).unwrap_err();
        assert!(e.contains("--bogus"));
    }

    #[test]
    fn rejects_flag_without_value() {
        let argv: Vec<String> = ["--tarball"].iter().map(|s| s.to_string()).collect();
        let e = parse_args(&argv).unwrap_err();
        assert!(e.contains("--tarball"));
    }

    #[test]
    fn expand_tilde_resolves_home() {
        std::env::set_var("HOME", "/Users/test");
        let p = expand_tilde("~/foo/bar");
        assert_eq!(p, PathBuf::from("/Users/test/foo/bar"));
    }

    #[test]
    fn expand_tilde_passthrough_for_non_tilde() {
        let p = expand_tilde("/abs/path");
        assert_eq!(p, PathBuf::from("/abs/path"));
    }

    #[test]
    fn load_signing_key_accepts_base64_seed() {
        // 32 bytes = base64 length 44 (with padding).
        let seed = [42u8; 32];
        let b64 = B64.encode(seed);
        let f = write_key_file(&b64);
        let sk = load_signing_key(f.path()).unwrap();
        assert_eq!(sk.to_bytes(), seed);
    }

    #[test]
    fn load_signing_key_rejects_wrong_length() {
        let f = write_key_file(&B64.encode([0u8; 16]));
        let e = load_signing_key(f.path()).unwrap_err();
        assert!(e.contains("32 bytes"));
    }

    #[test]
    fn load_signing_key_rejects_bad_base64() {
        let f = write_key_file("!!!not-base64!!!");
        let e = load_signing_key(f.path()).unwrap_err();
        assert!(e.contains("base64"));
    }

    #[test]
    fn load_signing_key_skips_comments_and_blank_lines() {
        let seed = [9u8; 32];
        let f = write_key_file(&B64.encode(seed));
        let sk = load_signing_key(f.path()).unwrap();
        assert_eq!(sk.to_bytes(), seed);
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        // sha256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hex_lower_pads_zero_bytes() {
        assert_eq!(hex_lower(&[0, 1, 15, 16, 255]), "00010f10ff");
    }

    #[test]
    fn as_rust_array_formats_32_bytes() {
        let s = as_rust_array(&[0xab; 32]);
        // 32 bytes / 8 per chunk = 4 chunks, formatted with commas
        assert!(s.starts_with('['));
        assert!(s.ends_with(']'));
        assert_eq!(s.matches("0xab").count(), 32);
    }

    /// End-to-end: sign the fixture entry and assert the verifier
    /// (using the derived public key) accepts the signature.
    #[test]
    fn signing_round_trip_verifies() {
        use slab_lib::marketplace::verify::verify_entry;

        let seed = [123u8; 32];
        let sk = SigningKey::from_bytes(&seed);

        let unsigned = IndexEntryUnsigned {
            id: "com.example.rt".into(),
            name: "RT".into(),
            version: "0.1.0".into(),
            description: "round trip".into(),
            author: "Cake".into(),
            download_url: "https://example.com/rt.tar.gz".into(),
            sha256: sha256_hex(b"some-tarball-bytes"),
            size_bytes: 17,
            slab_compat: ">=1.4.0".into(),
        };
        let canonical = serde_json::to_vec(&unsigned).unwrap();
        let sig = sk.sign(&canonical);

        let entry = IndexEntry {
            id: unsigned.id.clone(),
            name: unsigned.name.clone(),
            version: unsigned.version.clone(),
            description: unsigned.description.clone(),
            author: unsigned.author.clone(),
            download_url: unsigned.download_url.clone(),
            sha256: unsigned.sha256.clone(),
            size_bytes: unsigned.size_bytes,
            slab_compat: unsigned.slab_compat.clone(),
            signature: B64.encode(sig.to_bytes()),
        };

        verify_entry(&entry, sk.verifying_key().as_bytes()).unwrap();
    }
}
