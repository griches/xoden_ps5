//! Existing private keyring format with explicit host-owned atomic persistence.
//! The sample Store is memory-only: failed commits and Drop never rewrite disk.
use super::keychain::{decode_secret, encode_secret};
use crate::tokens::store::{TokenBackend, TokenStoreError};
use keyring_core::{api::CredentialStoreApi, sample::Store};
use std::{
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
};

const LIMIT: usize = 8 * 1024 * 1024;
type Commit = Arc<dyn Fn(&[u8]) -> bool + Send + Sync>;
pub struct HostFileBackend {
    store: Arc<Store>,
    commit: Commit,
    gate: Mutex<()>,
}
fn invalid() -> TokenStoreError {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Invalid private credential snapshot",
    )
    .into()
}
impl HostFileBackend {
    /// The host selects the private path and supplies a policy-checked atomic
    /// writer for that same file. Missing/symlinked/oversized stores are refused.
    pub fn open(path: &Path, commit: Commit) -> Result<Self, TokenStoreError> {
        if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(invalid());
        }
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let file = options.open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > LIMIT as u64 {
            return Err(invalid());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if metadata.uid() != unsafe { libc::geteuid() }
                || metadata.permissions().mode() & 0o077 != 0
            {
                return Err(invalid());
            }
        }
        let mut bytes = Vec::new();
        file.take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > LIMIT {
            return Err(invalid());
        }
        let credentials = ron::de::from_bytes(&bytes).map_err(|_| invalid())?;
        // No backing path: keyring-core's implicit Drop save is disabled.
        let store = Store::new_internal(credentials, None);
        Ok(Self {
            store,
            commit,
            gate: Mutex::new(()),
        })
    }
    fn entry(&self, key: &str) -> Result<keyring_core::Entry, TokenStoreError> {
        Ok(self.store.build(crate::secrets::SERVICE_NAME, key, None)?)
    }
    fn persist(&self) -> Result<(), TokenStoreError> {
        let bytes = ron::ser::to_string(&self.store.creds).map_err(|_| invalid())?;
        if bytes.len() > LIMIT {
            return Err(invalid());
        }
        if !(self.commit)(bytes.as_bytes()) {
            return Err(std::io::Error::other("Private credential commit failed").into());
        }
        Ok(())
    }
    fn change(&self, key: &str, value: Option<&[u8]>) -> Result<(), TokenStoreError> {
        let _guard = self.gate.lock().unwrap_or_else(|p| p.into_inner());
        let entry = self.entry(key)?;
        let previous = match entry.get_secret() {
            Ok(bytes) => Some(bytes),
            Err(keyring_core::Error::NoEntry) => None,
            Err(error) => return Err(error.into()),
        };
        let encoded = value.map(encode_secret);
        match &encoded {
            Some(bytes) => entry.set_secret(bytes)?,
            None => match entry.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => (),
                Err(error) => return Err(error.into()),
            },
        }
        if let Err(error) = self.persist() {
            // Restore the in-memory view too. The host's failed atomic commit
            // leaves its original file untouched; this Store never saves at Drop.
            match previous {
                Some(bytes) => entry.set_secret(&bytes)?,
                None => match entry.delete_credential() {
                    Ok(()) | Err(keyring_core::Error::NoEntry) => (),
                    Err(error) => return Err(error.into()),
                },
            }
            return Err(error);
        }
        Ok(())
    }
}
impl TokenBackend for HostFileBackend {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, TokenStoreError> {
        let _guard = self.gate.lock().unwrap_or_else(|p| p.into_inner());
        match self.entry(key)?.get_secret() {
            Ok(bytes) => Ok(Some(decode_secret(bytes)?)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn set(&self, key: &str, value: &[u8]) -> Result<(), TokenStoreError> {
        self.change(key, Some(value))
    }
    fn remove(&self, key: &str) -> Result<(), TokenStoreError> {
        self.change(key, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        path::PathBuf,
        sync::atomic::{AtomicBool, Ordering},
    };
    struct Folder(PathBuf);
    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> (Folder, PathBuf) {
        let root = std::env::temp_dir().join(format!("xodus-host-file-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("synthetic-keyring.ron");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).unwrap().write_all(b"{}").unwrap();
        (Folder(root), path)
    }
    fn commit(path: &Path, bytes: &[u8]) -> bool {
        let temp = path.with_extension("next");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        drop(file);
        std::fs::rename(temp, path).unwrap();
        true
    }
    #[test]
    fn credential_host_file_roundtrip_preserves_sample_format_without_global_keyring() {
        let (_folder, path) = fixture();
        let target = path.clone();
        let before = keyring_core::get_default_store();
        let backend =
            HostFileBackend::open(&path, Arc::new(move |bytes| commit(&target, bytes))).unwrap();
        backend
            .set("active-account", b"synthetic-old-account")
            .unwrap();
        backend
            .set("phone-candidate", b"synthetic-new-phone")
            .unwrap();
        let store = Store::new_with_backing(path.to_str().unwrap()).unwrap();
        let entry = store
            .build(crate::secrets::SERVICE_NAME, "active-account", None)
            .unwrap();
        assert_eq!(
            decode_secret(entry.get_secret().unwrap()).unwrap(),
            b"synthetic-old-account"
        );
        let after = keyring_core::get_default_store();
        assert!(match (before, after) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(&a, &b),
            _ => false,
        });
        drop(entry);
        drop(store);
        drop(backend);
        let reloaded =
            HostFileBackend::open(&path, Arc::new(|_| panic!("reads must never commit"))).unwrap();
        assert_eq!(
            reloaded.get("phone-candidate").unwrap().unwrap(),
            b"synthetic-new-phone"
        );
        assert!(reloaded.get("missing").unwrap().is_none());
    }
    #[test]
    fn credential_host_file_failed_commit_rolls_back_memory_and_drop_keeps_original_bytes() {
        let (_folder, path) = fixture();
        let target = path.clone();
        let fail = Arc::new(AtomicBool::new(false));
        let injected = fail.clone();
        let backend = HostFileBackend::open(
            &path,
            Arc::new(move |bytes| !injected.load(Ordering::SeqCst) && commit(&target, bytes)),
        )
        .unwrap();
        backend
            .set("active-account", b"synthetic-old-account")
            .unwrap();
        let original = std::fs::read(&path).unwrap();
        fail.store(true, Ordering::SeqCst);
        assert!(
            backend
                .set("phone-candidate", b"synthetic-new-secret")
                .is_err()
        );
        assert!(backend.get("phone-candidate").unwrap().is_none());
        assert!(backend.remove("active-account").is_err());
        assert_eq!(
            backend.get("active-account").unwrap().unwrap(),
            b"synthetic-old-account"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
        drop(backend);
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    #[test]
    fn credential_host_file_refuses_missing_corrupt_oversized_or_public_files() {
        let (_folder, path) = fixture();
        let no_commit: Commit = Arc::new(|_| panic!("validation must never commit"));
        assert!(HostFileBackend::open(&path.with_extension("missing"), no_commit.clone()).is_err());
        std::fs::write(&path, b"synthetic-secret-not-ron").unwrap();
        let error = HostFileBackend::open(&path, no_commit.clone())
            .err()
            .unwrap();
        assert!(!error.to_string().contains("synthetic-secret"));
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(LIMIT as u64 + 1)
            .unwrap();
        assert!(HostFileBackend::open(&path, no_commit.clone()).is_err());
        std::fs::write(&path, b"{}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let link = path.with_extension("link");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(HostFileBackend::open(&link, no_commit.clone()).is_err());
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(HostFileBackend::open(&path, no_commit).is_err());
        }
    }
}
