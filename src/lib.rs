#![doc = include_str!("../README.md")]

mod album;
mod artist;
pub mod auth;
mod playlist;
mod search;
mod track;

pub use album::*;
pub use artist::*;
pub use auth::{OAuthErrorBody, PkcePending};
pub use playlist::*;
pub use search::*;
pub use track::*;

use arc_swap::ArcSwapOption;
use async_recursion::async_recursion;
use log::trace;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fmt::Display;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use strum_macros::{AsRefStr, EnumString};
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::time::sleep;

pub(crate) static TIDAL_AUTH_API_BASE_URL: &str = "https://auth.tidal.com/v1";
pub(crate) static TIDAL_API_BASE_URL: &str = "https://api.tidal.com/v1";
pub(crate) const TIDAL_USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 12; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/91.0.4472.114 Safari/537.36";
const INITIAL_BACKOFF_MILLIS: u64 = 100;
const DEFAULT_MAX_BACKOFF_MILLIS: u64 = 5_000;
const MAX_TOKEN_REFRESH_ATTEMPTS: u8 = 10;

/// Response from the device authorization endpoint containing the information
/// needed for the user to complete the OAuth2 device flow.
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # let client = tidalrs::TidalClient::new("client_id".to_string());
/// let device_auth = client.device_authorization().await?;
/// println!("Visit: {}", device_auth.url);
/// println!("Enter code: {}", device_auth.user_code);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthorizationResponse {
    /// The URL the user should visit to authorize the application
    #[serde(rename = "verificationUriComplete")]
    pub url: String,
    /// The device code used to complete the authorization flow
    pub device_code: String,
    /// How long the device code remains valid (in seconds)
    pub expires_in: u64,
    /// The code the user enters on the authorization page
    pub user_code: String,
    /// Suggested polling interval in seconds (RFC 8628)
    #[serde(default = "default_device_interval")]
    pub interval: u64,
}

fn default_device_interval() -> u64 {
    crate::auth::DEFAULT_DEVICE_INTERVAL_SECS
}

/// Represents a Tidal user account with all associated profile information.
///
/// This structure contains user data returned during authentication
/// and can be used to identify the authenticated user.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct User {
    /// Whether the user has accepted the End User License Agreement
    #[serde(rename = "acceptedEULA")]
    pub accepted_eula: bool,
    /// Whether an account link has been created
    pub account_link_created: bool,
    /// User's address (if provided)
    pub address: Option<String>,
    /// Apple ID associated with the account (if any)
    pub apple_uid: Option<String>,
    /// Channel ID associated with the user
    pub channel_id: u64,
    /// User's city (if provided)
    pub city: Option<String>,
    /// User's country code (e.g., "US", "GB")
    pub country_code: String,
    /// Unix timestamp when the account was created
    pub created: u64,
    /// User's email address
    pub email: String,
    /// Whether the email address has been verified
    pub email_verified: bool,
    /// Facebook UID associated with the account (if any)
    pub facebook_uid: Option<u64>,
    /// User's first name (if provided)
    pub first_name: Option<String>,
    /// User's full name (if provided)
    pub full_name: Option<String>,
    /// Google UID associated with the account
    pub google_uid: Option<String>,
    /// User's last name (if provided)
    pub last_name: Option<String>,
    /// Whether this is a new user account
    pub new_user: bool,
    /// User's nickname (if provided)
    pub nickname: Option<String>,
    /// Parent ID associated with the user
    pub parent_id: u64,
    /// User's phone number (if provided)
    pub phone_number: Option<String>,
    /// User's postal code (if provided)
    pub postalcode: Option<String>,
    /// Unix timestamp when the account was last updated
    pub updated: u64,
    /// User's US state (if provided and in US)
    pub us_state: Option<String>,
    /// Unique user ID
    pub user_id: u64,
    /// User's username
    pub username: String,
}

/// Complete authorization token response from Tidal's OAuth2 endpoint.
///
/// This contains all the tokens and user information needed to authenticate
/// API requests and manage the user session.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthzToken {
    /// Access token for API authentication
    #[serde(rename = "access_token")]
    pub access_token: String,
    /// Name of the client application
    pub client_name: String,
    /// Token expiration time in seconds
    #[serde(rename = "expires_in")]
    pub expires_in: i64,
    /// Refresh token for obtaining new access tokens
    #[serde(rename = "refresh_token")]
    pub refresh_token: Option<String>,
    /// OAuth2 scope granted to the application
    pub scope: String,
    /// Type of token (typically "Bearer")
    #[serde(rename = "token_type")]
    pub token_type: String,
    /// User information
    pub user: User,
    /// User ID (same as user.user_id but as i64)
    #[serde(rename = "user_id")]
    pub user_id: i64,
}

impl AuthzToken {
    /// Calculate the Unix timestamp when this token expires.
    pub fn expires_timestamp(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        now.as_secs() + self.expires_in as u64
    }

    pub fn authz(&self) -> Authz {
        Authz {
            access_token: self.access_token.clone(),
            refresh_token: self.refresh_token.clone(),
            user_id: Some(self.user_id as u64),
            country_code: Some(self.user.country_code.clone()),
            expires_timestamp: Some(self.expires_timestamp()),
        }
    }
}

/// Error response from the Tidal API.
///
/// This represents errors returned by Tidal's API endpoints and includes
/// both HTTP status codes and Tidal-specific error information.
#[derive(Debug, Serialize, Clone)]
pub struct TidalApiError {
    /// HTTP status code
    pub status: u16,
    /// Tidal-specific sub-status code
    pub sub_status: u64,
    /// Human-readable error message
    pub user_message: String,
}

impl<'de> Deserialize<'de> for TidalApiError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // First deserialize to a generic Value
        let value: serde_json::Value = serde_json::Value::deserialize(deserializer)?;

        // Extract status (should be consistent)
        // TODO: Apparently this *isn't* consistent, so we need to handle it better
        let status = value
            .get("status")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| serde::de::Error::custom("Missing or invalid 'status' field"))?
            as u16;

        // Extract sub_status - try both snake_case and camelCase
        let sub_status = value
            .get("sub_status")
            .or_else(|| value.get("subStatus"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| {
                serde::de::Error::custom("Missing or invalid 'sub_status'/'subStatus' field")
            })?;

        // Extract user_message - try both snake_case and camelCase, default to empty string
        let user_message = value
            .get("user_message")
            .or_else(|| value.get("userMessage"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(TidalApiError {
            status,
            sub_status,
            user_message,
        })
    }
}

impl Display for TidalApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Tidal API error: {} {} {}",
            self.status, self.sub_status, self.user_message
        )
    }
}

