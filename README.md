# Tidalrs

[![Crates.io](https://img.shields.io/crates/v/tidalrs.svg)](https://crates.io/crates/tidalrs)
[![Documentation](https://docs.rs/tidalrs/badge.svg)](https://docs.rs/tidalrs)
[![GitHub](https://img.shields.io/badge/GitHub-phayes%2Ftidalrs-181717?logo=github)](https://github.com/phayes/tidalrs)

A comprehensive Rust client library for the Tidal music streaming service API. This library provides async/await support, automatic token refresh, and a clean, type-safe interface for interacting with Tidal's music catalog and user data.

**Note**: This library targets Tidal's v1 API. For Tidal's v2 JSON:API, see [tidalv2](https://github.com/phayes/tidalv2).

## Features

- **Complete Music API**: Access tracks, albums, artists, and playlists
- **Audio Streaming**: Stream tracks in various quality levels via DASH-MPEG
- **Advanced Search**: Search across all content types with filtering
- **User Management**: Manage favorites, playlists, and user data
- **OAuth2 Authentication**: Device flow, PKCE, and client-credentials grants with automatic token refresh
- **Async/Await**: Built on Tokio for high-performance async operations
- **Type Safety**: Comprehensive type definitions for all Tidal API responses
- **Cross-Platform**: Works on all platforms supported by Rust

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
tidalrs = "0.6.0"
tokio = { version = "1.0", features = ["full"] }
```

### Basic Usage

```rust,no_run
use tidalrs::{AudioQuality, TidalClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client with your Tidal client ID and secret
    let client = TidalClient::new("your_client_id".to_string())
        .with_client_secret("your_client_secret");
    
    // Authenticate using device flow
    let device_auth = client.device_authorization().await?;
    println!("Visit: {}", device_auth.url);
    println!("Enter code: {}", device_auth.user_code);
    
    let authz_token = client
        .wait_for_authorization(
            &device_auth.device_code,
            device_auth.expires_in,
            device_auth.interval,
        )
        .await?;
    
    // Now you can use the authenticated client
    let track = client.track(123456789).await?;
    println!("Track: {} by {}", track.title, track.artists[0].name);

    // Play the track
    let track_stream = client.track_stream(123456789, AudioQuality::Lossless).await?;
    let music_bytes_stream = track_stream.stream().await?;
    
    Ok(())
}
```

### Using Existing Authentication

If you already have authentication tokens:

```rust,no_run
use tidalrs::{TidalClient, Authz};

let authz = Authz::new(
    "access_token".to_string(),
    Some("refresh_token".to_string()),
    Some(12345),
    Some("US".to_string()),
    None,
);

let client = TidalClient::new("your_client_id".to_string())
    .with_authz(authz);
# let _ = client;
```

Application tokens (no user) can be created with `with_access_token` or `client_credentials()`. Those sessions can call catalog endpoints but user-specific methods such as favorites and playlists return `Error::UserAuthenticationRequired`.

## API Overview

### Authentication

The library supports Tidal's OAuth2 device flow, PKCE, and client-credentials grants:

```rust,no_run
use tidalrs::TidalClient;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = TidalClient::new("client_id".to_string())
    .with_client_secret("client_secret");

// Start device authorization
let device_auth = client.device_authorization().await?;

// User visits the URL and enters the code
println!("Visit: {}", device_auth.url);
println!("Code: {}", device_auth.user_code);

// Poll until the user completes authorization
let authz_token = client
    .wait_for_authorization(
        &device_auth.device_code,
        device_auth.expires_in,
        device_auth.interval,
    )
    .await?;
# let _ = authz_token;
# Ok(())
# }
```

PKCE is available via `start_pkce` / `finish_pkce` for browser redirect flows. `authorize` remains available for a single token exchange if you prefer to poll yourself.

### Searching

Search across all content types:

```rust,no_run
use tidalrs::{SearchQuery, ResourceType};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
# let client = tidalrs::TidalClient::new("client_id".to_string());
let mut query = SearchQuery::new("Radiohead");
query.limit = Some(10);
query.search_types = Some(vec![ResourceType::Artist, ResourceType::Album]);

let results = client.search(query).await?;

for artist in results.artists.items {
    println!("Artist: {}", artist.name);
}

for album in results.albums.items {
    println!("Album: {}", album.title);
}
# Ok(())
# }
```

### Tracks

Get track information and stream audio:

```rust,no_run
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
# let client = tidalrs::TidalClient::new("client_id".to_string());
# use tidalrs::AudioQuality;
// Get track details
let track = client.track(123456789).await?;
println!("{} - {}", track.title, track.artists[0].name);

// Get streaming URL
let stream = client.track_stream(123456789, AudioQuality::Lossless).await?;
let audio_stream = stream.stream().await?;

// Add to favorites
client.add_favorite_track(123456789).await?;
# let _ = audio_stream;
# Ok(())
# }
```

### Albums

Work with albums and their tracks:

```rust,no_run
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
# let client = tidalrs::TidalClient::new("client_id".to_string());
// Get album information
let album = client.album(987654321).await?;
println!("Album: {}", album.title);

// Get album tracks
let tracks = client.album_tracks(987654321, Some(0), Some(50)).await?;
for track in tracks.items {
    println!("Track {}: {}", track.track_number, track.title);
}

// Add album to favorites
client.add_favorite_album(987654321).await?;
# Ok(())
# }
```

### Artists

Explore artist information and their albums:

```rust,no_run
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
# let client = tidalrs::TidalClient::new("client_id".to_string());
// Get artist details
let artist = client.artist(456789123).await?;
println!("Artist: {}", artist.name);

// Get artist's albums
let albums = client.artist_albums(456789123, None, Some(0), Some(20)).await?;
for album in albums.items {
    println!("Album: {}", album.title);
}
# Ok(())
# }
```

### Playlists

Manage playlists and their contents:

```rust,no_run
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
# let client = tidalrs::TidalClient::new("client_id".to_string());
// Create a new playlist
let playlist = client.create_playlist("My Playlist", "A great playlist").await?;
println!("Created playlist: {}", playlist.title);

let etag = playlist.etag.clone().unwrap();
let track_ids = vec![123456789, 987654321];
client.add_tracks_to_playlist(&playlist.uuid, &etag, track_ids, false).await?;

let tracks = client.playlist_tracks(&playlist.uuid, Some(0), Some(100)).await?;
for track in tracks.items {
    println!("Track: {}", track.title);
}

client.remove_track_from_playlist(&playlist.uuid, &etag, 123456789).await?;
# Ok(())
# }
```

### User Favorites

Manage user's favorite content:

```rust,no_run
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
# let client = tidalrs::TidalClient::new("client_id".to_string());
// Get favorite tracks
let favorite_tracks = client.favorite_tracks(Some(0), Some(50), None, None).await?;
for fav_track in favorite_tracks.items {
    println!("Favorite: {}", fav_track.item.title);
}

// Get favorite albums
let favorite_albums = client.favorite_albums(Some(0), Some(20), None, None).await?;
for fav_album in favorite_albums.items {
    println!("Favorite album: {}", fav_album.item.title);
}

// Get favorite artists
let favorite_artists = client.favorite_artists(Some(0), Some(20), None, None).await?;
for fav_artist in favorite_artists.items {
    println!("Favorite artist: {}", fav_artist.item.name);
}
# Ok(())
# }
```

## Audio Quality

The library supports all Tidal audio quality levels:

```rust,no_run
use tidalrs::AudioQuality;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
# let client = tidalrs::TidalClient::new("client_id".to_string());
# let track_id = 123456789;
// Available quality levels:
// - AudioQuality::Low
// - AudioQuality::High
// - AudioQuality::Lossless
// - AudioQuality::HiResLossless

let stream = client.track_stream(track_id, AudioQuality::Lossless).await?;
# let _ = stream;
# Ok(())
# }
```

## Configuration

Configure the client for different regions and locales:

```rust,no_run
use tidalrs::{DeviceType, TidalClient};

let client = TidalClient::new("client_id".to_string())
    .with_country_code("GB".to_string())  // United Kingdom
    .with_locale("en_GB".to_string())     // British English
    .with_device_type(DeviceType::Browser);
# let _ = client;
```

## Token Refresh

The client automatically handles token refresh, but you can also set up callbacks:

```rust,no_run
use tidalrs::TidalClient;

let client = TidalClient::new("client_id".to_string())
    .with_authz_refresh_callback(|new_authz| {
        println!("Tokens refreshed for user: {:?}", new_authz.user_id);
        // Save the new tokens to persistent storage
    });
# let _ = client;
```

## Examples

Check the `examples/` directory for complete working examples:

- `basic_search.rs` - Simple search functionality
- `playlist_management.rs` - Creating and managing playlists
- `audio_streaming.rs` - Streaming and playing audio
- `favorites_management.rs` - Managing user favorites

## Requirements

- Rust 1.70+
- Tokio runtime
- Valid Tidal API client credentials

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Disclaimer

This library is not officially affiliated with Tidal. Use at your own risk and ensure compliance with Tidal's Terms of Service.
