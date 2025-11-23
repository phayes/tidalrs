//! Playlist management example demonstrating how to work with playlists.
//!
//! This example shows how to:
//! - Create a new playlist
//! - Add tracks to a playlist
//! - List playlist tracks
//! - Remove tracks from a playlist
//! - Get user's playlists

use tidalrs::{Authz, TidalClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Create a client with existing authentication
    // In a real application, you would load this from storage
    let authz = Authz::new(
        "your_access_token".to_string(),
        "your_refresh_token".to_string(),
        12345, // Your user ID
        Some("US".to_string()),
    );

    let client = TidalClient::new("your_client_id".to_string()).with_authz(authz);

    // Check if we're authenticated
    match client.get_user_id() {
        Some(user_id) => {
            println!("Authenticated as user: {}", user_id);
        }
        None => {
            println!("Not authenticated. Please run the authentication example first.");
            return Ok(());
        }
    }

    // Get user's existing playlists
    println!("Getting user's playlists...");
    let playlists = client.user_playlists(Some(0), Some(20)).await?;

    println!("Found {} playlists:", playlists.total);
    for playlist in &playlists.items {
        println!(
            "  - {} ({} tracks) - {}",
            playlist.title, playlist.number_of_tracks, playlist.uuid
        );
    }

    // Create a new playlist
    println!("\nCreating a new playlist...");
    let new_playlist = client
        .create_playlist(
            "TidalRS Test Playlist",
            "A test playlist created with TidalRS",
        )
        .await?;

    println!(
        "Created playlist: {} ({})",
        new_playlist.title, new_playlist.uuid
    );

    // Search for some tracks to add to the playlist
    println!("\nSearching for tracks to add...");
    let mut query = tidalrs::SearchQuery::new("Radiohead");
    query.limit = Some(5);
    query.search_types = Some(vec![tidalrs::ResourceType::Track]);

    let search_results = client.search(query).await?;

    if search_results.tracks.items.is_empty() {
        println!("No tracks found to add to playlist");
        return Ok(());
    }

    // Add tracks to the playlist
    let track_ids: Vec<u64> = search_results
        .tracks
        .items
        .iter()
        .take(3) // Add first 3 tracks
        .map(|track| track.id)
        .collect();

    println!("Adding {} tracks to playlist...", track_ids.len());
    for track in &search_results.tracks.items[..track_ids.len()] {
        println!("  - {}", track.title);
    }

    // Get the playlist again to get the current etag
    let playlist_with_etag = client.playlist(&new_playlist.uuid).await?;
    let etag = playlist_with_etag
        .etag
        .as_ref()
        .ok_or("Playlist etag not found")?;

    client
        .add_tracks_to_playlist(
            &new_playlist.uuid,
            etag,
            track_ids,
            false, // Don't allow duplicates
        )
        .await?;

    println!("Tracks added successfully!");

    // List tracks in the playlist
    println!("\nTracks in playlist:");
    let playlist_tracks = client
        .playlist_tracks(&new_playlist.uuid, Some(0), Some(50))
        .await?;

    for (index, track) in playlist_tracks.items.iter().enumerate() {
        println!(
            "  {}. {} by {}",
            index + 1,
            track.title,
            track
                .artists
                .first()
                .map(|a| a.name.as_str())
                .unwrap_or("Unknown")
        );
    }

    // Remove a track from the playlist (remove the first track)
    if !playlist_tracks.items.is_empty() {
        println!("\nRemoving first track from playlist...");
        let track_to_remove = playlist_tracks.items[0].id;

        // Get fresh etag
        let fresh_playlist = client.playlist(&new_playlist.uuid).await?;
        let fresh_etag = fresh_playlist
            .etag
            .as_ref()
            .ok_or("Playlist etag not found")?;

        client
            .remove_track_from_playlist(&new_playlist.uuid, fresh_etag, track_to_remove)
            .await?;

        println!("Track removed successfully!");
    }

    // List tracks again to confirm removal
    println!("\nUpdated tracks in playlist:");
    let updated_tracks = client
        .playlist_tracks(&new_playlist.uuid, Some(0), Some(50))
        .await?;

    for (index, track) in updated_tracks.items.iter().enumerate() {
        println!(
            "  {}. {} by {}",
            index + 1,
            track.title,
            track
                .artists
                .first()
                .map(|a| a.name.as_str())
                .unwrap_or("Unknown")
        );
    }

    println!("\nPlaylist management example completed!");

    Ok(())
}
