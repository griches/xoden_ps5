//! Isolated native phone sign-in validation. The candidate is never adopted as
//! the active account. Game-runtime acceptance is an explicit subsequent test.
use super::*;
use xodus::api::live::device_code::{DeviceCodeClient, Poll, Scope};

pub(super) async fn run(
    client: &reqwest::Client,
    tokens: &TokenManager,
    root: &Path,
    req: &Request,
) -> Result<()> {
    state(root, req, "checking-sign-in", json!({}))?;
    let product = req.product.as_deref().ok_or("request-product")?;
    let plan = crate::package_plan::resolve(client, product, &req.market)
        .await
        .map_err(|_| "package-resolution")?;
    let content = plan
        .select(req.content.as_deref())
        .map_err(|_| "package-selection")?;
    // A test may renew its own device copy. Never provision another device or
    // open the working account, and never overwrite its stored device tickets.
    let memory = TokenManager::with_memory();
    memory
        .save_device_license(
            &tokens
                .get_device_license()
                .map_err(|_| "existing-device-required")?,
        )
        .map_err(|_| "device-copy")?;
    if let Ok(device) = tokens.get_device_sts_token() {
        memory
            .save_device_token(xodus::tokens::PASSPORT_STS.into(), device)
            .map_err(|_| "device-copy")?;
    }
    tokio::time::timeout(
        Duration::from_secs(30),
        xodus::tokens::device::ensure_device_credentials(client, &memory),
    )
    .await
    .map_err(|_| "device-timeout")?
    .map_err(|error| error.diagnostic_code())?;
    let login = DeviceCodeClient::new(client.clone());
    // Rechecking a previously verified candidate must prove a genuine refresh
    // after restart, without asking the player to approve another device code.
    // A full sign-out removes this record and therefore always starts fresh.
    let candidate = if let Some(saved) = tokens
        .phone_candidate()
        .map_err(|_| "phone-candidate-read")?
    {
        state(root, req, "checking-game-access", json!({}))?;
        login
            .refresh(&saved, Some(Scope::Xbox))
            .await
            .map_err(|_| "phone-candidate-refresh")?
    } else {
        let mut challenge = login
            .start(Scope::Xbox)
            .await
            .map_err(|_| "phone-code-request")?;
        let expires =
            chrono::Utc::now() + chrono::Duration::seconds(challenge.expires_in().as_secs() as i64);
        state(
            root,
            req,
            "sign-in-code",
            json!({"userCode":challenge.user_code,
        "verificationUri":challenge.verification_uri,"expiresAt":expires.to_rfc3339()}),
        )?;
        let candidate = loop {
            match login.poll(&mut challenge).await {
                Ok(Poll::Pending) => continue,
                Ok(Poll::Complete(candidate)) => break candidate,
                Err(error) => {
                    return Err(match error {
                        xodus::api::live::device_code::Error::Expired => "phone-code-expired",
                        xodus::api::live::device_code::Error::Denied => "phone-sign-in-denied",
                        xodus::api::live::device_code::Error::Cancelled => {
                            "phone-sign-in-cancelled"
                        }
                        _ => "phone-sign-in-failed",
                    });
                }
            }
        };
        candidate
    };
    // Overwrite the short code immediately; it never enters the public ABI.
    state(root, req, "checking-game-access", json!({}))?;
    let checked = tokio::time::timeout(Duration::from_secs(120), async {
        let mut xbox = None;
        for prefix in ["t=", "d="] {
            if let Ok(user) = xodus::api::xbox::authenticate_xbox_user(
                client,
                format!("{prefix}{}", candidate.access_token()),
            )
            .await
            {
                xbox = Some(user);
                break;
            }
        }
        let xbox = xbox.ok_or("phone-xbox-sign-in")?;
        xodus::api::xbox::request_xsts_token(client, xbox.token.clone(), "http://xboxlive.com")
            .await
            .map_err(|_| "phone-xbox-profile-ticket")?;
        xodus::api::xbox::request_xsts_token(client, xbox.token, "http://mp.microsoft.com/")
            .await
            .map_err(|_| "phone-marketplace-ticket")?;
        let store = login
            .refresh(&candidate, Some(Scope::Store))
            .await
            .map_err(|_| "phone-store-scope")?;
        let (key, license) = crate::license::get_candidate_license(
            client,
            &memory,
            &store,
            content.into(),
            req.market.clone(),
        )
        .await
        .map_err(|_| "phone-pc-license")?;
        if license.content_keys.len() != 1 {
            return Err("phone-content-key-count");
        }
        license
            .content_keys
            .into_values()
            .next()
            .ok_or("phone-content-key")?
            .unpack(&key)
            .map_err(|_| "phone-content-key-authentication")?;
        login
            .refresh(&store, Some(Scope::Xbox))
            .await
            .map_err(|_| "phone-refresh-after-store")
    })
    .await
    .map_err(|_| "phone-check-timeout")??;
    tokens
        .save_phone_candidate(&checked)
        .map_err(|_| "phone-candidate-save")?;
    state(
        root,
        req,
        "phone-sign-in-verified",
        json!({"pcLicenseVerified":true,"runtimeVerified":false,"activeAccountReplaced":false}),
    )
}
