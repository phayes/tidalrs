//! OAuth helpers: PKCE crypto, authorize-URL construction, and token-endpoint POSTs.
//!
//! Token requests intentionally bypass [`crate::TidalClient::do_request`] so they
//! never attach a Bearer token or trigger expired-token refresh.

use crate::{Authz, AuthzToken, Error, TIDAL_USER_AGENT, TidalApiError, TidalClient};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use log::{debug, info, trace};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;

/// TIDAL browser authorize endpoint used by the PKCE flow.
pub const PKCE_AUTH_URL: &str = "https://login.tidal.com/authorize";

/// Default polling interval (seconds) when the device-authorization response omits one.
pub const DEFAULT_DEVICE_INTERVAL_SECS: u64 = 5;

/// RFC 8628 increment applied to the polling interval after a `slow_down` response.
const SLOW_DOWN_INCREMENT_SECS: u64 = 5;

/// Path of the OAuth token endpoint, relative to the auth server base URL.
const TOKEN_PATH: &str = "/oauth2/token";

/// Path of the OAuth device authorization endpoint, relative to the auth server base URL.
pub(crate) const DEVICE_AUTHORIZATION_PATH: &str = "/oauth2/device_authorization";

/// In-flight PKCE state stored on the client between [`TidalClient::start_pkce`]
/// and [`TidalClient::finish_pkce`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkcePending {
    pub code_verifier: String,
    pub code_challenge: String,
    pub client_unique_key: String,
    pub redirect_uri: String,
}

/// Access-token response from the client-credentials grant.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ClientCredentialsToken {
    pub access_token: String,
    pub expires_in: u64,
}

/// OAuth error body returned by the token endpoint (RFC 6749 §5.2, RFC 8628 §3.5).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OAuthErrorBody {
    pub error: String,
    #[serde(default)]
    pub error_description: String,
    pub sub_status: Option<u64>,
}

/// Generate a new PKCE verifier, S256 challenge, and client unique key.
pub fn generate_pkce_pending(redirect_uri: impl Into<String>) -> PkcePending {
    let bits: u64 = rand::random();
    let client_unique_key = format!("{:02x}", bits);

    let mut random_bytes = [0u8; 32];
    rand::fill(&mut random_bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(random_bytes);

    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let code_challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    PkcePending {
        code_verifier,
        code_challenge,
        client_unique_key,
        redirect_uri: redirect_uri.into(),
    }
}

/// Build the TIDAL authorize URL for a pending PKCE session.
pub fn pkce_authorization_url(client_id: &str, pending: &PkcePending) -> Result<String, Error> {
    let mut url = url::Url::parse(PKCE_AUTH_URL)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &pending.redirect_uri)
        .append_pair("client_id", client_id)
        .append_pair("lang", "EN")
        .append_pair("appMode", "android")
        .append_pair("client_unique_key", &pending.client_unique_key)
        .append_pair("code_challenge", &pending.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("restrict_signup", "true");
    Ok(url.to_string())
}

/// Extract the authorization `code` query parameter from a PKCE redirect URL.
pub fn parse_pkce_redirect(redirect_url: &str) -> Result<String, Error> {
    let url = url::Url::parse(redirect_url)?;
    url.query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.into_owned())
        .ok_or(Error::PkceRedirectMissingCode)
}

/// Parse an RFC 8628 OAuth error object from a token-endpoint body.
pub fn parse_oauth_error(body: &[u8]) -> Option<OAuthErrorBody> {
    serde_json::from_slice(body).ok()
}

/// Whether a token-endpoint body is the RFC 8628 `authorization_pending` keep-polling signal.
pub fn is_authorization_pending(body: &[u8]) -> bool {
    parse_oauth_error(body).is_some_and(|err| err.error == "authorization_pending")
}

/// Whether a token-endpoint body is the RFC 8628 `slow_down` signal.
pub fn is_slow_down(body: &[u8]) -> bool {
    parse_oauth_error(body).is_some_and(|err| err.error == "slow_down")
}

/// Convert a failed token-endpoint response into an [`Error`].
///
/// OAuth error bodies are checked first. Device-flow pending responses include
/// both `error` and `sub_status`; without this ordering they would deserialize
/// as a TIDAL API error and lose the RFC 8628 `error` field.
fn token_error(status: StatusCode, body: &[u8]) -> Error {
    if let Some(err) = parse_oauth_error(body) {
        return Error::OAuth {
            error: err.error,
            description: err.error_description,
        };
    }
    if let Ok(tidal_err) = serde_json::from_slice::<TidalApiError>(body) {
        return Error::TidalApiError(tidal_err);
    }
    Error::TidalApiError(TidalApiError {
        status: status.as_u16(),
        sub_status: 0,
        user_message: String::from_utf8_lossy(body).into_owned(),
    })
}

