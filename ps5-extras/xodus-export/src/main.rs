// Export one complete browser session, never a mixture of old and new accounts.
use base64::Engine as _;
use keyring_core::api::CredentialStoreApi;
use std::{io::Write, path::Path};
const SERVICE: &str = "Xodus Service";

fn valid_json(bytes: &[u8]) -> Result<serde_json::Value, &'static str> {
    let decoded = match bytes.strip_prefix(b"XODUS_KEYRING_BASE64_V1:") {
        Some(value) => base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|_| "Invalid saved session.")?,
        None => bytes.to_vec(),
    };
    serde_json::from_slice(&decoded).map_err(|_| "Invalid saved session.")
}
fn collect(
    mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>, &'static str>,
) -> Result<Vec<(&'static str, Vec<u8>)>, &'static str> {
    let mut entries = Vec::new();
    for key in ["dev_license", "device-tokens"] {
        let bytes = read(key)?.ok_or("Device credentials are missing. Sign in again.")?;
        valid_json(&bytes)?;
        entries.push((key, bytes));
    }
    if let Some(bytes) = read("user-session")? {
        let value = valid_json(&bytes)?;
        if value["user"]["username"]
            .as_str()
            .is_none_or(|s| s.trim().is_empty())
            || value["user"]["puid"].as_str().is_none_or(str::is_empty)
            || value["tokens"]
                .as_object()
                .is_none_or(|tokens| tokens.is_empty())
        {
            return Err("The saved browser session is incomplete. Sign in again.");
        }
        entries.push(("user-session", bytes));
    } else {
        for key in ["user-tokens", "user-DA"] {
            let bytes = read(key)?.ok_or("Browser sign-in is missing. Sign in again.")?;
            valid_json(&bytes)?;
            entries.push((key, bytes));
        }
    }
    Ok(entries)
}
fn run(path: &Path) -> Result<(), &'static str> {
    let source = apple_native_keyring_store::keychain::Store::new()
        .map_err(|_| "Could not open the Mac Keychain.")?;
    let entries = collect(|key| {
        match source
            .build(SERVICE, key, None)
            .and_then(|entry| entry.get_secret())
        {
            Ok(bytes) => Ok(Some(bytes)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(_) => Err("Could not read the saved browser session from Keychain."),
        }
    })?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|_| "Could not create the private session export.")?;
    let result = (|| {
        file.write_all(b"{}")
            .and_then(|_| file.sync_all())
            .map_err(|_| "Could not initialize the session export.")?;
        drop(file);
        let store = keyring_core::sample::Store::new_with_backing(
            path.to_str().ok_or("Invalid export path.")?,
        )
        .map_err(|_| "Could not open the private session export.")?;
        for (key, bytes) in &entries {
            store
                .build(SERVICE, key, None)
                .and_then(|entry| entry.set_secret(bytes))
                .map_err(|_| "Could not save the session export.")?;
        }
        store
            .save()
            .map_err(|_| "Could not save the session export.")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(path);
    }
    result
}
fn main() -> std::process::ExitCode {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: xodus-export <new-private-file.ron>");
        return std::process::ExitCode::FAILURE;
    };
    match run(Path::new(&path)) {
        Ok(()) => {
            println!("Browser session exported.");
            std::process::ExitCode::SUCCESS
        }
        Err(code) => {
            eprintln!("{code}");
            std::process::ExitCode::FAILURE
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn credentials(key: &str) -> Option<Vec<u8>> {
        Some(match key {
            "user-session" => br#"{"user":{"username":"new@example.test","puid":"new"},"tokens":{"sts":"synthetic"}}"#.to_vec(),
            "user-DA" | "user-tokens" => panic!("must not read the old account when a new session exists"),
            _ => b"{}".to_vec(),
        })
    }
    #[test]
    fn current_browser_session_does_not_export_stale_legacy_or_phone_account() {
        let entries = collect(|key| Ok(credentials(key))).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.0).collect::<Vec<_>>(),
            ["dev_license", "device-tokens", "user-session"]
        );
    }
    #[test]
    fn corrupt_new_session_never_falls_back_to_the_old_account() {
        assert!(
            collect(|key| Ok(if key == "user-session" {
                Some(b"invalid-secret".to_vec())
            } else {
                credentials(key)
            }))
            .is_err()
        );
    }
    #[test]
    fn missing_device_or_session_is_an_error_instead_of_a_partial_export() {
        for missing in ["dev_license", "device-tokens", "user-session"] {
            assert!(
                collect(|key| Ok(
                    if key == missing || key == "user-DA" || key == "user-tokens" {
                        None
                    } else {
                        credentials(key)
                    }
                ))
                .is_err()
            );
        }
    }
    #[test]
    fn encoded_sessions_keep_their_exact_bytes() {
        let raw = credentials("user-session").unwrap();
        let encoded = [
            b"XODUS_KEYRING_BASE64_V1:".to_vec(),
            base64::engine::general_purpose::STANDARD
                .encode(raw)
                .into_bytes(),
        ]
        .concat();
        let entries = collect(|key| {
            Ok(if key == "user-session" {
                Some(encoded.clone())
            } else {
                credentials(key)
            })
        })
        .unwrap();
        assert_eq!(entries.last().unwrap().1, encoded);
    }
}
