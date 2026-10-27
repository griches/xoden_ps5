use super::*;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn server(
    replies: Vec<(u16, String)>,
) -> (DeviceCodeClient, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        for (status, body) in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut headers = vec![];
            while !headers.ends_with(b"\r\n\r\n") {
                headers.push(socket.read_u8().await.unwrap());
                assert!(headers.len() < 8192);
            }
            let header = String::from_utf8(headers).unwrap();
            let length: usize = header
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .unwrap()
                .parse()
                .unwrap();
            let mut data = vec![0; length];
            socket.read_exact(&mut data).await.unwrap();
            requests.push(String::from_utf8(data).unwrap());
            socket.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
        requests
    });
    (
        DeviceCodeClient {
            client: Client::new(),
            connect: format!("{base}/connect"),
            token: format!("{base}/token"),
        },
        task,
    )
}
fn challenge() -> Value {
    json!({"user_code":"TEST-CODE","device_code":"synthetic-device-secret","verification_uri":"https://www.microsoft.com/link","interval":5,"expires_in":900})
}
fn tokens() -> Value {
    json!({"access_token":"synthetic-access-secret","refresh_token":"synthetic-refresh-secret","user_id":"synthetic-user","expires_in":3600})
}
fn ready(challenge: &mut Challenge) {
    challenge.next_poll = Instant::now();
}

#[tokio::test]
async fn device_code_pending_slowdown_and_success_keep_credentials_out_of_display_fields() {
    let (client, task) = server(vec![
        (200, challenge().to_string()),
        (400, json!({"error":"authorization_pending"}).to_string()),
        (400, json!({"error":"slow_down"}).to_string()),
        (200, tokens().to_string()),
    ])
    .await;
    let mut code = client.start(Scope::Xbox).await.unwrap();
    assert_eq!(code.user_code, "TEST-CODE");
    assert_eq!(code.interval, Duration::from_secs(5));
    ready(&mut code);
    assert!(matches!(
        client.poll(&mut code).await.unwrap(),
        Poll::Pending
    ));
    ready(&mut code);
    assert!(matches!(
        client.poll(&mut code).await.unwrap(),
        Poll::Pending
    ));
    assert_eq!(code.interval, Duration::from_secs(10));
    ready(&mut code);
    let Poll::Complete(session) = client.poll(&mut code).await.unwrap() else {
        panic!("not complete")
    };
    assert_eq!(session.access_token(), "synthetic-access-secret");
    assert_eq!(session.refresh_token(), "synthetic-refresh-secret");
    assert_eq!(session.user_id(), Some("synthetic-user"));
    assert!(code.device_code.is_empty());
    assert!(code.expires_in().is_zero());
    let requests = task.await.unwrap();
    assert!(requests[0].contains("client_id=000000004424da1f"));
    assert!(
        requests[1..]
            .iter()
            .all(|r| r.contains("device_code=synthetic-device-secret"))
    );
}

#[tokio::test]
async fn device_code_cancel_and_expiry_make_no_network_request() {
    let mut code = Challenge {
        user_code: "TEST".into(),
        verification_uri: "https://www.microsoft.com/link".into(),
        device_code: "synthetic".into(),
        deadline: Instant::now() + Duration::from_secs(30),
        next_poll: Instant::now(),
        interval: Duration::from_secs(1),
        cancelled: false,
        scope: Scope::Xbox,
    };
    // No listener: a network request would produce Pending rather than these errors.
    let client = DeviceCodeClient {
        client: Client::new(),
        connect: String::new(),
        token: "http://127.0.0.1:1".into(),
    };
    code.cancel();
    assert!(matches!(
        client.poll(&mut code).await,
        Err(Error::Cancelled)
    ));
    assert!(code.device_code.is_empty());
    code.cancelled = false;
    code.deadline = Instant::now();
    assert!(matches!(client.poll(&mut code).await, Err(Error::Expired)));
}

