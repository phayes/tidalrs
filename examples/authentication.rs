//! Authentication example demonstrating OAuth2 device, PKCE, and client-credentials flows.
//!
//! Device flow is the default path shown below. PKCE and client credentials are
//! documented in comments. Client-credentials sessions can call catalog APIs but
//! not user-specific methods such as favorites and playlists.

use std::io::{self, Write};
use tidalrs::TidalClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let client_id = "your_client_id";
    let client_secret = "your_client_secret";

    let client = TidalClient::new(client_id.to_string())
        .with_client_secret(client_secret)
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

    let authz = authz_token.authz();
    println!("\nCurrent tokens:");
    println!("   Access token: {}...", &authz.access_token[..20]);
    if let Some(refresh_token) = &authz.refresh_token {
        println!("   Refresh token: {}...", &refresh_token[..20]);
    }
    println!("   User ID: {:?}", authz.user_id);
    println!("   Country: {:?}", authz.country_code);

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
