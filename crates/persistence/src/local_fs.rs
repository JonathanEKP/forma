//! Local filesystem object store (plan section 8.6).

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::error::{PersistenceError, Result};
use crate::traits::ObjectStore;

pub struct LocalObjectStore {
    root: PathBuf,
}

impl LocalObjectStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolves a key inside the root, refusing anything that would escape it.
    ///
    /// Keys come from the database, but a traversal here would turn a bad row
    /// into arbitrary filesystem access, so it is checked rather than trusted.
    fn resolve(&self, key: &str) -> Result<PathBuf> {
        if key.is_empty() {
            return Err(PersistenceError::Storage("empty storage key".to_string()));
        }

        let mut path = self.root.clone();
        for part in Path::new(key).components() {
            match part {
                std::path::Component::Normal(segment) => path.push(segment),
                _ => {
                    return Err(PersistenceError::Storage(format!(
                        "storage key {key:?} must be a plain relative path"
                    )))
                }
            }
        }
        Ok(path)
    }
}

#[async_trait]
impl ObjectStore for LocalObjectStore {
    async fn put(&self, key: &str, bytes: &[u8]) -> Result<()> {
        let path = self.resolve(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| PersistenceError::Storage(format!("{}: {e}", parent.display())))?;
        }
        tokio::fs::write(&path, bytes)
            .await
            .map_err(|e| PersistenceError::Storage(format!("{}: {e}", path.display())))
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let path = self.resolve(key)?;
        tokio::fs::read(&path)
            .await
            .map_err(|e| PersistenceError::Storage(format!("{}: {e}", path.display())))
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self.resolve(key)?.exists())
    }

    /// A key ending in `/` names a prefix rather than a file, which is how an
    /// all-pages export records itself (plan section 71: one directory of PNGs
    /// that a future ZIP replaces). Deleting one has to take the whole tree, or
    /// the files outlive the row that pointed at them.
    async fn delete(&self, key: &str) -> Result<()> {
        let path = self.resolve(key)?;
        if key.ends_with('/') {
            return match tokio::fs::remove_dir_all(&path).await {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(PersistenceError::Storage(format!(
                    "{}: {e}",
                    path.display()
                ))),
            };
        }
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(PersistenceError::Storage(format!(
                "{}: {e}",
                path.display()
            ))),
        }
    }
}

/// Where an asset's bytes live, sharded by hash so one directory never holds
/// every file (plan section 44).
///
/// Real hashes are 64 hex characters, but this must never panic on a short one:
/// a library that aborts on unexpected input takes the server down with it.
pub fn asset_storage_key(hash: &str, extension: &str) -> String {
    let shard = |range: std::ops::Range<usize>| hash.get(range).unwrap_or("__");
    format!("assets/{}/{}/{hash}.{extension}", shard(0..2), shard(2..4))
}

pub fn export_storage_key(export_id: &str, filename: &str) -> String {
    format!("exports/{export_id}/{filename}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trips_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalObjectStore::new(dir.path());

        store.put("assets/ab/cd/x.png", b"bytes").await.unwrap();
        assert!(store.exists("assets/ab/cd/x.png").await.unwrap());
        assert_eq!(store.get("assets/ab/cd/x.png").await.unwrap(), b"bytes");

        store.delete("assets/ab/cd/x.png").await.unwrap();
        assert!(!store.exists("assets/ab/cd/x.png").await.unwrap());
    }

    #[tokio::test]
    async fn deleting_a_missing_key_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalObjectStore::new(dir.path());
        assert!(store.delete("assets/nothing.png").await.is_ok());
    }

    #[tokio::test]
    async fn refuses_to_escape_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let store = LocalObjectStore::new(dir.path());

        for key in ["../escape.png", "/etc/passwd", "a/../../b.png", ""] {
            assert!(
                store.put(key, b"x").await.is_err(),
                "key {key:?} should have been refused"
            );
        }
    }

    #[test]
    fn shards_asset_keys_by_hash() {
        assert_eq!(
            asset_storage_key("abcdef123456", "png"),
            "assets/ab/cd/abcdef123456.png"
        );
    }

    #[test]
    fn a_short_hash_does_not_panic() {
        // Never reachable with BLAKE3, but a panic here would abort the server.
        for hash in ["", "a", "abc"] {
            let key = asset_storage_key(hash, "png");
            assert!(key.starts_with("assets/"), "{key}");
        }
    }
}
