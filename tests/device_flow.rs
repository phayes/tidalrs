//! Integration tests for the OAuth2 device authorization flow.
//!
//! These tests use `mockito` to stand up a local HTTP server in place of
//! `https://auth.tidal.com/v1`, exercising `do_request`'s error-discrimination
//! logic end-to-end without real network calls.

use std::time::Duration;

use tidalrs::{Error, TidalClient};

/// Build a test client pointed at a mockito server instead of TIDAL's auth endpoint.
fn test_client(server_url: &str) -> TidalClient {
    TidalClient::new("test_client_id".to_string()).with_auth_base_url(server_url.to_string())
}

// ─── AuthorizationPending ────────────────────────────────────────────────────

/// TIDAL returns `{"error": "authorization_pending", ...}` (RFC 8628 §3.5)
/// while the user hasn't yet completed browser auth. This should produce
/// `Error::AuthorizationPending`, not a generic `TidalApiError { status: 400 }`.
#[tokio::test]
async fn test_authorize_returns_authorization_pending() {
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
    assert!(
        matches!(result, Err(Error::AuthorizationPending)),
        "expected AuthorizationPending, got: {:?}",
        result
    );
}

/// Ensure `AuthorizationPending` is distinct from a generic TIDAL API error
/// that also returns 400 (e.g. invalid client credentials).
#[tokio::test]
async fn test_other_400_is_not_authorization_pending() {
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
        "expected TidalApiError for a non-pending 400, got: {:?}",
        result
    );
    assert!(
        !matches!(result, Err(Error::AuthorizationPending)),
        "a non-pending 400 must not be AuthorizationPending"
    );
}

/// Verify the error message is human-readable and contains meaningful text.
#[test]
fn test_authorization_pending_display() {
    let err = Error::AuthorizationPending;
    let msg = err.to_string();
    assert!(
        msg.contains("pending") || msg.contains("Authorization"),
        "error message should describe the pending state, got: {msg}"
    );
}

#[tokio::test]
async fn test_polling_loop_succeeds_after_pending() {
    // AuthzToken uses explicit rename for some fields, camelCase for others.
    // Fields with #[serde(rename = "...")] keep their literal name;
    // the rest follow rename_all = "camelCase" on the struct.
    let authz_token_body = r#"{
        "access_token": "some_token",
        "clientName": "some_client",
        "expires_in": 4444,
        "refresh_token": "some_refresh",
        "scope": "r_usr w_usr w_sub",
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

    let mut server = mockito::Server::new_async().await;

    let mock_pending = server
        .mock("POST", "/oauth2/token")
        .with_status(400)
        .with_header("content-type", "application/json")
        .with_body(r#"{"error":"authorization_pending","error_description":"The authorization request is still pending"}"#)
        .expect(1)
        .create_async()
        .await;

    let mock_success = server
        .mock("POST", "/oauth2/token")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(authz_token_body)
        .expect(1)
        .create_async()
        .await;

    let client = test_client(&server.url());
    loop {
        match client.authorize("device_code", "client_secret").await {
            Ok(token) => {
                assert_eq!(token.access_token, "some_token");
                assert_eq!(token.expires_in, 4444);
                assert_eq!(token.user.user_id, 8675309);
                assert_eq!(token.user.country_code, "CA");
                break;
            }
            Err(Error::AuthorizationPending) => {
                tokio::time::sleep(Duration::from_millis(1)).await;
                continue;
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    mock_pending.assert_async().await;
    mock_success.assert_async().await;
}