impl TidalApiError {
    /// Whether this error indicates the access token expired and should be refreshed.
    pub fn is_expired_token(&self) -> bool {
        self.sub_status == 11003
    }
}

/// Errors that can occur when using the TidalRS library.
///
/// This enum covers all possible error conditions including network issues,
/// API errors, authentication problems, and streaming issues.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// HTTP request failed (network issues, timeouts, etc.)
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    /// Tidal API returned an error response
    #[error("Tidal API error: {0}")]
    TidalApiError(TidalApiError),
    /// No authorization token available for refresh
    #[error("No authz token available to refresh client authorization")]
    NoAuthzToken,
    /// JSON serialization/deserialization failed
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    /// No primary streaming URL available for the track
    #[error("No primary streaming URL available")]
    NoPrimaryUrl,
    /// Failed to initialize audio stream
    #[error("Stream initialization error: {0}")]
    StreamInitializationError(String),
    /// No access token available - client needs authentication
    #[error("No access token available - have you authorized the client?")]
    NoAccessTokenAvailable,
    /// Requested audio quality not available for this track
    #[error("Track at this playback quality not available, try a lower quality")]
    TrackQualityNotAvailable,
    /// User authentication required for this operation
    #[error("User authentication required - please login first")]
    UserAuthenticationRequired,
    /// Track not found in the specified playlist
    #[error("Track {1} not found on playlist {0}")]
    PlaylistTrackNotFound(String, u64),
    /// Exponential backoff exceeded the maximum duration while handling rate limits
    #[error("Hit rate limit backoff ceiling of {0}ms without recovery")]
    RateLimitBackoffExceeded(u64),
    /// Client secret is required for this authorization flow
    #[error("Client secret is required for this authorization flow")]
    ClientSecretRequired,
    /// PKCE login has not been started; call `start_pkce` first
    #[error("PKCE login has not been started; call start_pkce first")]
    PkceNotStarted,
    /// Authorization code was not found in the PKCE redirect URL
    #[error("Authorization code not found in PKCE redirect URL")]
    PkceRedirectMissingCode,
    /// Device authorization timed out before the user completed login
    #[error("Device authorization timed out before the user completed login")]
    DeviceAuthorizationTimeout,
    /// OAuth device authorization was rejected or failed
    #[error("OAuth device authorization failed: {0}")]
    DeviceAuthorizationDenied(String),
    /// The OAuth token endpoint returned an error response
    #[error("OAuth error {error}: {description}")]
    OAuth { error: String, description: String },
    /// URL parsing failed
    #[error("Invalid URL: {0}")]
    Url(#[from] url::ParseError),
}

/// Callback function type for handling authorization token refresh events.
///
/// This callback is invoked whenever the client automatically refreshes
/// the access token. Use this to persist updated tokens to storage.
pub type AuthzCallback = Arc<dyn Fn(Authz) + Send + Sync>;

/// Main client for interacting with the Tidal API.
///
/// The `TidalClient` provides an interface for accessing Tidal's
/// music catalog, managing user data, and streaming audio content. It handles
/// authentication, automatic token refresh, and provides type-safe methods
/// for all API operations.
///
/// # Example
///
/// ```no_run
/// use tidalrs::TidalClient;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a new client
/// let client = TidalClient::new("your_client_id".to_string())
///     .with_client_secret("your_client_secret");
///
/// // Authenticate using device flow
/// let device_auth = client.device_authorization().await?;
/// println!("Visit: {}", device_auth.url);
///
/// // Complete authentication
/// let authz_token = client
///     .wait_for_authorization(
///         &device_auth.device_code,
///         device_auth.expires_in,
///         device_auth.interval,
///     )
///     .await?;
///
/// // Now use the authenticated client
/// let track = client.track(123456789).await?;
/// println!("Playing: {}", track.title);
/// # Ok(())
/// # }
/// ```
///
/// # Thread Safety
///
/// `TidalClient` is designed to be used across multiple threads safely.
/// All methods are async and the client uses internal synchronization
/// for token management.
pub struct TidalClient {
    pub client: reqwest::Client,
    pub(crate) client_id: String,
    pub(crate) client_secret: Option<String>,
    pub(crate) auth_base_url: String,
    pub(crate) authz: ArcSwapOption<Authz>,
    pub(crate) authz_update_semaphore: Semaphore,
    pub(crate) pkce_pending: ArcSwapOption<crate::auth::PkcePending>,
    pub(crate) country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<DeviceType>,
    on_authz_refresh_callback: Option<AuthzCallback>,
    backoff: Mutex<Option<u64>>,
    max_backoff_millis: Option<u64>,
}

/// Authorization tokens and user information for API access.
///
/// This structure contains the authentication data needed to make
/// authenticated requests to the Tidal API. It can be serialized and stored
/// persistently to avoid re-authentication.
///
/// # Example
///
/// ```no_run
/// use tidalrs::{Authz, TidalClient};
///
/// // Create Authz from stored tokens
/// let authz = Authz::new(
///     "access_token".to_string(),
///     Some("refresh_token".to_string()),
///     Some(12345),
///     Some("US".to_string()),
///     None,
/// );
///
/// // Create client with existing authentication
/// let client = TidalClient::new("client_id".to_string())
///     .with_authz(authz);
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Authz {
    /// Access token for API authentication
    pub access_token: String,
    /// Refresh token for obtaining new access tokens.
    ///
    /// Absent for client-credentials and access-token-only sessions.
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// User ID associated with these tokens.
    ///
    /// Absent for application tokens that are not bound to a user.
    #[serde(default)]
    pub user_id: Option<u64>,
    /// User's country code (affects content availability)
    pub country_code: Option<String>,
    /// Unix timestamp when the access token expires, if known
    pub expires_timestamp: Option<u64>,
}

