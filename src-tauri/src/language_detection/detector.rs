//! The spoken-language detector model (VoxLingua107 ECAPA), loaded in-process
//! on the CPU when first needed and unloaded with the transcription model.

use anyhow::Result;
use log::{debug, info};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use transcribe_cpp::{Backend, LangIdOptions, LangIdSession, Model, ModelOptions};

use super::Identified;

#[derive(Default)]
pub struct LanguageDetector {
    // `LangIdSession` is Send but not Sync.
    loaded: Mutex<Option<Loaded>>,
    /// An unload is pending: it found a run holding the model, so that run
    /// drops it once it lets go of the lock.
    unload_requested: AtomicBool,
}

struct Loaded {
    path: PathBuf,
    session: LangIdSession,
    /// The last answer, keyed by what it was asked: the Multi-STT slots of one
    /// recording hear the same speech, so they share one detector run.
    last: Option<(u64, Vec<String>, Identified)>,
}

impl Loaded {
    fn open(path: &Path) -> Result<Self> {
        let started = Instant::now();
        let model = Model::load_with(
            path,
            &ModelOptions {
                backend: Backend::Cpu,
                device: None,
                ..Default::default()
            },
        )
        .map_err(|e| anyhow::anyhow!("Failed to load the language detector: {e}"))?;
        // The session keeps the native model alive on its own.
        let session = model
            .langid_session()
            .map_err(|e| anyhow::anyhow!("Model is not a language detector: {e}"))?;
        info!(
            "Loaded the language detector {} in {} ms",
            path.file_name().unwrap_or_default().to_string_lossy(),
            started.elapsed().as_millis()
        );
        Ok(Self {
            path: path.to_path_buf(),
            session,
            last: None,
        })
    }
}

/// A cheap identity for a speech window: its length and an FNV-1a hash of
/// every 61st sample's bits. Only ever compared with the previous window.
fn fingerprint(speech: &[f32]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325 ^ speech.len() as u64;
    for sample in speech.iter().step_by(61) {
        hash ^= u64::from(sample.to_bits());
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

impl LanguageDetector {
    /// Rank `allowed` (the detector's labels) for 16 kHz mono `speech`,
    /// loading the model at `path` first if it isn't loaded.
    pub fn identify(&self, path: &Path, speech: &[f32], allowed: &[String]) -> Result<Identified> {
        let result = self.run(path, speech, allowed);
        // Checked only after the lock is released: an unload that found it
        // held posted its request first, so it can't be missed here.
        if self.unload_requested.load(Ordering::SeqCst) {
            self.unload();
        }
        result
    }

    fn run(&self, path: &Path, speech: &[f32], allowed: &[String]) -> Result<Identified> {
        let mut guard = self.loaded.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.as_ref().is_some_and(|l| l.path != path) {
            *guard = None;
        }
        let loaded = match &mut *guard {
            Some(loaded) => loaded,
            slot @ None => slot.insert(Loaded::open(path)?),
        };
        let key = fingerprint(speech);
        if let Some((last_key, last_allowed, identified)) = &loaded.last
            && *last_key == key
            && last_allowed == allowed
        {
            return Ok(Identified {
                elapsed: Duration::ZERO,
                ..identified.clone()
            });
        }
        let started = Instant::now();
        let result = loaded
            .session
            .run(
                speech,
                &LangIdOptions {
                    allowed: Some(allowed.to_vec()),
                },
            )
            .map_err(|e| anyhow::anyhow!("language detection: {e}"))?;
        let identified = Identified {
            candidates: result
                .candidates
                .into_iter()
                .map(|c| (c.code, c.p))
                .collect(),
            allowed_mass: result.allowed_mass,
            elapsed: started.elapsed(),
        };
        loaded.last = Some((key, allowed.to_vec(), identified.clone()));
        Ok(identified)
    }

    /// Never waits: an unload that lands mid-run is done by the run when it
    /// ends, so a model unload never blocks its caller.
    pub fn unload(&self) {
        // Posted before trying the lock, so a run that releases it in between
        // still sees the request.
        self.unload_requested.store(true, Ordering::SeqCst);
        let mut loaded = match self.loaded.try_lock() {
            Ok(loaded) => loaded,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        self.unload_requested.store(false, Ordering::SeqCst);
        if loaded.take().is_some() {
            debug!("Language detector unloaded");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opt-in: the real detector over real speech. Point the `PROBE_LANGID_MODEL`
    /// flag at the detector's GGUF and `PROBE_LANGID_DIR` at a folder of
    /// `fleurs-<code>.wav` clips; each clip present must be identified as its
    /// own language among all of them. Skips when either flag is unset.
    #[test]
    fn probe_real_detector() {
        let (Some(model), Some(dir)) = (
            crate::utils::app_env_var("PROBE_LANGID_MODEL"),
            crate::utils::app_env_var("PROBE_LANGID_DIR"),
        ) else {
            eprintln!("skipped: set PROBE_LANGID_MODEL and PROBE_LANGID_DIR");
            return;
        };
        let codes = ["de", "en", "es", "fr", "ja", "ru", "zh"];
        let allowed: Vec<String> = codes.iter().map(|c| c.to_string()).collect();
        crate::managers::transcription::init_transcribe_backend();
        let detector = LanguageDetector::default();
        let mut checked = 0;
        for code in codes {
            let wav = std::path::Path::new(&dir).join(format!("fleurs-{code}.wav"));
            if !wav.is_file() {
                continue;
            }
            let audio = crate::audio_toolkit::read_wav_samples(&wav).expect("read wav");
            let speech = crate::language_detection::speech_head(&audio, 5 * 16_000);
            let identified = detector
                .identify(std::path::Path::new(&model), &speech, &allowed)
                .expect("identify");
            eprintln!(
                "{code}: {:?} ({:.2} s of speech, {} ms)",
                &identified.candidates[..2],
                speech.len() as f32 / 16_000.0,
                identified.elapsed.as_millis()
            );
            assert_eq!(identified.candidates[0].0, code, "{}", wav.display());
            checked += 1;
        }
        assert!(checked > 0, "no fleurs-<code>.wav in {dir}");
        detector.unload();
    }

    #[test]
    fn the_fingerprint_tells_windows_apart() {
        let a = vec![0.1f32; 16_000];
        let mut b = a.clone();
        b[61 * 7] = 0.2;
        assert_eq!(fingerprint(&a), fingerprint(&a.clone()));
        assert_ne!(fingerprint(&a), fingerprint(&b));
        assert_ne!(fingerprint(&a), fingerprint(&a[..15_999]));
    }
}
