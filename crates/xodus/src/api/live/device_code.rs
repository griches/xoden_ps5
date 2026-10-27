//! Phone-assisted Microsoft login without a desktop webview. This returns a
//! candidate OAuth session only: it never reads, removes or replaces the active
//! TokenManager account. PC licensing must be verified before adopting it.
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tokio::time::{Instant, sleep_until, timeout};

// Keep the existing Xodus application identity. Its device-code endpoint was
// checked independently; issuing a challenge does not prove PC-license access.
const CLIENT_ID: &str = "000000004424da1f";
const CONNECT: &str = "https://login.live.com/oauth20_connect.srf";
const TOKEN: &str = "https://login.live.com/oauth20_token.srf";
const RESPONSE_LIMIT: usize = 64 * 1024;

#[derive(Clone, Copy)]
pub enum Scope {
    Xbox,
    Store,
    XboxSignin,
}
impl Scope {
    fn wire(self) -> &'static str {
        match self {
            Self::Xbox => "service::user.auth.xboxlive.com::MBI_SSL",
            Self::Store => "service::www.microsoft.com::MBI_SSL",
            Self::XboxSignin => "xboxlive.signin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Microsoft sign-in could not connect. Try again.")]
    Network,
    #[error("Microsoft returned an unsupported sign-in response.")]
    Response,
    #[error("Microsoft sign-in was declined.")]
    Denied,
    #[error("The sign-in code expired. Request a new code.")]
    Expired,
    #[error("Sign-in was cancelled.")]
    Cancelled,
    #[error("Microsoft rejected the sign-in session. Sign in again.")]
    Rejected,
}

// No Debug or Serialize: device codes and returned credentials must not leak
// into logs, the public progress ABI, or a shared catalogue file.
pub struct Challenge {
    pub user_code: String,
    pub verification_uri: String,
    device_code: String,
    deadline: Instant,
    next_poll: Instant,
    interval: Duration,
    cancelled: bool,
    scope: Scope,
}
impl Challenge {
    pub fn expires_in(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.device_code.clear();
    }
}

pub struct Credentials {
    access_token: String,
    refresh_token: String,
    user_id: Option<String>,
    expires_at: chrono::DateTime<chrono::Utc>,
    scope: Scope,
    client_id: String,
}
impl Credentials {
    pub fn access_token(&self) -> &str {
        &self.access_token
    }
    pub fn refresh_token(&self) -> &str {
        &self.refresh_token
    }
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }
    pub fn expires_in(&self) -> Duration {
        (self.expires_at - chrono::Utc::now())
            .to_std()
            .unwrap_or(Duration::ZERO)
    }

    // This wire format is exclusively for the host-selected private credential
    // backend. The public credential type still has no Debug/Serialize support.
    pub(crate) fn private_candidate_record(&self) -> Result<Vec<u8>, Error> {
        if self.client_id != CLIENT_ID
            || !matches!(self.scope, Scope::Xbox)
            || self.user_id.is_none()
        {
            return Err(Error::Response);
        }
        serde_json::to_vec(&serde_json::json!({"schema":1,"client_id":CLIENT_ID,
            "access_token":self.access_token,"refresh_token":self.refresh_token,
            "user_id":self.user_id,"expires_at":self.expires_at.timestamp()}))
        .map_err(|_| Error::Response)
    }

    pub(crate) fn from_private_candidate_record(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > RESPONSE_LIMIT {
            return Err(Error::Response);
        }
        let body: Value = serde_json::from_slice(bytes).map_err(|_| Error::Response)?;
        let fields = body.as_object().ok_or(Error::Response)?;
        if fields.len() != 6
            || body["schema"] != 1
            || body["client_id"] != CLIENT_ID
            || fields.keys().any(|key| {
                ![
                    "schema",
                    "client_id",
                    "access_token",
                    "refresh_token",
                    "user_id",
                    "expires_at",
                ]
                .contains(&key.as_str())
            })
        {
            return Err(Error::Response);
        }
        let expiry = body["expires_at"]
            .as_i64()
            .and_then(|t| chrono::DateTime::from_timestamp(t, 0))
            .ok_or(Error::Response)?;
        // An expired access token can still have a usable refresh token. Never
        // stretch its lifetime when restoring; callers refresh before using it.
        if expiry > chrono::Utc::now() + chrono::Duration::hours(24) {
            return Err(Error::Response);
        }
        Ok(Self {
            access_token: text(&body, "access_token", 16384)?,
            refresh_token: text(&body, "refresh_token", 16384)?,
            user_id: Some(text(&body, "user_id", 256)?),
            expires_at: expiry,
            scope: Scope::Xbox,
            client_id: CLIENT_ID.into(),
        })
    }
}
pub enum Poll {
    Pending,
    Complete(Credentials),
}