#[tokio::test]
async fn device_code_rejects_untrusted_login_links_and_invalid_lifetimes() {
    for (key, value) in [
        ("verification_uri", json!("https://evil.invalid/link")),
        ("interval", json!(0)),
        ("expires_in", json!(86400)),
        ("device_code", json!("")),
        ("user_code", json!("bad\ncode")),
    ] {
        let mut body = challenge();
        body[key] = value;
        let (client, task) = server(vec![(200, body.to_string())]).await;
        assert!(matches!(
            client.start(Scope::Xbox).await,
            Err(Error::Response)
        ));
        task.await.unwrap();
    }
}

#[tokio::test]
async fn device_code_transient_and_denied_responses_never_return_provider_details() {
    let (client, task) = server(vec![
        (200, challenge().to_string()),
        (503, "private-provider-message".into()),
        (429, "private-provider-message".into()),
        (
            400,
            json!({"error":"access_denied","error_description":"private-provider-message"})
                .to_string(),
        ),
    ])
    .await;
    let mut code = client.start(Scope::Xbox).await.unwrap();
    ready(&mut code);
    assert!(matches!(
        client.poll(&mut code).await.unwrap(),
        Poll::Pending
    ));
    ready(&mut code);
    assert!(matches!(
        client.poll(&mut code).await.unwrap(),
        Poll::Pending
    ));
    assert_eq!(code.interval, Duration::from_secs(10));
    ready(&mut code);
    let error = match client.poll(&mut code).await {
        Err(error) => error,
        _ => panic!("denial accepted"),
    };
    assert_eq!(error, Error::Denied);
    assert!(!error.to_string().contains("private"));
    assert!(code.cancelled);
    task.await.unwrap();
}

#[tokio::test]
async fn device_code_refresh_supports_rotation_and_store_scope_without_mutating_old_session() {
    let previous = credentials(&tokens(), None, Scope::Xbox).unwrap();
    let mut unchanged = tokens();
    unchanged.as_object_mut().unwrap().remove("refresh_token");
    let mut rotated = tokens();
    rotated["refresh_token"] = json!("synthetic-rotated-secret");
    let (client, task) = server(vec![
        (200, unchanged.to_string()),
        (200, rotated.to_string()),
        (
            400,
            json!({"error":"invalid_grant","error_description":"private-secret"}).to_string(),
        ),
    ])
    .await;
    let fresh = client.refresh(&previous, Some(Scope::Store)).await.unwrap();
    assert_eq!(fresh.refresh_token(), previous.refresh_token());
    let rotated = client.refresh(&previous, None).await.unwrap();
    assert_eq!(rotated.refresh_token(), "synthetic-rotated-secret");
    assert!(matches!(
        client.refresh(&previous, None).await,
        Err(Error::Rejected)
    ));
    assert_eq!(previous.refresh_token(), "synthetic-refresh-secret");
    let requests = task.await.unwrap();
    assert!(requests[0].contains("scope=service%3A%3Awww.microsoft.com%3A%3AMBI_SSL"));
}

#[tokio::test]
async fn device_code_bounds_responses_and_rejects_refresh_account_changes() {
    let previous = credentials(&tokens(), None, Scope::Xbox).unwrap();
    let mut other = tokens();
    other["user_id"] = json!("another-user");
    let (client, task) = server(vec![
        (200, "x".repeat(RESPONSE_LIMIT + 1)),
        (200, other.to_string()),
    ])
    .await;
    assert!(matches!(
        client.start(Scope::Xbox).await,
        Err(Error::Response)
    ));
    assert!(matches!(
        client.refresh(&previous, None).await,
        Err(Error::Response)
    ));
    assert_eq!(previous.user_id(), Some("synthetic-user"));
    task.await.unwrap();
}

