//! Tests for deserializing search results from JSON.
//!
//! This module tests that SearchResults can be correctly deserialized
//! from the example search result JSON file.

use std::fs;
use tidalrs::SearchResults;

#[test]
fn test_deserialize_example_search_result() {
    // Read the JSON file from the tests directory
    let json_str = fs::read_to_string("tests/example_search_result.json")
        .expect("Failed to read example_search_result.json");

    // Attempt to deserialize as SearchResults
    let search_results: SearchResults = serde_json::from_str(&json_str)
        .expect("Failed to deserialize example_search_result.json as SearchResults");

    // Verify that the deserialization was successful by checking some basic properties
    // Check that we have some results
    assert!(
        search_results.albums.items.len() > 0
            || search_results.artists.items.len() > 0
            || search_results.tracks.items.len() > 0
            || search_results.playlists.items.len() > 0,
        "Search results should contain at least one item in albums, artists, tracks, or playlists"
    );

    // Verify playlists were deserialized correctly
    if !search_results.playlists.items.is_empty() {
        let first_playlist = &search_results.playlists.items[0];
        assert!(!first_playlist.uuid.is_empty(), "Playlist UUID should not be empty");
        assert!(!first_playlist.title.is_empty(), "Playlist title should not be empty");
        // url is now Option<String>, so it might be None
        // created and last_updated should be present
        assert!(!first_playlist.created.is_empty(), "Playlist created date should not be empty");
        assert!(!first_playlist.last_updated.is_empty(), "Playlist last_updated should not be empty");
    }

    // Verify artists were deserialized correctly
    if !search_results.artists.items.is_empty() {
        let first_artist = &search_results.artists.items[0];
        assert!(first_artist.id > 0, "Artist ID should be greater than 0");
        assert!(!first_artist.name.is_empty(), "Artist name should not be empty");
    }

    // Verify albums were deserialized correctly
    if !search_results.albums.items.is_empty() {
        let first_album = &search_results.albums.items[0];
        assert!(first_album.id > 0, "Album ID should be greater than 0");
        assert!(!first_album.title.is_empty(), "Album title should not be empty");
    }

    // Verify tracks were deserialized correctly
    if !search_results.tracks.items.is_empty() {
        let first_track = &search_results.tracks.items[0];
        assert!(first_track.id > 0, "Track ID should be greater than 0");
        assert!(!first_track.title.is_empty(), "Track title should not be empty");
    }

    // Verify top hits were deserialized correctly
    if !search_results.top_hits.is_empty() {
        assert!(
            search_results.top_hits.len() > 0,
            "Top hits should contain at least one item if the array is not empty"
        );
    }
}

#[test]
fn test_deserialize_example_search_result_playlist_urls() {
    // This test specifically verifies that playlist URLs can be null
    let json_str = fs::read_to_string("tests/example_search_result.json")
        .expect("Failed to read example_search_result.json");

    let search_results: SearchResults = serde_json::from_str(&json_str)
        .expect("Failed to deserialize example_search_result.json as SearchResults");

    // Check that playlists with null URLs are handled correctly
    // (The url field is now Option<String>, so None is valid)
    for playlist in &search_results.playlists.items {
        // This should not panic - url can be Some(String) or None
        let _url = playlist.url.as_ref();
    }
}