pub struct DeviceCodeClient {
    client: Client,
    connect: String,
    token: String,
}
impl DeviceCodeClient {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            connect: CONNECT.into(),
            token: TOKEN.into(),
        }
    }
    async fn post(
        &self,
        endpoint: &str,
        fields: &[(&str, &str)],
        wait: Duration,
    ) -> Result<(u16, Value), Error> {
        timeout(wait, async {
            let form = url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(fields.iter().copied())
                .finish();
            let mut response = self
                .client
                .post(endpoint)
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(form)
                .send()
                .await
                .map_err(|_| Error::Network)?;
            let status = response.status().as_u16();
            if response
                .content_length()
                .is_some_and(|n| n > RESPONSE_LIMIT as u64)
            {
                return Err(Error::Response);
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| Error::Network)? {
                if chunk.len() > RESPONSE_LIMIT - bytes.len() {
                    return Err(Error::Response);
                }
                bytes.extend_from_slice(&chunk);
            }
            // Transient service failures need not contain JSON.
            if status == 408 || status == 429 || status >= 500 {
                return Ok((status, Value::Null));
            }
            serde_json::from_slice(&bytes)
                .map(|v| (status, v))
                .map_err(|_| Error::Response)
        })
        .await
        .map_err(|_| Error::Network)?
    }
    pub async fn start(&self, scope: Scope) -> Result<Challenge, Error> {
        let (status, body) = self
            .post(
                &self.connect,
                &[
                    ("client_id", CLIENT_ID),
                    ("scope", scope.wire()),
                    ("response_type", "device_code"),
                ],
                Duration::from_secs(15),
            )
            .await?;
        if status != 200 {
            return Err(Error::Rejected);
        }
        let user_code = text(&body, "user_code", 32)?;
        if !user_code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(Error::Response);
        }
        let device_code = text(&body, "device_code", 16384)?;
        let verification_uri = text(&body, "verification_uri", 512)?;
        // A service response must never send the user to a third-party login.
        if !matches!(
            verification_uri.as_str(),
            "https://www.microsoft.com/link" | "https://microsoft.com/link"
        ) {
            return Err(Error::Response);
        }
        let expires = seconds(&body, "expires_in", 3600)?;
        let interval = seconds(&body, "interval", 60)?;
        let now = Instant::now();
        Ok(Challenge {
            user_code,
            verification_uri,
            device_code,
            deadline: now + expires,
            next_poll: now + interval,
            interval,
            cancelled: false,
            scope,
        })
    }
    /// Drop this future to interrupt a pending network request or wait. Cancel
    /// the challenge as well when the user abandons the whole sign-in attempt.
    pub async fn poll(&self, challenge: &mut Challenge) -> Result<Poll, Error> {
        if challenge.cancelled {
            return Err(Error::Cancelled);
        }
        if challenge.expires_in().is_zero() {
            return Err(Error::Expired);
        }
        sleep_until(challenge.next_poll.min(challenge.deadline)).await;
        let remaining = challenge.expires_in();
        if remaining.is_zero() {
            return Err(Error::Expired);
        }
        challenge.next_poll = Instant::now() + challenge.interval;
        let response = self
            .post(
                &self.token,
                &[
                    ("client_id", CLIENT_ID),
                    ("device_code", &challenge.device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ],
                Duration::from_secs(15).min(remaining),
            )
            .await;
        let (status, body) = match response {
            // An outage does not invalidate an existing or candidate account.
            Err(Error::Network) => return Ok(Poll::Pending),
            other => other?,
        };
        if status == 200 {
            let credentials = credentials(&body, None, challenge.scope)?;
            challenge.device_code.clear();
            challenge.deadline = Instant::now();
            return Ok(Poll::Complete(credentials));
        }
        match body.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => Ok(Poll::Pending),
            Some("slow_down") => {
                slow_down(challenge);
                Ok(Poll::Pending)
            }
            Some("expired_token") => {
                challenge.deadline = Instant::now();
                Err(Error::Expired)
            }
            Some("authorization_declined" | "access_denied") => {
                challenge.cancel();
                Err(Error::Denied)
            }
            _ if status == 429 => {
                slow_down(challenge);
                Ok(Poll::Pending)
            }
            _ if status == 408 || status >= 500 => Ok(Poll::Pending),
            _ => Err(Error::Rejected),
        }
    }
    /// Refresh into a separate candidate. The caller adopts it only after its
    /// validation succeeds; failed refresh leaves the old credentials untouched.
    pub async fn refresh(
        &self,
        previous: &Credentials,
        scope: Option<Scope>,
    ) -> Result<Credentials, Error> {
        let scope = scope.unwrap_or(previous.scope);
        self.refresh_for_client(previous, &previous.client_id, scope)
            .await
    }

    /// Isolated runtime compatibility probe. Microsoft must grant this game's
    /// application identity; a successful app-level login alone is insufficient.
    /// The result remains separate and cannot rewrite the original session.
    pub async fn refresh_for_title(
        &self,
        previous: &Credentials,
        client_id: &str,
        full_trust: bool,
    ) -> Result<Credentials, Error> {
        if client_id.len() != 16 || !client_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Response);
        }
        self.refresh_for_client(
            previous,
            client_id,
            if full_trust {
                Scope::Xbox
            } else {
                Scope::XboxSignin
            },
        )
        .await
    }

    async fn refresh_for_client(
        &self,
        previous: &Credentials,
        client_id: &str,
        scope: Scope,
    ) -> Result<Credentials, Error> {
        let (status, body) = self
            .post(
                &self.token,
                &[
                    ("client_id", client_id),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", &previous.refresh_token),
                    ("scope", scope.wire()),
                ],
                Duration::from_secs(15),
            )
            .await?;
        if status == 408 || status == 429 || status >= 500 {
            return Err(Error::Network);
        }
        if status != 200 {
            return Err(Error::Rejected);
        }
        if client_id != previous.client_id && body.get("refresh_token").is_none() {
            return Err(Error::Response);
        }
        let mut fresh = credentials(&body, Some(&previous.refresh_token), scope)?;
        fresh.client_id = client_id.into();
        if previous.user_id.is_some()
            && fresh.user_id.is_some()
            && previous.user_id != fresh.user_id
        {
            return Err(Error::Response);
        }
        if fresh.user_id.is_none() {
            fresh.user_id = previous.user_id.clone();
        }
        Ok(fresh)
    }
}
fn slow_down(challenge: &mut Challenge) {
    challenge.interval = (challenge.interval + Duration::from_secs(5)).min(Duration::from_secs(60));
    challenge.next_poll = Instant::now() + challenge.interval;
}
fn text(body: &Value, key: &str, limit: usize) -> Result<String, Error> {
    let value = body
        .get(key)
        .and_then(Value::as_str)
        .ok_or(Error::Response)?;
    if value.is_empty() || value.len() > limit || value.bytes().any(|b| b.is_ascii_control()) {
        return Err(Error::Response);
    }
    Ok(value.into())
}
fn seconds(body: &Value, key: &str, maximum: u64) -> Result<Duration, Error> {
    let value = body
        .get(key)
        .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok()))
        .ok_or(Error::Response)?;
    if value == 0 || value > maximum {
        return Err(Error::Response);
    }
    Ok(Duration::from_secs(value))
}
fn credentials(body: &Value, fallback: Option<&str>, scope: Scope) -> Result<Credentials, Error> {
    let refresh_token = if body.get("refresh_token").is_some() {
        text(body, "refresh_token", 16384)?
    } else {
        fallback.ok_or(Error::Response)?.into()
    };
    let user_id = if body.get("user_id").is_some() {
        Some(text(body, "user_id", 256)?)
    } else {
        None
    };
    Ok(Credentials {
        access_token: text(body, "access_token", 16384)?,
        refresh_token,
        user_id,
        expires_at: chrono::Utc::now()
            + chrono::Duration::from_std(seconds(body, "expires_in", 86400)?)
                .map_err(|_| Error::Response)?,
        scope,
        client_id: CLIENT_ID.into(),
    })
}

#[cfg(test)]
#[path = "device_code_tests.rs"]
mod tests;
