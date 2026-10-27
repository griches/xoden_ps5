use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::models::secrets::{Device, Token, TokenStore, User};
use crate::models::xbox::XstsResponse;
use crate::tokens::backend::{KeychainBackend, MemoryBackend};
use crate::tokens::store::{ExpiringTokenBackend, TokenBackend, TokenStoreError};

mod keys {
    pub const DEV_LICENSE: &str = "dev_license";
    pub const DEVICE_TOKENS: &str = "device-tokens";
    pub const USER_TOKENS: &str = "user-tokens";
    pub const USER_INFO: &str = "user-DA";
    pub const USER_SESSION: &str = "user-session";
    pub const PHONE_CANDIDATE: &str = "phone-sign-in-candidate";
    pub const PHONE_SESSION: &str = "phone-user-session";
    pub const BROWSER_SESSION: &str = "runtime-browser-session";
    pub const BROWSER_CANDIDATE: &str = "browser-sign-in-candidate";
}

#[derive(serde::Serialize, serde::Deserialize)]
struct UserSession {
    user: User,
    tokens: HashMap<String, Token>,
    #[serde(default)]
    primary_browser: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeBrowserSession {
    phone_id: String,
    session: UserSession,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserCandidate {
    expected_account: Option<String>,
    session: UserSession,
}

pub const PASSPORT_STS: &str = "http://Passport.NET/STS";

pub(crate) struct DeviceRenewalGuard {
    _local: tokio::sync::OwnedMutexGuard<()>,
    #[cfg(unix)]
    _process: Option<std::fs::File>,
}

#[cfg(test)]
mod credential_account_metadata_tests {
    use super::*;
    #[tokio::test]
    async fn credential_memory_device_renewal_is_shared_by_clones_and_isolated_between_stores() {
        let manager = TokenManager::with_memory();
        let clone = manager.clone();
        let first = manager.lock_device_renewal().await.unwrap();
        #[cfg(unix)]
        assert!(first._process.is_none());
        assert!(tokio::time::timeout(std::time::Duration::from_millis(20), clone.lock_device_renewal()).await.is_err());
        let separate = TokenManager::with_memory();
        let independent = tokio::time::timeout(std::time::Duration::from_millis(100), separate.lock_device_renewal()).await.unwrap().unwrap();
        drop(independent);
        drop(first);
        assert!(tokio::time::timeout(std::time::Duration::from_millis(100), clone.lock_device_renewal()).await.unwrap().is_ok());
    }
    #[test]
    fn credential_account_metadata_snapshot_is_atomic_and_logout_removes_it() {
        let manager = TokenManager::with_memory();
        assert!(manager.get_user_sts_session().unwrap().is_none());
        manager.save_user(&User { puid: "legacy".into(), username: "legacy".into() }).unwrap();
        manager.save_user_token(PASSPORT_STS.into(), Token::Compact("legacy-token".into())).unwrap();
        assert!(manager.get_user_sts_session().unwrap().is_none());
        manager.save_user_session(User { puid: "0123456789abcdef".into(), username: "synthetic".into() },
            HashMap::from([(PASSPORT_STS.into(), Token::Compact("synthetic-session-ticket".into()))])).unwrap();
        let (user, token) = manager.get_user_sts_session().unwrap().unwrap();
        assert_eq!(user.puid, "0123456789abcdef");
        assert!(matches!(token, Token::Compact(value) if value == "synthetic-session-ticket"));
        manager.remove_persistent().unwrap();
        assert!(manager.get_user_sts_session().unwrap().is_none());
    }
    #[test]
    fn credential_account_metadata_never_pairs_different_sessions_under_concurrent_reads() {
        let manager = TokenManager::with_memory();
        let save = |m: &TokenManager, name: &str| m.save_user_session(User { puid: name.into(), username: name.into() },
            HashMap::from([(PASSPORT_STS.into(), Token::Compact(name.into()))])).unwrap();
        save(&manager, "a");
        let writer = manager.clone();
        let thread = std::thread::spawn(move || for _ in 0..1000 { save(&writer, "b"); save(&writer, "a"); });
        for _ in 0..10000 {
            let (user, token) = manager.get_user_sts_session().unwrap().unwrap();
            assert!(matches!(token, Token::Compact(value) if value == user.puid));
        }
        thread.join().unwrap();
    }
}

/// Semantic facade over the two storage tiers: a persistent, keychain-backed tier
/// for STS/device/user credentials, and an ephemeral tier for short-lived
/// per-relying-party XSTS tokens. Centralizes the read-merge-write pattern that was
/// previously duplicated across `xodus-cli` and `xodus-service`.
#[derive(Clone)]
pub struct TokenManager {
    persistent: Arc<dyn TokenBackend>,
    ephemeral: Arc<dyn ExpiringTokenBackend>,
    device_renewal: Arc<tokio::sync::Mutex<()>>,
    phone_renewal: Arc<tokio::sync::Mutex<()>>,
    account_write: Arc<std::sync::Mutex<()>>,
    isolated: bool,
    #[cfg(unix)]
    renewal_root: Option<std::path::PathBuf>,
}

impl TokenManager {
    pub fn new(
        persistent: Arc<dyn TokenBackend>,
        ephemeral: Arc<dyn ExpiringTokenBackend>,
    ) -> Self {
        Self {
            persistent,
            ephemeral,
            device_renewal: Arc::new(tokio::sync::Mutex::new(())),
            phone_renewal: Arc::new(tokio::sync::Mutex::new(())),
            account_write: Arc::new(std::sync::Mutex::new(())),
            isolated: false,
            #[cfg(unix)]
            renewal_root: None,
        }
    }

    /// Keychain for persistent storage, in-memory for ephemeral - the default
    /// wiring for both `xodus-cli` and `xodus-service` today.
    pub fn with_keychain_and_memory() -> Self {
        Self::new(
            Arc::new(KeychainBackend),
            Arc::new(MemoryBackend::default()),
        )
    }

    /// Fully isolated credentials. Clones serialize renewal together without
    /// accessing a process-shared filesystem lock or the system keychain.
    pub fn with_memory() -> Self {
        let mut manager = Self::new(
            Arc::new(MemoryBackend::default()),
            Arc::new(MemoryBackend::default()),
        );
        manager.isolated = true;
        manager
    }

    pub fn with_host_file(path:&std::path::Path,commit:Arc<dyn Fn(&[u8])->bool+Send+Sync>)->Result<Self,TokenStoreError> {
        let manager = Self::new(Arc::new(super::backend::HostFileBackend::open(path,commit)?),Arc::new(MemoryBackend::default()));
        #[cfg(unix)]
        let manager = Self { renewal_root: Some(path.parent().ok_or(TokenStoreError::NotFound)?.join("session-lock")), ..manager };
        Ok(manager)
    }

    pub(crate) async fn lock_device_renewal(&self) -> Result<DeviceRenewalGuard, TokenStoreError> {
        let local = self.device_renewal.clone().lock_owned().await;
        #[cfg(unix)]
        let process = if self.isolated { None } else { Some(super::session::renewal_lock(self.renewal_root.as_deref()).await?) };
        Ok(DeviceRenewalGuard { _local: local, #[cfg(unix)] _process: process })
    }

    pub fn remove_persistent(&self) -> Result<(), TokenStoreError> {
        let _guard = self.account_write.lock().map_err(|_| TokenStoreError::WriteVerification)?;
        self.persistent.remove(keys::PHONE_SESSION)?;
        self.persistent.remove(keys::BROWSER_SESSION)?;
        self.persistent.remove(keys::BROWSER_CANDIDATE)?;
        self.remove_phone_candidate()?;
        self.persistent.remove(keys::DEVICE_TOKENS)?;
        self.persistent.remove(keys::USER_TOKENS)?;
        self.persistent.remove(keys::USER_INFO)?;
        // Clear legacy entries first so a failed logout never exposes a stale
        // account underneath a still-valid session.
        self.persistent.remove(keys::USER_SESSION)
    }

    // ---- Device identity / license -----------------------------------------

    /// A phone-login candidate is deliberately not a user session. Existing
    /// downloads and runtime callers continue to read the active account only.
    pub fn phone_candidate(&self) -> Result<Option<crate::api::live::device_code::Credentials>, TokenStoreError> {
        self.persistent.get(keys::PHONE_CANDIDATE)?.map(|bytes|
            crate::api::live::device_code::Credentials::from_private_candidate_record(&bytes)
                .map_err(|_| TokenStoreError::NotFound)).transpose()
    }

    pub fn save_phone_candidate(&self, candidate: &crate::api::live::device_code::Credentials) -> Result<(), TokenStoreError> {
        let bytes = candidate.private_candidate_record().map_err(|_| TokenStoreError::NotFound)?;
        Self::write_verified(&*self.persistent, keys::PHONE_CANDIDATE, &bytes)
    }

    pub fn remove_phone_candidate(&self) -> Result<(), TokenStoreError> {
        self.persistent.remove(keys::PHONE_CANDIDATE)
    }

    pub fn active_phone(&self) -> Result<Option<crate::api::live::device_code::Credentials>, TokenStoreError> {
        if self.user_session()?.is_some_and(|session|session.primary_browser) { return Ok(None); }
        self.persistent.get(keys::PHONE_SESSION)?.map(|bytes|
            crate::api::live::device_code::Credentials::from_private_candidate_record(&bytes)
                .map_err(|_| TokenStoreError::WriteVerification)).transpose()
    }

    /// Save only a freshly authenticated browser session whose Xbox identity
    /// the caller has verified against this exact active phone session.
    /// Device keys and the library's account namespace are never replaced.
    pub fn save_runtime_browser_session(&self, expected: &crate::api::live::device_code::Credentials,
        user: User, tokens: HashMap<String, Token>) -> Result<(), TokenStoreError> {
        if user.username.trim().is_empty() || user.puid.is_empty() || !tokens.contains_key(PASSPORT_STS) {
            return Err(TokenStoreError::WriteVerification);
        }
        let before = expected.private_candidate_record().map_err(|_| TokenStoreError::WriteVerification)?;
        let phone_id = expected.user_id().ok_or(TokenStoreError::WriteVerification)?.to_owned();
        let _guard = self.account_write.lock().map_err(|_| TokenStoreError::WriteVerification)?;
        if self.persistent.get(keys::PHONE_SESSION)?.as_deref() != Some(before.as_slice()) {
            return Err(TokenStoreError::NotFound);
        }
        let bytes = serde_json::to_vec(&RuntimeBrowserSession { phone_id, session: UserSession { user, tokens, primary_browser:false } })?;
        Self::write_verified(&*self.persistent, keys::BROWSER_SESSION, &bytes)
    }

    /// Project the same-account browser grant into an isolated runtime store.
    /// The returned store cannot write to the console's account or device keys.
    pub fn runtime_browser_tokens(&self) -> Result<Option<Self>, TokenStoreError> {
        if let Some(session)=self.user_session()?.filter(|session|session.primary_browser) {
            let memory=Self::with_memory();
            memory.save_device_license(&self.get_device_license()?)?;
            if let Ok(ticket)=self.get_device_sts_token(){memory.save_device_token(PASSPORT_STS.into(),ticket)?;}
            memory.save_user_session(session.user,session.tokens)?;
            return Ok(Some(memory));
        }
        let Some(phone) = self.active_phone()? else { return Ok(None); };
        let Some(bytes) = self.persistent.get(keys::BROWSER_SESSION)? else { return Ok(None); };
        let saved: RuntimeBrowserSession = serde_json::from_slice(&bytes)?;
        if Some(saved.phone_id.as_str()) != phone.user_id() { return Err(TokenStoreError::WriteVerification); }
        let memory = Self::with_memory();
        memory.save_device_license(&self.get_device_license()?)?;
        if let Ok(ticket) = self.get_device_sts_token() { memory.save_device_token(PASSPORT_STS.into(), ticket)?; }
        memory.save_user_session(saved.session.user, saved.session.tokens)?;
        Ok(Some(memory))
    }

    /// An opaque account namespace, never a fabricated Store PUID or email.
    pub fn active_account(&self) -> Result<String, TokenStoreError> {
        if let Some(phone) = self.active_phone()? {
            return Ok(format!("phone:{}", phone.user_id().ok_or(TokenStoreError::NotFound)?));
        }
        Ok(self.get_user()?.username)
    }

    /// Explicit library refresh may activate a verified phone login only after
    /// sign-out. It must never silently replace a different saved account.
    pub fn activate_phone_candidate(&self) -> Result<(), TokenStoreError> {
        let _guard = self.account_write.lock().map_err(|_| TokenStoreError::WriteVerification)?;
        if self.active_phone()?.is_some() { return Ok(()); }
        match self.get_user() {
            Err(TokenStoreError::NotFound) => {},
            Err(error) => return Err(error),
            Ok(_) => return Err(TokenStoreError::WriteVerification),
        }
        let candidate = self.phone_candidate()?.ok_or(TokenStoreError::NotFound)?;
        Self::write_verified(&*self.persistent, keys::PHONE_SESSION, &candidate.private_candidate_record().map_err(|_| TokenStoreError::WriteVerification)?)
    }

    /// Refresh one active phone session at a time, preserving its account and
    /// refusing to resurrect a session removed during an outstanding request.
    pub async fn active_phone_ticket(&self, client: &reqwest::Client, scope: crate::api::live::device_code::Scope)
        -> Result<Option<crate::api::live::device_code::Credentials>, TokenStoreError> {
        use crate::api::live::device_code::{DeviceCodeClient, Scope};
        let _renewal = self.phone_renewal.lock().await;
        let Some(previous) = self.active_phone()? else { return Ok(None); };
        let login = DeviceCodeClient::new(client.clone());
        let fresh = login.refresh(&previous, Some(scope)).await.map_err(|_| TokenStoreError::DeviceAuthentication)?;
        let saved = if matches!(scope, Scope::Xbox) { None } else {
            Some(login.refresh(&fresh, Some(Scope::Xbox)).await.map_err(|_| TokenStoreError::DeviceAuthentication)?)
        };
        self.save_phone_refresh(&previous,saved.as_ref().unwrap_or(&fresh))?;
        Ok(Some(fresh))
    }

    pub(crate) fn save_phone_refresh(&self, previous: &crate::api::live::device_code::Credentials,
        fresh: &crate::api::live::device_code::Credentials) -> Result<(),TokenStoreError> {
        if previous.user_id()!=fresh.user_id() { return Err(TokenStoreError::WriteVerification); }
        let before=previous.private_candidate_record().map_err(|_| TokenStoreError::WriteVerification)?;
        let bytes=fresh.private_candidate_record().map_err(|_| TokenStoreError::WriteVerification)?;
        let _guard = self.account_write.lock().map_err(|_| TokenStoreError::WriteVerification)?;
        if self.persistent.get(keys::PHONE_SESSION)?.as_deref() != Some(before.as_slice()) { return Err(TokenStoreError::NotFound); }
        Self::write_verified(&*self.persistent, keys::PHONE_SESSION, &bytes)
    }

    pub fn get_device_license(&self) -> Result<Device, TokenStoreError> {
        let bytes = self
            .persistent
            .get(keys::DEV_LICENSE)?
            .ok_or(TokenStoreError::NotFound)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save_device_license(&self, device: &Device) -> Result<(), TokenStoreError> {
        Self::write_verified(
            &*self.persistent,
            keys::DEV_LICENSE,
            &serde_json::to_vec(device)?,
        )
    }

    pub fn remove_device_license(&self) -> Result<(), TokenStoreError> {
        self.persistent.remove(keys::DEV_LICENSE)
    }

    // ---- Device STS tokens (keyed by SOAP "applies_to" address) -----------

    pub fn get_device_token_for(&self, address: &str) -> Result<Option<Token>, TokenStoreError> {
        Self::read_token_store(&*self.persistent, keys::DEVICE_TOKENS, address)
    }

    pub fn save_device_token(&self, address: String, token: Token) -> Result<(), TokenStoreError> {
        Self::write_token_store(&*self.persistent, keys::DEVICE_TOKENS, address, token)
    }

    pub fn get_device_sts_token(&self) -> Result<Token, TokenStoreError> {
        self.get_device_token_for(PASSPORT_STS)?
            .ok_or(TokenStoreError::NotFound)
    }

    // ---- User STS tokens (keyed by SOAP "applies_to" address) --------------

    pub fn get_user_token_for(&self, address: &str) -> Result<Option<Token>, TokenStoreError> {
        if let Some(session) = self.user_session()? {
            return Ok(session.tokens.get(address).cloned());
        }
        Self::read_token_store(&*self.persistent, keys::USER_TOKENS, address)
    }

    pub fn save_user_token(&self, address: String, token: Token) -> Result<(), TokenStoreError> {
        if let Some(mut session) = self.user_session()? {
            session.tokens.insert(address, token);
            return Self::write_verified(
                &*self.persistent,
                keys::USER_SESSION,
                &serde_json::to_vec(&session)?,
            );
        }
        Self::write_token_store(&*self.persistent, keys::USER_TOKENS, address, token)
    }

    pub fn get_user_sts_token(&self) -> Result<Token, TokenStoreError> {
        self.get_user_token_for(PASSPORT_STS)?
            .ok_or(TokenStoreError::NotFound)
    }

    // ---- User info -----------------------------------------------------------

    /// An atomic authenticated user/token pair from the single session record.
    /// Legacy independent entries are not sufficient for identity projection.
    pub fn get_user_sts_session(&self) -> Result<Option<(User, Token)>, TokenStoreError> {
        let Some(mut session) = self.user_session()? else { return Ok(None) };
        let token = session.tokens.remove(PASSPORT_STS).ok_or(TokenStoreError::NotFound)?;
        Ok(Some((session.user, token)))
    }

    pub fn get_user(&self) -> Result<User, TokenStoreError> {
        if let Some(session) = self.user_session()? {
            return Ok(session.user);
        }
        let bytes = self
            .persistent
            .get(keys::USER_INFO)?
            .ok_or(TokenStoreError::NotFound)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save_user(&self, user: &User) -> Result<(), TokenStoreError> {
        Self::write_verified(
            &*self.persistent,
            keys::USER_INFO,
            &serde_json::to_vec(user)?,
        )
    }

    fn user_session(&self) -> Result<Option<UserSession>, TokenStoreError> {
        self.persistent
            .get(keys::USER_SESSION)?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
            .transpose()
    }

    /// Publish a newly authenticated account and its complete token set using
    /// one keyring value. Never merge tokens belonging to the previous account.
    /// Legacy entries remain readable until the first successful session save.
    pub fn save_user_session(
        &self,
        user: User,
        tokens: HashMap<String, Token>,
    ) -> Result<(), TokenStoreError> {
        if tokens.is_empty() || user.username.trim().is_empty() || user.puid.is_empty() {
            return Err(TokenStoreError::NotFound);
        }
        let session = UserSession { user, tokens, primary_browser:false };
        Self::write_verified(
            &*self.persistent,
            keys::USER_SESSION,
            &serde_json::to_vec(&session)?,
        )
    }

    /// A single atomic record selects browser authorisation for library,
    /// licensing and runtime use. Failed/late saves retain the previous account.
    pub fn save_primary_browser_session(&self, expected: Option<&str>, user: User,
        tokens: HashMap<String, Token>) -> Result<(), TokenStoreError> {
        if user.username.trim().is_empty() || user.puid.is_empty() || !tokens.contains_key(PASSPORT_STS) {
            return Err(TokenStoreError::WriteVerification);
        }
        let _guard=self.account_write.lock().map_err(|_|TokenStoreError::WriteVerification)?;
        let current=match self.active_account() {Ok(value)=>Some(value),Err(TokenStoreError::NotFound)=>None,Err(error)=>return Err(error)};
        if current.as_deref()!=expected {return Err(TokenStoreError::NotFound);}
        let session=UserSession{user,tokens,primary_browser:true};
        Self::write_verified(&*self.persistent,keys::USER_SESSION,&serde_json::to_vec(&session)?)?;
        // Cleanup cannot undo a successfully committed active session.
        let _=self.persistent.remove(keys::BROWSER_CANDIDATE);
        Ok(())
    }

    /// Preserve a newly approved grant for verification retries without making
    /// it visible to library, licensing or runtime callers as an active account.
    pub fn save_browser_candidate(&self, expected: Option<&str>, user: User, tokens: HashMap<String,Token>) -> Result<(),TokenStoreError> {
        if user.username.trim().is_empty() || user.puid.is_empty() ||
            !tokens.get(PASSPORT_STS).is_some_and(|token|super::session::state(token,chrono::Utc::now())==super::session::State::Current) {
            return Err(TokenStoreError::WriteVerification);
        }
        let _guard=self.account_write.lock().map_err(|_|TokenStoreError::WriteVerification)?;
        let current=match self.active_account(){Ok(value)=>Some(value),Err(TokenStoreError::NotFound)=>None,Err(error)=>return Err(error)};
        if current.as_deref()!=expected {return Err(TokenStoreError::NotFound);}
        let candidate=BrowserCandidate { expected_account:expected.map(str::to_owned),session:UserSession{user,tokens,primary_browser:false} };
        Self::write_verified(&*self.persistent,keys::BROWSER_CANDIDATE,&serde_json::to_vec(&candidate)?)
    }

    pub fn browser_candidate(&self, expected: Option<&str>) -> Result<Option<(User,HashMap<String,Token>)>,TokenStoreError> {
        let _guard=self.account_write.lock().map_err(|_|TokenStoreError::WriteVerification)?;
        let Some(bytes)=self.persistent.get(keys::BROWSER_CANDIDATE)? else {return Ok(None);};
        let candidate:BrowserCandidate=serde_json::from_slice(&bytes)?;
        let current=match self.active_account(){Ok(value)=>Some(value),Err(TokenStoreError::NotFound)=>None,Err(error)=>return Err(error)};
        if current.as_deref()!=expected || candidate.expected_account.as_deref()!=expected ||
            !candidate.session.tokens.get(PASSPORT_STS).is_some_and(|token|super::session::state(token,chrono::Utc::now())==super::session::State::Current) {
            self.persistent.remove(keys::BROWSER_CANDIDATE)?;
            return Ok(None);
        }
        Ok(Some((candidate.session.user,candidate.session.tokens)))
    }

    // ---- Ephemeral XSTS-by-relying-party cache --------------------------------

    pub fn get_cached_xsts(&self, relying_party: &str) -> Option<XstsResponse> {
        let bytes = self.ephemeral.get(relying_party).ok()??;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn cache_xsts(&self, relying_party: &str, token: &XstsResponse) {
        self.cache_xsts_response(relying_party, token);
    }

    fn cache_xsts_response(&self, key: &str, token: &XstsResponse) {
        let Ok(bytes) = serde_json::to_vec(token) else {
            return;
        };
        let remaining = (token.not_after - chrono::Utc::now())
            .to_std()
            .unwrap_or(std::time::Duration::ZERO);
        let _ = self
            .ephemeral
            .set_with_expiry(key, &bytes, Instant::now() + remaining);
    }

    // ---- shared TokenStore read/modify/write helper ---------------------------

    fn write_verified(
        backend: &dyn TokenBackend,
        key: &str,
        bytes: &[u8],
    ) -> Result<(), TokenStoreError> {
        backend.set(key, bytes)?;
        match backend.get(key)? {
            Some(saved) if saved == bytes => Ok(()),
            _ => Err(TokenStoreError::WriteVerification),
        }
    }

    fn read_token_store(
        backend: &dyn TokenBackend,
        key: &str,
        address: &str,
    ) -> Result<Option<Token>, TokenStoreError> {
        let Some(bytes) = backend.get(key)? else {
            return Ok(None);
        };
        let store: TokenStore = serde_json::from_slice(&bytes)?;
        Ok(store.tokens.get(address).cloned())
    }

    fn write_token_store(
        backend: &dyn TokenBackend,
        key: &str,
        address: String,
        token: Token,
    ) -> Result<(), TokenStoreError> {
        let mut tokens: HashMap<String, Token> = match backend.get(key)? {
            Some(bytes) if !bytes.is_empty() => {
                match serde_json::from_slice::<TokenStore>(&bytes) {
                    Ok(store) => store.tokens,
                    // Only a freshly authenticated device token may replace its
                    // damaged cache. Never discard device identity or user tokens.
                    Err(_) if key == keys::DEVICE_TOKENS => HashMap::new(),
                    Err(error) => return Err(error.into()),
                }
            }
            _ => HashMap::new(),
        };
        tokens.insert(address, token);
        Self::write_verified(backend, key, &serde_json::to_vec(&TokenStore { tokens })?)
    }
}