#[tokio::test]
async fn device_code_title_refresh_binds_client_scope_and_keeps_app_session_separate() {
    let previous = credentials(&tokens(), None, Scope::Xbox).unwrap();
    let mut title = tokens();
    title["refresh_token"] = json!("synthetic-title-refresh");
    let (client, task) = server(vec![
        (200, title.to_string()),
        (200, title.to_string()),
        (400, json!({"error":"invalid_grant"}).to_string()),
    ])
    .await;
    let fresh = client
        .refresh_for_title(&previous, "0000000041180A5D", false)
        .await
        .unwrap();
    assert_eq!(fresh.client_id, "0000000041180A5D");
    assert_eq!(previous.client_id, CLIENT_ID);
    assert_eq!(previous.refresh_token(), "synthetic-refresh-secret");
    let _ = client.refresh(&fresh, None).await.unwrap();
    assert!(matches!(
        client
            .refresh_for_title(&previous, "0000000041180A5D", true)
            .await,
        Err(Error::Rejected)
    ));
    let requests = task.await.unwrap();
    assert!(requests[0].contains("client_id=0000000041180A5D"));
    assert!(requests[0].contains("scope=Xboxlive.signin"));
    assert!(requests[1].contains("client_id=0000000041180A5D"));
    assert!(requests[1].contains("refresh_token=synthetic-title-refresh"));
    assert!(requests[2].contains("scope=service%3A%3Auser.auth.xboxlive.com%3A%3AMBI_SSL"));
}

#[tokio::test]
async fn device_code_title_refresh_rejects_invalid_clients_and_missing_bound_refresh() {
    let previous = credentials(&tokens(), None, Scope::Xbox).unwrap();
    let mut missing = tokens();
    missing.as_object_mut().unwrap().remove("refresh_token");
    let mut other = tokens();
    other["user_id"] = json!("different-account");
    let (client, task) = server(vec![(200, missing.to_string()), (200, other.to_string())]).await;
    for invalid in ["", "not-a-client-id!!", "0000000041180A5D&scope=other"] {
        assert!(matches!(
            client.refresh_for_title(&previous, invalid, false).await,
            Err(Error::Response)
        ));
    }
    assert!(matches!(
        client
            .refresh_for_title(&previous, "0000000041180A5D", false)
            .await,
        Err(Error::Response)
    ));
    assert!(matches!(
        client
            .refresh_for_title(&previous, "0000000041180A5D", false)
            .await,
        Err(Error::Response)
    ));
    task.await.unwrap();
}

#[test]
fn device_code_private_candidate_restores_real_expiry_and_rejects_foreign_fields() {
    let session = credentials(&tokens(), None, Scope::Xbox).unwrap();
    let bytes = session.private_candidate_record().unwrap();
    let fresh = Credentials::from_private_candidate_record(&bytes).unwrap();
    assert_eq!(fresh.access_token(), session.access_token());
    assert_eq!(fresh.user_id(), session.user_id());
    assert!(fresh.expires_in() <= session.expires_in() && fresh.expires_in().as_secs() > 3500);
    let mut record: Value = serde_json::from_slice(&bytes).unwrap();
    record["expires_at"] = json!(chrono::Utc::now().timestamp() - 10);
    let expired =
        Credentials::from_private_candidate_record(&serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(expired.expires_in().is_zero());
    for (field, value) in [
        ("schema", json!(2)),
        ("client_id", json!("0000000041180A5D")),
        ("user_id", json!("")),
        ("refresh_token", json!("bad\nsecret")),
        ("expires_at", json!(chrono::Utc::now().timestamp() + 90000)),
        ("foreign", json!("never-accept")),
    ] {
        let mut bad = record.clone();
        bad[field] = value;
        assert!(
            Credentials::from_private_candidate_record(&serde_json::to_vec(&bad).unwrap()).is_err()
        );
    }
    assert!(Credentials::from_private_candidate_record(&vec![0; RESPONSE_LIMIT + 1]).is_err());
    assert!(
        credentials(&tokens(), None, Scope::Store)
            .unwrap()
            .private_candidate_record()
            .is_err()
    );
}
