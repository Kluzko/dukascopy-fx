//! Checkpoint storage for incremental fetch flows.

use crate::error::DukascopyError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Abstraction for persisting per-stream checkpoints.
pub trait CheckpointStore: Send + Sync {
    /// Reads checkpoint for a key.
    fn get(&self, key: &str) -> Result<Option<DateTime<Utc>>, DukascopyError>;
    /// Persists checkpoint for a key.
    fn set(&self, key: &str, timestamp: DateTime<Utc>) -> Result<(), DukascopyError>;
    /// Persists multiple checkpoints in one operation.
    fn set_many(&self, updates: &[(String, DateTime<Utc>)]) -> Result<(), DukascopyError> {
        for (key, timestamp) in updates {
            self.set(key, timestamp.to_owned())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CheckpointState {
    checkpoints: HashMap<String, DateTime<Utc>>,
}

/// JSON file-based checkpoint store.
#[derive(Debug)]
pub struct FileCheckpointStore {
    path: PathBuf,
    state: Mutex<CheckpointState>,
}

impl FileCheckpointStore {
    /// Opens a file checkpoint store from a path. Creates an empty store when file does not exist.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DukascopyError> {
        let path = path.as_ref().to_path_buf();
        let state = if path.exists() {
            let content = fs::read_to_string(&path).map_err(|err| {
                DukascopyError::Unknown(format!(
                    "Failed to read checkpoint file '{}': {}",
                    path.display(),
                    err
                ))
            })?;
            serde_json::from_str::<CheckpointState>(&content).map_err(|err| {
                DukascopyError::InvalidRequest(format!(
                    "Invalid checkpoint file '{}': {}",
                    path.display(),
                    err
                ))
            })?
        } else {
            CheckpointState::default()
        };

        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    fn persist(&self, state: &CheckpointState) -> Result<(), DukascopyError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                DukascopyError::Unknown(format!(
                    "Failed to create checkpoint directory '{}': {}",
                    parent.display(),
                    err
                ))
            })?;
        }

        let temp_path = self.path.with_extension("tmp");
        let content = serde_json::to_string_pretty(state).map_err(|err| {
            DukascopyError::Unknown(format!("Failed to serialize checkpoint state: {}", err))
        })?;

        fs::write(&temp_path, content).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to write checkpoint temp file '{}': {}",
                temp_path.display(),
                err
            ))
        })?;

        fs::rename(&temp_path, &self.path).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to replace checkpoint file '{}': {}",
                self.path.display(),
                err
            ))
        })?;

        Ok(())
    }
}

impl CheckpointStore for FileCheckpointStore {
    fn get(&self, key: &str) -> Result<Option<DateTime<Utc>>, DukascopyError> {
        let state = self
            .state
            .lock()
            .map_err(|err| DukascopyError::Unknown(format!("Checkpoint lock poisoned: {}", err)))?;
        Ok(state.checkpoints.get(key).cloned())
    }

    fn set(&self, key: &str, timestamp: DateTime<Utc>) -> Result<(), DukascopyError> {
        self.set_many(&[(key.to_string(), timestamp)])
    }

    fn set_many(&self, updates: &[(String, DateTime<Utc>)]) -> Result<(), DukascopyError> {
        let mut state = self
            .state
            .lock()
            .map_err(|err| DukascopyError::Unknown(format!("Checkpoint lock poisoned: {}", err)))?;
        for (key, timestamp) in updates {
            state.checkpoints.insert(key.clone(), timestamp.to_owned());
        }
        self.persist(&state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_set_and_get_checkpoint() {
        let unique = format!(
            "dukascopy_fx_checkpoint_test_{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);

        let store = FileCheckpointStore::open(&path).unwrap();
        let ts = Utc.with_ymd_and_hms(2025, 1, 3, 12, 0, 0).unwrap();
        store.set("EURUSD:3600", ts).unwrap();

        let loaded = FileCheckpointStore::open(&path).unwrap();
        assert_eq!(loaded.get("EURUSD:3600").unwrap(), Some(ts));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_set_many_persists_all_checkpoints() {
        let unique = format!(
            "dukascopy_fx_checkpoint_test_many_{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);

        let store = FileCheckpointStore::open(&path).unwrap();
        let ts1 = Utc.with_ymd_and_hms(2025, 1, 3, 12, 0, 0).unwrap();
        let ts2 = Utc.with_ymd_and_hms(2025, 1, 4, 12, 0, 0).unwrap();
        let updates = vec![
            ("EURUSD:3600".to_string(), ts1),
            ("GBPUSD:3600".to_string(), ts2),
        ];
        store.set_many(&updates).unwrap();

        let loaded = FileCheckpointStore::open(&path).unwrap();
        assert_eq!(loaded.get("EURUSD:3600").unwrap(), Some(ts1));
        assert_eq!(loaded.get("GBPUSD:3600").unwrap(), Some(ts2));

        let _ = fs::remove_file(path);
    }
}
