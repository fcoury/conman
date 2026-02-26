use aes_gcm_siv::aead::{Aead, KeyInit};
use aes_gcm_siv::{Aes256GcmSiv, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use conman_core::ConmanError;
use rand::RngCore;
use sha2::{Digest, Sha256};

fn cipher(master_key: &str) -> Aes256GcmSiv {
    let mut hasher = Sha256::new();
    hasher.update(master_key.as_bytes());
    let digest = hasher.finalize();
    Aes256GcmSiv::new_from_slice(digest.as_slice()).expect("sha256 must produce 32-byte key")
}

pub fn encrypt_secret(master_key: &str, plaintext: &str) -> Result<String, ConmanError> {
    let cipher = cipher(master_key);
    let mut nonce = [0_u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);

    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
        .map_err(|e| ConmanError::Internal {
            message: format!("failed to encrypt secret: {e}"),
        })?;

    let mut payload = Vec::with_capacity(12 + ciphertext.len());
    payload.extend_from_slice(&nonce);
    payload.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(payload))
}

pub fn decrypt_secret(master_key: &str, encrypted: &str) -> Result<String, ConmanError> {
    let bytes = STANDARD
        .decode(encrypted)
        .map_err(|e| ConmanError::Validation {
            message: format!("invalid encrypted secret payload: {e}"),
        })?;
    if bytes.len() < 13 {
        return Err(ConmanError::Validation {
            message: "encrypted secret payload is too short".to_string(),
        });
    }

    let (nonce, ciphertext) = bytes.split_at(12);
    let cipher = cipher(master_key);
    let plaintext = cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|e| ConmanError::Internal {
            message: format!("failed to decrypt secret: {e}"),
        })?;

    String::from_utf8(plaintext).map_err(|e| ConmanError::Internal {
        message: format!("decrypted secret is not utf8: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{decrypt_secret, encrypt_secret};

    #[test]
    fn roundtrip_secret_encryption() {
        let encrypted = encrypt_secret("master-key", "super-secret").expect("encrypted");
        let decrypted = decrypt_secret("master-key", &encrypted).expect("decrypted");
        assert_eq!(decrypted, "super-secret");
    }
}
