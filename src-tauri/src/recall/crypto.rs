//! Recall vault encryption (fork feature).
//!
//! Suite `ZV1`: every note file is encrypted with its own random 256-bit
//! content key under XChaCha20-Poly1305, and that key is wrapped with the
//! master key. The master key is derived from the passphrase with Argon2id
//! (parameters stored alongside, so they can rise with hardware without
//! breaking old vaults) and — this is the reversibility contract — lives in
//! memory only while the vault is unlocked. Turning encryption off decrypts
//! every note back to plain Markdown and removes the key file.
//!
//! File shapes:
//!
//! - `vault.json` — `{ version, kdf params, salt, verifier }`, where the
//!   verifier is a fixed plaintext sealed under the master key: the
//!   passphrase check for unlock and disable, without any passphrase or
//!   key material on disk.
//! - `notes/<id>.rcl` — `magic ‖ wrapped key (nonce‖ct) ‖ content nonce ‖
//!   ciphertext`. Decrypting yields the exact bytes of the original `.md`
//!   file (frontmatter included), so enable → disable is byte-faithful.
//!
//! Off is the default and a first-class state: a vault without
//! `vault.json` is a plain folder of Markdown, and every command in this
//! module is a no-op there.

use argon2::Argon2;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{Ordering, compiler_fence};

use crate::recall::notes_dir;

/// Magic + format version of the `.rcl` envelope. The suite is identified
/// in every file, so a future `ZV2` can re-wrap in place.
const NOTE_MAGIC: &[u8; 8] = b"ZER0RCL1";
const KEY_FILE: &str = "vault.json";
const SALT_LEN: usize = 32;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
/// Fixed plaintext sealed under the master key: proving it decrypts is
/// proving the passphrase, without storing either the key or its hash.
const VERIFIER_PLAINTEXT: &[u8] = b"zer0-recall-vault-verifier-v1";

/// The passphrase has to clear this bar; there is no recovery beyond it.
pub const MIN_PASSPHRASE_LEN: usize = 8;

/// Argon2id defaults: 64 MiB of memory-hard work. The parameters are
/// stored in `vault.json`, so raising these later only affects new vaults.
pub const DEFAULT_M_COST_KIB: u32 = 65536;
pub const DEFAULT_T_COST: u32 = 3;
pub const DEFAULT_P_COST: u32 = 4;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct KdfParams {
    /// Argon2 memory cost, KiB.
    pub m_cost_kib: u32,
    /// Argon2 time cost (passes).
    pub t_cost: u32,
    /// Argon2 parallelism.
    pub p_cost: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VaultKeyFile {
    pub version: u32,
    pub kdf: KdfParams,
    pub salt_hex: String,
    pub verifier_nonce_hex: String,
    pub verifier_ct_hex: String,
}

/// The derived master key (also the shape of a per-file content key). Only
/// ever exists in memory: the session mutex below is the whole persistence
/// story, and every `MasterKey` copy is wiped when it drops. That is
/// best-effort hygiene, not a guarantee: the short-lived cipher built for each
/// seal/open holds its own copy (`chacha20poly1305` is built without its
/// `zeroize` feature), and the KDF's working memory is not wiped either.
#[derive(Clone)]
pub struct MasterKey([u8; KEY_LEN]);

impl MasterKey {
    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new((&self.0).into())
    }
}

impl Drop for MasterKey {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// Overwrite key material before its memory is released. Volatile writes and
/// a fence keep the compiler from eliding stores to memory about to be freed.
fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: `byte` is a valid, aligned, exclusive reference to a `u8`.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

/// What [`enable`] reports back to the UI.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct EnableReport {
    pub notes_encrypted: u32,
}

/// The folder earlier versions wrote a plaintext copy of every note into when
/// encryption was enabled. It is never written any more, and one left behind
/// by an older version is removed as soon as the vault is encrypted
/// ([`enable`], [`unlock`]): plaintext notes beside an encrypted vault
/// would make the encryption pointless.
const LEGACY_BACKUP_DIR: &str = "backup";

/// Remove the plaintext backup folder an older version left in `root`.
fn remove_legacy_backup(root: &Path) {
    let dir = root.join(LEGACY_BACKUP_DIR);
    if !dir.is_dir() {
        return;
    }
    match fs::remove_dir_all(&dir) {
        Ok(()) => log::info!(
            "Recall: removed the plaintext backup folder an earlier version left in the \
             encrypted vault ({})",
            dir.display()
        ),
        Err(e) => log::warn!(
            "Recall: could not remove the plaintext backup folder {}: {e}",
            dir.display()
        ),
    }
}

