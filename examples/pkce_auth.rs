//! PKCE authentication example demonstrating the OAuth browser login.
//!
//! This example shows how to:
//! - Start a PKCE login and get the URL to open in a browser
//! - Finish the login from the URL the browser is redirected to
//! - Make an authenticated API call
//!
//! Requires `TIDAL_CLIENT_ID`. Set `TIDAL_REDIRECT_URI` to use a redirect URI
//! registered for your client other than `https://tidal.com/android/login/auth`.
//! See `examples/common/mod.rs`.

mod common;

use std::io::{self, Write};

const DEFAULT_REDIRECT_URI: &str = "https://tidal.com/android/login/auth";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Create a client from TIDAL_CLIENT_ID
    let client = common::client_from_env()?.with_authz_refresh_callback(|new_authz| {
        println!("Tokens refreshed for user: {:?}", new_authz.user_id);
        // In a real application, save these tokens to persistent storage.
    });

    // Start the login and send the user to Tidal's login page
    let redirect_uri = common::optional_env("TIDAL_REDIRECT_URI")
        .unwrap_or_else(|| DEFAULT_REDIRECT_URI.to_string());
    let login_url = client.start_pkce(&redirect_uri)?;

    println!("1. Open this URL in your browser and log in:");
    println!("   {login_url}");
    println!("2. After logging in, your browser is sent to {redirect_uri}.");
    println!("   The page may not load; copy the full URL from the address bar.");
    print!("3. Paste it here: ");
    io::stdout().flush()?;

    // Exchange the code in the redirect URL for tokens
    let mut redirect_url = String::new();
    io::stdin().read_line(&mut redirect_url)?;
    let authz_token = client.finish_pkce(redirect_url.trim()).await?;

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

    // Make an authenticated API call
    println!("\nTesting authenticated API call...");
    let favorites = client.favorite_tracks(Some(0), Some(5), None, None).await?;
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

    Ok(())
}
