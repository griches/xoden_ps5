//! In-process xodus service for Wine builds that cannot run xodus-service
//! beside the game (prospero-win on the PS5 cannot create processes).
//!
//! windows/build.sh links this into xgameruntime.dll. Its DllMain calls
//! [`xodus_wrapper_start`], which sets XODUS_BRIDGE_PORT and XODUS_BRIDGE_KEY
//! for the real runtime (xgameruntime_impl.dll, reached through forwarded
//! exports) and serves the bridge protocol on 127.0.0.1 from a thread.
//! Tokens live in a keyring-core sample store file: XODUS_TOKEN_FILE if set,
//! else the console-wide sign-in at /data/playden/xbox/xodus-keyring.ron (Wine
//! drive Z:), else %USERPROFILE%\.xodus-keyring.ron in the game's bottle.

use std::hash::{BuildHasher, RandomState};
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use xodus::models::live::ExchangeUserTokenOutcome;
use xodus::models::secrets::{LegacyToken, Token};
use xodus::models::soap;
use xodus::models::xgameruntime::xuser::{MSATokenRequest, MSATokenResponse};
use xodus::proto::xodus::XodusMessageType;
use xodus::tokens::TokenManager;

const XML_MAGIC: u32 = 0x58445358;
const PORT: u16 = 47830;

type Error = Box<dyn std::error::Error + Send + Sync>;

static LOG: OnceLock<Mutex<Option<std::fs::File>>> = OnceLock::new();

fn profile_path(name: &str) -> String {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\users\\Public".to_string());
    format!("{home}\\{name}")
}

const SHARED_TOKEN_FILE: &str = "Z:\\data\\playden\\xbox\\xodus-keyring.ron";

fn token_file() -> String {
    if let Ok(path) = std::env::var("XODUS_TOKEN_FILE") {
        return path;
    }
    if std::path::Path::new(SHARED_TOKEN_FILE).exists() {
        return SHARED_TOKEN_FILE.to_string();
    }
    profile_path(".xodus-keyring.ron")
}

fn log(message: impl AsRef<str>) {
    // Public live diagnostics contain fixed codes only. The private log retains
    // details; tokens, headers, URLs and provider text never enter this file.
    #[cfg(windows)]
    if let Some(code)=public_status_code(message.as_ref()) {
        if let Ok(mut file)=std::fs::OpenOptions::new().create(true).append(true).open(r"Z:\data\playden\logs\xbox-runtime-status.log") {
            if file.metadata().is_ok_and(|meta|meta.len()<128*1024) {
                let _=writeln!(file,"{} pid={} {}",chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),std::process::id(),code);
            }
        }
    }
    let file = LOG.get_or_init(|| {
        Mutex::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(profile_path("xodus-wrapper.log"))
                .ok(),
        )
    });
    if let Ok(mut file) = file.lock()
        && let Some(file) = file.as_mut()
    {
        let _ = writeln!(
            file,
            "{} {}",
            chrono::Utc::now().format("%H:%M:%S%.3f"),
            message.as_ref()
        );
    }
}

fn public_status_code(message: &str) -> Option<&'static str> {
    if message.starts_with("start pid ") { Some("started") }
    else if message=="listening" { Some("listening") }
    else if message.starts_with("MSA token for ") { Some("msa-request") }
    else if message=="Phone game application refresh granted" { Some("game-application-granted") }
    else if message=="Phone game application refresh rejected" { Some("game-application-rejected") }
    else if message=="Phone test MSA service ticket issued; game acceptance pending" { Some("phone-ticket-issued") }
    else if message.contains("Phone runtime refresh network failure") { Some("phone-refresh-network") }
    else if message.contains("Phone runtime refresh invalid response") { Some("phone-refresh-response") }
    else if message.contains("Microsoft rejected the phone runtime scope") { Some("phone-scope-rejected") }
    else if message.starts_with("Phone runtime CA bundle ") { Some("phone-trust-unavailable") }
    else if message.contains("Microsoft rejected the phone runtime ticket") { Some("phone-ticket-rejected") }
    else if message.contains("Microsoft rejected the runtime device ticket") { Some("device-ticket-rejected") }
    else { None }
}

fn random_key() -> String {
    let state = RandomState::new();
    (0..4u64)
        .map(|i| format!("{:016x}", state.hash_one((i, std::process::id()))))
        .collect()
}

