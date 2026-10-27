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
