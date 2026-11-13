//! The secrets kept encrypted at rest: opened with the monolith's key, closed again with the key of
//! the service that keeps them now.
//!
//! The monolith, Propolis and the calendar seal a secret the same way: AES-256-GCM under a 32-byte
//! key, a fresh 96-bit nonce per secret, ciphertext and nonce in two `bytea` columns. That covers a
//! person's TOTP secret (Propolis), the token of someone invited to an event by address (the
//! calendar) and the instance's VAPID key (the monolith kept it in `instance_settings`; the hub
//! reads it from a PEM file).

use aes_gcm::aead::{Aead, Generate, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use p256::ecdsa::SigningKey;
use p256::pkcs8::{EncodePrivateKey, LineEnding};

/// A key of 64 hexadecimal characters, as every `*_SECRET_KEY` of an instance is written.
pub fn key_from_hex(hex: &str) -> Result<[u8; 32], String> {
    let bytes = from_hex(hex.trim()).ok_or("a key is 64 hexadecimal characters")?;
    bytes
        .try_into()
        .map_err(|_| "a key is 64 hexadecimal characters".to_owned())
}

fn from_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A `bytea` as PostgreSQL writes it in JSON (`\x0a1b...`).
pub fn bytea(text: &str) -> Option<Vec<u8>> {
    from_hex(text.strip_prefix("\\x")?)
}

/// Bytes as a `bytea` text, for a JSON row.
pub fn to_bytea(bytes: &[u8]) -> String {
    format!("\\x{}", to_hex(bytes))
}

pub fn open(key: &[u8; 32], ciphertext: &[u8], nonce: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let nonce = Nonce::try_from(nonce).ok()?;
    cipher.decrypt(&nonce, ciphertext).ok()
}

pub fn seal(key: &[u8; 32], plaintext: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let nonce = Nonce::generate();
    let ciphertext = cipher.encrypt(&nonce, plaintext).ok()?;
    Some((ciphertext, nonce.to_vec()))
}

/// A secret sealed under `from` (as `bytea` texts), sealed again under `to`: `None` when `from`
/// does not open it.
pub fn reseal(
    from: &[u8; 32],
    to: &[u8; 32],
    ciphertext: &str,
    nonce: &str,
) -> Option<(String, String)> {
    let plain = open(from, &bytea(ciphertext)?, &bytea(nonce)?)?;
    let (ciphertext, nonce) = seal(to, &plain)?;
    Some((to_bytea(&ciphertext), to_bytea(&nonce)))
}

/// The monolith's VAPID key (a P-256 scalar sealed under its key) as the PKCS#8 PEM the hub reads.
pub fn vapid_pem(key: &[u8; 32], ciphertext: &str, nonce: &str) -> Option<String> {
    let scalar = open(key, &bytea(ciphertext)?, &bytea(nonce)?)?;
    let signing = SigningKey::from_slice(&scalar).ok()?;
    signing
        .to_pkcs8_pem(LineEnding::LF)
        .ok()
        .map(|pem| pem.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_moves_from_one_key_to_another() {
        let (from, to) = ([7u8; 32], [9u8; 32]);
        let (ciphertext, nonce) = seal(&from, b"JBSWY3DPEHPK3PXP").unwrap();
        let (ciphertext, nonce) =
            reseal(&from, &to, &to_bytea(&ciphertext), &to_bytea(&nonce)).unwrap();
        let plain = open(&to, &bytea(&ciphertext).unwrap(), &bytea(&nonce).unwrap()).unwrap();
        assert_eq!(plain, b"JBSWY3DPEHPK3PXP");
        assert!(reseal(&from, &to, &ciphertext, &nonce).is_none());
    }

    #[test]
    fn the_vapid_key_becomes_a_pem() {
        let key = [3u8; 32];
        let (ciphertext, nonce) = seal(&key, &[1u8; 32]).unwrap();
        let pem = vapid_pem(&key, &to_bytea(&ciphertext), &to_bytea(&nonce)).unwrap();
        assert!(pem.starts_with("-----BEGIN PRIVATE KEY-----"));
    }

    #[test]
    fn keys_are_read_from_hex() {
        assert_eq!(key_from_hex(&"0a".repeat(32)).unwrap(), [10u8; 32]);
        assert!(key_from_hex("0a").is_err());
    }
}
