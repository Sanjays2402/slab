// PDF V1 (40-bit RC4) encryption + decryption.
//
// V1 is the broadest-compatible mode and avoids an AES dependency.
// AES (V4/V5) lands in a later release once Aes128CryptFilter Arc plumbing is wired.

use crate::pdf::PdfError;
use lopdf::{Document, EncryptionState, EncryptionVersion, Permissions};
use std::path::Path;

/// Encrypt `input` PDF with `password` (used as both owner + user) and write to `output`.
pub fn encrypt(input: &Path, output: &Path, password: &str) -> Result<(), PdfError> {
    if password.is_empty() {
        return Err(PdfError::Other("password must not be empty".into()));
    }
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load(input)?;
    if doc.is_encrypted() {
        return Err(PdfError::Other("input is already encrypted".into()));
    }
    let version = EncryptionVersion::V1 {
        document: &doc,
        owner_password: password,
        user_password: password,
        permissions: Permissions::all(),
    };
    let state = EncryptionState::try_from(version)
        .map_err(|e| PdfError::Other(format!("building encryption state: {e:?}")))?;
    doc.encrypt(&state)
        .map_err(|e| PdfError::Other(format!("encrypt failed: {e:?}")))?;
    doc.save(output)?;
    Ok(())
}

/// Decrypt `input` PDF with `password` and write plaintext to `output`.
///
/// Returns `PdfError::WrongPassword` when lopdf rejects the supplied
/// password (either at load-time via `load_with_password` or at the
/// follow-up `decrypt()` step). The frontend `DecryptModal` uses this
/// to drive the red-shake retry UX (issue #23 acceptance).
pub fn decrypt(input: &Path, output: &Path, password: &str) -> Result<(), PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load_with_password(input, password).map_err(|e| {
        if is_wrong_password_err(&e) {
            PdfError::WrongPassword
        } else {
            PdfError::from(e)
        }
    })?;
    if doc.is_encrypted() {
        doc.decrypt(password).map_err(|e| {
            // lopdf's runtime decrypt returns a plain string error; sniff it.
            let msg = format!("{e:?}").to_lowercase();
            if msg.contains("invalid password") || msg.contains("wrong password") {
                PdfError::WrongPassword
            } else {
                PdfError::Other(format!("decrypt failed: {e:?}"))
            }
        })?;
    }
    doc.save(output)?;
    Ok(())
}

/// Heuristic sniff for "wrong password" inside an `lopdf::Error`. lopdf
/// doesn't expose a typed `InvalidPassword` variant in the version we
/// pin, so we match on the rendered message — narrow enough that legit
/// IO/parse errors don't get mis-classified.
fn is_wrong_password_err(e: &lopdf::Error) -> bool {
    if matches!(e, lopdf::Error::InvalidPassword) {
        return true;
    }
    let msg = format!("{e:?}").to_lowercase();
    msg.contains("invalid password")
        || msg.contains("wrong password")
        || msg.contains("incorrectpassword")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::test_fixtures::make_n_page_pdf;

    #[test]
    fn encrypt_then_decrypt_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let enc = tmp.path().join("enc.pdf");
        make_n_page_pdf(&src, 2);
        encrypt(&src, &enc, "slab").unwrap();
        let probe = Document::load(&enc).unwrap();
        assert!(probe.is_encrypted());
        let doc = Document::load_with_password(&enc, "slab").unwrap();
        assert_eq!(doc.get_pages().len(), 2);
    }

    #[test]
    fn encrypt_rejects_empty_password() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let dst = tmp.path().join("out.pdf");
        make_n_page_pdf(&src, 1);
        assert!(encrypt(&src, &dst, "").is_err());
    }

    #[test]
    fn decrypt_round_trip_to_file() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let enc = tmp.path().join("enc.pdf");
        let dec = tmp.path().join("dec.pdf");
        make_n_page_pdf(&src, 3);
        encrypt(&src, &enc, "slab").unwrap();
        decrypt(&enc, &dec, "slab").unwrap();
        let doc = Document::load(&dec).unwrap();
        assert!(!doc.is_encrypted());
        assert_eq!(doc.get_pages().len(), 3);
    }

    #[test]
    fn decrypt_with_wrong_password_returns_wrong_password_variant() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let enc = tmp.path().join("enc.pdf");
        let dec = tmp.path().join("dec.pdf");
        make_n_page_pdf(&src, 1);
        encrypt(&src, &enc, "correct-horse").unwrap();
        let err = decrypt(&enc, &dec, "battery-staple").unwrap_err();
        assert!(
            matches!(err, PdfError::WrongPassword),
            "expected WrongPassword, got {err:?}",
        );
        // The dec file must NOT have been written — the bad-password path
        // returns before `doc.save()`.
        assert!(!dec.exists(), "no output file on wrong password");
    }

    #[test]
    fn decrypt_missing_input_returns_input_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let nope = tmp.path().join("nope.pdf");
        let out = tmp.path().join("out.pdf");
        let err = decrypt(&nope, &out, "pw").unwrap_err();
        assert!(
            matches!(err, PdfError::InputMissing(_)),
            "expected InputMissing, got {err:?}",
        );
    }

    #[test]
    fn decrypt_empty_password_on_encrypted_file_is_wrong_password() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.pdf");
        let enc = tmp.path().join("enc.pdf");
        let dec = tmp.path().join("dec.pdf");
        make_n_page_pdf(&src, 1);
        encrypt(&src, &enc, "letmein").unwrap();
        let err = decrypt(&enc, &dec, "").unwrap_err();
        assert!(
            matches!(err, PdfError::WrongPassword),
            "expected WrongPassword for empty pw, got {err:?}",
        );
    }
}
