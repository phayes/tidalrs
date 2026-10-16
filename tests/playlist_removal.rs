//! Integration tests for removing a track from a playlist by its ID.
//!
//! These tests use `mockito` to stand up a local HTTP server in place of
//! `https://api.tidal.com/v1`, so the index search can be checked against
//! playlists with several pages, videos, and inaccurate totals.

use mockito::{Matcher, Server, ServerGuard};
use tidalrs::{Authz, Error, TidalClient};

const PLAYLIST: &str = "pl-uuid";
const ETAG: &str = "etag-1";

fn test_client(server_url: &str) -> TidalClient {
    TidalClient::new("client_id".to_string())
        .with_api_base_url(server_url.to_string())
        .with_authz(Authz::new(
            "access".to_string(),
            Some("refresh".to_string()),
            Some(42),
            Some("US".to_string()),
            None,
        ))
}

/// A playlist item entry of the given kind and ID.
fn entry(kind: &str, id: u64) -> String {
    format!(r#"{{"type":"{kind}","cut":null,"item":{{"id":{id}}}}}"#)
}

/// Serves one page of `/playlists/{PLAYLIST}/items` at `offset`.
async fn page(server: &mut ServerGuard, offset: usize, total: usize, entries: &[String]) {
    let body = format!(
        r#"{{"limit":100,"offset":{offset},"totalNumberOfItems":{total},"items":[{}]}}"#,
        entries.join(",")
    );
    server
        .mock("GET", format!("/playlists/{PLAYLIST}/items").as_str())
        .match_query(Matcher::UrlEncoded("offset".into(), offset.to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .expect(1)
        .create_async()
        .await;
}

/// Expects exactly `hits` deletes of the entry at `index`, sent with the playlist's ETag.
async fn delete(server: &mut ServerGuard, index: usize, hits: usize) -> mockito::Mock {
    server
        .mock(
            "DELETE",
            format!("/playlists/{PLAYLIST}/items/{index}").as_str(),
        )
        .match_header("if-none-match", ETAG)
        .with_status(200)
        .expect(hits)
        .create_async()
        .await
}

#[tokio::test]
async fn removes_a_track_found_on_a_later_page_by_its_playlist_position() {
    let mut server = Server::new_async().await;
    page(&mut server, 0, 4, &[entry("track", 1), entry("track", 2)]).await;
    page(&mut server, 2, 4, &[entry("track", 3), entry("track", 7)]).await;
    let removed = delete(&mut server, 3, 1).await;

    test_client(&server.url())
        .remove_track_from_playlist(PLAYLIST, ETAG, 7)
        .await
        .expect("track removed");

    removed.assert_async().await;
}

#[tokio::test]
async fn counts_videos_when_finding_the_track_position() {
    let mut server = Server::new_async().await;
    page(
        &mut server,
        0,
        3,
        &[entry("video", 7), entry("video", 9), entry("track", 7)],
    )
    .await;
    let removed = delete(&mut server, 2, 1).await;

    test_client(&server.url())
        .remove_track_from_playlist(PLAYLIST, ETAG, 7)
        .await
        .expect("track removed");

    removed.assert_async().await;
}

#[tokio::test]
async fn stops_on_an_empty_page_when_the_total_promises_more() {
    let mut server = Server::new_async().await;
    page(&mut server, 0, 10, &[entry("track", 1), entry("track", 2)]).await;
    page(&mut server, 2, 10, &[]).await;
    let removed = delete(&mut server, 0, 0).await;

    let err = test_client(&server.url())
        .remove_track_from_playlist(PLAYLIST, ETAG, 7)
        .await
        .expect_err("track is not in the playlist");

    assert!(matches!(err, Error::PlaylistTrackNotFound(ref id, 7) if id == PLAYLIST));
    removed.assert_async().await;
}

#[tokio::test]
async fn stops_when_a_page_returns_more_than_the_total() {
    let mut server = Server::new_async().await;
    page(&mut server, 0, 1, &[entry("track", 1), entry("track", 2)]).await;
    let removed = delete(&mut server, 0, 0).await;

    let err = test_client(&server.url())
        .remove_track_from_playlist(PLAYLIST, ETAG, 7)
        .await
        .expect_err("track is not in the playlist");

    assert!(matches!(err, Error::PlaylistTrackNotFound(ref id, 7) if id == PLAYLIST));
    removed.assert_async().await;
}
