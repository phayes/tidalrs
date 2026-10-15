//! Live integration tests for the Tidal v1 API.
//!
//! These tests require the following environment variables to be set:
//! - `TIDAL_CLIENT_ID`: Your Tidal API client ID
//! - `TIDAL_CLIENT_SECRET`: Your Tidal API client secret (optional)
//! - `TIDAL_REFRESH_TOKEN`: Refresh token for automatic token renewal (required)
//! - `TIDAL_ACCESS_TOKEN`: Valid access token (optional; refreshed if expired)
//! - `TIDAL_USER_ID`: Authenticated user ID (required for user-collection walking
//!   unless the stored `Authz` already includes a user id)
//!
//! The tests perform read-only operations to validate API parsing and resource walking.
//! They are marked with `#[ignore]` and require the `--ignored` flag to run.
//!
//! Run with: `cargo test --ignored -- --nocapture`
//! Or run a specific test: `cargo test test_search_and_walk_resources --ignored -- --nocapture`

use async_recursion::async_recursion;
use log::{info, trace};
use std::collections::{HashMap, HashSet};
use std::env;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, Once};
use tidalrs::{
    AudioQuality, Authz, Error, Resource, ResourceType, SearchQuery, TidalApiError, TidalClient,
};

/// Ensure logging is only initialized once across all tests.
static INIT_LOGGER: Once = Once::new();

fn init_logging_once() {
    INIT_LOGGER.call_once(|| {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    });
}

/// Maximum total number of API requests allowed across a single test.
const MAX_TOTAL_REQUESTS: usize = 50;

/// How many related items to fetch when walking a parent resource.
const WALK_PAGE_LIMIT: u32 = 10;

static TOTAL_REQUESTS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WalkKind {
    Album,
    Artist,
    Track,
    Playlist,
    TrackStream,
    ArtistBio,
}

static PROCESSED_RESOURCES: LazyLock<Mutex<HashMap<WalkKind, HashSet<String>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn can_make_request() -> bool {
    let can_make_request = TOTAL_REQUESTS.load(Ordering::SeqCst) < MAX_TOTAL_REQUESTS;
    if !can_make_request && let Ok(processed) = PROCESSED_RESOURCES.lock() {
        let mut total_resources = 0;
        for (resource_type, resource_set) in processed.iter() {
            let count = resource_set.len();
            total_resources += count;
            info!("  {:?}: {} resources processed", resource_type, count);
        }
        info!("Total unique resources processed: {}", total_resources);
    }
    can_make_request
}

fn increment_request_count() {
    let total = TOTAL_REQUESTS.fetch_add(1, Ordering::SeqCst) + 1;
    info!("Total requests: {} / {}", total, MAX_TOTAL_REQUESTS);
}

fn reset_request_count() {
    TOTAL_REQUESTS.store(0, Ordering::SeqCst);
}

fn reset_processed_resources() {
    if let Ok(mut processed) = PROCESSED_RESOURCES.lock() {
        processed.clear();
    }
}

fn skippable_resource_details(err: &Error) -> Option<String> {
    match err {
        Error::TidalApiError(TidalApiError {
            status: 404,
            user_message,
            ..
        }) => Some(format!("404: {user_message}")),
        _ => None,
    }
}

/// Log 404s, then skip that item so the walk can continue.
fn skip_invalid_resource(context: &str, err: &Error) -> bool {
    let Some(details) = skippable_resource_details(err) else {
        return false;
    };
    info!("Skipping {context}: {details}");
    true
}

fn is_resource_processed(kind: WalkKind, resource_id: &str) -> bool {
    if let Ok(processed) = PROCESSED_RESOURCES.lock()
        && let Some(type_set) = processed.get(&kind)
    {
        return type_set.contains(resource_id);
    }
    false
}

fn mark_resource_processed(kind: WalkKind, resource_id: &str) {
    if let Ok(mut processed) = PROCESSED_RESOURCES.lock() {
        processed
            .entry(kind)
            .or_insert_with(HashSet::new)
            .insert(resource_id.to_string());
    }
}