/// Best-effort removal of a plaintext file's contents: overwritten with zeros
/// and flushed before it is deleted, so the note is not left readable in the
/// freed blocks of a plain disk. (An SSD, a journaling or copy-on-write file
/// system, or a synced folder can still keep old blocks; that is outside what
/// an application can guarantee.)
fn shred_file(path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let len = fs::metadata(path)?.len();
    {
        let mut file = fs::OpenOptions::new().write(true).open(path)?;
        let zeros = [0u8; 8192];
        let mut left = len;
        while left > 0 {
            let n = left.min(zeros.len() as u64) as usize;
            file.write_all(&zeros[..n])?;
            left -= n as u64;
        }
        file.sync_all()?;
    }
    fs::remove_file(path)
}

/// The vault's encryption state, as the page's toggle renders it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct RecallEncryptionStatus {
    /// `vault.json` exists — the notes on disk are ciphertext.
    pub enabled: bool,
    /// The session holds the derived key.
    pub unlocked: bool,
}

/// The session's unlocked state. App exit clears it with the process; the
/// Lock button and [`disable`] clear it explicitly.
static SESSION_KEY: Mutex<Option<MasterKey>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// session state
// ---------------------------------------------------------------------------

/// Whether the session holds a derived key. Kept next to `session_key` so
/// the UI's status read does not clone key material.
pub fn is_unlocked() -> bool {
    SESSION_KEY.lock().unwrap().is_some()
}

pub fn session_lock() {
    *SESSION_KEY.lock().unwrap() = None;
}

fn session_unlock_with(key: MasterKey) {
    *SESSION_KEY.lock().unwrap() = Some(key);
}

/// The key the session holds, or the "locked" error every locked-path
/// command answers with.
pub(crate) fn session_key() -> Result<MasterKey, String> {
    SESSION_KEY
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Vault is locked".to_string())
}

// ---------------------------------------------------------------------------
// primitives
// ---------------------------------------------------------------------------

fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    rand::rng().fill_bytes(&mut buf);
    buf
}

/// Argon2id, the stored parameters, the stored salt → the master key.
fn derive_master_key(
    passphrase: &str,
    salt: &[u8],
    params: &KdfParams,
) -> Result<MasterKey, String> {
    let argon = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::default(),
        argon2::Params::new(
            params.m_cost_kib,
            params.t_cost,
            params.p_cost,
            Some(KEY_LEN),
        )
        .map_err(|e| format!("Bad KDF parameters: {e}"))?,
    );
    let mut out = [0u8; KEY_LEN];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut out)
        .map_err(|e| format!("Key derivation failed: {e}"))?;
    Ok(MasterKey(out))
}

/// Seal under `key`: fresh 192-bit nonce, ciphertext with the Poly1305 tag
/// appended. `(nonce, ct)` is the universal wire shape of this module.
fn seal(key: &MasterKey, plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let nonce_bytes = random_bytes(NONCE_LEN);
    let nonce = XNonce::try_from(&nonce_bytes[..]).map_err(|_| "Bad nonce length".to_string())?;
    let ct = key
        .cipher()
        .encrypt(&nonce, plaintext)
        .map_err(|e| format!("Encryption failed: {e}"))?;
    Ok((nonce_bytes, ct))
}

/// What [`open`] answers when the tag check fails. Only [`verify_passphrase`]
/// can tell a wrong passphrase from a damaged file, so only it says so.
const DECRYPT_FAILED: &str = "Decryption failed (wrong key or corrupt file)";

