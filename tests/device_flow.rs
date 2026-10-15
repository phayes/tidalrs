//! Integration tests for OAuth authentication flows.
//!
//! These tests use `mockito` to stand up a local HTTP server in place of
//! `https://auth.tidal.com/v1`, exercising the dedicated auth transport
//! without real network calls.

use tidalrs::{Error, TidalClient};

fn test_client(server_url: &str) -> TidalClient {
    TidalClient::new("test_client_id".to_string()).with_auth_base_url(server_url.to_string())
}

fn test_client_with_secret(server_url: &str) -> TidalClient {
    test_client(server_url).with_client_secret("client_secret")
}

const AUTHZ_TOKEN_BODY: &str = r#"{
    "access_token": "some_token",
    "clientName": "some_client",
    "expires_in": 4444,
    "refresh_token": "some_refresh",
    "scope": "r_usr w_usr",
    "token_type": "Bearer",
    "user": {
        "acceptedEULA": true,
        "accountLinkCreated": false,
        "channelId": 1,
        "countryCode": "CA",
        "created": 0,
        "email": "null@example.com",
        "emailVerified": true,
        "newUser": false,
        "parentId": 0,
        "updated": 0,
        "userId": 8675309,
        "username": "someuser"
    },
    "user_id": 8675309
}"#;

const DEVICE_AUTH_BODY: &str = r#"{
    "verificationUriComplete": "link.tidal.com/browse?code=ABCD",
    "deviceCode": "device-code",
    "expiresIn": 300,
    "userCode": "ABCD",
    "interval": 5
}"#;

#[tokio::test]
async fn test_device_authorization_stores_interval_and_https_url() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/oauth2/device_authorization")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(DEVICE_AUTH_BODY)
        .create_async()
        .await;

    let client = test_client(&server.url());
    let resp = client.device_authorization().await.expect("device auth");

    mock.assert_async().await;
    assert_eq!(resp.url, "https://link.tidal.com/browse?code=ABCD");
    assert_eq!(resp.device_code, "device-code");
    assert_eq!(resp.user_code, "ABCD");
    assert_eq!(resp.expires_in, 300);
    assert_eq!(resp.interval, 5);
}

#[tokio::test]
async fn test_device_authorization_defaults_interval() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/oauth2/device_authorization")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "verificationUriComplete": "link.tidal.com/browse",
                "deviceCode": "device-code",
                "expiresIn": 300,
                "userCode": "ABCD"
            }"#,
        )
        .create_async()
        .await;

    let client = test_client(&server.url());
    let resp = client.device_authorization().await.expect("device auth");
    mock.assert_async().await;
    assert_eq!(resp.interval, 5);
}

