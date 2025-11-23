//! Audio streaming example demonstrating how to stream and play audio.
//!
//! This example shows how to:
//! - Get track streaming information
//! - Download and stream audio
//! - Handle different audio qualities
//!
//! Note: This example requires the `rodio` crate for audio playback.
//! Add it to your Cargo.toml:
//! [dependencies]
//! rodio = "0.21"

use tidalrs::{AudioQuality, Authz, TidalClient};

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

    // Search for a track to stream
    println!("Searching for a track to stream...");
    let mut query = tidalrs::SearchQuery::new("Radiohead Creep");
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

    // Get track details
    println!("\nTrack details:");
    println!("  ID: {}", track.id);
    println!("  Duration: {} seconds", track.duration);
    println!("  Audio Quality: {:?}", track.audio_quality);
    println!("  Explicit: {}", track.explicit);

    // Try different audio qualities
    let qualities = vec![
        AudioQuality::Low,
        AudioQuality::High,
        AudioQuality::Lossless,
        AudioQuality::HiResLossless,
    ];

    for quality in qualities {
        println!("\nTesting {} quality...", format!("{:?}", quality));

        match client.track_stream(track.id, quality).await {
            Ok(stream) => {
                println!("Stream available for {} quality", format!("{:?}", quality));
                println!("  Codec: {}", stream.codec);
                println!("  Audio Mode: {}", stream.audio_mode);
                println!("  URLs: {}", stream.urls.len());

                if let Some(primary_url) = stream.primary_url() {
                    println!("  Primary URL: {}...", &primary_url[..50]);
                }

                let audio_stream = stream.stream().await?;

                let device_output =
                    rodio::stream::OutputStreamBuilder::open_default_stream().unwrap();
                let sink = rodio::Sink::connect_new(device_output.mixer());
                let decoder = rodio::Decoder::new(audio_stream).unwrap();
                sink.append(decoder);
                sink.play();
                sink.sleep_until_end();

                break; // Use the first available quality
            }
            Err(e) => {
                println!("{} quality not available: {}", format!("{:?}", quality), e);
            }
        }
    }

    // Get track playback info (alternative method)
    println!("\nGetting track playback info...");
    match client
        .track_playback_info(track.id, AudioQuality::High)
        .await
    {
        Ok(playback_info) => {
            println!("Playback info retrieved:");
            println!("  Asset Presentation: {}", playback_info.asset_presentation);
            println!("  Audio Mode: {}", playback_info.audio_mode);
            println!("  Audio Quality: {}", playback_info.audio_quality);
            if let Some(bit_depth) = playback_info.bit_depth {
                println!("  Bit Depth: {} bits", bit_depth);
            }
            if let Some(sample_rate) = playback_info.sample_rate {
                println!("  Sample Rate: {} Hz", sample_rate);
            }
            println!(
                "  Track Peak Amplitude: {:.2}",
                playback_info.track_peak_amplitude
            );
            println!(
                "  Album Peak Amplitude: {:.2}",
                playback_info.album_peak_amplitude
            );
        }
        Err(e) => {
            println!("Failed to get playback info: {}", e);
        }
    }

    // Get DASH playback info (for advanced streaming)
    println!("\nGetting DASH playback info...");
    match client
        .track_dash_playback_info(track.id, AudioQuality::Lossless)
        .await
    {
        Ok(dash_info) => {
            println!("DASH playback info retrieved:");
            println!("  Audio Quality: {:?}", dash_info.audio_quality);
            println!("  Bit Depth: {} bits", dash_info.bit_depth);
            println!("  Sample Rate: {} Hz", dash_info.sample_rate);
            println!("  Manifest MIME Type: {}", dash_info.manifest_mime_type);

            // Decode the manifest (it's base64 encoded)
            match dash_info.unpack_manifest() {
                Ok(manifest) => {
                    println!(
                        "  Manifest (first 200 chars): {}...",
                        &manifest.chars().take(200).collect::<String>()
                    );
                }
                Err(e) => {
                    println!("  Failed to decode manifest: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Failed to get DASH playback info: {}", e);
        }
    }

    println!("\n🎉 Audio streaming example completed!");
    println!("\nTo actually play audio, you would:");
    println!("  1. Use the stream() method to get a StreamDownload");
    println!("  2. Use an audio library like rodio to play the stream");
    println!("  3. Handle audio device selection and playback controls");

    Ok(())
}
