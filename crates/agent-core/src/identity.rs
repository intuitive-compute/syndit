use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(thiserror::Error, Debug)]
pub enum IdentityError {
    #[error("invalid key length: expected 32 bytes, got {0}")]
    InvalidKeyLength(usize),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("refusing to derive a key path from agent_id with no usable characters")]
    EmptyAgentId,
}

pub struct AgentIdentity {
    pub agent_id: String,
    pub signing_key: SigningKey,
}

impl AgentIdentity {
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }
}

pub struct KeyStore;

impl KeyStore {
    pub fn default_key_path(agent_id: &str) -> Result<PathBuf, IdentityError> {
        let safe: String = agent_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        if safe.is_empty() {
            return Err(IdentityError::EmptyAgentId);
        }
        let base = dirs::config_dir().ok_or_else(|| {
            IdentityError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no config dir",
            ))
        })?;
        Ok(base.join("syndit").join(safe).join("key"))
    }

    pub fn load(path: &Path) -> Result<SigningKey, IdentityError> {
        if !path.exists() {
            return Err(IdentityError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("key file not found: {}", path.display()),
            )));
        }
        tighten_perms_if_loose(path)?;
        let bytes = fs::read(path)?;
        if bytes.len() != 32 {
            return Err(IdentityError::InvalidKeyLength(bytes.len()));
        }
        let mut buf = [0u8; 32];
        buf.copy_from_slice(&bytes);
        Ok(SigningKey::from_bytes(&buf))
    }

    pub fn load_or_generate(path: &Path) -> Result<SigningKey, IdentityError> {
        if path.exists() {
            tighten_perms_if_loose(path)?;
            let bytes = fs::read(path)?;
            if bytes.len() != 32 {
                return Err(IdentityError::InvalidKeyLength(bytes.len()));
            }
            let mut buf = [0u8; 32];
            buf.copy_from_slice(&bytes);
            Ok(SigningKey::from_bytes(&buf))
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let key = SigningKey::generate(&mut OsRng);
            write_key_file(path, &key.to_bytes())?;
            Ok(key)
        }
    }
}

#[cfg(unix)]
fn write_key_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)
}

#[cfg(not(unix))]
fn write_key_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(bytes)
}

#[cfg(unix)]
fn tighten_perms_if_loose(path: &Path) -> Result<(), IdentityError> {
    use std::os::unix::fs::PermissionsExt;
    let current = fs::metadata(path)?.permissions().mode() & 0o777;
    if current & 0o077 != 0 {
        eprintln!(
            "agent-core: tightening key file perms on {} from {:#o} to 0o600",
            path.display(),
            current
        );
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn tighten_perms_if_loose(_path: &Path) -> Result<(), IdentityError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_key_path_sanitizes_traversal() {
        let p = KeyStore::default_key_path("..").unwrap();
        assert!(p.ends_with("syndit/__/key"), "got: {}", p.display());
        let p = KeyStore::default_key_path(".").unwrap();
        assert!(p.ends_with("syndit/_/key"), "got: {}", p.display());
        let p = KeyStore::default_key_path("agent:local:joseph").unwrap();
        assert!(p.ends_with("syndit/agent_local_joseph/key"), "got: {}", p.display());
    }

    #[test]
    fn default_key_path_rejects_empty_after_sanitization() {
        let err = KeyStore::default_key_path("").unwrap_err();
        assert!(matches!(err, IdentityError::EmptyAgentId));
    }

    #[test]
    #[cfg(unix)]
    fn new_key_file_is_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        let _ = KeyStore::load_or_generate(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0o600, got {mode:#o}");
    }

    #[test]
    #[cfg(unix)]
    fn loose_perms_are_tightened_on_load() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        fs::write(&path, [0u8; 32]).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&path, perms).unwrap();
        let _ = KeyStore::load_or_generate(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected perms tightened to 0o600, got {mode:#o}");
    }
}
