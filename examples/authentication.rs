//! Authentication example demonstrating OAuth2 device, PKCE, and client-credentials flows.
//!
//! Device flow is the default path shown below. PKCE and client credentials are
//! documented in comments. Client-credentials sessions can call catalog APIs but
//! not user-specific methods such as favorites and playlists.
//!
//! Requires `TIDAL_CLIENT_ID` and `TIDAL_CLIENT_SECRET`; see `examples/common/mod.rs`.

mod common;

use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let client = common::client_from_env()?
        .with_client_secret(common::required_env("TIDAL_CLIENT_SECRET")?)
        .with_authz_refresh_callback(|new_authz| {
            println!("Tokens refreshed for user: {:?}", new_authz.user_id);
            // In a real application, save these tokens to persistent storage.
        });

    println!("Starting device authorization...");
    let device_auth = client.device_authorization().await?;

    println!("\nPlease complete the following steps:");
    println!("1. Visit: {}", device_auth.url);
    println!("2. Enter this code: {}", device_auth.user_code);
    println!("3. Leave this running until authorization completes...");

    io::stdout().flush()?;

    let authz_token = client
        .wait_for_authorization(
            &device_auth.device_code,
            device_auth.expires_in,
            device_auth.interval,
        )
        .await?;

    println!("\nAuthentication successful!");
    println!(
        "User: {} ({})",
        authz_token.user.username, authz_token.user.email
    );
    println!("Country: {}", authz_token.user.country_code);
    println!("User ID: {}", authz_token.user.user_id);

    // Print the settings the other examples read, ready to paste into a shell
    let authz = authz_token.authz();
    println!("\nTo run the other examples, set:");
    println!(
        "export TIDAL_CLIENT_ID='{}'",
        std::env::var("TIDAL_CLIENT_ID")?
    );
    println!("export TIDAL_ACCESS_TOKEN='{}'", authz.access_token);
    if let Some(refresh_token) = &authz.refresh_token {
        println!("export TIDAL_REFRESH_TOKEN='{refresh_token}'");
    }
    if let Some(user_id) = authz.user_id {
        println!("export TIDAL_USER_ID='{user_id}'");
    }
    if let Some(country_code) = &authz.country_code {
        println!("export TIDAL_COUNTRY_CODE='{country_code}'");
    }

    println!("\nTesting authenticated API call...");
    match client.get_user_id() {
        Some(user_id) => {
            println!("Client is authenticated for user: {}", user_id);

            match client.favorite_tracks(Some(0), Some(5), None, None).await {
                Ok(favorites) => {
                    println!("Found {} favorite tracks", favorites.total);
                    for fav in &favorites.items {
                        println!(
                            "   - {} by {}",
                            fav.item.title,
                            fav.item
                                .artists
                                .first()
                                .map(|a| a.name.as_str())
                                .unwrap_or("Unknown")
                        );
                    }
                }
                Err(e) => {
                    println!("Failed to get favorites: {}", e);
                }
            }
        }
        None => {
            println!("Client is not a user-authenticated session");
        }
    }

    Ok(())
}