/// Called from DllMain: only sets variables and starts a thread.
#[unsafe(no_mangle)]
pub extern "C" fn xodus_wrapper_start() {
    let key = random_key();
    // SAFETY: DllMain runs before the game has other threads reading the environment.
    unsafe {
        std::env::set_var("XODUS_BRIDGE_PORT", PORT.to_string());
        std::env::set_var("XODUS_BRIDGE_KEY", &key);
    }
    let _ = std::thread::Builder::new()
        .name("xodus-wrapper".into())
        .spawn(move || service_main(key));
}

struct Service {
    key: String,
    client: reqwest::Client,
    tokens: Arc<TokenManager>,
    store: Arc<keyring_core::sample::Store>,
    device: tokio::sync::Mutex<Option<LegacyToken>>,
    phone_test: bool,
    phone: tokio::sync::Mutex<Option<xodus::api::live::device_code::Credentials>>,
}

fn service_main(key: String) {
    let requested_test = std::env::var("XODUS_TEST_PHONE_LOGIN").as_deref() == Ok("1");
    let tokens_path = token_file();
    log(format!(
        "start pid {} port {PORT} tokens {tokens_path}",
        std::process::id()
    ));
    let store = match load_store(&tokens_path, true) {
        Ok(store) => store,
        Err(err) => return log(format!("token store: {err}")),
    };
    keyring_core::set_default_store(store.clone());
    let tokens = Arc::new(TokenManager::with_keychain_and_memory());
    let active_phone = match tokens.active_phone() {
        Ok(phone) => phone.is_some(),
        Err(_) => return log("Active phone session is invalid; refusing legacy fallback"),
    };
    let phone_test = requested_test || active_phone;
    let store = if phone_test {
        store
    } else {
        match load_store(&tokens_path, false) {
            Ok(store) => {
                keyring_core::set_default_store(store.clone());
                store
            }
            Err(_) => return log("Legacy token store could not be opened"),
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return log(format!("tokio: {err}")),
    };
    runtime.block_on(async move {
        let mut builder = reqwest::Client::builder()
            .user_agent("xodus-wrapper/0.1")
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none());
        if phone_test {
            // Wine's system certificate service is not the native app's trust
            // store. Use its packaged CA bundle with full TLS verification.
            let path=std::env::var("XODUS_CA_BUNDLE").unwrap_or_else(|_|r"Z:\data\homebrew\FAKE77031\trust\cacert.pem".into());
            let pem=match std::fs::read(path) { Ok(pem) if pem.len()<=2*1024*1024=>pem,_=>return log("Phone runtime CA bundle unavailable") };
            let certificates=match reqwest::Certificate::from_pem_bundle(&pem) { Ok(certificates) if !certificates.is_empty()=>certificates,_=>return log("Phone runtime CA bundle invalid") };
            builder=builder.tls_certs_only(certificates);
        }
        let client = match builder.build() {
            Ok(client) => client,
            Err(err) => return log(format!("http client: {err}")),
        };
        let listener = match tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await {
            Ok(listener) => listener,
            Err(err) => return log(format!("bind 127.0.0.1:{PORT}: {err}")),
        };
        log("listening");
        let service = Arc::new(Service {
            key,
            client,
            tokens,
            store,
            device: tokio::sync::Mutex::new(None),
            // Explicit test mode only. It never falls back to the working
            // account and never saves this process's keyring snapshot.
            phone_test,
            phone: tokio::sync::Mutex::new(None),
        });
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let service = service.clone();
                    tokio::spawn(async move { service.connection(stream).await });
                }
                Err(err) => log(format!("accept: {err}")),
            }
        }
    });
}

fn load_store(path: &str, read_only: bool) -> Result<Arc<keyring_core::sample::Store>, Error> {
    if !read_only {
        return Ok(keyring_core::sample::Store::new_with_backing(path)?);
    }
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > 8 * 1024 * 1024 {
        return Err("Invalid phone-test credential file".into());
    }
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Invalid phone-test credential file".into());
    }
    Ok(keyring_core::sample::Store::new_internal(
        ron::de::from_bytes(&bytes).map_err(|_| "Invalid phone-test credential file")?,
        None,
    ))
}

