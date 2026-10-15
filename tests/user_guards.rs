//! User-authenticated-session guards for user-specific API methods.

use tidalrs::{Authz, Error, TidalClient};

fn app_token_client() -> TidalClient {
    TidalClient::new("client_id".to_string()).with_access_token("app_token")
}

fn user_client() -> TidalClient {
    TidalClient::new("client_id".to_string()).with_authz(Authz::new(
        "access".to_string(),
        Some("refresh".to_string()),
        Some(42),
        Some("US".to_string()),
        None,
    ))
}

#[tokio::test]
async fn access_token_only_session_cannot_list_favorite_tracks() {
    let err = app_token_client()
        .favorite_tracks(None, None, None, None)
        .await
        .expect_err("guard");
    assert!(matches!(err, Error::UserAuthenticationRequired));
}

#[tokio::test]
async fn access_token_only_session_cannot_create_playlist() {
    let err = app_token_client()
        .create_playlist("title", "desc")
        .await
        .expect_err("guard");
    assert!(matches!(err, Error::UserAuthenticationRequired));
}

#[tokio::test]
async fn access_token_only_session_cannot_add_favorite_album() {
    let err = app_token_client()
        .add_favorite_album(1)
        .await
        .expect_err("guard");
    assert!(matches!(err, Error::UserAuthenticationRequired));
}

#[tokio::test]
async fn access_token_only_session_cannot_list_user_playlists() {
    let err = app_token_client()
        .user_playlists(None, None)
        .await
        .expect_err("guard");
    assert!(matches!(err, Error::UserAuthenticationRequired));
}

#[tokio::test]
async fn unauthenticated_client_cannot_use_user_methods() {
    let client = TidalClient::new("client_id".to_string());
    let err = client.add_favorite_track(1).await.expect_err("guard");
    assert!(matches!(err, Error::UserAuthenticationRequired));
}

#[test]
fn user_session_exposes_user_id_for_guards() {
    assert_eq!(user_client().get_user_id(), Some(42));
    assert_eq!(app_token_client().get_user_id(), None);
}