impl Authz {
    pub fn new(
        access_token: String,
        refresh_token: Option<String>,
        user_id: Option<u64>,
        country_code: Option<String>,
        expires_timestamp: Option<u64>,
    ) -> Self {
        Self {
            access_token,
            refresh_token,
            user_id,
            country_code,
            expires_timestamp,
        }
    }

    pub(crate) fn token_renewal(&self, has_client_secret: bool) -> TokenRenewal {
        if let Some(refresh_token) = &self.refresh_token
            && !refresh_token.is_empty()
        {
            return TokenRenewal::RefreshToken(refresh_token.clone());
        }
        if has_client_secret {
            return TokenRenewal::ClientCredentials;
        }
        TokenRenewal::None
    }
}

/// How an expired access token can be renewed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TokenRenewal {
    RefreshToken(String),
    ClientCredentials,
    None,
}

impl TidalClient {
    /// Create a new TidalClient with the given client ID.
    ///
    /// # Arguments
    ///
    /// * `client_id` - Your Tidal API client ID
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// let client = TidalClient::new("your_client_id".to_string());
    /// ```
    pub fn new(client_id: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            client_id,
            client_secret: None,
            auth_base_url: TIDAL_AUTH_API_BASE_URL.to_string(),
            authz: ArcSwapOption::from(None),
            authz_update_semaphore: Semaphore::new(1),
            pkce_pending: ArcSwapOption::from(None),
            country_code: None,
            locale: None,
            device_type: None,
            on_authz_refresh_callback: None,
            backoff: Mutex::new(None),
            max_backoff_millis: None,
        }
    }

    /// Override the authentication base URL.
    ///
    /// The default is `https://auth.tidal.com/v1`. Override this to point at a
    /// Affects `device_authorization()`, `authorize()`, `wait_for_authorization()`,
    /// PKCE token exchange, client credentials, and token refresh.
    ///
    /// # Example
    ///
    /// ```
    /// use tidalrs::TidalClient;
    ///
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_auth_base_url("http://localhost:1234".to_string());
    /// ```
    pub fn with_auth_base_url(mut self, url: String) -> Self {
        self.auth_base_url = url;
        self
    }

    /// Set a custom HTTP client using the builder pattern.
    ///
    /// This is useful when you need to configure the HTTP client with custom
    /// settings like timeouts, proxies, or custom headers.
    ///
    /// # Arguments
    ///
    /// * `client` - Custom reqwest HTTP client
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// let custom_client = reqwest::Client::builder()
    ///     .timeout(std::time::Duration::from_secs(30))
    ///     .build()
    ///     .unwrap();
    ///
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_client(custom_client);
    /// ```
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    /// Set existing authentication tokens using the builder pattern.
    ///
    /// This is useful when you have previously stored authentication tokens
    /// and want to avoid re-authentication. The client will use these tokens
    /// for API requests and automatically refresh them when needed.
    ///
    /// # Arguments
    ///
    /// * `authz` - Existing authorization tokens
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::{TidalClient, Authz};
    ///
    /// let authz = Authz::new(
    ///     "access_token".to_string(),
    ///     Some("refresh_token".to_string()),
    ///     Some(12345),
    ///     Some("US".to_string()),
    ///     None,
    /// );
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_authz(authz);
    /// ```
    pub fn with_authz(mut self, authz: Authz) -> Self {
        self.authz = ArcSwapOption::from_pointee(authz);
        self
    }

    /// Set the OAuth client secret used by device-code polling, PKCE refresh,
    /// and the client-credentials grant.
    pub fn with_client_secret(mut self, client_secret: impl Into<String>) -> Self {
        self.client_secret = Some(client_secret.into());
        self
    }

    /// Authenticate with a pre-existing access token and no refresh token.
    ///
    /// Use [`TidalClient::with_authz`] when you also have a refresh token to persist.
    /// The resulting session has no user ID, so user-specific methods return
    /// [`Error::UserAuthenticationRequired`].
    pub fn with_access_token(self, access_token: impl Into<String>) -> Self {
        self.with_authz(Authz {
            access_token: access_token.into(),
            refresh_token: None,
            user_id: None,
            country_code: None,
            expires_timestamp: None,
        })
    }

    /// Set the locale for API requests using the builder pattern.
    ///
    /// This affects the language of returned content and metadata. The locale
    /// should be in the format "language_COUNTRY" (e.g., "en_US", "en_GB", "de_DE").
    ///
    /// # Arguments
    ///
    /// * `locale` - The locale string (e.g., "en_US", "fr_FR", "de_DE")
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_locale("en_GB".to_string());
    /// ```
    pub fn with_locale(mut self, locale: String) -> Self {
        self.locale = Some(locale);
        self
    }

    /// Set the device type for API requests using the builder pattern.
    ///
    /// This affects the user agent and may influence content availability
    /// and API behavior. Different device types may have different access
    /// to certain features or content.
    ///
    /// By default, the device type is set to `DeviceType::Browser`.
    ///
    /// # Arguments
    ///
    /// * `device_type` - The device type to use for API requests
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::{TidalClient, DeviceType};
    ///
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_device_type(DeviceType::Browser);
    /// ```
    pub fn with_device_type(mut self, device_type: DeviceType) -> Self {
        self.device_type = Some(device_type);
        self
    }

    /// Set the country code for API requests using the builder pattern.
    ///
    /// This affects content availability and regional restrictions. The country
    /// code should be a two-letter ISO country code (e.g., "US", "GB", "DE").
    /// This setting takes priority over the country code from authentication.
    ///
    /// # Arguments
    ///
    /// * `country_code` - Two-letter ISO country code (e.g., "US", "GB", "DE")
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_country_code("GB".to_string());
    /// ```
    pub fn with_country_code(mut self, country_code: String) -> Self {
        self.country_code = Some(country_code);
        self
    }

    /// Set a callback function for authorization token refresh using the builder pattern.
    ///
    /// This callback is invoked whenever the client automatically refreshes
    /// the access token. Use this to persist updated tokens to storage when
    /// they are automatically refreshed by the client.
    ///
    /// # Arguments
    ///
    /// * `authz_refresh_callback` - Callback function that receives the new `Authz` when tokens are refreshed
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_authz_refresh_callback(|new_authz| {
    ///         println!("Tokens refreshed for user: {:?}", new_authz.user_id);
    ///         // Save tokens to persistent storage
    ///         todo!();
    ///     });
    /// ```
    pub fn with_authz_refresh_callback<F>(mut self, authz_refresh_callback: F) -> Self
    where
        F: Fn(Authz) + Send + Sync + 'static,
    {
        self.on_authz_refresh_callback = Some(Arc::new(authz_refresh_callback));
        self
    }

    /// Set the maximum backoff time in milliseconds for rate limit retries using the builder pattern.
    ///
    /// When the client encounters a 429 (Too Many Requests) or 500 (Internal Server Error) response,
    /// it will retry the request with exponential backoff. This setting controls the maximum
    /// backoff time before giving up.
    ///
    /// Setting this to `0` disables backoff retries entirely - the client will immediately
    /// return errors for 429 and 500 responses without retrying.
    ///
    /// The default value is 5000ms (5 seconds).
    ///
    /// # Arguments
    ///
    /// * `max_backoff_millis` - Maximum backoff time in milliseconds, or `0` to disable retries
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// // Disable backoff retries
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_max_backoff_millis(0);
    ///
    /// // Set custom max backoff to 10 seconds
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_max_backoff_millis(10_000);
    /// ```
    pub fn with_max_backoff_millis(mut self, max_backoff_millis: u64) -> Self {
        self.max_backoff_millis = Some(max_backoff_millis);
        self
    }

    /// Get the current country code for API requests.
    ///
    /// Returns the explicitly set country code, or falls back to the user's
    /// country from their authentication, or "US" as a final fallback.
    pub fn get_country_code(&self) -> String {
        match &self.country_code {
            Some(country_code) => country_code.clone(),
            None => match &self.get_authz() {
                Some(authz) => authz.country_code.clone().unwrap_or_else(|| "US".into()),
                None => "US".into(),
            },
        }
    }

    /// Get the current locale for API requests.
    ///
    /// Returns the explicitly set locale or "en_US" as default.
    pub fn get_locale(&self) -> String {
        self.locale.clone().unwrap_or_else(|| "en_US".into())
    }

    /// Get the current device type for API requests.
    ///
    /// Returns the explicitly set device type or `DeviceType::Browser` as default.
    pub fn get_device_type(&self) -> DeviceType {
        self.device_type.unwrap_or(DeviceType::Browser)
    }

    /// Get the current user ID if a user-authenticated session is present.
    ///
    /// Returns `None` if the client is not authenticated or the current session
    /// is an application token without a user.
    pub fn get_user_id(&self) -> Option<u64> {
        self.get_authz().and_then(|authz| authz.user_id)
    }

    /// Set the country code for API requests.
    ///
    /// This affects content availability and regional restrictions.
    pub fn set_country_code(&mut self, country_code: String) {
        self.country_code = Some(country_code);
    }

    /// Set the locale for API requests.
    ///
    /// This affects the language of returned content and metadata.
    pub fn set_locale(&mut self, locale: String) {
        self.locale = Some(locale);
    }

    /// Set the device type for API requests.
    ///
    /// This may affect content availability and API behavior.
    pub fn set_device_type(&mut self, device_type: DeviceType) {
        self.device_type = Some(device_type);
    }

    /// Set the maximum backoff time in milliseconds for rate limit retries.
    ///
    /// When the client encounters a 429 (Too Many Requests) or 500 (Internal Server Error) response,
    /// it will retry the request with exponential backoff. This setting controls the maximum
    /// backoff time before giving up.
    ///
    /// Setting this to `0` disables backoff retries entirely - the client will immediately
    /// return errors for 429 and 500 responses without retrying.
    ///
    /// The default value is 5000ms (5 seconds).
    ///
    /// # Arguments
    ///
    /// * `max_backoff_millis` - Maximum backoff time in milliseconds, or `0` to disable retries
    pub fn set_max_backoff_millis(&mut self, max_backoff_millis: u64) {
        self.max_backoff_millis = Some(max_backoff_millis);
    }

    /// Get the maximum backoff time in milliseconds for rate limit retries.
    ///
    /// Returns the configured value or the default (5000ms).
    pub fn get_max_backoff_millis(&self) -> u64 {
        self.max_backoff_millis
            .unwrap_or(DEFAULT_MAX_BACKOFF_MILLIS)
    }

    /// Set a callback function to be called when authorization tokens are refreshed.
    ///
    /// This is useful for persisting updated tokens to storage when they are
    /// automatically refreshed by the client.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// # fn example() {
    /// let mut client = TidalClient::new("client_id".to_string());
    /// client.on_authz_refresh(|new_authz| {
    ///     println!("Tokens refreshed for user: {:?}", new_authz.user_id);
    ///     // Save tokens to persistent storage.
    /// });
    /// # }
    /// ```
    pub fn on_authz_refresh<F>(&mut self, f: F)
    where
        F: Fn(Authz) + Send + Sync + 'static,
    {
        self.on_authz_refresh_callback = Some(Arc::new(f));
    }

    /// Get the current authorization tokens.
    ///
    /// Returns `None` if the client is not authenticated. This is useful for
    /// persisting tokens when shutting down the client.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = TidalClient::new("client_id".to_string());
    /// if let Some(authz) = client.get_authz() {
    ///     // Save tokens for next session
    ///     println!("User ID: {:?}", authz.user_id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_authz(&self) -> Option<Arc<Authz>> {
        self.authz.load_full()
    }

    fn can_renew_token(&self) -> bool {
        self.get_authz().is_some_and(|authz| {
            authz.token_renewal(self.client_secret.is_some()) != TokenRenewal::None
        })
    }

    async fn refresh_authz(&self) -> Result<(), Error> {
        // Try to become the single refresher
        let permit: Option<SemaphorePermit> = self.authz_update_semaphore.try_acquire().ok();

        match permit {
            // We're the single refresher, fetch the new authz and update the client
            Some(permit) => {
                let authz = self.get_authz().ok_or(Error::NoAuthzToken)?;

                let new_authz = match authz.token_renewal(self.client_secret.is_some()) {
                    TokenRenewal::RefreshToken(refresh_token) => {
                        let basic_auth = self
                            .client_secret
                            .as_deref()
                            .map(|secret| (self.client_id.as_str(), secret));

                        let resp: AuthzToken = self
                            .post_token(
                                &[
                                    ("client_id", self.client_id.as_str()),
                                    ("refresh_token", refresh_token.as_str()),
                                    ("grant_type", "refresh_token"),
                                    ("scope", "r_usr w_usr"),
                                ],
                                basic_auth,
                            )
                            .await?;

                        let expires_timestamp = Some(resp.expires_timestamp());
                        let new_authz = Authz {
                            access_token: resp.access_token,
                            refresh_token: resp
                                .refresh_token
                                .or_else(|| authz.refresh_token.clone()),
                            user_id: Some(resp.user.user_id),
                            country_code: match &authz.country_code {
                                Some(country_code) => Some(country_code.clone()),
                                None => Some(resp.user.country_code.clone()),
                            },
                            expires_timestamp,
                        };
                        self.authz.store(Some(Arc::new(new_authz.clone())));
                        new_authz
                    }
                    TokenRenewal::ClientCredentials => self.client_credentials().await?,
                    TokenRenewal::None => return Err(Error::NoAuthzToken),
                };

                trace!("TIDAL API Token refreshed");

                drop(permit);

                if let Some(cb) = &self.on_authz_refresh_callback {
                    cb(new_authz);
                }

                Ok(())
            }
            None => {
                // Someone else is refreshing—await completion cooperatively
                // Acquire then drop to wait for the in-flight refresh to finish.
                let _ = self.authz_update_semaphore.acquire().await;
                Ok(())
            }
        }
    }

    // Do a GET or DELETE request to the given URL.
    pub(crate) async fn do_request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        url: &str,
        params: Option<serde_json::Value>,
        etag: Option<&str>,
    ) -> Result<T, Error> {
        self.do_request_with_refresh_attempts(method, url, params, etag, 0)
            .await
    }

    #[async_recursion]
    async fn do_request_with_refresh_attempts<T>(
        &self,
        method: reqwest::Method,
        url: &str,
        params: Option<serde_json::Value>,
        etag: Option<&str>,
        token_refresh_attempts: u8,
    ) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        self.await_rate_limit_backoff().await;

        let mut req = match method {
            reqwest::Method::GET => self.client.get(url),
            reqwest::Method::DELETE => self.client.delete(url),
            reqwest::Method::POST => self.client.post(url),
            _ => panic!("Invalid method: {}", method),
        };

        if let Some(etag) = etag {
            req = req.header(reqwest::header::IF_NONE_MATCH, etag);
        }

        if let Some(authz) = self.get_authz() {
            req = req.header(
                reqwest::header::AUTHORIZATION,
                &format!("Bearer {}", authz.access_token),
            );
        }

        req = req.header(reqwest::header::USER_AGENT, TIDAL_USER_AGENT);

        if let Some(params) = params.as_ref() {
            match method {
                reqwest::Method::POST => req = req.form(params),
                reqwest::Method::GET => req = req.query(params),
                reqwest::Method::DELETE => req = req.query(params),
                _ => panic!("Invalid method for params: {}", method),
            }
        }

        let resp = req.send().await?;

        let etag: Option<String> = resp.headers().get("ETag").map(|etag| {
            let etag = etag.to_str().expect("Invalid ETag header").to_string();

            match serde_json::from_str::<String>(&etag) {
                Ok(etag) => etag,
                Err(_) => etag,
            }
        });

        let status = resp.status();
        let body = resp.bytes().await?;

        // Parse it into a value
        let mut value: serde_json::Value = if body.is_empty() {
            serde_json::Value::Null
        } else {
            match serde_json::from_slice(&body) {
                Ok(value) => value,
                Err(e) => {
                    let error_message = String::from_utf8_lossy(&body);
                    if log::log_enabled!(log::Level::Warn) {
                        log::warn!("Requested URL: {}", url);
                        log::warn!("JSON deserialization error: {}", e);
                        log::warn!("Response: {}", error_message);
                    }
                    return Err(Error::TidalApiError(TidalApiError {
                        status: status.as_u16(),
                        sub_status: 0,
                        user_message: error_message.to_string(),
                    }));
                }
            }
        };

        log::trace!(
            "Response from TIDAL: {}",
            serde_json::to_string_pretty(&value).unwrap()
        );

        if status.is_success() {
            self.reset_rate_limit_backoff();

            // If we have an etag, add it to the response, if the value doesn't already exist
            if let Some(etag) = etag
                && value.get("etag").is_none()
            {
                value["etag"] = serde_json::Value::String(etag);
            }

            let resp: T = match serde_json::from_value(value.clone()) {
                Ok(t) => t,
                Err(e) => {
                    if log::log_enabled!(log::Level::Warn) {
                        let problem_value_pretty = serde_json::to_string_pretty(&value).unwrap();
                        log::warn!("Requested URL: {}", url);
                        log::warn!("JSON deserialization error: {}", e);
                        log::warn!("Response: {}", problem_value_pretty);
                    }
                    return Err(Error::TidalApiError(TidalApiError {
                        status: status.as_u16(),
                        sub_status: 0,
                        user_message: e.to_string(),
                    }));
                }
            };

            Ok(resp)
        } else {
            if status.as_u16() == 429 || status.as_u16() == 500 {
                // Skip retry if backoff is disabled (max_backoff_millis == 0)
                if self.get_max_backoff_millis() == 0 {
                    self.reset_rate_limit_backoff();
                } else {
                    // Increase backoff and retry
                    // The backoff wait will happen at the start of do_request
                    self.increase_rate_limit_backoff()?;
                    return self
                        .do_request_with_refresh_attempts(
                            method,
                            url,
                            params,
                            etag.as_deref(),
                            token_refresh_attempts,
                        )
                        .await;
                }
            } else {
                self.reset_rate_limit_backoff();
            }

            let tidal_err = match serde_json::from_value::<TidalApiError>(value.clone()) {
                Ok(e) => e,
                Err(e) => {
                    if log::log_enabled!(log::Level::Warn) {
                        let problem_value_pretty = serde_json::to_string_pretty(&value).unwrap();
                        log::warn!("Requested URL: {}", url);
                        log::warn!("JSON deserialization error of TidalApiError: {}", e);
                        log::warn!("Response: {}", problem_value_pretty);
                    }
                    return Err(Error::TidalApiError(TidalApiError {
                        status: status.as_u16(),
                        sub_status: 0,
                        user_message: e.to_string(),
                    }));
                }
            };

            if status.as_u16() == 401
                && tidal_err.is_expired_token()
                && self.can_renew_token()
                && token_refresh_attempts < MAX_TOKEN_REFRESH_ATTEMPTS
            {
                trace!("TIDAL API Token expired, refreshing");
                self.refresh_authz().await?;
                return self
                    .do_request_with_refresh_attempts(
                        method,
                        url,
                        params,
                        etag.as_deref(),
                        token_refresh_attempts + 1,
                    )
                    .await;
            }

            if log::log_enabled!(log::Level::Warn) {
                let pretty_err = serde_json::to_string_pretty(&tidal_err).unwrap();
                log::warn!("Requested URL: {}", url);
                log::warn!("TIDAL API Error: {}", pretty_err);
            }

            Err(Error::TidalApiError(tidal_err))
        }
    }

    async fn await_rate_limit_backoff(&self) {
        // Skip backoff if disabled
        if self.get_max_backoff_millis() == 0 {
            return;
        }

        let delay = {
            let guard = self
                .backoff
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *guard
        };

        if let Some(ms) = delay
            && ms > 0
        {
            sleep(Duration::from_millis(ms)).await;
        }
    }

    fn increase_rate_limit_backoff(&self) -> Result<(), Error> {
        let max_backoff = self.get_max_backoff_millis();

        // Skip if backoff is disabled
        if max_backoff == 0 {
            return Ok(());
        }

        let mut guard = self
            .backoff
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let next = match *guard {
            Some(current) => current.saturating_mul(2),
            None => INITIAL_BACKOFF_MILLIS,
        };

        if next >= max_backoff {
            *guard = Some(max_backoff);
            return Err(Error::RateLimitBackoffExceeded(max_backoff));
        }

        *guard = Some(next);
        Ok(())
    }

    fn reset_rate_limit_backoff(&self) {
        let mut guard = self
            .backoff
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_some() {
            *guard = None;
        }
    }

    /// Start the OAuth2 device authorization flow.
    ///
    /// This initiates the device flow authentication process. The user must
    /// visit the returned URL and enter the user code to complete authentication.
    ///
    /// # Returns
    ///
    /// A `DeviceAuthorizationResponse` containing the URL to visit and the
    /// user code to enter.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = TidalClient::new("client_id".to_string());
    /// let device_auth = client.device_authorization().await?;
    /// println!("Visit: {}", device_auth.url);
    /// println!("Enter code: {}", device_auth.user_code);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn device_authorization(&self) -> Result<DeviceAuthorizationResponse, Error> {
        let mut resp: DeviceAuthorizationResponse = self
            .post_auth_form(
                crate::auth::DEVICE_AUTHORIZATION_PATH,
                &[
                    ("client_id", self.client_id.as_str()),
                    ("scope", "r_usr w_usr"),
                ],
                None,
            )
            .await?;

        // TIDAL returns verificationUriComplete without a scheme.
        resp.url = format!("https://{url}", url = resp.url);

        Ok(resp)
    }

    /// Complete the OAuth2 device authorization flow.
    ///
    /// Call this method after the user has visited the authorization URL and
    /// entered the user code. This completes the authentication process and
    /// stores the tokens in the client.
    ///
    /// # Arguments
    ///
    /// * `device_code` - The device code from `device_authorization()`
    /// * `client_secret` - Your Tidal API client secret
    ///
    /// # Returns
    ///
    /// An `AuthzToken` containing all user and token information.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = TidalClient::new("client_id".to_string());
    /// let device_code = "device_code";
    /// let client_secret = "client_secret";
    /// let authz_token = client.authorize(device_code, client_secret).await?;
    /// println!("Authenticated as: {}", authz_token.user.username);
    ///
    /// // Get the authz token to store in persistent storage
    /// let authz = authz_token.authz();
    /// std::fs::write("authz.json", serde_json::to_string(&authz).unwrap()).unwrap();
    /// # Ok(())
    /// # }
    /// ```
    pub async fn authorize(
        &self,
        device_code: &str,
        client_secret: &str,
    ) -> Result<AuthzToken, Error> {
        let resp: AuthzToken = self
            .post_token(
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

        self.store_user_authz(&resp);
        Ok(resp)
    }
}