impl Service {
    async fn connection<S: AsyncRead + AsyncWrite + Unpin>(&self, mut stream: S) {
        let mut key = [0u8; 64];
        if stream.read_exact(&mut key).await.is_err() || key != self.key.as_bytes() {
            return log("connection without the bridge key");
        }
        loop {
            let mut magic = [0u8; 4];
            if stream.read_exact(&mut magic).await.is_err() {
                return;
            }
            let magic = u32::from_le_bytes(magic);
            if magic != XML_MAGIC {
                return log(format!("unsupported message magic {magic:#x}"));
            }
            let (Ok(kind), Ok(size)) = (stream.read_u16_le().await, stream.read_u16_le().await)
            else {
                return;
            };
            let mut body = vec![0; size as usize];
            if stream.read_exact(&mut body).await.is_err() {
                return;
            }
            let message = XodusMessageType::try_from(kind as i32).unwrap_or_default();
            let reply = match self.message(message, body).await {
                Ok(reply) => reply,
                Err(err) => {
                    log(format!("{message:?} failed: {err}"));
                    vec![]
                }
            };
            if !self.phone_test {
                if let Err(err) = self.store.save() {
                    log(format!("saving tokens: {err}"));
                }
            }
            let mut frame = Vec::with_capacity(8 + reply.len());
            frame.extend(XML_MAGIC.to_le_bytes());
            frame.extend((kind + 1).to_le_bytes());
            frame.extend((reply.len() as u16).to_le_bytes());
            frame.extend(reply);
            if stream.write_all(&frame).await.is_err() {
                return;
            }
        }
    }

    async fn device_token(&self) -> Result<LegacyToken, Error> {
        let mut device = self.device.lock().await;
        if device.is_none() {
            xodus::tokens::device::ensure_device_credentials(&self.client, &self.tokens).await?;
            let Token::Legacy(token) = self.tokens.get_device_sts_token()? else {
                return Err("device token is not a legacy token".into());
            };
            *device = Some(token);
        }
        Ok(device.clone().unwrap())
    }