/// Open `(nonce, ct)` under `key`. A wrong key fails the tag check here —
/// the only passphrase check that exists.
fn open(key: &MasterKey, nonce: &[u8], ct: &[u8]) -> Result<Vec<u8>, String> {
    if nonce.len() != NONCE_LEN {
        return Err("Corrupt vault file: bad nonce length".to_string());
    }
    let nonce = XNonce::try_from(nonce).map_err(|_| "Bad nonce length".to_string())?;
    key.cipher()
        .decrypt(&nonce, ct)
        .map_err(|_| DECRYPT_FAILED.to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
    // Byte slicing below needs ASCII, and an odd length would drop a nibble.
    if !hex.len().is_multiple_of(2) || !hex.is_ascii() {
        return Err("Corrupt vault file: bad hex".to_string());
    }
    (0..hex.len() / 2)
        .map(|i| {
            u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                .map_err(|e| format!("Corrupt vault file: {e}"))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// key file
// ---------------------------------------------------------------------------

/// `Some` when the vault is encrypted — the presence of `vault.json` is
/// the whole on/off state.
pub fn key_file(root: &Path) -> Option<VaultKeyFile> {
    let path = root.join(KEY_FILE);
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn is_encrypted(root: &Path) -> bool {
    root.join(KEY_FILE).exists()
}

/// The key file of an encrypted vault. A `vault.json` that exists but does
/// not parse is reported as corrupt, not as encryption being off.
fn existing_key_file(root: &Path) -> Result<VaultKeyFile, String> {
    if !is_encrypted(root) {
        return Err("Encryption is not enabled".to_string());
    }
    key_file(root).ok_or_else(|| "Corrupt vault key file".to_string())
}

fn verify_passphrase(file: &VaultKeyFile, passphrase: &str) -> Result<MasterKey, String> {
    let salt = hex_decode(&file.salt_hex)?;
    let key = derive_master_key(passphrase, &salt, &file.kdf)?;
    let nonce = hex_decode(&file.verifier_nonce_hex)?;
    let ct = hex_decode(&file.verifier_ct_hex)?;
    let plain = open(&key, &nonce, &ct).map_err(|e| {
        if e == DECRYPT_FAILED {
            "Wrong passphrase".to_string()
        } else {
            e
        }
    })?;
    if plain != VERIFIER_PLAINTEXT {
        return Err("Wrong passphrase".to_string());
    }
    Ok(key)
}

fn write_key_file(root: &Path, file: &VaultKeyFile) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|e| format!("Failed to create vault: {e}"))?;
    let json = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    let path = root.join(KEY_FILE);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| format!("Failed to write vault key file: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Failed to finalize vault key file: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// encrypted files
//
// The same envelope serves notes and audio: any file becomes
// `magic ‖ wrapped key ‖ nonce ‖ ciphertext`, decrypting to its exact
// original bytes. Notes just give it a path under `notes/`.
// ---------------------------------------------------------------------------

/// Write `bytes` encrypted to `path`. Per-file content key, wrapped with
/// the master key — re-keying later re-wraps 32 bytes per file, not the
/// file bodies.
pub fn write_encrypted_file(path: &Path, master: &MasterKey, bytes: &[u8]) -> Result<(), String> {
    // Filled in place, so no unwiped temporary copy of the key is left behind.
    let file_key = MasterKey({
        let mut k = [0u8; KEY_LEN];
        rand::rng().fill_bytes(&mut k);
        k
    });
    let (wrapped_nonce, wrapped_ct) = seal(master, &file_key.0)?;
    let (content_nonce, content_ct) = seal(&file_key, bytes)?;

    let mut file = Vec::with_capacity(
        NOTE_MAGIC.len() + NONCE_LEN + wrapped_ct.len() + NONCE_LEN + content_ct.len(),
    );
    file.extend_from_slice(NOTE_MAGIC);
    file.extend_from_slice(&wrapped_nonce);
    file.extend_from_slice(&wrapped_ct);
    file.extend_from_slice(&content_nonce);
    file.extend_from_slice(&content_ct);

    let tmp = path.with_extension("enc.tmp");
    fs::write(&tmp, file).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("Failed to finalize {}: {e}", path.display()))?;
    Ok(())
}

/// The exact bytes that were encrypted. The wrong session key fails the
/// tag check, not silently.
pub fn read_encrypted_file(path: &Path, master: &MasterKey) -> Result<Vec<u8>, String> {
    let file = fs::read(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    if file.len() < NOTE_MAGIC.len() + NONCE_LEN + KEY_LEN + 16 + NONCE_LEN + 16
        || &file[..NOTE_MAGIC.len()] != NOTE_MAGIC
    {
        return Err(format!("Corrupt encrypted file {}", path.display()));
    }
    let mut at = NOTE_MAGIC.len();
    let wrapped_nonce = &file[at..at + NONCE_LEN];
    at += NONCE_LEN;
    // 32-byte wrapped key + 16-byte Poly1305 tag: the slice MUST stop at the
    // tag, or the content nonce and ciphertext would be swallowed into it.
    let wrapped_ct = &file[at..at + KEY_LEN + 16];
    at += KEY_LEN + 16;
    let content_nonce = &file[at..at + NONCE_LEN];
    let content_ct = &file[at + NONCE_LEN..];
    let mut file_key_bytes = open(master, wrapped_nonce, wrapped_ct)?;
    let file_key = (file_key_bytes.len() == KEY_LEN).then(|| {
        let mut k = [0u8; KEY_LEN];
        k.copy_from_slice(&file_key_bytes);
        MasterKey(k)
    });
    // The unwrapped key's heap copy is wiped before the Vec frees it.
    wipe(&mut file_key_bytes);
    let file_key = file_key.ok_or("Bad key length")?;
    open(&file_key, content_nonce, content_ct)
}

pub(crate) fn encrypted_note_path(root: &Path, id: &str) -> std::path::PathBuf {
    notes_dir(root).join(format!("{id}.rcl"))
}

pub fn write_note_encrypted(
    root: &Path,
    id: &str,
    master: &MasterKey,
    bytes: &[u8],
) -> Result<(), String> {
    write_encrypted_file(&encrypted_note_path(root, id), master, bytes)
}

pub fn read_note_encrypted(root: &Path, id: &str, master: &MasterKey) -> Result<Vec<u8>, String> {
    read_encrypted_file(&encrypted_note_path(root, id), master)
}

// ---------------------------------------------------------------------------
// toggle
// ---------------------------------------------------------------------------

fn note_ids_in(root: &Path, extension: &str) -> Result<Vec<String>, String> {
    let dir = notes_dir(root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read vault: {e}"))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some(extension)
            && let Some(id) = path.file_stem().and_then(|s| s.to_str())
        {
            ids.push(id.to_string());
        }
    }
    ids.sort();
    Ok(ids)
}

/// Stems of the encrypted note files, sorted — what a locked vault can
/// still enumerate.
pub(crate) fn note_ids_encrypted(root: &Path) -> Result<Vec<String>, String> {
    note_ids_in(root, "rcl")
}

/// Encrypt the whole vault in place. Every `.md` becomes `.rcl` (same
/// stem), and no plaintext copy is kept anywhere: each note's ciphertext is
/// read back and decrypted, and must match the original byte for byte,
/// before the plaintext file is overwritten and deleted — so a failed write
/// stops the run with that note still in plain form instead of losing it.
/// The vault is left unlocked — the user just proved the passphrase.
pub fn enable(root: &Path, passphrase: &str) -> Result<EnableReport, String> {
    if passphrase.chars().count() < MIN_PASSPHRASE_LEN {
        return Err(format!(
            "Use at least {MIN_PASSPHRASE_LEN} characters for the passphrase"
        ));
    }
    if is_encrypted(root) {
        return Err("Encryption is already enabled".to_string());
    }

    let ids = note_ids_in(root, "md")?;

    let salt = random_bytes(SALT_LEN);
    let kdf = KdfParams {
        m_cost_kib: DEFAULT_M_COST_KIB,
        t_cost: DEFAULT_T_COST,
        p_cost: DEFAULT_P_COST,
    };
    let key = derive_master_key(passphrase, &salt, &kdf)?;
    let (verifier_nonce, verifier_ct) = seal(&key, VERIFIER_PLAINTEXT)?;
    write_key_file(
        root,
        &VaultKeyFile {
            version: 1,
            kdf,
            salt_hex: hex_encode(&salt),
            verifier_nonce_hex: hex_encode(&verifier_nonce),
            verifier_ct_hex: hex_encode(&verifier_ct),
        },
    )?;

    let mut encrypted = 0u32;
    for id in &ids {
        let plain_path = notes_dir(root).join(format!("{id}.md"));
        let raw = fs::read(&plain_path).map_err(|e| format!("Failed to read note {id}: {e}"))?;
        write_note_encrypted(root, id, &key, &raw)?;
        // The ciphertext must give the note back before the plaintext goes:
        // with no plaintext copy kept anywhere, this check is the safety net.
        let round_trip = read_encrypted_file(&encrypted_note_path(root, id), &key)?;
        if round_trip != raw {
            return Err(format!(
                "Encrypting note {id} did not round-trip; it was left unencrypted"
            ));
        }
        shred_file(&plain_path)
            .map_err(|e| format!("Failed to remove plaintext note {id}: {e}"))?;
        encrypted += 1;
    }

    // The audio the vault holds is encrypted with the same envelope. A crash
    // mid-conversion is recoverable — every file is either its plain self or
    // a `.rcl`, and `disable` reads both.
    encrypt_audio_dir(root, &key)?;
    remove_legacy_backup(root);

    // The derived index is plaintext metadata (titles, tags). Zero
    // knowledge means it does not survive the toggle; `disable` rebuilds it
    // from the decrypted notes.
    let _ = fs::remove_file(root.join(crate::recall::INDEX_FILE));

    session_unlock_with(key);
    Ok(EnableReport {
        notes_encrypted: encrypted,
    })
}

/// Every recording in `audio/` becomes a `.rcl` beside itself. Idempotent
/// per file: anything already encrypted (or nothing at all) is fine.
fn encrypt_audio_dir(root: &Path, key: &MasterKey) -> Result<u32, String> {
    let audio_dir = root.join(crate::recall::AUDIO_SUBDIR);
    if !audio_dir.exists() {
        return Ok(0);
    }
    let mut count = 0u32;
    for entry in fs::read_dir(&audio_dir)
        .map_err(|e| format!("Failed to read audio folder: {e}"))?
        .flatten()
    {
        let path = entry.path();
        // A sub-folder (or anything else that is not a file) is left alone:
        // failing on it here would abort `enable` with the notes already
        // encrypted and the session not unlocked.
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) == Some("rcl") {
            continue;
        }
        let bytes =
            fs::read(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        let name = path
            .file_name()
            .ok_or_else(|| format!("Bad audio file name {}", path.display()))?
            .to_os_string();
        let dest = audio_dir.join(format!("{}.rcl", name.to_string_lossy()));
        write_encrypted_file(&dest, key, &bytes)?;
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove plaintext audio {}: {e}", path.display()))?;
        count += 1;
    }
    Ok(count)
}

/// The audio goes back to its plain self, original name restored — the
/// inverse of [`encrypt_audio_dir`].
fn decrypt_audio_dir(root: &Path, key: &MasterKey) -> Result<u32, String> {
    let audio_dir = root.join(crate::recall::AUDIO_SUBDIR);
    if !audio_dir.exists() {
        return Ok(0);
    }
    let mut count = 0u32;
    for entry in fs::read_dir(&audio_dir)
        .map_err(|e| format!("Failed to read audio folder: {e}"))?
        .flatten()
    {
        let path = entry.path();
        let name = match path.file_name().map(|n| n.to_string_lossy().to_string()) {
            Some(n) => n,
            None => continue,
        };
        let Some(plain_name) = name.strip_suffix(".rcl") else {
            continue;
        };
        let bytes = read_encrypted_file(&path, key)?;
        let dest = audio_dir.join(plain_name);
        let tmp = dest.with_extension("plain.tmp");
        fs::write(&tmp, &bytes).map_err(|e| format!("Failed to write {}: {e}", dest.display()))?;
        fs::rename(&tmp, &dest)
            .map_err(|e| format!("Failed to finalize {}: {e}", dest.display()))?;
        fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove encrypted audio {}: {e}", path.display()))?;
        count += 1;
    }
    Ok(count)
}

/// The other direction of the toggle, and it must ask for the key it is
/// giving up: every `.rcl` is decrypted back to its `.md` bytes, the key
/// file is removed, and the session is locked. The vault is fully decrypted
/// *before* encryption counts as disabled.
pub fn disable(root: &Path, passphrase: &str) -> Result<u32, String> {
    let file = existing_key_file(root)?;
    let key = verify_passphrase(&file, passphrase)?;

    let ids = note_ids_in(root, "rcl")?;
    let mut decrypted = 0u32;
    for id in &ids {
        let bytes = read_note_encrypted(root, id, &key)?;
        let path = notes_dir(root).join(format!("{id}.md"));
        let tmp = path.with_extension("md.tmp");
        fs::write(&tmp, &bytes).map_err(|e| format!("Failed to write note {id}: {e}"))?;
        fs::rename(&tmp, &path).map_err(|e| format!("Failed to finalize note {id}: {e}"))?;
        fs::remove_file(encrypted_note_path(root, id))
            .map_err(|e| format!("Failed to remove encrypted note {id}: {e}"))?;
        decrypted += 1;
    }

    decrypt_audio_dir(root, &key)?;

    fs::remove_file(root.join(KEY_FILE)).map_err(|e| format!("Failed to remove key file: {e}"))?;
    session_lock();

    // The index cache comes back now that the metadata is plaintext again.
    crate::recall::rebuild_index(root)?;
    Ok(decrypted)
}

/// Verify the passphrase against the key file's verifier and hold the key
/// in memory until [`session_lock`] or app exit.
pub fn unlock(root: &Path, passphrase: &str) -> Result<(), String> {
    let file = existing_key_file(root)?;
    let key = verify_passphrase(&file, passphrase)?;
    session_unlock_with(key);
    // A vault encrypted by an earlier version may still hold that version's
    // plaintext backup of every note; it goes on the owner's first unlock.
    remove_legacy_backup(root);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recall::{
        create_note, create_note_any, delete_note_any, list_notes_any, read_note, read_note_any,
        save_transcription_any, write_note, write_note_any,
    };
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("zer0-recall-enc-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    const PASS: &str = "correct horse battery";

    #[test]
    fn enable_encrypts_disable_restores_bytes() {
        let _guard = TEST_SERIAL.lock().unwrap();
        let root = temp_root("toggle");
        let meta = create_note(&root, "Secret", &["tag".into()]).unwrap();
        write_note(
            &root,
            &meta.id,
            "Secret",
            &["tag".into()],
            "Body line\n\nsecond",
        )
        .unwrap();
        // A recording saved with the note: the vault copies it into audio/,
        // and the toggle must carry it both ways with the notes.
        let recording = root.join("source-recording.wav");
        fs::write(&recording, b"fake wav bytes").unwrap();
        let note_meta = save_transcription_any(
            &root,
            "spoken words",
            None,
            Some("test-model".to_string()),
            None,
            &[],
            Some(&recording),
        )
        .unwrap();
        assert_eq!(
            note_meta.audio_file.as_deref(),
            Some("source-recording.wav")
        );
        assert!(root.join("audio").join("source-recording.wav").exists());

        let report = enable(&root, PASS).unwrap();
        assert_eq!(report.notes_encrypted, 2); // the note + the saved transcription
        assert!(is_encrypted(&root));
        // The plaintext is gone from the notes folder; ciphertext is not the note.
        assert!(!notes_dir(&root).join(format!("{}.md", meta.id)).exists());
        let rcl = fs::read(encrypted_note_path(&root, &meta.id)).unwrap();
        assert!(!windows_contains(&rcl, b"Body line"));
        // No plaintext copy anywhere: no backup folder, no .md left behind.
        assert_eq!(report.notes_encrypted, 2);
        assert!(!root.join(LEGACY_BACKUP_DIR).exists());
        assert!(note_ids_in(&root, "md").unwrap().is_empty());
        // The audio is ciphertext too, and the plaintext index is gone.
        assert!(!root.join("audio").join("source-recording.wav").exists());
        assert!(root.join("audio").join("source-recording.wav.rcl").exists());
        assert!(!root.join(crate::recall::INDEX_FILE).exists());

        // Locked: nothing reads.
        session_lock();
        assert!(read_note_any(&root, &meta.id).is_err());
        assert!(matches!(list_notes_any(&root), Ok((_, true))));

        // Unlocked session: the note reads and writes transparently.
        unlock(&root, PASS).unwrap();
        let note = read_note_any(&root, &meta.id).unwrap();
        assert_eq!(note.meta.title, "Secret");
        assert_eq!(note.body, "Body line\n\nsecond\n");
        write_note_any(&root, &meta.id, "Secret", &["tag".into()], "Edited").unwrap();
        assert_eq!(read_note_any(&root, &meta.id).unwrap().body, "Edited\n");

        // Wrong passphrase refuses to decrypt.
        assert!(disable(&root, "wrong-passphrase").is_err());

        // The right passphrase restores the plain vault.
        let count = disable(&root, PASS).unwrap();
        assert_eq!(count, 2);
        assert!(!is_encrypted(&root));
        // The audio is a plain recording again, bytes restored, and the
        // metadata index was rebuilt from the decrypted notes.
        assert!(root.join("audio").join("source-recording.wav").exists());
        assert!(!root.join("audio").join("source-recording.wav.rcl").exists());
        assert_eq!(
            fs::read(root.join("audio").join("source-recording.wav")).unwrap(),
            b"fake wav bytes"
        );
        assert!(root.join(crate::recall::INDEX_FILE).exists());
        let after = fs::read(notes_dir(&root).join(format!("{}.md", meta.id))).unwrap();
        // Byte-faithful: what went in is what came out (modulo the edit).
        let note = read_note(&root, &meta.id).unwrap();
        assert_eq!(note.body, "Edited\n");
        assert_eq!(note.meta.title, "Secret");
        drop(after);
        let _ = fs::remove_dir_all(&root);
    }

    fn windows_contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    /// A vault encrypted by an earlier version still holds that version's
    /// plaintext `backup/`; the owner's unlock removes it.
    #[test]
    fn unlock_removes_a_legacy_plaintext_backup() {
        let root = temp_root("legacy-backup");
        create_note(&root, "Old", &[]).unwrap();
        enable(&root, PASS).unwrap();
        let legacy = root.join(LEGACY_BACKUP_DIR).join("20260901-120000");
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("old.md"), "plaintext").unwrap();
        session_lock();
        unlock(&root, PASS).unwrap();
        assert!(!root.join(LEGACY_BACKUP_DIR).exists());
        session_lock();
        let _ = fs::remove_dir_all(&root);
    }

    /// The session key is a process-global, so the tests that exercise it
    /// must not run against each other in parallel.
    static TEST_SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn wrong_passphrase_never_unlocks() {
        let _guard = TEST_SERIAL.lock().unwrap();
        let root = temp_root("wrong");
        create_note(&root, "N", &[]).unwrap();
        enable(&root, PASS).unwrap();
        session_lock();
        assert!(unlock(&root, "nope-nope-nope").is_err());
        assert!(!is_unlocked());
        unlock(&root, PASS).unwrap();
        assert!(is_unlocked());
        session_lock();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn writes_require_unlock_and_save_transcription_encrypts() {
        let _guard = TEST_SERIAL.lock().unwrap();
        let root = temp_root("write");
        // A plain vault accepts the write; encryption has not been enabled.
        assert!(create_note_any(&root, "X", &[]).is_ok());
        session_lock();
        enable(&root, PASS).unwrap();
        let meta =
            save_transcription_any(&root, "spoken words", None, None, None, &[], None).unwrap();
        assert_eq!(meta.word_count, 2);
        assert!(encrypted_note_path(&root, &meta.id).exists());
        let (notes, locked) = list_notes_any(&root).unwrap();
        assert!(!locked);
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].title, "spoken words");
        // Locked, writers refuse — the notes are ciphertext the session
        // cannot open, and Save-to-Recall must not look like it succeeded.
        session_lock();
        assert!(create_note_any(&root, "Y", &[]).is_err());
        assert!(save_transcription_any(&root, "more", None, None, None, &[], None).is_err());
        unlock(&root, PASS).unwrap();
        delete_note_any(&root, &meta.id).unwrap();
        assert_eq!(list_notes_any(&root).unwrap().0.len(), 1);
        session_lock();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn short_passphrases_are_refused() {
        let root = temp_root("short");
        assert!(enable(&root, "short").is_err());
        assert!(!is_encrypted(&root));
        // Counted in characters: three 3-byte characters are still too short.
        assert!(enable(&root, "日本語").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn malformed_hex_is_an_error_not_a_panic() {
        assert_eq!(hex_decode("0aff").unwrap(), vec![0x0a, 0xff]);
        assert!(hex_decode("abc").is_err());
        assert!(hex_decode("é0").is_err());
        assert!(hex_decode("zz").is_err());
    }

    #[test]
    fn unparseable_key_file_is_reported_as_corrupt() {
        let root = temp_root("corrupt-key");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(KEY_FILE), "{ not json").unwrap();
        assert_eq!(unlock(&root, PASS).unwrap_err(), "Corrupt vault key file");
        assert_eq!(disable(&root, PASS).unwrap_err(), "Corrupt vault key file");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn wipe_zeroes_key_material() {
        let mut bytes = [0xA5u8; KEY_LEN];
        wipe(&mut bytes);
        assert_eq!(bytes, [0u8; KEY_LEN]);
    }
}
