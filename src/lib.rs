#![doc = include_str!("../README.md")]

mod album;
mod artist;
mod playlist;
mod search;
mod track;

pub use album::*;
pub use artist::*;
pub use playlist::*;
pub use search::*;
pub use track::*;

use arc_swap::ArcSwapOption;
use async_recursion::async_recursion;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::fmt::Display;
use std::sync::Arc;
use strum_macros::{AsRefStr, EnumString};
use tokio::sync::{Semaphore, SemaphorePermit};

pub(crate) static TIDAL_AUTH_API_BASE_URL: &str = "https://auth.tidal.com/v1";
pub(crate) static TIDAL_API_BASE_URL: &str = "https://api.tidal.com/v1";

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
    /// User's birthday (if provided)
    pub birthday: Option<String>,
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
    pub google_uid: String,
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
    pub fn authz(&self) -> Option<Authz> {
        if let Some(refresh_token) = self.refresh_token.clone() {
            Some(Authz {
                access_token: self.access_token.clone(),
                refresh_token: refresh_token,
                user_id: self.user_id as u64,
                country_code: Some(self.user.country_code.clone()),
            })
        } else {
            None
        }
    }
}

/// Error response from the Tidal API.
///
/// This represents errors returned by Tidal's API endpoints and includes
/// both HTTP status codes and Tidal-specific error information.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TidalApiError {
    /// HTTP status code
   pub status: u16,
    /// Tidal-specific sub-status code
    #[serde(rename = "sub_status")]
    pub sub_status: u64,
    /// Human-readable error message
    #[serde(rename = "userMessage")]
    #[serde(default)]
    pub user_message: String,
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
/// use tidalrs::{TidalClient, Authz};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a new client
/// let mut client = TidalClient::new("your_client_id".to_string());
///
/// // Authenticate using device flow
/// let device_auth = client.device_authorization().await?;
/// println!("Visit: {}", device_auth.url);
///
/// // Complete authentication
/// let authz_token = client.authorize(&device_auth.device_code, "client_secret").await?;
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
    client_id: String,
    authz: ArcSwapOption<Authz>,
    authz_update_semaphore: Semaphore,
    country_code: Option<String>,
    locale: Option<String>,
    device_type: Option<DeviceType>,
    on_authz_refresh_callback: Option<AuthzCallback>,
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
///     "refresh_token".to_string(),
///     12345,
///     Some("US".to_string()),
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
    /// Refresh token for obtaining new access tokens
    pub refresh_token: String,
    /// User ID associated with these tokens
    pub user_id: u64,
    /// User's country code (affects content availability)
    pub country_code: Option<String>,
}

