//! OS keychain for SSH identity passphrases.
//!
//! Service name is always `"tnls"` so credentials show up clearly in
//! Windows Credential Manager / macOS Keychain / Secret Service.
//! The account name is scoped to the tunnel id: `tnls:<id>:passphrase`.
//! All errors are surfaced as [`TunnelError::Storage`] so IPC can render them.

use keyring::Entry;

use crate::tunnel::TunnelError;

const SERVICE: &str = "tnls";

fn account(tunnel_id: &str) -> String {
    format!("{tunnel_id}:passphrase")
}

fn entry(tunnel_id: &str) -> Result<Entry, TunnelError> {
    Entry::new(SERVICE, &account(tunnel_id))
        .map_err(|e| TunnelError::Storage(format!("keychain: {e}")))
}

/// Store or update the passphrase for `tunnel_id`. An empty string deletes.
pub fn set_passphrase(tunnel_id: &str, passphrase: &str) -> Result<(), TunnelError> {
    if passphrase.is_empty() {
        return delete_passphrase(tunnel_id);
    }
    let entry = entry(tunnel_id)?;
    entry
        .set_password(passphrase)
        .map_err(|e| TunnelError::Storage(format!("keychain set: {e}")))
}

/// Retrieve the passphrase for `tunnel_id`, if any. Returns `None` when no
/// credential exists. Any other error is surfaced.
pub fn get_passphrase(tunnel_id: &str) -> Result<Option<String>, TunnelError> {
    let entry = entry(tunnel_id)?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(TunnelError::Storage(format!("keychain get: {e}"))),
    }
}

/// Delete the stored passphrase for `tunnel_id`. No-op when absent.
pub fn delete_passphrase(tunnel_id: &str) -> Result<(), TunnelError> {
    let entry = entry(tunnel_id)?;
    match entry.delete_password() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(TunnelError::Storage(format!("keychain delete: {e}"))),
    }
}

/// Whether a passphrase is stored for `tunnel_id`. Best-effort: any error is
/// treated as "not present".
pub fn has_passphrase(tunnel_id: &str) -> bool {
    matches!(get_passphrase(tunnel_id), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn fresh_id() -> String {
        format!("test-{}", Uuid::new_v4())
    }

    #[test]
    fn set_get_delete_roundtrip() {
        let id = fresh_id();
        // Ensure clean start.
        let _ = delete_passphrase(&id);
        assert_eq!(get_passphrase(&id).unwrap(), None);

        set_passphrase(&id, "s3cr3t").unwrap();
        assert_eq!(get_passphrase(&id).unwrap().as_deref(), Some("s3cr3t"));
        assert!(has_passphrase(&id));

        set_passphrase(&id, "").unwrap();
        assert_eq!(get_passphrase(&id).unwrap(), None);
        assert!(!has_passphrase(&id));
    }

    #[test]
    fn delete_nonexistent_is_ok() {
        let id = fresh_id();
        delete_passphrase(&id).unwrap();
        delete_passphrase(&id).unwrap();
    }
}
