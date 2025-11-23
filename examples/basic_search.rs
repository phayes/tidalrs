//! Basic search example demonstrating how to search for music on Tidal.
//!
//! This example shows how to:
//! - Create a TidalClient
//! - Search for artists, albums, and tracks
//! - Display search results

use tidalrs::{ResourceType, SearchQuery, TidalClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Create a client (you'll need to provide your actual client ID)
    let client = TidalClient::new("your_client_id".to_string());

    // Search for "Radiohead"
    let mut query = SearchQuery::new("Radiohead");
    query.limit = Some(10);
    query.search_types = Some(vec![
        ResourceType::Artist,
        ResourceType::Album,
        ResourceType::Track,
    ]);

    println!("Searching for: {}", query.query);
    let results = client.search(query).await?;

    // Display artists
    if !results.artists.items.is_empty() {
        println!("\nArtists:");
        for artist in &results.artists.items {
            println!("  - {} (ID: {})", artist.name, artist.id);
        }
    }

    // Display albums
    if !results.albums.items.is_empty() {
        println!("\nAlbums:");
        for album in &results.albums.items {
            println!(
                "  - {} by {} (ID: {})",
                album.title,
                album
                    .artists
                    .first()
                    .map(|a| a.name.as_str())
                    .unwrap_or("Unknown"),
                album.id
            );
        }
    }

    // Display tracks
    if !results.tracks.items.is_empty() {
        println!("\nTracks:");
        for track in &results.tracks.items {
            println!(
                "  - {} by {} (ID: {})",
                track.title,
                track
                    .artists
                    .first()
                    .map(|a| a.name.as_str())
                    .unwrap_or("Unknown"),
                track.id
            );
        }
    }

    // Display top hits
    if !results.top_hits.is_empty() {
        println!("\nTop Hits:");
        for hit in &results.top_hits {
            match hit {
                tidalrs::Resource::Artists(artist) => {
                    println!("  - Artist: {} (ID: {})", artist.name, artist.id);
                }
                tidalrs::Resource::Albums(album) => {
                    println!("  - Album: {} (ID: {})", album.title, album.id);
                }
                tidalrs::Resource::Tracks(track) => {
                    println!("  - Track: {} (ID: {})", track.title, track.id);
                }
                tidalrs::Resource::Playlists(playlist) => {
                    println!("  - Playlist: {} (ID: {})", playlist.title, playlist.uuid);
                }
                _ => {
                    println!("  - Other: {}", hit.id());
                }
            }
        }
    }

    Ok(())
}
