//! Fresh Microsoft browser authorization with the PS5 acting as the broker.
//! The helper receives no device keys. TLS is pinned to the fingerprint shown
//! on the TV; only the authenticated Microsoft callback crosses this channel.
use super::*;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_rustls::{TlsAcceptor, rustls};
use sha2::{Digest, Sha256};
use xodus::models::{live::{DAProperty, ExchangeUserTokenOutcome, HostBridgeMessage}, secrets, soap};

const LIMIT: usize = 128*1024;
const CLIENT: &str = "000000004424da1f";

fn current_event(event:&Value,session:u64)->Result<bool> {
    let received=event["session"].as_u64().ok_or("browser-session-order")?;
    if received<session {return Ok(false);}
    if received!=session {return Err("browser-session-order");}
    Ok(true)
}

async fn frame<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> Result<Value> {
    let mut bytes=Vec::new();
    reader.take((LIMIT+1) as u64).read_until(b'\n', &mut bytes).await.map_err(|_| "browser-disconnected")?;
    if bytes.len()>LIMIT || bytes.last()!=Some(&b'\n') { return Err("browser-frame-size"); }
    serde_json::from_slice(&bytes).map_err(|_| "browser-frame-json")
}
async fn send<W: tokio::io::AsyncWrite + Unpin>(writer: &mut W, value: Value) -> Result<()> {
    let mut bytes=serde_json::to_vec(&value).map_err(|_| "browser-frame-json")?;
    if bytes.len()>LIMIT { return Err("browser-frame-size"); }
    bytes.push(b'\n');writer.write_all(&bytes).await.map_err(|_| "browser-disconnected")?;
    writer.flush().await.map_err(|_| "browser-disconnected")
}

fn headers() -> Value {
    json!({"cxh-capabilities":"{\"PrivatePropertyBag\":1,\"PasswordlessConnect\":1,\"PreferAssociate\":1,\"ChromelessUI\":0}",
        "cxh-correlationid":uuid::Uuid::new_v4().to_string(),"cxh-msabinaryversion":"55",
        "cxh-identityclientbinaryversion":"3","cxh-osversioninfo":"{\"platformId\":2,\"majorVersion\":10,\"minorVersion\":0,\"buildNumber\":26100}",
        "cxh-platform":"CloudExperienceHost.Platform.DESKTOP","cxh-protocol":"TokenBroker",
        "cxh-source":"TokenBroker","hostapp":"CloudExperienceHost"})
}
fn trusted_url(value: &str) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url|url.scheme()=="https" && url.port().is_none() && url.username().is_empty() && url.password().is_none()
        && matches!(url.host_str(),Some("login.live.com"|"account.live.com"|"login.microsoftonline.com"|"login.microsoft.com"|"account.microsoft.com"|"sisu.xboxlive.com")))
}
fn issued(body: soap::BodyContent) -> Result<std::collections::HashMap<String,secrets::Token>> {
    let responses=match body {
        soap::BodyContent::RequestSecurityTokenResponseCollection(value)=>value.security_tokens,
        soap::BodyContent::RequestSecurityTokenResponse(value)=>vec![*value],
        _=>return Err("browser-token-shape"),
    };
    let mut output=std::collections::HashMap::new();
    for response in responses {
        let address=response.applies_to.endpoint_reference.address.clone();
        let token=secrets::Token::try_from(response).map_err(|_| "browser-token-parse")?;
        let address=match &token { secrets::Token::Legacy(value)=>value.key_name.clone().unwrap_or(address),_=>address };
        if output.insert(address,token).is_some() { return Err("browser-token-duplicate"); }
    }
    if !output.contains_key(xodus::tokens::PASSPORT_STS) { return Err("browser-session-missing"); }
    Ok(output)
}

