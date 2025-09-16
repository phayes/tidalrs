# TidalRS

A comprehensive Rust client library for the Tidal music streaming service API. This library provides async/await support, automatic token refresh, and a clean, type-safe interface for interacting with Tidal's music catalog and user data.

## Features

- 🎵 **Complete Music API**: Access tracks, albums, artists, and playlists
- 🎧 **Audio Streaming**: Download and stream tracks in various quality levels
- 🔍 **Advanced Search**: Search across all content types with filtering
- 👤 **User Management**: Manage favorites, playlists, and user data
- 🔐 **OAuth2 Authentication**: Device flow authentication with automatic token refresh
- 🚀 **Async/Await**: Built on Tokio for high-performance async operations
- 🛡️ **Type Safety**: Comprehensive type definitions for all Tidal API responses
- 📱 **Cross-Platform**: Works on all platforms supported by Rust

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
tidalrs = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
```

### Basic Usage

```rust
use tidalrs::{TidalClient, Authz};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client with your Tidal client ID
    let mut client = TidalClient::new("your_client_id".to_string());
    
    // Authenticate using device flow
    let device_auth = client.device_authorization().await?;
    println!("Visit: {}", device_auth.url);
    println!("Enter code: {}", device_auth.user_code);
    
    // After user authorizes, complete the flow
    let authz_token = client.authorize(&device_auth.device_code, "your_client_secret").await?;
    
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

```rust
use tidalrs::{TidalClient, Authz};

let authz = Authz::new(
    "access_token".to_string(),
    "refresh_token".to_string(),
    user_id,
    Some("US".to_string()),
);

let client = TidalClient::new("your_client_id".to_string())
    .with_authz(authz);
```

## API Overview

### Authentication

The library supports Tidal's OAuth2 device flow authentication:

```rust
// Start device authorization
let device_auth = client.device_authorization().await?;

// User visits the URL and enters the code
println!("Visit: {}", device_auth.url);
println!("Code: {}", device_auth.user_code);

// Complete authorization
let authz_token = client.authorize(&device_auth.device_code, client_secret).await?;
```

### Searching

Search across all content types:

```rust
use tidalrs::{SearchQuery, ResourceType};

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
```

### Tracks

Get track information and stream audio:

```rust
// Get track details
let track = client.track(123456789).await?;
println!("{} - {}", track.title, track.artists[0].name);

// Get streaming URL
let stream = client.track_stream(123456789, AudioQuality::Lossless).await?;
let audio_stream = stream.stream().await?;

// Add to favorites
client.add_favorite_track(123456789).await?;
```

### Albums

Work with albums and their tracks:

```rust
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
```

### Artists

Explore artist information and their albums:

```rust
// Get artist details
let artist = client.artist(456789123).await?;
println!("Artist: {}", artist.name);

// Get artist's albums
let albums = client.artist_albums(456789123, Some(0), Some(20), None).await?;
for album in albums.items {
    println!("Album: {}", album.title);
}
```

### Playlists

Manage playlists and their contents:

```rust
// Create a new playlist
let playlist = client.create_playlist("My Playlist", "A great playlist").await?;
println!("Created playlist: {}", playlist.title);

// Add tracks to playlist
let track_ids = vec![123456789, 987654321];
client.add_tracks_to_playlist(&playlist.uuid, &playlist.etag.unwrap(), track_ids, false).await?;

// Get playlist tracks
let tracks = client.playlist_tracks(&playlist.uuid, Some(0), Some(100)).await?;
for track in tracks.items {
    println!("Track: {}", track.title);
}

// Remove a track from playlist
client.remove_track_from_playlist(&playlist.uuid, &playlist.etag.unwrap(), 123456789).await?;
```

### User Favorites

Manage user's favorite content:

```rust
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
    println!("Favorite artist: {}", fav_artist.name);
}
```

## Audio Quality

The library supports all Tidal audio quality levels:

```rust
use tidalrs::AudioQuality;

// Available quality levels:
// - AudioQuality::Low
// - AudioQuality::High  
// - AudioQuality::Lossless
// - AudioQuality::HiResLossless

let stream = client.track_stream(track_id, AudioQuality::Lossless).await?;
```

## Configuration

Configure the client for different regions and locales:

```rust
let client = TidalClient::new("client_id".to_string())
    .with_country_code("GB".to_string())  // United Kingdom
    .with_locale("en_GB".to_string())     // British English
    .with_device_type(DeviceType::Browser);
```

## Token Refresh

The client automatically handles token refresh, but you can also set up callbacks:

```rust
use std::sync::Arc;

let client = TidalClient::new("client_id".to_string())
    .with_authz_refresh_callback(Arc::new(|new_authz| {
        println!("Tokens refreshed for user: {}", new_authz.user_id);
        // Save the new tokens to persistent storage
    }));
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
