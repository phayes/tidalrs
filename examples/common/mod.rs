//! Client setup shared by the examples, configured from environment variables:
//!
//! - `TIDAL_CLIENT_ID` (required)
//! - `TIDAL_CLIENT_SECRET` (authentication example only)
//! - `TIDAL_REDIRECT_URI` (pkce_auth example only, optional)
//! - `TIDAL_ACCESS_TOKEN` (required by examples that act as a user)
//! - `TIDAL_REFRESH_TOKEN` (optional, lets the client renew an expired access token)
//! - `TIDAL_USER_ID` (required by examples that act as a user)
//! - `TIDAL_COUNTRY_CODE` (optional, defaults to `US`)
//!
//! Run the authentication or pkce_auth example to log in and get these values.

// Each example uses only some of these helpers.
#![allow(dead_code)]

use std::error::Error;
use tidalrs::{Authz, TidalClient};

/// A client with no user credentials.
pub fn client_from_env() -> Result<TidalClient, Box<dyn Error>> {
    Ok(TidalClient::new(required_env("TIDAL_CLIENT_ID")?))
}

/// A client authorized as the user whose tokens are in the environment.
pub fn authorized_client_from_env() -> Result<TidalClient, Box<dyn Error>> {
    // Parse the optional user ID up front so a typo is reported clearly.
    let user_id = optional_env("TIDAL_USER_ID")
        .map(|id| id.parse::<u64>())
        .transpose()
        .map_err(|error| format!("TIDAL_USER_ID is not a valid user ID: {error}"))?;

    let authz = Authz::new(
        required_env("TIDAL_ACCESS_TOKEN")?,
        optional_env("TIDAL_REFRESH_TOKEN"),
        user_id,
        optional_env("TIDAL_COUNTRY_CODE"),
        None,
    );

    Ok(client_from_env()?.with_authz(authz))
}

/// The value of `name`, or an error naming the missing variable.
pub fn required_env(name: &str) -> Result<String, Box<dyn Error>> {
    optional_env(name).ok_or_else(|| format!("environment variable {name} is not set").into())
}

/// The value of `name`, treating an empty value as unset.
pub fn optional_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}
