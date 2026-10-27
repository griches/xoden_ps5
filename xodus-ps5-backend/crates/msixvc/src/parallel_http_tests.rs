use super::*;
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn policy() -> Policy {
    Policy {
        timeout: Duration::from_secs(2),
        budget: Duration::from_secs(5),
        backoff: Duration::from_millis(1),
        retries: 0,
    }
}
fn payload(start: u64, end: u64) -> Vec<u8> {
    (start..=end).map(|n| (n % 251) as u8).collect()
}
async fn request(socket: &mut tokio::net::TcpStream) -> (u64, u64) {
    let mut bytes = vec![];
    while !bytes.ends_with(b"\r\n\r\n") {
        bytes.push(socket.read_u8().await.unwrap());
        assert!(bytes.len() < 8192);
    }
    let text = String::from_utf8(bytes).unwrap().to_lowercase();
    let range = text
        .lines()
        .find_map(|l| l.strip_prefix("range: bytes="))
        .unwrap();
    let (start, end) = range.split_once('-').unwrap();
    (start.parse().unwrap(), end.parse().unwrap())
}
async fn reply(socket: &mut tokio::net::TcpStream, start: u64, end: u64, total: u64) {
    let header = format!(
        "HTTP/1.1 206 Test\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        end - start + 1
    );
    if socket.write_all(header.as_bytes()).await.is_ok() {
        socket.write_all(&payload(start, end)).await.ok();
    }
}

#[tokio::test]
async fn parallel_http_overlapping_requests_deliver_exact_order_and_bound_ranges() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/package?private=never-log",
        listener.local_addr().unwrap()
    );
    let barrier = Arc::new(tokio::sync::Barrier::new(4));
    let server = tokio::spawn(async move {
        let mut tasks = vec![];
        for _ in 0..4 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                let (start, end) = request(&mut socket).await;
                assert_eq!(end - start + 1, 32768);
                // All four connections must be open before any body is sent.
                barrier.wait().await;
                if start == 4096 {
                    sleep(Duration::from_millis(20)).await;
                }
                reply(&mut socket, start, end, 200000).await;
                (start, end)
            }));
        }
        let mut ranges = vec![];
        for task in tasks {
            ranges.push(task.await.unwrap());
        }
        ranges.sort();
        ranges
    });
    let mut reader = OrderedRanges::new(
        &reqwest::Client::new(),
        &url,
        4096,
        135167,
        policy(),
        32768,
        4,
    );
    let mut bytes = vec![];
    timeout(Duration::from_secs(3), async {
        while let Some(chunk) = reader.next().await.unwrap() {
            assert!(chunk.len() <= 16384);
            bytes.extend_from_slice(&chunk);
        }
    })
    .await
    .unwrap();
    assert_eq!(bytes, payload(4096, 135167));
    assert_eq!(
        server.await.unwrap(),
        vec![
            (4096, 36863),
            (36864, 69631),
            (69632, 102399),
            (102400, 135167)
        ]
    );
}

#[tokio::test]
async fn parallel_http_disconnect_resumes_at_delivered_offset_not_prefetched_offset() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/package", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(vec![]));
    let server_seen = seen.clone();
    let server = tokio::spawn(async move {
        let mut tasks = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (mut socket, _) = result.unwrap(); let seen = server_seen.clone();
                    tasks.spawn(async move {
                        let (start,end) = request(&mut socket).await;
                        seen.lock().unwrap().push((start,end));
                        reply(&mut socket,start,end,262144).await;
                    });
                }
                _ = sleep(Duration::from_millis(150)) => break,
            }
        }
        while let Some(result) = tasks.join_next().await {
            result.unwrap();
        }
    });
    let mut reader = OrderedRanges::new(
        &reqwest::Client::new(),
        &url,
        4096,
        135167,
        policy(),
        32768,
        4,
    );
    let first = reader.next().await.unwrap().unwrap();
    assert_eq!(first.len(), 16384);
    let mut bytes = first.to_vec();
    reader.disconnect();
    while let Some(chunk) = reader.next().await.unwrap() {
        bytes.extend_from_slice(&chunk);
    }
    server.await.unwrap();
    assert_eq!(bytes, payload(4096, 135167));
    assert!(seen.lock().unwrap().contains(&(20480, 53247)));
}

