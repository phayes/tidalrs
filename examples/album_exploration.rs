//! Album exploration example demonstrating how to work with albums and their tracks.
//!
//! This example shows how to:
//! - Get album information
//! - List album tracks
//! - Get artist information
//! - List artist albums
//! - Handle pagination

use tidalrs::{TidalClient, Authz, AlbumType};

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
    
    let client = TidalClient::new("your_client_id".to_string())
        .with_authz(authz);

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

    // Search for an album
    println!("Searching for an album...");
    let mut query = tidalrs::SearchQuery::new("Radiohead OK Computer");
    query.limit = Some(1);
    query.search_types = Some(vec![tidalrs::ResourceType::Album]);
    
    let search_results = client.search(query).await?;
    
    if search_results.albums.items.is_empty() {
        println!("No albums found");
        return Ok(());
    }

    let album = &search_results.albums.items[0];
    println!("Found album: {} by {}", 
        album.title,
        album.artists.first().map(|a| a.name.as_str()).unwrap_or("Unknown")
    );

    // Get detailed album information
    println!("\nGetting detailed album information...");
    let detailed_album = client.album(album.id).await?;
    
    println!("Album Details:");
    println!("  Title: {}", detailed_album.title);
    println!("  Artists: {}", detailed_album.artists.iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", "));
    println!("  Type: {:?}", detailed_album.album_type);
    println!("  Duration: {} seconds", detailed_album.duration);
    println!("  Number of tracks: {}", detailed_album.number_of_tracks);
    println!("  Number of volumes: {}", detailed_album.number_of_volumes);
    println!("  Release date: {:?}", detailed_album.release_date);
    println!("  Audio quality: {:?}", detailed_album.audio_quality);
    println!("  Explicit: {}", detailed_album.explicit);
    println!("  Popularity: {}", detailed_album.popularity);
    println!("  URL: {}", detailed_album.url);

    // Get album cover URL
    if let Some(cover_url) = detailed_album.cover_url(640, 640) {
        println!("  Cover URL: {}", cover_url);
    }

    // Get album tracks
    println!("\nGetting album tracks...");
    let album_tracks = client.album_tracks(detailed_album.id, Some(0), Some(50)).await?;
    
    println!("Found {} tracks:", album_tracks.total);
    for (index, track) in album_tracks.items.iter().enumerate() {
        println!("  {}. {} ({}) - {} seconds", 
            index + 1,
            track.title,
            track.track_number,
            track.duration
        );
        
        if !track.artists.is_empty() {
            println!("     Artists: {}", track.artists.iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", "));
        }
        
        if track.explicit {
            println!("     Explicit");
        }
    }

    // Get artist information
    if !detailed_album.artists.is_empty() {
        let artist = &detailed_album.artists[0];
        println!("\nGetting artist information...");
        
        let artist_info = client.artist(artist.id).await?;
        println!("Artist Details:");
        println!("  Name: {}", artist_info.name);
        println!("  ID: {}", artist_info.id);
        println!("  Popularity: {:?}", artist_info.popularity);
        println!("  Spotlighted: {}", artist_info.spotlighted);
        println!("  URL: {}", artist_info.url);
        
        if let Some(picture_url) = artist_info.picture_url(640, 640) {
            println!("  Picture URL: {}", picture_url);
        }

        // Get artist's albums
        println!("\nGetting artist's albums...");
        let artist_albums = client.artist_albums(
            artist_info.id,
            None, // All album types
            Some(0),
            Some(20)
        ).await?;

        println!("Found {} albums by {}:", artist_albums.total, artist_info.name);
        for (index, album) in artist_albums.items.iter().enumerate() {
            println!("  {}. {} ({:?}) - {} tracks", 
                index + 1,
                album.title,
                album.album_type,
                album.number_of_tracks
            );
            println!("     Release date: {:?}", album.release_date);
            println!("     Audio quality: {:?}", album.audio_quality);
        }

        // Get specific album types
        println!("\nGetting artist's singles and EPs...");
        let singles_and_eps = client.artist_albums(
            artist_info.id,
            Some(AlbumType::EpsAndSingles),
            Some(0),
            Some(10)
        ).await?;

        if !singles_and_eps.items.is_empty() {
            println!("Found {} singles and EPs:", singles_and_eps.total);
            for (index, album) in singles_and_eps.items.iter().enumerate() {
                println!("  {}. {} ({:?})", 
                    index + 1,
                    album.title,
                    album.album_type
                );
            }
        }

        // Get compilations
        println!("\n📀 Getting artist's compilations...");
        let compilations = client.artist_albums(
            artist_info.id,
            Some(AlbumType::Compilations),
            Some(0),
            Some(10)
        ).await?;

        if !compilations.items.is_empty() {
            println!("Found {} compilations:", compilations.total);
            for (index, album) in compilations.items.iter().enumerate() {
                println!("  {}. {} ({:?})", 
                    index + 1,
                    album.title,
                    album.album_type
                );
            }
        }
    }

    // Demonstrate pagination
    println!("\nDemonstrating pagination...");
    let mut offset = 0;
    let limit = 5;
    let mut total_processed = 0;

    while total_processed < album_tracks.total && total_processed < 15 { // Limit to first 15 tracks
        let page = client.album_tracks(detailed_album.id, Some(offset), Some(limit)).await?;
        
        println!("Page {} (offset: {}, limit: {}):", 
            (offset / limit) + 1, offset, limit);
        
        for track in &page.items {
            println!("  - {}", track.title);
        }
        
        total_processed += page.items.len();
        offset += limit;
        
        if page.items.is_empty() {
            break;
        }
    }

    println!("\nAlbum exploration example completed!");

    Ok(())
}