/// Device type for API requests.
///
/// This affects the user agent and may influence content availability
/// and API behavior.
#[derive(
    Debug, Serialize, Deserialize, Default, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DeviceType {
    /// Browser-based client
    #[default]
    Browser,
}

/// Audio quality levels available for streaming.
///
/// Higher quality levels may require a Tidal HiFi subscription.
/// The actual quality available depends on the user's subscription
/// and the track's availability.
///
/// # Example
///
/// ```no_run
/// use tidalrs::{AudioQuality, TidalClient};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # let client = TidalClient::new("client_id".to_string());
/// let track_id = 123456789;
/// let stream = client.track_stream(track_id, AudioQuality::Lossless).await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AudioQuality {
    /// Low quality (typically 96 kbps AAC)
    Low,
    /// High quality (typically 320 kbps AAC)
    High,
    /// Lossless quality (FLAC, typically 44.1 kHz / 16-bit)
    Lossless,
    /// Hi-Res Lossless quality (FLAC, up to 192 kHz / 24-bit)
    HiResLossless,
}

/// Sort order for listing operations.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Order {
    /// Sort by date
    Date,
}

/// Direction for sorting operations.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderDirection {
    /// Ascending order
    Asc,
    /// Descending order
    Desc,
}

/// Media metadata associated with tracks and albums.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadata {
    /// Tags associated with the media
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Types of resources available in the Tidal API.
///
/// Used for search filtering and resource identification.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ResourceType {
    /// Artist resource
    Artist,
    /// Album resource
    Album,
    /// Track resource
    Track,
    /// Video resource
    Video,
    /// Playlist resource
    Playlist,
    /// User profile resource
    UserProfile,
}

