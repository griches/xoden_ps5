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
        let _ = writeln!(file, "{} {}", chrono::Utc::now().format("%H:%M:%S%.3f"), message.as_ref());
    }
}

fn random_key() -> String {
    let state = RandomState::new();
    (0..4u64).map(|i| format!("{:016x}", state.hash_one((i, std::process::id())))).collect()
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
}

fn service_main(key: String) {
    let tokens_path = token_file();
    log(format!("start pid {} port {PORT} tokens {tokens_path}", std::process::id()));
    let store = match keyring_core::sample::Store::new_with_backing(&tokens_path) {
        Ok(store) => store,
        Err(err) => return log(format!("token store: {err}")),
    };
    keyring_core::set_default_store(store.clone());
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return log(format!("tokio: {err}")),
    };
    runtime.block_on(async move {
        let client = match reqwest::Client::builder().user_agent("xodus-wrapper/0.1").build() {
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
            tokens: Arc::new(TokenManager::with_keychain_and_memory()),
            store,
            device: tokio::sync::Mutex::new(None),
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
            let (Ok(kind), Ok(size)) = (stream.read_u16_le().await, stream.read_u16_le().await) else {
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
            if let Err(err) = self.store.save() {
                log(format!("saving tokens: {err}"));
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
            xodus::tokens::device::ensure_device_credentials(&self.client, &self.tokens).await;
            let Token::Legacy(token) = self.tokens.get_device_sts_token()? else {
                return Err("device token is not a legacy token".into());
            };
            *device = Some(token);
        }
        Ok(device.clone().unwrap())
    }

    // Same exchange as xodus-service's MsaTokenRequest handler.
    async fn message(&self, message: XodusMessageType, body: Vec<u8>) -> Result<Vec<u8>, Error> {
        match message {
            XodusMessageType::Ping => Ok(body),
            XodusMessageType::MsaTokenRequest => {
                let request = quick_xml::de::from_str::<MSATokenRequest>(std::str::from_utf8(&body)?)?;
                log(format!("MSA token for {} full trust {}", request.client_id, request.msa_full_trust));
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
                    match Token::from(t) {
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
                            format!("scope={scope}&api-version=2.0&clientid={}", request.client_id),
                            Some(soap::PolicyReference::token_broker()),
                        ),
                        ("http://Passport.NET/tb".to_string(), None),
                    ],
                )
                .await?;
                let ExchangeUserTokenOutcome::Issued(soap::BodyContent::RequestSecurityTokenResponseCollection(
                    mut collection,
                )) = outcome
                else {
                    return Err("Microsoft did not issue a user token (sign in again)".into());
                };
                if let Some(sts) = collection.security_tokens.pop() {
                    let address = sts.applies_to.endpoint_reference.address.clone();
                    let sts: Token = sts.into();
                    let address = match &sts {
                        Token::Legacy(legacy) => legacy.key_name.clone().unwrap_or(address),
                        _ => address,
                    };
                    if let Err(err) = self.tokens.save_user_token(address, sts) {
                        log(format!("keeping refreshed STS token: {err}"));
                    }
                }
                let token = collection.security_tokens.remove(0);
                let expiry = chrono::DateTime::parse_from_rfc3339(&token.lifetime.expires)?;
                let Token::Compact(user_token) = Token::from(token) else {
                    return Err("user token is not compact".into());
                };
                let (device_rps, device_expiry) = device_rps.unwrap_or_default();
                let reply = MSATokenResponse {
                    token: user_token,
                    expiry: expiry.timestamp(),
                    device_expiry,
                    device_rps,
                };
                log("MSA token issued");
                Ok(quick_xml::se::to_string(&reply)?.into_bytes())
            }
            other => Err(format!("unsupported request {other:?}").into()),
        }
    }
}