pub(super) async fn run(client: &reqwest::Client,tokens: &TokenManager,root: &Path,req: &Request) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(600),execute(client,tokens,root,req)).await.map_err(|_| "browser-sign-in-expired")?
}
async fn execute(client: &reqwest::Client,tokens: &TokenManager,root: &Path,req: &Request) -> Result<()> {
    state(root,req,"checking-sign-in",json!({}))?;
    let expected=match tokens.active_account(){Ok(value)=>Some(value),Err(xodus::tokens::store::TokenStoreError::NotFound)=>None,Err(_)=>return Err("browser-session-read")};
    let expected_xuid=if expected.is_some(){Some(super::super::profile::xbox_user_id(tokens).await?)}else{None};
    xodus::tokens::device::ensure_device_credentials(client,tokens).await.map_err(|_| "browser-device-refresh")?;
    let memory=TokenManager::with_memory();
    memory.save_device_license(&tokens.get_device_license().map_err(|_| "existing-device-required")?).map_err(|_| "device-copy")?;
    if let Ok(ticket)=tokens.get_device_sts_token() { memory.save_device_token(xodus::tokens::PASSPORT_STS.into(),ticket).map_err(|_| "device-copy")?; }
    xodus::tokens::device::ensure_device_credentials(client,&memory).await.map_err(|_| "browser-device-refresh")?;
    let secrets::Token::Legacy(device)=memory.get_device_sts_token().map_err(|_| "browser-device-ticket")? else { return Err("browser-device-ticket"); };
    let rcgen::CertifiedKey { cert,signing_key }=rcgen::generate_simple_self_signed(vec!["gamepass-console.local".into()]).map_err(|_| "browser-certificate")?;
    let fingerprint=Sha256::digest(cert.der()).iter().map(|b|format!("{b:02x}")).collect::<String>();
    let config=rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .with_safe_default_protocol_versions().map_err(|_| "browser-tls")?.with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()],rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into()).map_err(|_| "browser-tls")?;
    let listener=tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED,47834)).await.map_err(|_| "browser-listener")?;
    let port=listener.local_addr().map_err(|_| "browser-listener")?.port();
    state(root,req,"browser-pairing",json!({"port":port,"fingerprint":fingerprint,"expiresAt":(chrono::Utc::now()+chrono::Duration::minutes(10)).to_rfc3339()}))?;
    let acceptor=TlsAcceptor::from(Arc::new(config));
    // Certificate probes contain no application data. A disconnected probe may
    // reconnect after the helper has verified the TV fingerprint.
    let (mut reader,mut writer)=loop {
        let (tcp,_)=listener.accept().await.map_err(|_| "browser-accept")?;
        let Ok(Ok(tls))=tokio::time::timeout(Duration::from_secs(5),acceptor.accept(tcp)).await else { continue; };
        let (read,write)=tokio::io::split(tls);
        let mut reader=BufReader::new(read);
        let Ok(Ok(hello))=tokio::time::timeout(Duration::from_secs(5),frame(&mut reader)).await else { continue; };
        if hello!=json!({"type":"hello","version":1}) { continue; }
        break (reader,write);
    };
    drop(listener); // One helper, one fresh authorization, no background service.
    state(root,req,"browser-signing-in",json!({}))?;
    let mut url=format!("https://login.live.com/ppsecure/InlineLogin.srf?id=80604&scid=3&mkt=en-US&Platform=Windows10&clientid={CLIENT}&hosted=1");
    let mut finish=false;
    for session in 1..=8 {
        if !trusted_url(&url) { return Err("browser-untrusted-url"); }
        send(&mut writer,json!({"type":"open","session":session,"title":"Microsoft sign-in","url":url,"headers":headers()})).await?;
        let data=loop {
            let event=frame(&mut reader).await?;
            // Microsoft may finish a leg through both notify and post.srf.
            // Ignore its queued callbacks once that leg has closed, just as
            // the original Xodus webview transport does.
            if !current_event(&event,session)? {continue;}
            match event["type"].as_str() {
                Some("closed")=>return Err("browser-sign-in-cancelled"),
                Some("ipc")=>{
                    let payload=event["payload"].as_str().filter(|v|v.len()<=65536).ok_or("browser-payload")?;
                    if let Ok(data)=serde_json::from_str::<DAProperty>(payload) { break data; }
                    if let Ok(message)=serde_json::from_str::<HostBridgeMessage>(payload) {
                        if let Some(context)=message.get_context_invoke() {
                            send(&mut writer,json!({"type":"context","session":session,"context":context})).await?;
                        }
                    }
                },
                _=>return Err("browser-event"),
            }
        };
        state(root,req,"checking-browser-access",json!({}))?;
        let user=secrets::User { puid:data.puid,username:data.username };
        let ticket=secrets::LegacyToken { key_name:None,token:data.da_token,binary_secret:None,tpm_key:None,
            lifetime:soap::Timestamp { id:None,created:data.da_start_time,expires:data.da_expires } };
        let mut scopes=vec![("scope=service::user.auth.xboxlive.com::MBI_SSL&api-version=2.0".into(),Some(soap::PolicyReference::token_broker()))];
        if finish { scopes.push(("http://Passport.NET/tb".into(),None)); }
        let outcome=tokio::time::timeout(Duration::from_secs(60),xodus::api::live::exchange_user_token(client,ticket,user.username.clone(),device.clone(),Some(data.sts_inline_flow_token),None,CLIENT.into(),&scopes))
            .await.map_err(|_| "browser-exchange-timeout")?.map_err(|_| "browser-exchange-failed")?;
        send(&mut writer,json!({"type":"close","session":session})).await?;
        match outcome {
            ExchangeUserTokenOutcome::Fault(fault)=>{
                url=fault.and_then(|v|v.inline_auth_url).ok_or("browser-authorization-denied")?;
                finish=true;
            },
            ExchangeUserTokenOutcome::Issued(body)=>{
                let issued=issued(body)?;
                memory.save_user_session(secrets::User { puid:user.puid.clone(),username:user.username.clone() },issued.clone()).map_err(|_| "browser-session-verify")?;
                let signed_in_xuid=super::super::profile::xbox_user_id(&memory).await?;
                if expected_xuid.as_ref().is_some_and(|previous|previous!=&signed_in_xuid) { return Err("browser-account-mismatch"); }
                tokens.save_primary_browser_session(expected.as_deref(),user,issued).map_err(|_| "browser-session-save")?;
                state(root,req,"browser-sign-in-saved",json!({"browserSessionSaved":true,"runtimeVerified":false}))?;
                // Persistence already succeeded. A computer closing its window
                // cannot turn this committed sign-in into a failed account job.
                let _=send(&mut writer,json!({"type":"complete","saved":true})).await;
                return Ok(());
            },
        }
    }
    Err("browser-too-many-steps")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_duplicate_callbacks_cannot_abort_the_next_login_leg() {
        assert!(!current_event(&json!({"type":"ipc","session":1}),2).unwrap());
        assert!(current_event(&json!({"type":"ipc","session":2}),2).unwrap());
        assert!(current_event(&json!({"type":"ipc","session":3}),2).is_err());
    }
    #[tokio::test]
    async fn browser_frames_are_bounded_and_require_a_terminator() {
        assert!(frame(&mut &b"{}"[..]).await.is_err());
        assert!(frame(&mut &vec![b'x';LIMIT+1][..]).await.is_err());
        assert_eq!(frame(&mut &b"{\"type\":\"closed\"}\n"[..]).await.unwrap()["type"],"closed");
    }
    #[test]
    fn browser_final_leg_never_forwards_credentials_to_another_origin() {
        assert!(trusted_url("https://login.live.com/finish"));
        for value in ["http://login.live.com/", "https://login.live.com.evil.test/", "https://user@login.live.com/", "https://login.live.com:444/"] { assert!(!trusted_url(value)); }
        assert_ne!(headers()["cxh-correlationid"],headers()["cxh-correlationid"]);
    }
}