impl TidalClient {
    /// POST a form to an auth-server endpoint and deserialize a successful response.
    pub(crate) async fn post_auth_form<T: DeserializeOwned>(
        &self,
        path: &str,
        form: &[(&str, &str)],
        basic_auth: Option<(&str, &str)>,
    ) -> Result<T, Error> {
        let (status, body) = self.post_auth_form_raw(path, form, basic_auth).await?;
        if status.is_success() {
            Ok(serde_json::from_slice(&body)?)
        } else {
            Err(token_error(status, &body))
        }
    }

    /// POST a form to an auth-server endpoint and return the raw status and body.
    ///
    /// No Bearer token is attached and no expired-token refresh is attempted, so
    /// authorization flows never depend on the credentials they are establishing.
    pub(crate) async fn post_auth_form_raw(
        &self,
        path: &str,
        form: &[(&str, &str)],
        basic_auth: Option<(&str, &str)>,
    ) -> Result<(StatusCode, Vec<u8>), Error> {
        let url = format!("{}{}", self.auth_base_url, path);
        let mut req = self
            .client
            .post(&url)
            .header(reqwest::header::USER_AGENT, TIDAL_USER_AGENT)
            .form(form);

        if let Some((id, secret)) = basic_auth {
            req = req.basic_auth(id, Some(secret));
        }

        debug!("TIDAL auth request POST {}", url);
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.bytes().await?;
        debug!("TIDAL auth response status {}", status);
        Ok((status, body.to_vec()))
    }

    /// POST to the OAuth token endpoint and deserialize a successful response.
    pub(crate) async fn post_token<T: DeserializeOwned>(
        &self,
        form: &[(&str, &str)],
        basic_auth: Option<(&str, &str)>,
    ) -> Result<T, Error> {
        self.post_auth_form(TOKEN_PATH, form, basic_auth).await
    }

    /// POST to the OAuth token endpoint and return the raw status and body.
    pub(crate) async fn post_token_raw(
        &self,
        form: &[(&str, &str)],
        basic_auth: Option<(&str, &str)>,
    ) -> Result<(StatusCode, Vec<u8>), Error> {
        self.post_auth_form_raw(TOKEN_PATH, form, basic_auth).await
    }

    pub(crate) fn store_user_authz(&self, resp: &AuthzToken) -> Authz {
        let authz = Authz {
            access_token: resp.access_token.clone(),
            refresh_token: resp.refresh_token.clone(),
            user_id: Some(resp.user.user_id),
            country_code: self
                .country_code
                .clone()
                .or_else(|| Some(resp.user.country_code.clone())),
            expires_timestamp: Some(resp.expires_timestamp()),
        };
        self.authz.store(Some(Arc::new(authz.clone())));
        authz
    }

    /// Start the OAuth2 PKCE flow and return the URL the user should visit.
    ///
    /// Generates a code verifier/challenge pair and stores them on the client
    /// until [`TidalClient::finish_pkce`] is called. Use a HiRes-capable
    /// `client_id` (and matching `redirect_uri`) if you need HiRes streaming;
    /// official developer-portal clients may not unlock HiRes.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = TidalClient::new("hires_client_id".to_string())
    ///     .with_client_secret("hires_client_secret");
    /// let url = client.start_pkce("https://tidal.com/android/login/auth")?;
    /// println!("Visit: {}", url);
    /// # Ok(())
    /// # }
    /// ```
    pub fn start_pkce(&self, redirect_uri: &str) -> Result<String, Error> {
        let pending = generate_pkce_pending(redirect_uri);
        let url = pkce_authorization_url(&self.client_id, &pending)?;
        self.pkce_pending.store(Some(Arc::new(pending)));
        Ok(url)
    }

