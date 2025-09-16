//! Authentication example demonstrating the OAuth2 device flow.
//!
//! This example shows how to:
//! - Start the device authorization flow
//! - Complete authentication
//! - Save and restore authentication tokens

use tidalrs::{TidalClient};
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Create a client (you'll need to provide your actual client ID and secret)
    let client_id = "your_client_id";
    let client_secret = "your_client_secret";
    
    let mut client = TidalClient::new(client_id.to_string())
        .with_authz_refresh_callback(|new_authz| {
            println!("Tokens refreshed for user: {}", new_authz.user_id);
            // In a real application, you would save these tokens to persistent storage
            // For example: save_to_file(&new_authz);
        });

    // Start device authorization
    println!("Starting device authorization...");
    let device_auth = client.device_authorization().await?;

    println!("\nPlease complete the following steps:");
    println!("1. Visit: {}", device_auth.url);
    println!("2. Enter this code: {}", device_auth.user_code);
    println!("3. Press Enter here after completing authorization...");

    // Wait for user input
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    // Complete authorization
    println!("Completing authorization...");
    let authz_token = client.authorize(&device_auth.device_code, client_secret).await?;

    println!("\nAuthentication successful!");
    println!("User: {} ({})", authz_token.user.username, authz_token.user.email);
    println!("Country: {}", authz_token.user.country_code);
    println!("User ID: {}", authz_token.user.user_id);

    // Get current tokens for saving
    if let Some(authz) = authz_token.authz() {
        println!("\nCurrent tokens:");
        println!("   Access token: {}...", &authz.access_token[..20]);
        println!("   Refresh token: {}...", &authz.refresh_token[..20]);
        println!("   User ID: {}", authz.user_id);
        println!("   Country: {:?}", authz.country_code);

        // In a real application, you would save these tokens for later use
        // For example: save_tokens_to_file(&authz);
    }

    // Test authenticated API call
    println!("\nTesting authenticated API call...");
    match client.get_user_id() {
        Some(user_id) => {
            println!("Client is authenticated for user: {}", user_id);
            
            // Try to get user's favorite tracks (requires authentication)
            match client.favorite_tracks(Some(0), Some(5), None, None).await {
                Ok(favorites) => {
                    println!("Found {} favorite tracks", favorites.total);
                    for fav in &favorites.items {
                        println!("   - {} by {}", 
                            fav.item.title,
                            fav.item.artists.first().map(|a| a.name.as_str()).unwrap_or("Unknown")
                        );
                    }
                }
                Err(e) => {
                    println!("Failed to get favorites: {}", e);
                }
            }
        }
        None => {
            println!("Client is not authenticated");
        }
    }

    Ok(())
}