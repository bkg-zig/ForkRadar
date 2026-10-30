//! Token handling of the API server.
//!
//! Session tokens are stored as SHA-256 hash. GitHub access tokens are
//! encrypted with AES-256-GCM before they reach the database.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Errors of the crypto helpers.
#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("could not generate random bytes: {0}")]
    Random(String),
    #[error("could not encrypt the token: {0}")]
    Encrypt(String),
    #[error("could not decrypt the token: {0}")]
    Decrypt(String),
}

/// Generates a random token with 32 bytes of entropy.
pub fn random_token() -> Result<String, CryptoError> {
    let mut bytes = [0_u8; 32];
    rand::thread_rng()
        .try_fill_bytes(&mut bytes)
        .map_err(|err| CryptoError::Random(err.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Hashes a session token. Only the hash is stored in the database.
pub fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Encrypts a GitHub access token with AES-256-GCM.
///
/// The nonce is stored in front of the ciphertext.
#[derive(Clone)]
pub struct TokenCipher {
    cipher: Aes256Gcm,
}

impl std::fmt::Debug for TokenCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenCipher { key: [REDACTED] }")
    }
}

impl TokenCipher {
    pub fn new(key: [u8; 32]) -> Self {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        Self { cipher }
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<Vec<u8>, CryptoError> {
        let mut nonce_bytes = [0_u8; 12];
        rand::thread_rng()
            .try_fill_bytes(&mut nonce_bytes)
            .map_err(|err| CryptoError::Random(err.to_string()))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|err| CryptoError::Encrypt(err.to_string()))?;
        let mut output = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    pub fn decrypt(&self, data: &[u8]) -> Result<String, CryptoError> {
        if data.len() < 13 {
            return Err(CryptoError::Decrypt("ciphertext is too short".to_string()));
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|err| CryptoError::Decrypt(err.to_string()))?;
        String::from_utf8(plaintext).map_err(|err| CryptoError::Decrypt(err.to_string()))
    }
}

/// Computes the PKCE challenge of a verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_and_not_the_token() {
        let token = "session-token-value";
        let hash = hash_token(token);
        assert_eq!(hash, hash_token(token));
        assert_ne!(hash, token);
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn encryption_round_trip() {
        let cipher = TokenCipher::new([7_u8; 32]);
        let encrypted = cipher.encrypt("gho_secret_token").unwrap();
        assert!(!encrypted.windows(4).any(|part| part == b"gho_"));
        assert_eq!(cipher.decrypt(&encrypted).unwrap(), "gho_secret_token");
    }

    #[test]
    fn decryption_rejects_garbage() {
        let cipher = TokenCipher::new([7_u8; 32]);
        assert!(cipher.decrypt(b"short").is_err());
        assert!(cipher.decrypt(&[0_u8; 40]).is_err());
    }

    #[test]
    fn pkce_challenge_matches_the_rfc_example() {
        // RFC 7636 appendix B uses the verifier "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".
        let challenge = pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }
}
