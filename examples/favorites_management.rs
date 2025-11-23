//! Favorites management example demonstrating how to work with user favorites.
//!
//! This example shows how to:
//! - Get user's favorite tracks, albums, and artists
//! - Add items to favorites
//! - Remove items from favorites
//! - Paginate through favorites

use std::io::{self, Write};
use tidalrs::{Authz, Order, OrderDirection, TidalClient};

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

    // Get user's favorite tracks
    println!("Getting favorite tracks...");
    let favorite_tracks = client
        .favorite_tracks(
            Some(0),  // offset
            Some(10), // limit
            Some(Order::Date),
            Some(OrderDirection::Desc),
        )
        .await?;

    println!("Found {} favorite tracks:", favorite_tracks.total);
    for (index, fav_track) in favorite_tracks.items.iter().enumerate() {
        println!(
            "  {}. {} by {} (added: {})",
            index + 1,
            fav_track.item.title,
            fav_track
                .item
                .artists
                .first()
                .map(|a| a.name.as_str())
                .unwrap_or("Unknown"),
            fav_track.created
        );
    }

    // Get user's favorite albums
    println!("\nGetting favorite albums...");
    let favorite_albums = client
        .favorite_albums(
            Some(0),  // offset
            Some(10), // limit
            Some(Order::Date),
            Some(OrderDirection::Desc),
        )
        .await?;

    println!("Found {} favorite albums:", favorite_albums.total);
    for (index, fav_album) in favorite_albums.items.iter().enumerate() {
        println!(
            "  {}. {} by {} (added: {})",
            index + 1,
            fav_album.item.title,
            fav_album
                .item
                .artists
                .first()
                .map(|a| a.name.as_str())
                .unwrap_or("Unknown"),
            fav_album.created
        );
    }

    // Get user's favorite artists
    println!("\nGetting favorite artists...");
    let favorite_artists = client
        .favorite_artists(
            Some(0),  // offset
            Some(10), // limit
            Some(Order::Date),
            Some(OrderDirection::Desc),
        )
        .await?;

    println!("Found {} favorite artists:", favorite_artists.total);
    for (index, fav_artist) in favorite_artists.items.iter().enumerate() {
        println!(
            "  {}. {} (added: {})",
            index + 1,
            fav_artist.item.name,
            "N/A" // Artist favorites don't include creation date in the current API
        );
    }

    // Search for a track to add to favorites
    println!("\nSearching for a track to add to favorites...");
    let mut query = tidalrs::SearchQuery::new("Radiohead Paranoid Android");
    query.limit = Some(1);
    query.search_types = Some(vec![tidalrs::ResourceType::Track]);

    let search_results = client.search(query).await?;

    if search_results.tracks.items.is_empty() {
        println!("No tracks found");
        return Ok(());
    }

    let track = &search_results.tracks.items[0];
    println!(
        "Found track: {} by {}",
        track.title,
        track
            .artists
            .first()
            .map(|a| a.name.as_str())
            .unwrap_or("Unknown")
    );

    // Add track to favorites
    println!("\nAdding track to favorites...");
    match client.add_favorite_track(track.id).await {
        Ok(_) => {
            println!("Track added to favorites!");
        }
        Err(e) => {
            println!("Failed to add track to favorites: {}", e);
        }
    }

    // Search for an album to add to favorites
    println!("\nSearching for an album to add to favorites...");
    let mut album_query = tidalrs::SearchQuery::new("Radiohead OK Computer");
    album_query.limit = Some(1);
    album_query.search_types = Some(vec![tidalrs::ResourceType::Album]);

    let album_search_results = client.search(album_query).await?;

    if !album_search_results.albums.items.is_empty() {
        let album = &album_search_results.albums.items[0];
        println!(
            "Found album: {} by {}",
            album.title,
            album
                .artists
                .first()
                .map(|a| a.name.as_str())
                .unwrap_or("Unknown")
        );

        // Add album to favorites
        println!("\nAdding album to favorites...");
        match client.add_favorite_album(album.id).await {
            Ok(_) => {
                println!("Album added to favorites!");
            }
            Err(e) => {
                println!("Failed to add album to favorites: {}", e);
            }
        }
    }

    // Search for an artist to add to favorites
    println!("\nSearching for an artist to add to favorites...");
    let mut artist_query = tidalrs::SearchQuery::new("Radiohead");
    artist_query.limit = Some(1);
    artist_query.search_types = Some(vec![tidalrs::ResourceType::Artist]);

    let artist_search_results = client.search(artist_query).await?;

    if !artist_search_results.artists.items.is_empty() {
        let artist = &artist_search_results.artists.items[0];
        println!("Found artist: {}", artist.name);

        // Add artist to favorites
        println!("\nAdding artist to favorites...");
        match client.add_favorite_artist(artist.id).await {
            Ok(_) => {
                println!("Artist added to favorites!");
            }
            Err(e) => {
                println!("Failed to add artist to favorites: {}", e);
            }
        }
    }

    // Wait for user input before removing favorites
    println!("\nPress Enter to remove the items from favorites...");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    // Remove track from favorites
    if !search_results.tracks.items.is_empty() {
        let track = &search_results.tracks.items[0];
        println!("\nRemoving track from favorites...");
        match client.remove_favorite_track(track.id).await {
            Ok(_) => {
                println!("Track removed from favorites!");
            }
            Err(e) => {
                println!("Failed to remove track from favorites: {}", e);
            }
        }
    }

    // Remove album from favorites
    if !album_search_results.albums.items.is_empty() {
        let album = &album_search_results.albums.items[0];
        println!("\nRemoving album from favorites...");
        match client.remove_favorite_album(album.id).await {
            Ok(_) => {
                println!("Album removed from favorites!");
            }
            Err(e) => {
                println!("Failed to remove album from favorites: {}", e);
            }
        }
    }

    // Remove artist from favorites
    if !artist_search_results.artists.items.is_empty() {
        let artist = &artist_search_results.artists.items[0];
        println!("\nRemoving artist from favorites...");
        match client.remove_favorite_artist(artist.id).await {
            Ok(_) => {
                println!("Artist removed from favorites!");
            }
            Err(e) => {
                println!("Failed to remove artist from favorites: {}", e);
            }
        }
    }

    println!("\nFavorites management example completed!");

    Ok(())
}
