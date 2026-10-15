//! Rodio playback example demonstrating the highest-level way to play a track.
//!
//! This example shows how to:
//! - Open a track as a ready-to-play rodio source
//! - See which quality and transport were used after any fallback
//! - Watch download progress for MPEG-DASH streams
//! - Play the source through the default audio device
//!
//! Requires the `rodio` feature:
//! cargo run --example audio_streaming_rodio --features rodio
//!
//! To see each quality fallback as it happens:
//! RUST_LOG=tidalrs=debug cargo run --example audio_streaming_rodio --features rodio
//!
//! Configured from environment variables; see `examples/common/mod.rs`.

mod common;

use tidalrs::AudioQuality;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    // Create a client with existing authentication
    let client = common::authorized_client_from_env()?;

    // Check if we're authenticated
    if client.get_user_id().is_none() {
        println!("TIDAL_USER_ID is not set. Run the authentication example to get it.");
        return Ok(());
    }

    // Search for a track to play
    println!("Searching for a track to play...");
    let mut query = tidalrs::SearchQuery::new("Zeros & Ones by Silk Static");
    query.limit = Some(1);
    query.search_types = Some(vec![tidalrs::ResourceType::Track]);

    let search_results = client.search(query).await?;
    let Some(track) = search_results.tracks.items.first() else {
        println!("No tracks found");
        return Ok(());
    };
    println!(
        "Found track: {} by {}",
        track.title,
        track
            .artists
            .first()
            .map(|a| a.name.as_str())
            .unwrap_or("Unknown")
    );

    // Ask for the best quality; it falls back if that isn't available.
    // With only a track ID, use `client.track_rodio_source(id, quality)` instead.
    let source = track
        .rodio_source(&client, AudioQuality::HiResLossless)
        .await?;
    println!(
        "Playing {:?} over {:?}",
        source.audio_quality(),
        source.transport()
    );

    // Report download progress. Only DASH streams provide it, and the handle
    // must be taken before the source is handed to rodio.
    if let Some(buffer) = source.buffer() {
        buffer.on_change(|status| {
            println!(
                "  Downloaded {:.0}% ({} of {} fragments)",
                status.downloaded_percent(),
                status.downloaded.fragments,
                status.total.fragments,
            );
        });
    }

    // Play to the end. Rodio's player blocks, so keep it off the async runtime.
    tokio::task::spawn_blocking(move || {
        let handle =
            rodio::DeviceSinkBuilder::open_default_sink().expect("open default audio device");
        let player = rodio::Player::connect_new(handle.mixer());
        player.append(source);
        player.sleep_until_end();
    })
    .await?;

    println!("\n🎉 Rodio playback example completed!");

    Ok(())
}