#[tokio::test]
async fn test_authorize_returns_oauth_pending() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/oauth2/token")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error":"authorization_pending","error_description":"The authorization request is still pending"}"#)
        .create_async()
        .await;

    let client = test_client(&server.url());
    let result = client.authorize("device_code", "client_secret").await;

    mock.assert_async().await;
    match result {
        Err(Error::OAuth { error, description }) => {
            assert_eq!(error, "authorization_pending");
            assert!(description.contains("pending"));
        }
        other => panic!("expected OAuth pending error, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_other_400_is_tidal_api_error() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/oauth2/token")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status":400,"subStatus":1001,"userMessage":"Invalid client credentials"}"#)
        .create_async()
        .await;

    let client = test_client(&server.url());
    let result = client.authorize("device_code", "bad_secret").await;

    mock.assert_async().await;
    assert!(
        matches!(result, Err(Error::TidalApiError(_))),
        "expected TidalApiError for a non-oauth 400, got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_authorize_stores_user_authz() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/oauth2/token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(AUTHZ_TOKEN_BODY)
        .create_async()
        .await;

    let client = test_client(&server.url());
    let token = client
        .authorize("device_code", "client_secret")
        .await
        .expect("authorize");

    mock.assert_async().await;
    assert_eq!(token.access_token, "some_token");
    assert_eq!(token.user.user_id, 8675309);
    let authz = client.get_authz().expect("stored");
    assert_eq!(authz.access_token, "some_token");
    assert_eq!(authz.refresh_token.as_deref(), Some("some_refresh"));
    assert_eq!(authz.user_id, Some(8675309));
    assert_eq!(authz.country_code.as_deref(), Some("CA"));
    assert!(authz.expires_timestamp.is_some());
}

#[tokio::test]
async fn test_wait_for_authorization_requires_secret() {
    let client = TidalClient::new("client_id".to_string());
    let err = client
        .wait_for_authorization("device", 5, 1)
        .await
        .expect_err("secret required");
    assert!(matches!(err, Error::ClientSecretRequired));
}

#[tokio::test]
async fn test_wait_for_authorization_succeeds_after_pending() {
    let mut server = mockito::Server::new_async().await;
    let mock_pending = server
        .mock("POST", "/oauth2/token")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error":"authorization_pending","error_description":"Still pending"}"#)
        .expect(1)
        .create_async()
        .await;
    let mock_success = server
        .mock("POST", "/oauth2/token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(AUTHZ_TOKEN_BODY)
        .expect(1)
        .create_async()
        .await;

    let client = test_client_with_secret(&server.url());
    let token = client
        .wait_for_authorization("device_code", 5, 1)
        .await
        .expect("authorized");

    mock_pending.assert_async().await;
    mock_success.assert_async().await;
    assert_eq!(token.access_token, "some_token");
    assert_eq!(client.get_user_id(), Some(8675309));
}

#[tokio::test]
async fn test_wait_for_authorization_denied() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/oauth2/token")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error":"access_denied","error_description":"User denied the request"}"#)
        .create_async()
        .await;

    let client = test_client_with_secret(&server.url());
    let err = client
        .wait_for_authorization("device_code", 5, 1)
        .await
        .expect_err("denied");

    mock.assert_async().await;
    match err {
        Error::DeviceAuthorizationDenied(msg) => {
            assert!(msg.contains("access_denied"));
        }
        other => panic!("expected DeviceAuthorizationDenied, got {other:?}"),
    }
}

#[tokio::test]
async fn test_wait_for_authorization_timeout() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/oauth2/token")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error":"authorization_pending","error_description":"Still pending"}"#)
        .expect(1)
        .create_async()
        .await;

    let client = test_client_with_secret(&server.url());
    let err = client
        .wait_for_authorization("device_code", 1, 1)
        .await
        .expect_err("timeout");

    mock.assert_async().await;
    assert!(matches!(err, Error::DeviceAuthorizationTimeout));
}

#[tokio::test]
async fn test_finish_pkce_exchanges_code() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/oauth2/token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(AUTHZ_TOKEN_BODY)
        .create_async()
        .await;

    let client = test_client(&server.url());
    let url = client
        .start_pkce("https://tidal.com/android/login/auth")
        .expect("start");
    assert!(url.contains("code_challenge"));

    let token = client
        .finish_pkce("https://tidal.com/android/login/auth?code=abc123")
        .await
        .expect("finish");

    mock.assert_async().await;
    assert_eq!(token.access_token, "some_token");
    assert_eq!(client.get_user_id(), Some(8675309));
}

#[tokio::test]
async fn test_client_credentials_stores_app_session() {
    let mut server = mockito::Server::new_async().await;
    let mock = server
        .mock("POST", "/oauth2/token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"access_token":"app_token","expires_in":3600}"#)
        .create_async()
        .await;

    let client = test_client_with_secret(&server.url());
    let authz = client.client_credentials().await.expect("client creds");

    mock.assert_async().await;
    assert_eq!(authz.access_token, "app_token");
    assert_eq!(authz.refresh_token, None);
    assert_eq!(authz.user_id, None);
    assert!(authz.expires_timestamp.is_some());
    assert_eq!(client.get_user_id(), None);
}

#[tokio::test]
async fn test_client_credentials_requires_secret() {
    let client = TidalClient::new("client_id".to_string());
    let err = client
        .client_credentials()
        .await
        .expect_err("secret required");
    assert!(matches!(err, Error::ClientSecretRequired));
}

#[test]
fn test_oauth_error_display() {
    let err = Error::OAuth {
        error: "authorization_pending".into(),
        description: "still waiting".into(),
    };
    let msg = err.to_string();
    assert!(msg.contains("authorization_pending"));
    assert!(msg.contains("still waiting"));
}
