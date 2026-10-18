use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;

use super::*;

#[test]
fn loopback_names_and_ip_literals_are_served() {
    let hosts = HostAllowList::loopback();
    for host in [
        "localhost",
        "LOCALHOST:8080",
        "localhost.:3000",
        "127.0.0.1:51234",
        "[::1]:51234",
        "192.168.1.20:8080",
        "[fe80::1]",
    ] {
        assert!(hosts.allows(Some(host)), "{host}");
    }
}

#[test]
fn foreign_and_malformed_hosts_are_rejected() {
    let hosts = HostAllowList::loopback();
    for host in [
        "attacker.example",
        "attacker.example:51234",
        "127.0.0.1.attacker.example",
        "localhost.attacker.example",
        "sub.localhost",
        "",
        "a:b:c",
        "[::1",
        "evil.example:0",
    ] {
        assert!(!hosts.allows(Some(host)), "{host}");
    }
}

#[test]
fn missing_host_is_allowed() {
    assert!(HostAllowList::loopback().allows(None));
}

#[test]
fn hosts_of_allowed_origins_are_served() {
    let policy = OriginPolicy::parse_list("https://Termul.Example.com, http://box.local:8080")
        .expect("origins");
    let hosts = HostAllowList::from_origins(&policy);
    assert!(hosts.allows(Some("termul.example.com")));
    assert!(hosts.allows(Some("box.local:8080")));
    assert!(!hosts.allows(Some("other.example.com")));
}

#[test]
fn a_name_added_later_reaches_every_clone() {
    let hosts = HostAllowList::loopback();
    let serving = hosts.clone();
    assert!(!serving.allows(Some("abc.trycloudflare.com")));
    hosts.allow("ABC.trycloudflare.com");
    assert!(serving.allows(Some("abc.trycloudflare.com")));
}

async fn status_for(host: Option<&str>) -> StatusCode {
    let app = layer(
        axum::Router::new().route("/health", get(|| async { "ok" })),
        HostAllowList::loopback(),
    );
    let mut request = Request::builder().uri("/health");
    if let Some(host) = host {
        request = request.header("host", host);
    }
    app.oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn middleware_rejects_a_rebinding_host_with_403() {
    assert_eq!(
        status_for(Some("attacker.example:8080")).await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn middleware_serves_loopback_hosts() {
    assert_eq!(status_for(Some("127.0.0.1:8080")).await, StatusCode::OK);
    assert_eq!(status_for(Some("localhost:8080")).await, StatusCode::OK);
    assert_eq!(status_for(None).await, StatusCode::OK);
}

fn lines(raw: &str) -> impl Iterator<Item = std::io::Result<String>> + '_ {
    raw.split("\r\n").map(|line| Ok(line.to_string()))
}

#[test]
fn hand_parsed_header_block_yields_the_host() {
    let mut request = lines("Host: attacker.example:5000\r\nAccept: */*\r\n\r\nbody");
    let headers = read_header_block(&mut request).expect("complete block");
    assert_eq!(
        header_value(&headers, "HOST"),
        Some("attacker.example:5000")
    );
    assert_eq!(header_value(&headers, "cookie"), None);
}

#[test]
fn unterminated_header_block_is_none() {
    let mut request = lines("Host: 127.0.0.1:5000\r\nAccept: */*");
    assert_eq!(read_header_block(&mut request), None);
    let mut failing = std::iter::once(Err(std::io::Error::from(std::io::ErrorKind::TimedOut)));
    assert_eq!(read_header_block(&mut failing), None);
}

#[test]
fn any_accepts_every_host() {
    let hosts = HostAllowList::any();
    assert!(hosts.allows(Some("termul-server:8080")));
    assert!(hosts.allows(Some("termul.example.com")));
}
