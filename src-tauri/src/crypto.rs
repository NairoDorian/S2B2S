use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use base64::Engine;
use keyring::Entry;
use std::fs;
use std::sync::OnceLock;
use tauri::AppHandle;
use tauri::Manager;

static MASTER_KEY: OnceLock<Vec<u8>> = OnceLock::new();

/// Initialize the master key by checking the OS Keyring, then local fallback file.
/// If not found, generates a new one and stores it.
pub fn initialize(app_handle: &AppHandle) -> Result<(), String> {
    if MASTER_KEY.get().is_some() {
        return Ok(());
    }

    // Try OS Keyring
    let entry = Entry::new("s2b2s", "master_key").ok();
    let mut key_opt = None;

    if let Some(ref entry) = entry {
        if let Ok(password) = entry.get_password() {
            if let Ok(key) = base64::engine::general_purpose::STANDARD.decode(password.trim()) {
                if key.len() == 32 {
                    key_opt = Some(key);
                }
            }
        }
    }

    if key_opt.is_none() {
        // Try fallback file in app data directory
        if let Ok(app_data) = app_handle.path().app_data_dir() {
            let fallback_path = app_data.join(".master_key");
            if fallback_path.exists() {
                if let Ok(content) = fs::read_to_string(&fallback_path) {
                    if let Ok(key) =
                        base64::engine::general_purpose::STANDARD.decode(content.trim())
                    {
                        if key.len() == 32 {
                            key_opt = Some(key);
                        }
                    }
                }
            }
        }
    }

    let key = match key_opt {
        Some(k) => k,
        None => {
            // Generate a new cryptographically secure 32-byte key
            let mut new_key = vec![0u8; 32];
            getrandom::getrandom(&mut new_key)
                .map_err(|e| format!("Random generation failed: {}", e))?;
            let password = base64::engine::general_purpose::STANDARD.encode(&new_key);

            // Attempt to store it in the OS keyring
            let mut stored = false;
            if let Some(ref entry) = entry {
                if entry.set_password(&password).is_ok() {
                    stored = true;
                }
            }

            if !stored {
                // Keyring not available or failed, write to local fallback file
                if let Ok(app_data) = app_handle.path().app_data_dir() {
                    let _ = fs::create_dir_all(&app_data);
                    let fallback_path = app_data.join(".master_key");
                    let _ = fs::write(&fallback_path, &password);
                }
            }
            new_key
        }
    };

    let _ = MASTER_KEY.set(key);
    Ok(())
}

fn get_master_key() -> &'static [u8] {
    MASTER_KEY.get().map(|k| k.as_slice()).unwrap_or(&[0u8; 32])
}

/// Encrypts bytes using AES-256-GCM. Returns a base64 string prefixed with "enc:v1:".
pub fn encrypt(plaintext: &[u8]) -> Result<String, String> {
    let key_bytes = get_master_key();
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);

    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes)
        .map_err(|e| format!("Nonce generation failed: {}", e))?;
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Format: [cipher_id: 1 byte (0x00 = AES-GCM-256)] + [nonce: 12 bytes] + [ciphertext + tag]
    let mut payload = Vec::with_capacity(1 + 12 + ciphertext.len());
    payload.push(0x00);
    payload.extend_from_slice(&nonce_bytes);
    payload.extend_from_slice(&ciphertext);

    let b64 = base64::engine::general_purpose::STANDARD.encode(&payload);
    Ok(format!("enc:v1:{}", b64))
}

/// Decrypts a base64 string starting with "enc:v1:". Plaintext fallback if not prefixed.
pub fn decrypt(ciphertext: &str) -> Result<Vec<u8>, String> {
    if !ciphertext.starts_with("enc:v1:") {
        // Transparently treat as legacy plaintext
        return Ok(ciphertext.as_bytes().to_vec());
    }

    let b64_part = &ciphertext[7..];
    let payload = base64::engine::general_purpose::STANDARD
        .decode(b64_part)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    if payload.len() < 13 {
        return Err("Payload too short".to_string());
    }

    let cipher_id = payload[0];
    if cipher_id != 0x00 {
        return Err(format!("Unsupported cipher ID: {}", cipher_id));
    }

    let nonce_bytes = &payload[1..13];
    let encrypted_data = &payload[13..];

    let key_bytes = get_master_key();
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, encrypted_data)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    Ok(plaintext)
}

/// Encrypts a string.
pub fn encrypt_str(plaintext: &str) -> Result<String, String> {
    encrypt(plaintext.as_bytes())
}

/// Decrypts a string to UTF-8.
pub fn decrypt_str(ciphertext: &str) -> Result<String, String> {
    let bytes = decrypt(ciphertext)?;
    String::from_utf8(bytes).map_err(|e| format!("Invalid UTF-8 plaintext: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_decryption_roundtrip() {
        if MASTER_KEY.get().is_none() {
            let test_key = vec![0x42u8; 32];
            let _ = MASTER_KEY.set(test_key);
        }

        let plaintext = "secret_key_123456";
        let encrypted = encrypt_str(plaintext).expect("Encryption failed");
        assert!(encrypted.starts_with("enc:v1:"));

        let decrypted = decrypt_str(&encrypted).expect("Decryption failed");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_plaintext_fallback() {
        if MASTER_KEY.get().is_none() {
            let test_key = vec![0x42u8; 32];
            let _ = MASTER_KEY.set(test_key);
        }

        let plaintext = "legacy_unencrypted_value";
        let decrypted = decrypt_str(plaintext).expect("Decryption failed");
        assert_eq!(decrypted, plaintext);
    }
}
