//! Integration tests for the OAuth2 device authorization flow.
//!
//! These tests use `mockito` to stand up a local HTTP server in place of
//! `https://auth.tidal.com/v1`, exercising `do_request`'s error-discrimination
//! logic end-to-end without real network calls.

use tidalrs::{Error, TidalClient};

/// Build a test client pointed at a mockito server instead of TIDAL's auth endpoint.
fn test_client(server_url: &str) -> TidalClient {
    TidalClient::new("test_client_id".to_string())
        .with_auth_base_url(server_url.to_string())
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

// ─── YOUR CONTRIBUTION ───────────────────────────────────────────────────────
//
// Implement `test_polling_loop_succeeds_after_pending`.
//
// This test verifies that a caller who retries `authorize()` after receiving
// `AuthorizationPending` eventually gets `Ok(AuthzToken)` on success.
//
// Set up two mock responses on the same endpoint (mockito supports sequenced
// responses with `.with_body()` chained or separate `.mock()` calls with
// `expect(1)`):
//   1. First call  → 400 {"error":"authorization_pending","error_description":"..."}
//   2. Second call → 200 with a valid AuthzToken JSON body
//
// A minimal valid AuthzToken body (fill in the fields `authorize()` needs):
//
//   {
//     "access_token": "test_access",
//     "client_name": "test",
//     "expires_in": 3600,
//     "refresh_token": "test_refresh",
//     "scope": "r_usr w_usr w_sub",
//     "token_type": "Bearer",
//     "user": {
//       "accepted_eula": true, "account_link_created": false,
//       "channel_id": 1, "country_code": "US", "created": 0,
//       "email": "test@example.com", "email_verified": true,
//       "new_user": false, "parent_id": 0, "updated": 0,
//       "user_id": 12345, "username": "testuser"
//     },
//     "user_id": 12345
//   }
//
// Then write a small loop:
//   loop {
//     match client.authorize("device_code", "client_secret").await {
//       Ok(token)                        => { assert_eq!(...); break; }
//       Err(Error::AuthorizationPending) => { /* retry */ }
//       Err(e)                           => panic!("unexpected: {e}"),
//     }
//   }
//
// ─────────────────────────────────────────────────────────────────────────────