#[tokio::test]
async fn parallel_http_rejects_changed_total_and_caches_sanitized_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/package?private=never-log",
        listener.local_addr().unwrap()
    );
    let server = tokio::spawn(async move {
        let mut tasks = vec![];
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            tasks.push(tokio::spawn(async move {
                let (start, end) = request(&mut socket).await;
                reply(
                    &mut socket,
                    start,
                    end,
                    if start == 0 { 131072 } else { 131073 },
                )
                .await;
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
    });
    let mut reader =
        OrderedRanges::new(&reqwest::Client::new(), &url, 0, 65535, policy(), 32768, 2);
    let mut delivered = 0;
    let error = loop {
        match reader.next().await {
            Ok(Some(bytes)) => delivered += bytes.len(),
            Err(error) => break error,
            Ok(None) => panic!("changed package accepted"),
        }
    };
    assert_eq!(delivered, 32768);
    assert_eq!(error, TransferError::new("transfer-range", 206));
    assert!(!error.to_string().contains("private"));
    assert_eq!(reader.next().await.unwrap_err(), error);
    server.await.unwrap();
}

#[test]
fn parallel_http_small_segments_keep_the_single_stream_path() {
    let client = reqwest::Client::new();
    assert!(matches!(
        SegmentReader::new(&client, "http://synthetic.invalid", 0, 8191, policy()),
        SegmentReader::Single(_)
    ));
    assert!(matches!(
        SegmentReader::new(
            &client,
            "http://synthetic.invalid",
            0,
            8 * 1024 * 1024 - 1,
            policy()
        ),
        SegmentReader::Parallel(_)
    ));
}

#[tokio::test]
async fn parallel_http_drop_closes_speculative_connections() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/package", listener.local_addr().unwrap());
    let barrier = Arc::new(tokio::sync::Barrier::new(4));
    let server = tokio::spawn(async move {
        let mut tasks = vec![];
        for _ in 0..4 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                let (start,end) = request(&mut socket).await;
                barrier.wait().await;
                if start == 0 { reply(&mut socket,start,end,131072).await; }
                else {
                    socket.write_all(format!("HTTP/1.1 206 Test\r\nContent-Range: bytes {start}-{end}/131072\r\nContent-Length: 32768\r\n\r\n").as_bytes()).await.unwrap();
                    // A cancelled reader must close this unfinished body.
                    assert_eq!(timeout(Duration::from_secs(2),socket.read(&mut [0;1])).await.unwrap().unwrap(),0);
                }
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
    });
    let mut reader =
        OrderedRanges::new(&reqwest::Client::new(), &url, 0, 131071, policy(), 32768, 4);
    assert_eq!(reader.next().await.unwrap().unwrap(), payload(0, 16383));
    drop(reader);
    server.await.unwrap();
}

#[tokio::test]
#[ignore = "controlled throughput comparison; not a real CDN or console benchmark"]
async fn parallel_http_per_connection_benchmark() {
    let length = 8 * 1024 * 1024u64;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/package", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let mut tasks = vec![];
        for _ in 0..5 {
            let (mut socket, _) = listener.accept().await.unwrap();
            tasks.push(tokio::spawn(async move {
                let (start,end) = request(&mut socket).await;
                socket.write_all(format!("HTTP/1.1 206 Test\r\nContent-Range: bytes {start}-{end}/{length}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",end-start+1).as_bytes()).await.unwrap();
                let mut position=start;
                while position<=end {
                    let last=(position+32767).min(end);
                    sleep(Duration::from_millis(2)).await;
                    socket.write_all(&payload(position,last)).await.unwrap();
                    position=last+1;
                }
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
    });
    let client = reqwest::Client::new();
    let mut scalar = RangeReader::new(&client, &url, 0, length - 1, policy());
    let begin = Instant::now();
    let mut expected = vec![];
    while let Some(chunk) = scalar.next().await.unwrap() {
        expected.extend_from_slice(&chunk);
    }
    let scalar_time = begin.elapsed();
    let mut parallel = SegmentReader::new(&client, &url, 0, length - 1, policy());
    let begin = Instant::now();
    let mut actual = vec![];
    while let Some(chunk) = parallel.next().await.unwrap() {
        actual.extend_from_slice(&chunk);
    }
    let parallel_time = begin.elapsed();
    assert_eq!(actual, expected);
    assert_eq!(actual, payload(0, length - 1));
    println!(
        "Synthetic per-connection-limited server: single={:.2} MB/s parallel={:.2} MB/s ratio={:.2}x",
        length as f64 / scalar_time.as_secs_f64() / 1e6,
        length as f64 / parallel_time.as_secs_f64() / 1e6,
        scalar_time.as_secs_f64() / parallel_time.as_secs_f64()
    );
    server.await.unwrap();
}
