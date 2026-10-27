//! Interactive provider validation only. Keeps the candidate in memory; never
//! opens the working account/keychain, provisions a device or adopts a session.
use std::io::Write;
use xodus::api::live::device_code::{DeviceCodeClient, Poll, Scope};

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
            println!(
                "XBOX_REFRESH_AFTER_STORE_SCOPE_OK {}",
                login.refresh(&store, Some(Scope::Xbox)).await.is_ok()
            );
        }
        Err(error) => println!("MICROSOFT_STORE_SCOPE_FAILED {error}"),
    }
    println!("CANDIDATE_NOT_ADOPTED PC-license-and-runtime-ticket-validation-still-required");
}