impl ResourceType {
    pub fn as_str(&self) -> &str {
        match self {
            ResourceType::Artist => "ARTIST",
            ResourceType::Album => "ALBUM",
            ResourceType::Track => "TRACK",
            ResourceType::Video => "VIDEO",
            ResourceType::Playlist => "PLAYLIST",
            ResourceType::UserProfile => "USER_PROFILE",
        }
    }
}

impl std::str::FromStr for ResourceType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ARTIST" => Ok(ResourceType::Artist),
            "ARTISTS" => Ok(ResourceType::Artist),
            "ALBUM" => Ok(ResourceType::Album),
            "ALBUMS" => Ok(ResourceType::Album),
            "TRACK" => Ok(ResourceType::Track),
            "TRACKS" => Ok(ResourceType::Track),
            "VIDEO" => Ok(ResourceType::Video),
            "VIDEOS" => Ok(ResourceType::Video),
            "PLAYLIST" => Ok(ResourceType::Playlist),
            "PLAYLISTS" => Ok(ResourceType::Playlist),
            "USER_PROFILE" => Ok(ResourceType::UserProfile),
            "USER_PROFILES" => Ok(ResourceType::UserProfile),
            _ => Err(()),
        }
    }
}

impl From<String> for ResourceType {
    fn from(s: String) -> Self {
        s.parse().unwrap()
    }
}