fn live_client() -> TidalClient {
    let client_id =
        env::var("TIDAL_CLIENT_ID").expect("TIDAL_CLIENT_ID environment variable must be set");
    let refresh_token = env::var("TIDAL_REFRESH_TOKEN")
        .expect("TIDAL_REFRESH_TOKEN environment variable must be set");
    let access_token = env::var("TIDAL_ACCESS_TOKEN").unwrap_or_default();
    let user_id = match env::var("TIDAL_USER_ID") {
        Ok(value) => Some(
            value
                .parse::<u64>()
                .unwrap_or_else(|_| panic!("TIDAL_USER_ID must be a u64, got {value}")),
        ),
        Err(_) => None,
    };

    let authz = Authz::new(
        access_token,
        Some(refresh_token),
        user_id,
        Some("US".to_string()),
        Some(u64::MAX),
    );

    let mut client = TidalClient::new(client_id)
        .with_authz(authz)
        .with_country_code("US".to_string());

    if let Ok(secret) = env::var("TIDAL_CLIENT_SECRET")
        && !secret.is_empty()
    {
        client = client.with_client_secret(secret);
    }

    client
}

#[tokio::test]
#[ignore]
async fn test_search_and_walk_resources() {
    reset_request_count();
    reset_processed_resources();
    init_logging_once();

    let client = live_client();

    let search_query = "taylor swift";
    info!("Performing search for: {search_query}");

    if !can_make_request() {
        return;
    }

    increment_request_count();
    let mut query = SearchQuery::new(search_query);
    query.search_types = Some(vec![
        ResourceType::Album,
        ResourceType::Artist,
        ResourceType::Track,
        ResourceType::Playlist,
        ResourceType::Video,
    ]);
    query.include_did_you_mean = Some(true);

    match client.search(query).await {
        Ok(search_results) => {
            trace!(
                "Search successful: {} albums, {} artists, {} tracks, {} playlists, {} videos, {} top hits",
                search_results.albums.items.len(),
                search_results.artists.items.len(),
                search_results.tracks.items.len(),
                search_results.playlists.items.len(),
                search_results.videos.items.len(),
                search_results.top_hits.len()
            );
            walk_search_result(&client, &search_results).await;
        }
        Err(e) => {
            if skip_invalid_resource("search", &e) {
                return;
            }
            panic!("Search failed: {e:?}");
        }
    }
}

async fn walk_search_result(client: &TidalClient, search_results: &tidalrs::SearchResults) {
    for album in &search_results.albums.items {
        process_album(client, album.id, 2).await;
    }
    for artist in &search_results.artists.items {
        process_artist(client, artist.id, 2).await;
    }
    for track in &search_results.tracks.items {
        process_track(client, track.id, 2).await;
    }
    for playlist in &search_results.playlists.items {
        process_playlist(client, &playlist.uuid, 2).await;
    }
    if !search_results.videos.items.is_empty() {
        info!(
            "Skipping {} search videos: the v1 client has no video endpoint",
            search_results.videos.items.len()
        );
    }
    for top_hit in &search_results.top_hits {
        match top_hit {
            Resource::Albums(album) => process_album(client, album.id, 2).await,
            Resource::Artists(artist) => process_artist(client, artist.id, 2).await,
            Resource::Tracks(track) => process_track(client, track.id, 2).await,
            Resource::Playlists(playlist) => process_playlist(client, &playlist.uuid, 2).await,
            Resource::Videos(_) => {
                info!(
                    "Skipping top-hit video {}: no v1 video endpoint",
                    top_hit.id()
                );
            }
            Resource::UserProfiles(_) => {
                info!(
                    "Skipping top-hit user profile {}: no v1 profile walk",
                    top_hit.id()
                );
            }
        }
    }

    info!("Resource walking finished");
}

