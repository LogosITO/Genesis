//! Versioned, bounded JSON saves. Files are created once and never overwrite a previous save.

use crate::Runtime;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

/// Maximum encoded save size, in bytes.
pub const MAX_SAVE_BYTES: usize = 64 * 1024;
const FORMAT_VERSION: u32 = 2;

/// Save and load failure; a failed load leaves an existing runtime unchanged.
#[derive(Debug)]
pub enum PersistenceError {
    /// Filesystem operation failed.
    Io(std::io::Error),
    /// JSON syntax or type mismatch.
    Json(serde_json::Error),
    /// Unsupported version number.
    Version(u32),
    /// Invalid mathematical or relational state.
    InvalidState(&'static str),
    /// Encoded input exceeds the fixed size budget.
    TooLarge,
}
impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for PersistenceError {}
impl From<std::io::Error> for PersistenceError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<serde_json::Error> for PersistenceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SaveFile {
    format_version: u32,
    runtime: Runtime,
}

impl Runtime {
    /// Encodes a validated version-2 save. JSON is for inspection, not a stable public API.
    pub fn save_bytes(&self) -> Result<Vec<u8>, PersistenceError> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(&SaveFile {
            format_version: FORMAT_VERSION,
            runtime: self.clone(),
        })?;
        if bytes.len() > MAX_SAVE_BYTES {
            return Err(PersistenceError::TooLarge);
        }
        Ok(bytes)
    }
    /// Parses and validates a bounded complete save before returning state.
    pub fn load_bytes(bytes: &[u8]) -> Result<Self, PersistenceError> {
        if bytes.len() > MAX_SAVE_BYTES {
            return Err(PersistenceError::TooLarge);
        }
        let save: SaveFile = serde_json::from_slice(bytes)?;
        if !(1..=FORMAT_VERSION).contains(&save.format_version) {
            return Err(PersistenceError::Version(save.format_version));
        }
        save.runtime.validate()?;
        Ok(save.runtime)
    }
    /// Saves to a new path via a synced temporary file and hard link. Existing saves are never overwritten.
    pub fn save_new(&self, path: impl AsRef<Path>) -> Result<(), PersistenceError> {
        let bytes = self.save_bytes()?;
        let path = path.as_ref();
        let temp = path.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| PersistenceError::InvalidState("system clock"))?
                .as_nanos()
        ));
        let result = (|| -> Result<(), PersistenceError> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::hard_link(&temp, path)?;
            Ok(())
        })();
        let _ = fs::remove_file(&temp);
        result
    }
    /// Reads at most the save-size budget, then replaces nothing until full validation succeeds.
    pub fn load_file(path: impl AsRef<Path>) -> Result<Self, PersistenceError> {
        let file = fs::File::open(path)?;
        let mut bytes = Vec::new();
        std::io::Read::take(file, (MAX_SAVE_BYTES + 1) as u64).read_to_end(&mut bytes)?;
        Self::load_bytes(&bytes)
    }
    /// Replaces a running state only after complete parse and validation.
    pub fn load_into(&mut self, bytes: &[u8]) -> Result<(), PersistenceError> {
        *self = Self::load_bytes(bytes)?;
        Ok(())
    }
}
