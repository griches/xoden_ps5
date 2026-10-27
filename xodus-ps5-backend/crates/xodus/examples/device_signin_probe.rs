//! Interactive provider validation only. Keeps the candidate in memory; never
//! adopts a session. Optional PC-license validation reads only the existing
//! device credentials, copying them into memory before any renewal.
use std::io::Write;
use xodus::api::live::device_code::{DeviceCodeClient, Poll, Scope};
use xodus::models::secrets::Token;
use xodus::tokens::TokenManager;

struct LicenseProbe {
    device_ticket: String,
    device_key: xodus::licensing::splicense::DeviceKey,
    content: String,
    market: String,
}

// No provisioning and no writes to the existing credential store. In
// particular, never load the active user's credentials for this validation.
async fn license_probe(
    client: &reqwest::Client,
    content: String,
    market: String,
) -> Result<LicenseProbe, &'static str> {
    xodus::secrets::init_secrets().map_err(|_| "device-store")?;
    let existing = TokenManager::with_keychain_and_memory();
    let device = existing
        .get_device_license()
        .map_err(|_| "existing-device-required")?;
    let memory = TokenManager::with_memory();
    memory
        .save_device_license(&device)
        .map_err(|_| "device-copy")?;
    if let Ok(token) = existing.get_device_sts_token() {
        memory
            .save_device_token(xodus::tokens::PASSPORT_STS.into(), token)
            .map_err(|_| "device-copy")?;
    }
    xodus::tokens::device::ensure_device_credentials(client, &memory)
        .await
        .map_err(|_| "device-refresh")?;
    let Token::Legacy(device_token) = memory.get_device_sts_token().map_err(|_| "device-ticket")?
    else {
        return Err("device-ticket-format");
    };
    let reply = xodus::api::live::exchange_device_token(
        client,
        device_token,
        "{d6d5a677-0872-4ab0-9442-bb792fce85c5}".into(),
        "www.microsoft.com".into(),
        Some(xodus::models::soap::PolicyReference::mbi_ssl()),
    )
    .await
    .map_err(|_| "device-store-ticket")?;
    let Token::Compact(device_ticket) =
        Token::try_from(reply).map_err(|_| "device-store-ticket-format")?
    else {
        return Err("device-store-ticket-format");
    };
    let license = xodus::licensing::splicense::SPLicense::parse_base64(&device.splicense)
        .map_err(|_| "device-license-format")?;
    let device_key = license
        .encrypted_device_key
        .ok_or("device-license-key")?
        .derive_device_key();
    Ok(LicenseProbe {
        device_ticket,
        device_key,
        content,
        market,
    })
}