    /// Complete the PKCE flow by exchanging the authorization code from a redirect URL.
    ///
    /// Pass the full redirect URL the browser landed on (including `?code=`).
    pub async fn finish_pkce(&self, redirect_url: &str) -> Result<AuthzToken, Error> {
        let pending = self.pkce_pending.load_full().ok_or(Error::PkceNotStarted)?;
        let code = parse_pkce_redirect(redirect_url)?;

        let resp: AuthzToken = self
            .post_token(
                &[
                    ("code", code.as_str()),
                    ("client_id", self.client_id.as_str()),
                    ("grant_type", "authorization_code"),
                    ("redirect_uri", pending.redirect_uri.as_str()),
                    ("scope", "r_usr+w_usr"),
                    ("code_verifier", pending.code_verifier.as_str()),
                    ("client_unique_key", pending.client_unique_key.as_str()),
                ],
                None,
            )
            .await?;

        self.pkce_pending.store(None);
        self.store_user_authz(&resp);
        info!("PKCE authorization completed");
        Ok(resp)
    }

    /// Obtain an application access token via the OAuth2 client-credentials grant.
    ///
    /// Requires [`TidalClient::with_client_secret`]. The resulting session has no
    /// user and no refresh token; on expiry the client re-runs this grant when a
    /// secret is still configured. User-specific methods still require a
    /// user-authenticated session.
    pub async fn client_credentials(&self) -> Result<Authz, Error> {
        let secret = self
            .client_secret
            .as_deref()
            .ok_or(Error::ClientSecretRequired)?;

        let resp: ClientCredentialsToken = self
            .post_token(
                &[("grant_type", "client_credentials")],
                Some((self.client_id.as_str(), secret)),
            )
            .await?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let authz = Authz {
            access_token: resp.access_token,
            refresh_token: None,
            user_id: None,
            country_code: self.country_code.clone(),
            expires_timestamp: Some(now + resp.expires_in),
        };
        self.authz.store(Some(Arc::new(authz.clone())));
        trace!("Client-credentials token stored");
        Ok(authz)
    }