#[async_recursion]
async fn process_album(client: &TidalClient, album_id: u64, recurse: usize) {
    if !can_make_request() {
        return;
    }

    let album_key = album_id.to_string();
    if is_resource_processed(WalkKind::Album, &album_key) {
        trace!("Skipping already processed album: {album_id}");
        return;
    }
    mark_resource_processed(WalkKind::Album, &album_key);

    trace!("Loading album: {album_id}");
    increment_request_count();

    match client.album(album_id).await {
        Ok(album) => {
            if recurse > 0 {
                for artist in &album.artists {
                    process_artist(client, artist.id, recurse - 1).await;
                }

                if can_make_request() {
                    increment_request_count();
                    match client
                        .album_tracks(album_id, Some(0), Some(WALK_PAGE_LIMIT))
                        .await
                    {
                        Ok(tracks) => {
                            for track in tracks.items {
                                process_track(client, track.id, recurse - 1).await;
                            }
                        }
                        Err(e) => {
                            if !skip_invalid_resource(&format!("album tracks {album_id}"), &e) {
                                panic!("Failed to load album tracks {album_id}: {e:?}");
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            if skip_invalid_resource(&format!("album {album_id}"), &e) {
                return;
            }
            panic!("Failed to load album {album_id}: {e:?}");
        }
    }
}

#[async_recursion]
async fn process_artist(client: &TidalClient, artist_id: u64, recurse: usize) {
    if !can_make_request() {
        return;
    }

    let artist_key = artist_id.to_string();
    if is_resource_processed(WalkKind::Artist, &artist_key) {
        trace!("Skipping already processed artist: {artist_id}");
        return;
    }
    mark_resource_processed(WalkKind::Artist, &artist_key);

    trace!("Loading artist: {artist_id}");
    increment_request_count();

    match client.artist(artist_id).await {
        Ok(_artist) => {
            if recurse > 0 {
                process_artist_bio(client, artist_id).await;

                if can_make_request() {
                    increment_request_count();
                    match client
                        .artist_albums(artist_id, None, Some(0), Some(WALK_PAGE_LIMIT))
                        .await
                    {
                        Ok(albums) => {
                            for album in albums.items {
                                process_album(client, album.id, recurse - 1).await;
                            }
                        }
                        Err(e) => {
                            if !skip_invalid_resource(&format!("artist albums {artist_id}"), &e) {
                                panic!("Failed to load artist albums {artist_id}: {e:?}");
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            if skip_invalid_resource(&format!("artist {artist_id}"), &e) {
                return;
            }
            panic!("Failed to load artist {artist_id}: {e:?}");
        }
    }
}

async fn process_artist_bio(client: &TidalClient, artist_id: u64) {
    if !can_make_request() {
        return;
    }

    let artist_key = artist_id.to_string();
    if is_resource_processed(WalkKind::ArtistBio, &artist_key) {
        trace!("Skipping already processed biography for artist: {artist_id}");
        return;
    }
    mark_resource_processed(WalkKind::ArtistBio, &artist_key);

    trace!("Loading biography for artist: {artist_id}");
    increment_request_count();

    match client.artist_bio(artist_id, Some(true)).await {
        Ok(_bio) => {
            trace!("Biography loaded for artist: {artist_id}");
        }
        Err(e) => {
            if skip_invalid_resource(&format!("biography for artist {artist_id}"), &e) {
                return;
            }
            panic!("Failed to load biography for artist {artist_id}: {e:?}");
        }
    }
}

#[async_recursion]
async fn process_track(client: &TidalClient, track_id: u64, recurse: usize) {
    if !can_make_request() {
        return;
    }

    let track_key = track_id.to_string();
    if is_resource_processed(WalkKind::Track, &track_key) {
        trace!("Skipping already processed track: {track_id}");
        return;
    }
    mark_resource_processed(WalkKind::Track, &track_key);

    trace!("Loading track: {track_id}");
    increment_request_count();

    match client.track(track_id).await {
        Ok(track) => {
            process_track_stream(client, track_id).await;

            if recurse > 0 {
                for artist in &track.artists {
                    process_artist(client, artist.id, recurse - 1).await;
                }
                process_album(client, track.album.id, recurse - 1).await;
            }
        }
        Err(e) => {
            if skip_invalid_resource(&format!("track {track_id}"), &e) {
                return;
            }
            panic!("Failed to load track {track_id}: {e:?}");
        }
    }
}

async fn process_track_stream(client: &TidalClient, track_id: u64) {
    if !can_make_request() || is_resource_processed(WalkKind::TrackStream, &track_id.to_string()) {
        return;
    }

    mark_resource_processed(WalkKind::TrackStream, &track_id.to_string());
    trace!("Loading track stream: {track_id}");
    increment_request_count();

    match client
        .track_dash_playback_info(track_id, AudioQuality::Lossless)
        .await
    {
        Ok(playback_info) => {
            let manifest = playback_info.unpack_manifest().unwrap_or_else(|e| {
                panic!("Failed to decode DASH manifest for track {track_id}: {e}")
            });
            eprintln!("DASH manifest for track {track_id}:\n{manifest}");
        }
        Err(e) => {
            if skip_invalid_resource(&format!("track stream {track_id}"), &e) {
                return;
            }
            panic!("Failed to load track stream {track_id}: {e:?}");
        }
    }
}

#[async_recursion]
async fn process_playlist(client: &TidalClient, playlist_id: &str, recurse: usize) {
    if !can_make_request() {
        return;
    }

    if is_resource_processed(WalkKind::Playlist, playlist_id) {
        trace!("Skipping already processed playlist: {playlist_id}");
        return;
    }
    mark_resource_processed(WalkKind::Playlist, playlist_id);

    trace!("Loading playlist: {playlist_id}");
    increment_request_count();

    match client.playlist(playlist_id).await {
        Ok(_playlist) => {
            if recurse > 0 && can_make_request() {
                increment_request_count();
                match client
                    .playlist_tracks(playlist_id, Some(0), Some(WALK_PAGE_LIMIT))
                    .await
                {
                    Ok(tracks) => {
                        for track in tracks.items {
                            process_track(client, track.id, recurse - 1).await;
                        }
                    }
                    Err(e) => {
                        if !skip_invalid_resource(&format!("playlist tracks {playlist_id}"), &e) {
                            panic!("Failed to load playlist tracks {playlist_id}: {e:?}");
                        }
                    }
                }
            }
        }
        Err(e) => {
            if skip_invalid_resource(&format!("playlist {playlist_id}"), &e) {
                return;
            }
            panic!("Failed to load playlist {playlist_id}: {e:?}");
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_user_collections_and_walk() {
    reset_request_count();
    reset_processed_resources();
    init_logging_once();

    let client = live_client();
    let user_id = client.get_user_id().expect(
        "User collections require a user-authenticated session. Set TIDAL_USER_ID or provide tokens that include a user id.",
    );
    info!("Starting user collections integration test for user {user_id}");

    walk_user_collections(&client).await;
}

async fn walk_user_collections(client: &TidalClient) {
    if can_make_request() {
        info!("Processing user playlists...");
        increment_request_count();
        match client.user_playlists(None, Some(WALK_PAGE_LIMIT)).await {
            Ok(playlists) => {
                info!(
                    "Found {} playlists in user collection",
                    playlists.items.len()
                );
                for playlist in playlists.items {
                    process_playlist(client, &playlist.uuid, 2).await;
                }
            }
            Err(e) => {
                if !skip_invalid_resource("user playlists", &e) {
                    panic!("Could not fetch user playlists: {e:?}");
                }
            }
        }
    }

    if can_make_request() {
        info!("Processing user album collection...");
        increment_request_count();
        match client
            .favorite_albums(None, Some(WALK_PAGE_LIMIT), None, None)
            .await
        {
            Ok(albums) => {
                info!("Found {} albums in user collection", albums.items.len());
                for favorite in albums.items {
                    process_album(client, favorite.item.id, 2).await;
                }
            }
            Err(e) => {
                if !skip_invalid_resource("user album collection", &e) {
                    panic!("Could not fetch user album collection: {e:?}");
                }
            }
        }
    }

    if can_make_request() {
        info!("Processing user artist collection...");
        increment_request_count();
        match client
            .favorite_artists(None, Some(WALK_PAGE_LIMIT), None, None)
            .await
        {
            Ok(artists) => {
                info!("Found {} artists in user collection", artists.items.len());
                for favorite in artists.items {
                    process_artist(client, favorite.item.id, 2).await;
                }
            }
            Err(e) => {
                if !skip_invalid_resource("user artist collection", &e) {
                    panic!("Could not fetch user artist collection: {e:?}");
                }
            }
        }
    }

    if can_make_request() {
        info!("Processing user track collection...");
        increment_request_count();
        match client
            .favorite_tracks(None, Some(WALK_PAGE_LIMIT), None, None)
            .await
        {
            Ok(tracks) => {
                info!("Found {} tracks in user collection", tracks.items.len());
                for favorite in tracks.items {
                    process_track(client, favorite.item.id, 2).await;
                }
            }
            Err(e) => {
                if !skip_invalid_resource("user track collection", &e) {
                    panic!("Could not fetch user track collection: {e:?}");
                }
            }
        }
    }

    info!("User collections walking completed");
}
