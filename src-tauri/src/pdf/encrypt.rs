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
pub fn decrypt(input: &Path, output: &Path, password: &str) -> Result<(), PdfError> {
    if !input.exists() {
        return Err(PdfError::InputMissing(input.display().to_string()));
    }
    let mut doc = Document::load_with_password(input, password)?;
    if doc.is_encrypted() {
        doc.decrypt(password)
            .map_err(|e| PdfError::Other(format!("decrypt failed: {e:?}")))?;
    }
    doc.save(output)?;
    Ok(())
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
}