async fn check_license(
    client: &reqwest::Client,
    probe: LicenseProbe,
    store: &xodus::api::live::device_code::Credentials,
) -> Result<(), String> {
    let user = store
        .user_id()
        .ok_or("Microsoft returned no account reference.")?;
    let ticket = xodus::models::secrets::licensing_ticket(format!("t={}", store.access_token()));
    let (_, license) = xodus::licensing::content::get_license_content(
        client,
        probe.device_ticket,
        ticket,
        user.into(),
        probe.content,
        probe.market,
    )
    .await
    .map_err(|error| error.to_string())?;
    let license = xodus::licensing::splicense::SPLicense::parse_base64(&license.splicense_block)
        .map_err(|_| "Invalid PC content-license block.")?;
    if license.content_keys.len() != 1 {
        return Err("Unexpected PC content-key count.".into());
    }
    let key = license
        .content_keys
        .into_values()
        .next()
        .ok_or("Missing PC content key.")?;
    // Authentication of the actual content key proves more than HTTP success.
    key.unpack(&probe.device_key)
        .map_err(|_| "PC content-key authentication failed.")?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            println!("PROBE_FAILED client");
            return;
        }
    };
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let title = if args.len() >= 2 && args[args.len() - 2] == "--runtime-client" {
        let id = args.pop().unwrap();
        args.pop();
        if id.len() != 16 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            println!("PROBE_FAILED runtime-client-format");
            return;
        }
        Some(id)
    } else {
        None
    };
    let probe = match args.as_slice() {
        [] => None,
        [flag, content, market]
            if flag == "--pc-license"
                && uuid::Uuid::parse_str(content).is_ok_and(|id| id.to_string() == *content)
                && market.len() == 2
                && market.bytes().all(|b| b.is_ascii_uppercase()) =>
        {
            match license_probe(&client, content.clone(), market.clone()).await {
                Ok(probe) => {
                    println!("EXISTING_DEVICE_PREFLIGHT_OK");
                    Some(probe)
                }
                Err(code) => {
                    println!("PROBE_FAILED {code}");
                    return;
                }
            }
        }
        _ => {
            println!(
                "Usage: device_signin_probe [--pc-license CONTENT_UUID MARKET] [--runtime-client MSA_APP_ID]"
            );
            return;
        }
    };
    let login = DeviceCodeClient::new(client.clone());
    let mut challenge = match login.start(Scope::Xbox).await {
        Ok(value) => value,
        Err(error) => {
            println!("PROBE_FAILED {error}");
            return;
        }
    };
    println!("SIGN_IN_URL {}", challenge.verification_uri);
    println!("SIGN_IN_CODE {}", challenge.user_code);
    println!(
        "SIGN_IN_EXPIRES_SECONDS {}",
        challenge.expires_in().as_secs()
    );
    std::io::stdout().flush().ok();
    let candidate = loop {
        match login.poll(&mut challenge).await {
            Ok(Poll::Pending) => continue,
            Ok(Poll::Complete(value)) => break value,
            Err(error) => {
                println!("PROBE_FAILED {error}");
                return;
            }
        }
    };
    println!("OAUTH_SIGN_IN_OK");
    if let Some(title) = title {
        for full_trust in [false, true] {
            match login
                .refresh_for_title(&candidate, &title, full_trust)
                .await
            {
                Ok(ticket) => {
                    println!("RUNTIME_TITLE_REFRESH_OK full_trust={full_trust}");
                    let mut accepted = false;
                    for prefix in ["t=", "d="] {
                        if xodus::api::xbox::authenticate_xbox_user(
                            &client,
                            format!("{prefix}{}", ticket.access_token()),
                        )
                        .await
                        .is_ok()
                        {
                            accepted = true;
                            break;
                        }
                    }
                    println!(
                        "RUNTIME_TITLE_XBOX_TICKET_OK full_trust={full_trust} accepted={accepted}"
                    );
                }
                Err(error) => {
                    println!("RUNTIME_TITLE_REFRESH_FAILED full_trust={full_trust} {error}")
                }
            }
        }
    }
    let mut user = None;
    for prefix in ["t=", "d="] {
        if let Ok(value) = xodus::api::xbox::authenticate_xbox_user(
            &client,
            format!("{prefix}{}", candidate.access_token()),
        )
        .await
        {
            user = Some(value);
            break;
        }
    }
    if let Some(user) = user {
        let profile = xodus::api::xbox::request_xsts_token(
            &client,
            user.token.clone(),
            "http://xboxlive.com",
        )
        .await;
        println!("XBOX_PROFILE_TICKET_OK {}", profile.is_ok());
        let marketplace =
            xodus::api::xbox::request_xsts_token(&client, user.token, "http://mp.microsoft.com/")
                .await;
        println!("XBOX_MARKETPLACE_TICKET_OK {}", marketplace.is_ok());
    } else {
        println!("XBOX_USER_TICKET_OK false");
    }
    match login.refresh(&candidate, Some(Scope::Store)).await {
        Ok(store) => {
            println!("MICROSOFT_STORE_SCOPE_OK true");
            if let Some(probe) = probe {
                match check_license(&client, probe, &store).await {
                    Ok(()) => println!("PC_CONTENT_LICENSE_AND_KEY_OK true"),
                    Err(error) => println!("PC_CONTENT_LICENSE_FAILED {error}"),
                }
            }
            println!(
                "XBOX_REFRESH_AFTER_STORE_SCOPE_OK {}",
                login.refresh(&store, Some(Scope::Xbox)).await.is_ok()
            );
        }
        Err(error) => println!("MICROSOFT_STORE_SCOPE_FAILED {error}"),
    }
    println!("CANDIDATE_NOT_ADOPTED native-sign-in-and-in-game-runtime-acceptance-still-required");
}