impl From<&str> for ResourceType {
    fn from(s: &str) -> Self {
        s.parse().unwrap()
    }
}

/// A unified resource type that can represent any Tidal content.
///
/// This enum allows handling different types of resources in a type-safe way,
/// commonly used in search results and mixed content lists.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Resource {
    /// Artist resource
    Artists(Artist),
    /// Album resource
    Albums(Album),
    /// Track resource
    Tracks(Track),
    /// Playlist resource
    Playlists(Playlist),

    // TODO: Add proper support for videos and user profiles
    /// Video resource (currently as raw JSON)
    Videos(serde_json::Value),
    /// User profile resource (currently as raw JSON)
    UserProfiles(serde_json::Value),
}

impl Resource {
    pub fn id(&self) -> String {
        match self {
            Resource::Artists(artist) => artist.id.to_string(),
            Resource::Albums(album) => album.id.to_string(),
            Resource::Tracks(track) => track.id.to_string(),
            Resource::Playlists(playlist) => playlist.uuid.to_string(),
            Resource::Videos(video) => video
                .get("id")
                .unwrap_or(&serde_json::Value::Null)
                .to_string(),
            Resource::UserProfiles(user_profile) => user_profile
                .get("id")
                .unwrap_or(&serde_json::Value::Null)
                .to_string(),
        }
    }
}