    /// Poll the token endpoint until the user completes device authorization.
    ///
    /// Requires [`TidalClient::with_client_secret`]. HTTP 400 with
    /// `authorization_pending` continues polling; `slow_down` increases the
    /// interval; other RFC 8628 errors and timeout are fatal.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_client_secret("client_secret");
    /// let device_auth = client.device_authorization().await?;
    /// println!("Visit: {}", device_auth.url);
    /// let token = client
    ///     .wait_for_authorization(
    ///         &device_auth.device_code,
    ///         device_auth.expires_in,
    ///         device_auth.interval,
    ///     )
    ///     .await?;
    /// println!("Authenticated as: {}", token.user.username);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wait_for_authorization(
        &self,
        device_code: &str,
        expires_in: u64,
        interval: u64,
    ) -> Result<AuthzToken, Error> {
        let client_secret = self
            .client_secret
            .as_deref()
            .ok_or(Error::ClientSecretRequired)?;

        let mut remaining = expires_in;
        let mut sleep_secs = interval.max(1);

        while remaining > 0 {
            let (status, body) = self
                .post_token_raw(
                    &[
                        ("client_id", self.client_id.as_str()),
                        ("client_secret", client_secret),
                        ("device_code", device_code),
                        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                        ("scope", "r_usr w_usr"),
                    ],
                    None,
                )
                .await?;

            if status.is_success() {
                let resp: AuthzToken = serde_json::from_slice(&body)?;
                self.store_user_authz(&resp);
                info!("Device authorization completed");
                return Ok(resp);
            }

            if status.as_u16() == 400 {
                if is_authorization_pending(&body) {
                    debug!(
                        "device authorization still pending ({}s remaining)",
                        remaining
                    );
                    sleep(Duration::from_secs(sleep_secs)).await;
                    remaining = remaining.saturating_sub(sleep_secs);
                    continue;
                }
                if is_slow_down(&body) {
                    sleep_secs = sleep_secs.saturating_add(SLOW_DOWN_INCREMENT_SECS);
                    debug!(
                        "device authorization requested slow_down (sleep {}s, {}s remaining)",
                        sleep_secs, remaining
                    );
                    sleep(Duration::from_secs(sleep_secs)).await;
                    remaining = remaining.saturating_sub(sleep_secs);
                    continue;
                }
                if let Some(err) = parse_oauth_error(&body) {
                    return Err(Error::DeviceAuthorizationDenied(format!(
                        "{}: {}",
                        err.error, err.error_description
                    )));
                }
            }

            return Err(token_error(status, &body));
        }

        Err(Error::DeviceAuthorizationTimeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        let pending = generate_pkce_pending("https://tidal.com/android/login/auth");
        let mut hasher = Sha256::new();
        hasher.update(pending.code_verifier.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(pending.code_challenge, expected);
        assert_eq!(pending.redirect_uri, "https://tidal.com/android/login/auth");
        assert!(!pending.client_unique_key.is_empty());
    }

    #[test]
    fn pkce_authorization_url_includes_tidal_params() {
        let pending = generate_pkce_pending("https://tidal.com/android/login/auth");
        let url = pkce_authorization_url("client-id", &pending).expect("url");
        assert!(url.starts_with("https://login.tidal.com/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=client-id"));
        assert!(url.contains("appMode=android"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("restrict_signup=true"));
        assert!(url.contains(&pending.code_challenge));
    }

    #[test]
    fn parse_pkce_redirect_extracts_code() {
        let code = parse_pkce_redirect("https://tidal.com/android/login/auth?code=abc123&foo=bar")
            .expect("code");
        assert_eq!(code, "abc123");
    }

    #[test]
    fn parse_pkce_redirect_missing_code() {
        let err = parse_pkce_redirect("https://tidal.com/android/login/auth?error=access_denied")
            .expect_err("missing code");
        assert!(matches!(err, Error::PkceRedirectMissingCode));
    }

    #[test]
    fn authorization_pending_json_is_recognized() {
        let body = br#"{"status":400,"error":"authorization_pending","sub_status":0,"error_description":"Still pending"}"#;
        assert!(is_authorization_pending(body));
        assert!(!is_slow_down(body));
        let parsed = parse_oauth_error(body).expect("oauth error");
        assert_eq!(parsed.error, "authorization_pending");
    }

    #[test]
    fn token_error_surfaces_oauth_error_detail() {
        let body =
            br#"{"status":401,"error":"invalid_client","error_description":"Bad client secret"}"#;
        match token_error(StatusCode::UNAUTHORIZED, body) {
            Error::OAuth { error, description } => {
                assert_eq!(error, "invalid_client");
                assert_eq!(description, "Bad client secret");
            }
            other => panic!("expected OAuth error, got {other:?}"),
        }
    }

    #[test]
    fn token_error_keeps_tidal_api_error_detail() {
        let body = br#"{"status":401,"subStatus":11003,"userMessage":"Expired token"}"#;
        match token_error(StatusCode::UNAUTHORIZED, body) {
            Error::TidalApiError(err) => {
                assert_eq!(err.status, 401);
                assert_eq!(err.sub_status, 11003);
                assert_eq!(err.user_message, "Expired token");
            }
            other => panic!("expected TIDAL API error, got {other:?}"),
        }
    }

    #[test]
    fn token_error_prefers_oauth_over_tidal_when_sub_status_present() {
        let body = br#"{"status":400,"error":"authorization_pending","sub_status":1002,"error_description":"Still pending"}"#;
        match token_error(StatusCode::BAD_REQUEST, body) {
            Error::OAuth { error, description } => {
                assert_eq!(error, "authorization_pending");
                assert_eq!(description, "Still pending");
            }
            other => panic!("expected OAuth error, got {other:?}"),
        }
    }

    #[test]
    fn token_error_falls_back_to_raw_body() {
        match token_error(StatusCode::BAD_GATEWAY, b"upstream exploded") {
            Error::TidalApiError(err) => {
                assert_eq!(err.status, 502);
                assert_eq!(err.user_message, "upstream exploded");
            }
            other => panic!("expected unknown error, got {other:?}"),
        }
    }

    #[test]
    fn slow_down_json_is_recognized() {
        let body = br#"{"error":"slow_down","error_description":"Slow down"}"#;
        assert!(is_slow_down(body));
        assert!(!is_authorization_pending(body));
    }

    #[test]
    fn start_pkce_stores_pending_and_builds_url() {
        let client = TidalClient::new("client-id".to_string());
        let url = client
            .start_pkce("https://tidal.com/android/login/auth")
            .expect("start");
        assert!(url.starts_with("https://login.tidal.com/authorize?"));
        assert!(url.contains("client_id=client-id"));
        assert!(client.pkce_pending.load_full().is_some());
    }

    #[tokio::test]
    async fn finish_pkce_requires_start() {
        let client = TidalClient::new("client-id".to_string());
        let err = client
            .finish_pkce("https://tidal.com/android/login/auth?code=abc")
            .await
            .expect_err("not started");
        assert!(matches!(err, Error::PkceNotStarted));
    }
}