impl Authz {
    pub fn new(
        access_token: String,
        refresh_token: String,
        user_id: u64,
        country_code: Option<String>,
    ) -> Self {
        Self {
            access_token,
            refresh_token,
            user_id,
            country_code,
        }
    }
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
            authz: ArcSwapOption::from(None),
            authz_update_semaphore: Semaphore::new(1),
            country_code: None,
            locale: None,
            device_type: None,
            on_authz_refresh_callback: None,
        }
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
    ///     "refresh_token".to_string(),
    ///     12345,
    ///     Some("US".to_string()),
    /// );
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_authz(authz);
    /// ```
    pub fn with_authz(mut self, authz: Authz) -> Self {
        self.authz = ArcSwapOption::from_pointee(authz);
        self
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
    /// use std::sync::Arc;
    ///
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_authz_refresh_callback(|new_authz| {
    ///         println!("Tokens refreshed for user: {}", new_authz.user_id);
    ///         // Save tokens to persistent storage
    ///         todo!();
    ///     });
    /// ```
    pub fn with_authz_refresh_callback<F>(mut self, authz_refresh_callback: F) -> Self
    where
        F: Fn(Authz) + Send + Sync + 'static,
    {
        self.on_authz_refresh_callback = Some(Arc::new( authz_refresh_callback));
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
        self.device_type.unwrap_or_else(|| DeviceType::Browser)
    }

    /// Get the current user ID if authenticated.
    ///
    /// Returns `None` if the client is not authenticated.
    pub fn get_user_id(&self) -> Option<u64> {
        self.get_authz().map(|authz| authz.user_id)
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

    /// Set a callback function to be called when authorization tokens are refreshed.
    ///
    /// This is useful for persisting updated tokens to storage when they are
    /// automatically refreshed by the client.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::TidalClient;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = TidalClient::new("client_id".to_string())
    ///     .with_authz_refresh_callback(Arc::new(|new_authz| {
    ///         println!("Tokens refreshed for user: {}", new_authz.user_id);
    ///         // Save tokens to persistent storage
    ///     }));
    /// # Ok(())
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
    ///     println!("User ID: {}", authz.user_id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_authz(&self) -> Option<Arc<Authz>> {
        self.authz.load_full()
    }

    #[async_recursion]
    async fn refresh_authz(&self) -> Result<(), Error> {
        // Try to become the single refresher
        let permit: Option<SemaphorePermit> = match self.authz_update_semaphore.try_acquire() {
            Ok(p) => Some(p),
            Err(_) => None,
        };

        match permit {
            // We're the single refresher, fetch the new authz and update the client
            Some(permit) => {
                let url = format!("{TIDAL_AUTH_API_BASE_URL}/oauth2/token");

                let authz = self.get_authz().ok_or(Error::NoAuthzToken)?;

                let params = serde_json::json!({
                    "client_id": &self.client_id,
                    "refresh_token": authz.refresh_token,
                    "grant_type": "refresh_token",
                    "scope": "r_usr w_usr w_sub",
                });

                let resp: AuthzToken = self
                    .do_request(reqwest::Method::POST, &url, Some(params), None)
                    .await?;

                let new_authz = Authz {
                    access_token: resp.access_token,
                    refresh_token: resp
                        .refresh_token
                        .unwrap_or_else(|| authz.refresh_token.clone()),
                    user_id: resp.user.user_id,
                    country_code: match &authz.country_code {
                        Some(country_code) => Some(country_code.clone()),
                        None => Some(resp.user.country_code.clone()),
                    },
                };

                // Single, quick swap visible to all readers
                self.authz.store(Some(Arc::new(new_authz.clone())));

                drop(permit);

                // invoke callback if set
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
    #[async_recursion]
    pub(crate) async fn do_request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        url: &str,
        params: Option<serde_json::Value>,
        etag: Option<&str>,
    ) -> Result<T, Error> {
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

        req = req.header(reqwest::header::USER_AGENT, "Mozilla/5.0 (Linux; Android 12; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/91.0.4472.114 Safari/537.36");

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

        let status_code = resp.status().as_u16();

        if resp.status().is_success() {
            let body = resp.bytes().await?;

            // If it's an empty body, just encode a null value
            let mut value = if body.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::from_slice(&body)?
            };

            // Debug trace the response value
            if log::log_enabled!(log::Level::Trace) {
                let pretty_value = serde_json::to_string_pretty(&value).unwrap();
                log::trace!("Requestd URL: {}", url);
                log::trace!("Response {}", pretty_value);
            }

            // If we have an etag, add it to the response, if the value doesn't already exist
            if let Some(etag) = etag {
                if value.get("etag").is_none() {
                    value["etag"] = serde_json::Value::String(etag);
                }
            }

            let resp: T = match serde_json::from_value(value) {
                Ok(t) => t,
                Err(e) => {
                    let problem_value: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    let pretty_problem_value = serde_json::to_string_pretty(&problem_value).unwrap();
                    if log::log_enabled!(log::Level::Debug) {
                        log::debug!("Requested URL: {}", url);
                        log::debug!("JSON deserialization error: {}", e);
                        log::debug!("Response: {}", pretty_problem_value);
                    }
                    return Err(Error::SerdeJson(e));
                }
            };

            Ok(resp)
        } else {
            // If it's 401, we need to refresh the authz and try again
            if status_code == 401 {
                let err = resp.json::<TidalApiError>().await?;

                // Expired token, safe to refresh
                if err.sub_status == 11003 {
                    self.refresh_authz().await?;
                    return self.do_request(method, url, params, etag.as_deref()).await;
                }

                if log::log_enabled!(log::Level::Debug) {
                    log::debug!("Requested URL: {}", url);
                    log::debug!("TIDAL API Error: {}", err);
                }

                // Other error, return the error
                return Err(Error::TidalApiError(err));
            }

            // Parse the error message and maybe log it
            let err = resp.json::<TidalApiError>().await?;
            if log::log_enabled!(log::Level::Debug) {
                let pretty_err = serde_json::to_string_pretty(&err).unwrap();
                log::debug!("Requested URL: {}", url);
                log::debug!("TIDAL API Error: {}", pretty_err);
            }

            Err(Error::TidalApiError(err))
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
        let url = format!("{TIDAL_AUTH_API_BASE_URL}/oauth2/device_authorization");

        let params = serde_json::json!({
            "client_id": &self.client_id,
            "scope": "r_usr w_usr w_sub",
        });

        let mut resp: DeviceAuthorizationResponse = self
            .do_request(reqwest::Method::POST, &url, Some(params), None)
            .await?;

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
    /// let mut client = TidalClient::new("client_id".to_string());
    /// let device_code = "device_code";
    /// let client_secret = "client_secret";
    /// let authz_token = client.authorize(device_code, client_secret).await?;
    /// println!("Authenticated as: {}", authz_token.user.username);
    ///
    /// // Get the authz token to store in persistent storage
    /// let authz = authz_token.authz().unwrap();
    /// std::fs::write("authz.json", serde_json::to_string(&authz).unwrap()).unwrap();
    /// ```
    pub async fn authorize(
        &self,
        device_code: &str,
        client_secret: &str,
    ) -> Result<AuthzToken, Error> {
        let url = format!("{TIDAL_AUTH_API_BASE_URL}/oauth2/token");

        let params = serde_json::json!({
            "client_id": &self.client_id,
            "client_secret": client_secret,
            "device_code": &device_code,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            "scope": "r_usr w_usr w_sub",
        });

        let resp: AuthzToken = self
            .do_request(reqwest::Method::POST, &url, Some(params), None)
            .await?;

        let authz = Authz {
            access_token: resp.access_token.clone(),
            refresh_token: resp
                .refresh_token
                .clone()
                .expect("No refresh token received from Tidal after authorization"),
            user_id: resp.user.user_id,
            country_code: match &self.country_code {
                Some(country_code) => Some(country_code.clone()),
                None => Some(resp.user.country_code.clone()),
            },
        };

        self.authz.store(Some(Arc::new(authz)));

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