    async fn phone_token(&self, request: MSATokenRequest) -> Result<Vec<u8>, Error> {
        use xodus::api::live::device_code::{DeviceCodeClient, Scope};
        if request.client_id.len() != 16
            || !request.client_id.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("Invalid runtime application identity".into());
        }
        let mut held = self.phone.lock().await;
        if held.is_none() {
            *held = Some(
                match self
                    .tokens
                    .active_phone()
                    .map_err(|_| "Active phone session is invalid")?
                {
                    Some(active) => Some(active),
                    None => self
                        .tokens
                        .phone_candidate()
                        .map_err(|_| "Phone test credentials are invalid")?,
                }
                .ok_or("No phone test login is available")?,
            );
        }
        let previous = held.as_ref().ok_or("No phone test login is available")?;
        let login = DeviceCodeClient::new(self.client.clone());
        // The API grants a service-scoped MSA ticket to the signed-in app.
        // Whether the game's XAL accepts it is explicitly unproven until the
        // opt-in console test. Do not synthesize a game-client authorization.
        let title = login.refresh_for_title(previous,&request.client_id,request.msa_full_trust).await;
        let fresh = match title {
            Ok(title) => { log("Phone game application refresh granted");title },
            Err(xodus::api::live::device_code::Error::Rejected) => {
                log("Phone game application refresh rejected");
                login.refresh(
                previous,
                // This app's approved refresh grant is service-scoped. The
                // game's non-full-trust flag does not grant this app the
                // separate xboxlive.signin scope or a game-client grant.
                Some(Scope::Xbox),
                ).await.map_err(|error| match error {
                xodus::api::live::device_code::Error::Network => "Phone runtime refresh network failure",
                xodus::api::live::device_code::Error::Response => "Phone runtime refresh invalid response",
                _ => "Microsoft rejected the phone runtime scope",
                })?
            },
            Err(xodus::api::live::device_code::Error::Network) => return Err("Phone runtime refresh network failure".into()),
            Err(_) => return Err("Phone runtime refresh invalid response".into()),
        };
        let expiry = chrono::Utc::now().timestamp() + fresh.expires_in().as_secs() as i64;
        let mut token = None;
        for prefix in ["d=", "t="] {
            let ticket = format!("{prefix}{}", fresh.access_token());
            if xodus::api::xbox::authenticate_xbox_user(&self.client, ticket.clone())
                .await
                .is_ok()
            {
                token = Some(ticket);
                break;
            }
        }
        let token = token.ok_or("Microsoft rejected the phone runtime ticket")?;
        *held = Some(fresh);
        let device = self.device_token().await?;
        let response = xodus::api::live::exchange_device_token(
            &self.client,
            device,
            "{28C08266-F973-4AE6-FFE4-409B249F138F}".into(),
            "scope=service::user.auth.xboxlive.com::MBI_SSL".into(),
            Some(soap::PolicyReference::token_broker()),
        )
        .await
        .map_err(|_| "Microsoft rejected the runtime device ticket")?;
        let device_expiry = chrono::DateTime::parse_from_rfc3339(&response.lifetime.expires)
            .map_err(|_| "Invalid runtime device expiry")?
            .timestamp();
        let Token::Compact(device_rps) =
            Token::try_from(response).map_err(|_| "Invalid runtime device ticket")?
        else {
            return Err("Invalid runtime device ticket".into());
        };
        let reply = MSATokenResponse {
            token,
            expiry,
            device_rps,
            device_expiry,
            // OAuth's user_id is a CID. It must never be mislabelled as the
            // Store PUID or derived from an Xbox XUID.
            xbox_msa_account_id: None,
            store_msa_account_id: None,
        };
        log("Phone test MSA service ticket issued; game acceptance pending");
        Ok(quick_xml::se::to_string(&reply)?.into_bytes())
    }

    // Same exchange as xodus-service's MsaTokenRequest handler.
    async fn message(&self, message: XodusMessageType, body: Vec<u8>) -> Result<Vec<u8>, Error> {
        match message {
            XodusMessageType::Ping => Ok(body),
            XodusMessageType::MsaTokenRequest => {
                let request =
                    quick_xml::de::from_str::<MSATokenRequest>(std::str::from_utf8(&body)?)?;
                log(format!(
                    "MSA token for {} full trust {}",
                    request.client_id, request.msa_full_trust
                ));
                if self.phone_test {
                    return self.phone_token(request).await;
                }
                let Token::Legacy(user) = self.tokens.get_user_sts_token()? else {
                    return Err("user token is not a legacy token".into());
                };
                let device = self.device_token().await?;
                let scope = if request.msa_full_trust {
                    "service::user.auth.xboxlive.com::MBI_SSL"
                } else {
                    "xboxlive.signin"
                };
                let device_rps = xodus::api::live::exchange_device_token(
                    &self.client,
                    device.clone(),
                    "{28C08266-F973-4AE6-FFE4-409B249F138F}".to_string(),
                    "scope=service::user.auth.xboxlive.com::MBI_SSL".to_owned(),
                    Some(soap::PolicyReference::token_broker()),
                )
                .await
                .ok()
                .and_then(|t| {
                    let expiry = chrono::DateTime::parse_from_rfc3339(&t.lifetime.expires).ok()?;
                    match Token::try_from(t).ok()? {
                        Token::Compact(token) => Some((token, expiry.timestamp())),
                        _ => None,
                    }
                });
                let outcome = xodus::api::live::exchange_user_token(
                    &self.client,
                    user,
                    "USERNAME".to_string(),
                    device,
                    None,
                    Some("Silent".to_string()),
                    request.client_id.clone(),
                    &[
                        (
                            format!(
                                "scope={scope}&api-version=2.0&clientid={}",
                                request.client_id
                            ),
                            Some(soap::PolicyReference::token_broker()),
                        ),
                        ("http://Passport.NET/tb".to_string(), None),
                    ],
                )
                .await?;
                let ExchangeUserTokenOutcome::Issued(
                    soap::BodyContent::RequestSecurityTokenResponseCollection(mut collection),
                ) = outcome
                else {
                    return Err("Microsoft did not issue a user token (sign in again)".into());
                };
                if let Some(sts) = collection.security_tokens.pop() {
                    let address = sts.applies_to.endpoint_reference.address.clone();
                    let sts: Token = sts.try_into()?;
                    let address = match &sts {
                        Token::Legacy(legacy) => legacy.key_name.clone().unwrap_or(address),
                        _ => address,
                    };
                    if let Err(err) = self.tokens.save_user_token(address, sts) {
                        log(format!("keeping refreshed STS token: {err}"));
                    }
                }
                let token = collection
                    .security_tokens
                    .into_iter()
                    .next()
                    .ok_or("Microsoft returned no runtime user ticket")?;
                let expiry = chrono::DateTime::parse_from_rfc3339(&token.lifetime.expires)?;
                let Token::Compact(user_token) = Token::try_from(token)? else {
                    return Err("user token is not compact".into());
                };
                let (device_rps, device_expiry) = device_rps.unwrap_or_default();
                let reply = MSATokenResponse {
                    token: user_token,
                    expiry: expiry.timestamp(),
                    device_expiry,
                    device_rps,
                    xbox_msa_account_id: None,
                    store_msa_account_id: None,
                };
                log("MSA token issued");
                Ok(quick_xml::se::to_string(&reply)?.into_bytes())
            }
            other => Err(format!("unsupported request {other:?}").into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    #[test]
    fn public_bridge_diagnostics_never_return_provider_text_or_secrets() {
        assert_eq!(public_status_code("MSATokenRequest failed: Phone runtime refresh network failure private-secret"),Some("phone-refresh-network"));
        assert_eq!(public_status_code("provider echoed token=private-secret https://private.example"),None);
        assert_eq!(public_status_code("Phone game application refresh granted"),Some("game-application-granted"));
    }

    #[test]
    fn phone_test_store_drop_never_rewrites_the_shared_credential_snapshot() {
        use keyring_core::api::CredentialStoreApi;
        let name = format!(
            "xodus-readonly-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(name);
        std::fs::create_dir(&root).unwrap();
        let path = root.join("synthetic.ron");
        let empty = keyring_core::sample::Store::new_internal(Default::default(), None);
        let before = ron::ser::to_string(&empty.creds).unwrap();
        std::fs::write(&path, &before).unwrap();
        let store = load_store(path.to_str().unwrap(), true).unwrap();
        store
            .build("synthetic", "entry", None)
            .unwrap()
            .set_secret(b"synthetic-only")
            .unwrap();
        drop(store);
        assert_eq!(std::fs::read(&path).unwrap(), before.as_bytes());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    fn service() -> Service {
        // Never create Windows-style log filenames or access a real keyring.
        LOG.get_or_init(|| Mutex::new(None));
        Service {
            key: "synthetic".into(),
            client: reqwest::Client::new(),
            tokens: Arc::new(TokenManager::with_memory()),
            store: keyring_core::sample::Store::new_with_configuration(&HashMap::new()).unwrap(),
            device: tokio::sync::Mutex::new(None),
            phone_test: true,
            phone: tokio::sync::Mutex::new(None),
        }
    }

    #[tokio::test]
    async fn phone_runtime_never_falls_back_to_an_existing_working_account() {
        let service = service();
        service
            .tokens
            .save_user_session(
                xodus::models::secrets::User {
                    puid: "old-user".into(),
                    username: "old-user".into(),
                },
                HashMap::from([(
                    xodus::tokens::PASSPORT_STS.into(),
                    Token::Compact("old-working-secret".into()),
                )]),
            )
            .unwrap();
        let request = b"<MSATokenRequest><ClientId>0000000041180A5D</ClientId><MSAFullTrust>true</MSAFullTrust></MSATokenRequest>";
        let error = service
            .message(XodusMessageType::MsaTokenRequest, request.to_vec())
            .await
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "No phone test login is available");
        assert_eq!(service.tokens.get_user().unwrap().puid, "old-user");
        assert!(
            matches!(service.tokens.get_user_sts_token().unwrap(),Token::Compact(token) if token=="old-working-secret")
        );
        assert!(service.device.lock().await.is_none());
        assert_eq!(
            service
                .message(XodusMessageType::Ping, b"ping".to_vec())
                .await
                .unwrap(),
            b"ping"
        );
    }

    #[tokio::test]
    async fn phone_runtime_rejects_invalid_game_identity_before_credentials_or_network() {
        let service = service();
        let error = service
            .phone_token(MSATokenRequest {
                client_id: "foreign&scope=other".into(),
                allow_ui: false,
                msa_full_trust: true,
            })
            .await
            .err()
            .unwrap();
        assert_eq!(error.to_string(), "Invalid runtime application identity");
        assert!(service.phone.lock().await.is_none());
        assert!(service.device.lock().await.is_none());
    }

    #[test]
    fn runtime_reply_does_not_claim_cid_or_xuid_as_store_account_identity() {
        let response = MSATokenResponse {
            token: "synthetic-ticket".into(),
            expiry: 123,
            device_rps: "synthetic-device".into(),
            device_expiry: 123,
            xbox_msa_account_id: None,
            store_msa_account_id: None,
        };
        let xml = quick_xml::se::to_string(&response).unwrap();
        assert!(xml.contains("<Token>synthetic-ticket</Token>"));
        assert!(!xml.contains("MsaAccountId"));
    }
}