/// A paginated list response from the Tidal API.
///
/// This generic structure is used for all paginated endpoints and provides
/// information about the current page and total available items.
///
/// # Example
///
/// ```no_run
/// use tidalrs::{TidalClient, List};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// # let client = TidalClient::new("client_id".to_string());
/// let tracks: List<tidalrs::Track> = client.album_tracks(12345, Some(0), Some(50)).await?;
///
/// println!("Showing {} of {} tracks", tracks.items.len(), tracks.total);
/// for track in tracks.items {
///     println!("Track: {}", track.title);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct List<T> {
    /// Items in the current page
    pub items: Vec<T>,
    /// Offset of the current page
    pub offset: usize,
    /// Maximum number of items per page
    pub limit: usize,
    /// Total number of items available
    #[serde(rename = "totalNumberOfItems")]
    pub total: usize,

    /// ETag for optimistic concurrency control (used in playlist modifications)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub etag: Option<String>,
}

impl<T> List<T> {
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    // The number of items left to fetch
    pub fn num_left(&self) -> usize {
        let current_batch_size = self.items.len();
        self.total - self.offset - current_batch_size
    }
}

impl<T> Default for List<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            offset: 0,
            limit: 0,
            total: 0,
            etag: None,
        }
    }
}

// Utility function to deserialize a null value as a default value
pub(crate) fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + serde::Deserialize<'de>,
{
    Option::deserialize(deserializer).map(|opt| opt.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn authz_without_refresh_token_cannot_renew() {
        let authz = Authz::new("tok".to_string(), None, None, None, None);
        assert_eq!(authz.token_renewal(false), TokenRenewal::None);
        assert_eq!(authz.token_renewal(true), TokenRenewal::ClientCredentials);
    }

    #[test]
    fn authz_with_refresh_token_uses_refresh() {
        let authz = Authz::new(
            "tok".to_string(),
            Some("rt".to_string()),
            Some(1),
            None,
            None,
        );
        assert!(matches!(
            authz.token_renewal(true),
            TokenRenewal::RefreshToken(token) if token == "rt"
        ));
        assert!(matches!(
            authz.token_renewal(false),
            TokenRenewal::RefreshToken(_)
        ));
    }

    #[test]
    fn empty_refresh_token_falls_back_to_client_credentials() {
        let authz = Authz::new("tok".to_string(), Some(String::new()), None, None, None);
        assert_eq!(authz.token_renewal(false), TokenRenewal::None);
        assert_eq!(authz.token_renewal(true), TokenRenewal::ClientCredentials);
    }

    #[test]
    fn with_access_token_stores_token_only_authz() {
        let client = TidalClient::new("client-id".to_string()).with_access_token("abc");
        let authz = client.get_authz().expect("authz");
        assert_eq!(authz.access_token, "abc");
        assert_eq!(authz.refresh_token, None);
        assert_eq!(authz.user_id, None);
        assert!(!client.can_renew_token());
    }

    #[test]
    fn with_client_secret_enables_client_credentials_renewal() {
        let client = TidalClient::new("client-id".to_string())
            .with_client_secret("secret")
            .with_access_token("abc");
        assert!(client.can_renew_token());
        assert_eq!(
            client
                .get_authz()
                .unwrap()
                .token_renewal(client.client_secret.is_some()),
            TokenRenewal::ClientCredentials
        );
    }

    #[test]
    fn tidal_api_error_detects_expired_token() {
        let err = TidalApiError {
            status: 401,
            sub_status: 11003,
            user_message: "Token expired".into(),
        };
        assert!(err.is_expired_token());
        let other = TidalApiError {
            status: 401,
            sub_status: 2001,
            user_message: "Unauthorized".into(),
        };
        assert!(!other.is_expired_token());
    }

    #[test]
    fn get_user_id_is_none_for_app_tokens() {
        let client = TidalClient::new("client-id".to_string()).with_access_token("abc");
        assert_eq!(client.get_user_id(), None);

        let client = TidalClient::new("client-id".to_string()).with_authz(Authz::new(
            "tok".to_string(),
            Some("rt".to_string()),
            Some(42),
            None,
            None,
        ));
        assert_eq!(client.get_user_id(), Some(42));
    }

    const REFRESH_TOKEN_BODY: &str = r#"{
        "access_token": "new_access",
        "clientName": "some_client",
        "expires_in": 100,
        "scope": "r_usr w_usr",
        "token_type": "Bearer",
        "user": {
            "acceptedEULA": true,
            "accountLinkCreated": false,
            "channelId": 1,
            "countryCode": "US",
            "created": 0,
            "email": "null@example.com",
            "emailVerified": true,
            "newUser": false,
            "parentId": 0,
            "updated": 0,
            "userId": 42,
            "username": "someuser"
        },
        "user_id": 42
    }"#;

    #[tokio::test]
    async fn expired_token_refreshes_and_retains_refresh_token() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let mut server = mockito::Server::new_async().await;
        let expired = server
            .mock("GET", "/catalog")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":401,"subStatus":11003,"userMessage":"Expired token"}"#)
            .expect(1)
            .create_async()
            .await;
        let refresh = server
            .mock("POST", "/oauth2/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(REFRESH_TOKEN_BODY)
            .expect(1)
            .create_async()
            .await;
        let ok = server
            .mock("GET", "/catalog")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#""ok""#)
            .expect(1)
            .create_async()
            .await;

        let callbacks = Arc::new(AtomicUsize::new(0));
        let callbacks_clone = callbacks.clone();
        let client = TidalClient::new("client-id".to_string())
            .with_auth_base_url(server.url())
            .with_authz(Authz::new(
                "old_access".to_string(),
                Some("keep_me".to_string()),
                Some(42),
                Some("US".to_string()),
                None,
            ))
            .with_authz_refresh_callback(move |authz| {
                callbacks_clone.fetch_add(1, Ordering::SeqCst);
                assert_eq!(authz.access_token, "new_access");
                assert_eq!(authz.refresh_token.as_deref(), Some("keep_me"));
                assert_eq!(authz.user_id, Some(42));
            });

        let url = format!("{}/catalog", server.url());
        let body: String = client
            .do_request(reqwest::Method::GET, &url, None, None)
            .await
            .expect("retry after refresh");

        expired.assert_async().await;
        refresh.assert_async().await;
        ok.assert_async().await;
        assert_eq!(body, "ok");
        assert_eq!(callbacks.load(Ordering::SeqCst), 1);
        let stored = client.get_authz().expect("authz");
        assert_eq!(stored.access_token, "new_access");
        assert_eq!(stored.refresh_token.as_deref(), Some("keep_me"));
    }

    #[tokio::test]
    async fn expired_token_refresh_is_bounded() {
        let mut server = mockito::Server::new_async().await;
        let expired = server
            .mock("GET", "/catalog")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":401,"subStatus":11003,"userMessage":"Expired token"}"#)
            .expect(11)
            .create_async()
            .await;
        let refresh = server
            .mock("POST", "/oauth2/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(REFRESH_TOKEN_BODY)
            .expect(10)
            .create_async()
            .await;

        let client = TidalClient::new("client-id".to_string())
            .with_auth_base_url(server.url())
            .with_authz(Authz::new(
                "old_access".to_string(),
                Some("keep_me".to_string()),
                Some(42),
                None,
                None,
            ));

        let url = format!("{}/catalog", server.url());
        let err = client
            .do_request::<String>(reqwest::Method::GET, &url, None, None)
            .await
            .expect_err("refresh attempts exhausted");

        expired.assert_async().await;
        refresh.assert_async().await;
        match err {
            Error::TidalApiError(tidal_err) => {
                assert!(tidal_err.is_expired_token());
            }
            other => panic!("expected expired TidalApiError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn access_token_only_session_does_not_refresh() {
        let mut server = mockito::Server::new_async().await;
        let expired = server
            .mock("GET", "/catalog")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":401,"subStatus":11003,"userMessage":"Expired token"}"#)
            .expect(1)
            .create_async()
            .await;

        let client = TidalClient::new("client-id".to_string())
            .with_auth_base_url(server.url())
            .with_access_token("abc");

        let url = format!("{}/catalog", server.url());
        let err = client
            .do_request::<String>(reqwest::Method::GET, &url, None, None)
            .await
            .expect_err("cannot renew");

        expired.assert_async().await;
        assert!(matches!(err, Error::TidalApiError(_)));
    }

    #[tokio::test]
    async fn client_credentials_session_renews_on_expiry() {
        let mut server = mockito::Server::new_async().await;
        let expired = server
            .mock("GET", "/catalog")
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":401,"subStatus":11003,"userMessage":"Expired token"}"#)
            .expect(1)
            .create_async()
            .await;
        let token = server
            .mock("POST", "/oauth2/token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"access_token":"app_token","expires_in":3600}"#)
            .expect(1)
            .create_async()
            .await;
        let ok = server
            .mock("GET", "/catalog")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#""ok""#)
            .expect(1)
            .create_async()
            .await;

        let client = TidalClient::new("client-id".to_string())
            .with_auth_base_url(server.url())
            .with_client_secret("secret")
            .with_access_token("old");

        let url = format!("{}/catalog", server.url());
        let body: String = client
            .do_request(reqwest::Method::GET, &url, None, None)
            .await
            .expect("renewed");

        expired.assert_async().await;
        token.assert_async().await;
        ok.assert_async().await;
        assert_eq!(body, "ok");
        let stored = client.get_authz().expect("authz");
        assert_eq!(stored.access_token, "app_token");
        assert_eq!(stored.user_id, None);
    }
}
