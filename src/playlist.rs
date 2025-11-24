use crate::Error;
use crate::List;
use crate::TIDAL_API_BASE_URL;
use crate::TidalClient;
use crate::artist::ArtistSummary;
use crate::track::Track;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Represents a playlist from the Tidal catalog.
///
/// This structure contains all available information about a playlist,
/// including metadata, statistics, and modification capabilities.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    /// Unique playlist identifier (UUID format)
    pub uuid: String,
    /// Playlist title
    pub title: String,
    /// Tidal URL for the playlist
    pub url: String,
    /// Information about the playlist creator
    pub creator: PlaylistCreator,
    /// Playlist description
    #[serde(default)]
    pub description: String,

    /// Total number of tracks in the playlist
    pub number_of_tracks: u32,
    /// Total number of videos in the playlist
    pub number_of_videos: u32,
    /// Total duration of the playlist in seconds
    pub duration: u32,
    /// Popularity score for the playlist
    pub popularity: u32,

    /// ISO timestamp when the playlist was last updated
    pub last_updated: String,
    /// ISO timestamp when the playlist was created
    pub created: String,
    /// ISO timestamp when the last item was added to the playlist
    pub last_item_added_at: Option<String>,

    /// Type of playlist (e.g., "USER", "EDITORIAL")
    #[serde(rename = "type")]
    pub playlist_type: Option<String>,
    /// Whether the playlist is publicly visible
    pub public_playlist: bool,
    /// Playlist cover image identifier
    ///
    /// Use image_url() to get the full URL of the image
    pub image: Option<String>,
    /// Square version of the playlist cover image
    ///
    /// Use square_image_url() to get the full URL of the square image
    pub square_image: Option<String>,
    /// Custom image URL for the playlist
    pub custom_image_url: Option<String>,
    /// Artists promoted in this playlist
    pub promoted_artists: Option<Vec<ArtistSummary>>,

    /// ETag for concurrency control when modifying the playlist
    ///
    /// This is needed for adding or removing tracks from the playlist
    pub etag: Option<String>,
}

impl Playlist {
    /// Generate a URL for the playlist cover image at the specified dimensions.
    ///
    /// # Arguments
    ///
    /// * `height` - Height of the image in pixels
    /// * `width` - Width of the image in pixels
    ///
    /// # Returns
    ///
    /// Returns `Some(String)` with the full URL if an image is available,
    /// or `None` if no image is set.
    pub fn image_url(&self, height: u16, width: u16) -> Option<String> {
        self.image.as_ref().map(|image| {
            let image_path = image.replace('-', "/");
            format!("https://resources.tidal.com/images/{image_path}/{height}x{width}.jpg")
        })
    }

    /// Generate a URL for the square playlist cover image at the specified size.
    ///
    /// # Arguments
    ///
    /// * `size` - Size of the square image in pixels
    ///
    /// # Returns
    ///
    /// Returns `Some(String)` with the full URL if a square image is available,
    /// or `None` if no square image is set.
    pub fn square_image_url(&self, size: u16) -> Option<String> {
        self.square_image.as_ref().map(|square_image| {
            let square_image_path = square_image.replace('-', "/");
            format!("https://resources.tidal.com/images/{square_image_path}/{size}x{size}.jpg")
        })
    }
}

/// Information about the creator of a playlist.
///
/// This structure contains details about who created the playlist,
/// which can be a user or system-generated content.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct PlaylistCreator {
    /// The user ID of the playlist creator.
    /// Will be None or zero if the playlist creator is not a known user.
    #[serde(default)]
    pub id: Option<u64>,
}

/// A recommended item from a playlist recommendations response.
///
/// This structure wraps a track (or potentially other resource types in the future)
/// along with its type identifier.
///
/// This is an internal helper type used only for deserializing the API response.
#[derive(Debug, Serialize, Deserialize, Clone)]
struct PlaylistRecommendationItem {
    /// The recommended track
    item: Track,
    /// The type of the recommended item (e.g., "track")
    #[serde(rename = "type")]
    item_type: String,
}

impl TidalClient {
    /// Get playlist information by ID.
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - The unique identifier (UUID) of the playlist
    ///
    /// # Returns
    ///
    /// Returns a `Playlist` structure with all available metadata.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let playlist = client.playlist("12345678-1234-1234-1234-123456789abc").await?;
    /// println!("Playlist: {}", playlist.title);
    /// ```
    pub async fn playlist(&self, playlist_id: &str) -> Result<Playlist, Error> {
        let url = format!("{TIDAL_API_BASE_URL}/playlists/{playlist_id}");
        let params = serde_json::json!({
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });
        let resp: Playlist = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        Ok(resp)
    }

    /// Get all tracks in a specific playlist with pagination support.
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - The unique identifier (UUID) of the playlist
    /// * `offset` - Number of tracks to skip (default: 0)
    /// * `limit` - Maximum number of tracks to return (default: 100)
    ///
    /// # Returns
    ///
    /// Returns a paginated list of tracks in the playlist.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let tracks = client.playlist_tracks("12345678-1234-1234-1234-123456789abc", Some(0), Some(20)).await?;
    /// for track in tracks.items {
    ///     println!("Track: {}", track.title);
    /// }
    /// ```
    pub async fn playlist_tracks(
        &self,
        playlist_id: &str,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<List<Track>, Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);
        let url = format!("{TIDAL_API_BASE_URL}/playlists/{playlist_id}/tracks");
        let params = serde_json::json!({
            "offset": offset,
            "limit": limit,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: List<Track> = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        Ok(resp)
    }

    /// Create a new playlist for the authenticated user.
    ///
    /// # Arguments
    ///
    /// * `title` - The title of the new playlist
    /// * `description` - A description of the playlist
    ///
    /// # Returns
    ///
    /// Returns the newly created `Playlist` with all metadata.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let playlist = client.create_playlist("My Favorites", "A collection of my favorite songs").await?;
    /// println!("Created playlist: {}", playlist.title);
    /// ```
    pub async fn create_playlist(&self, title: &str, description: &str) -> Result<Playlist, Error> {
        let user_id = self
            .get_user_id()
            .ok_or(Error::UserAuthenticationRequired)?;
        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/playlists");
        let params = serde_json::json!({
            "title": title,
            "description": description,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });
        let resp: Playlist = self
            .do_request(Method::POST, &url, Some(params), None)
            .await?;
        Ok(resp)
    }

    /// Add multiple tracks to a playlist.
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - The unique identifier (UUID) of the playlist
    /// * `playlist_etag` - The ETag from the playlist (required for concurrency control)
    /// * `track_ids` - Vector of track IDs to add to the playlist
    /// * `add_dupes` - Whether to add duplicate tracks (true) or fail if duplicates exist (false)
    ///
    /// # Example
    ///
    /// ```no_run
    /// let playlist = client.playlist("12345678-1234-1234-1234-123456789abc").await?;
    /// let track_ids = vec![123456789, 987654321];
    /// client.add_tracks_to_playlist(&playlist.uuid, &playlist.etag.unwrap(), track_ids, false).await?;
    /// println!("Tracks added to playlist!");
    /// ```
    pub async fn add_tracks_to_playlist(
        &self,
        playlist_id: &str,
        playlist_etag: &str,
        track_ids: Vec<u64>,
        add_dupes: bool,
    ) -> Result<(), Error> {
        let url = format!("{TIDAL_API_BASE_URL}/playlists/{playlist_id}/items");

        // Convert track IDs to comma-separated string
        let track_ids_str = track_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");

        let on_dupes = if add_dupes { "ADD" } else { "FAIL" };

        let params = serde_json::json!({
            "trackIds": track_ids_str,
            "onDupes": on_dupes,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let _: Value = self
            .do_request(Method::POST, &url, Some(params), Some(playlist_etag))
            .await?;

        Ok(())
    }

    /// Remove a track from a playlist by its index position.
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - The unique identifier (UUID) of the playlist
    /// * `playlist_etag` - The ETag from the playlist (required for concurrency control)
    /// * `index` - The zero-based index of the track to remove
    ///
    /// # Example
    ///
    /// ```no_run
    /// let playlist = client.playlist("12345678-1234-1234-1234-123456789abc").await?;
    /// client.remove_track_from_playlist_by_index(&playlist.uuid, &playlist.etag.unwrap(), 0).await?;
    /// println!("Track removed from playlist!");
    /// ```
    pub async fn remove_track_from_playlist_by_index(
        &self,
        playlist_id: &str,
        playlist_etag: &str,
        index: usize,
    ) -> Result<(), Error> {
        let url = format!("{TIDAL_API_BASE_URL}/playlists/{playlist_id}/items/{index}");

        let _: Value = self
            .do_request(Method::DELETE, &url, None, Some(playlist_etag))
            .await?;

        Ok(())
    }

    /// Remove a specific track from a playlist by track ID.
    ///
    /// This method will search through the playlist to find the track
    /// and remove it. If the track appears multiple times, only the
    /// first occurrence will be removed.
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - The unique identifier (UUID) of the playlist
    /// * `playlist_etag` - The ETag from the playlist (required for concurrency control)
    /// * `track_id` - The unique identifier of the track to remove
    ///
    /// # Returns
    ///
    /// Returns an error if the track is not found in the playlist.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let playlist = client.playlist("12345678-1234-1234-1234-123456789abc").await?;
    /// client.remove_track_from_playlist(&playlist.uuid, &playlist.etag.unwrap(), 123456789).await?;
    /// println!("Track removed from playlist!");
    /// ```
    pub async fn remove_track_from_playlist(
        &self,
        playlist_id: &str,
        playlist_etag: &str,
        track_id: u64,
    ) -> Result<(), Error> {
        // Find the index of the track in the playlist

        let track_index: Option<u32>;
        let mut offset: u32 = 0;

        'outer: loop {
            let playlist_tracks = self
                .playlist_tracks(playlist_id, Some(offset), None)
                .await?;

            for (index, track) in playlist_tracks.items.iter().enumerate() {
                if track.id == track_id {
                    track_index = Some(index as u32);
                    break 'outer;
                }
            }

            if playlist_tracks.num_left() == 0 {
                return Err(Error::PlaylistTrackNotFound(
                    playlist_id.to_string(),
                    track_id,
                ));
            }

            offset += playlist_tracks.items.len() as u32;
        }

        let track_index = track_index.ok_or(Error::PlaylistTrackNotFound(
            playlist_id.to_string(),
            track_id,
        ))?;

        self.remove_track_from_playlist_by_index(playlist_id, playlist_etag, track_index as usize)
            .await?;

        Ok(())
    }

    /// Get all playlists created by the authenticated user.
    ///
    /// # Arguments
    ///
    /// * `offset` - Number of playlists to skip (default: 0)
    /// * `limit` - Maximum number of playlists to return (default: 100)
    ///
    /// # Returns
    ///
    /// Returns a paginated list of playlists created by the user.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let playlists = client.user_playlists(None, Some(10)).await?;
    /// for playlist in playlists.items {
    ///     println!("Playlist: {}", playlist.title);
    /// }
    /// ```
    pub async fn user_playlists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<List<Playlist>, Error> {
        let user_id = self
            .get_user_id()
            .ok_or(Error::UserAuthenticationRequired)?;
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);
        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/playlists");
        let params = serde_json::json!({
            "offset": offset,
            "limit": limit,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: List<Playlist> = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        Ok(resp)
    }

    /// Get recommended tracks for a specific playlist with pagination support.
    ///
    /// This method retrieves tracks that Tidal recommends based on the
    /// playlist's content and user preferences.
    ///
    /// # Arguments
    ///
    /// * `playlist_id` - The unique identifier (UUID) of the playlist
    /// * `offset` - Number of recommendations to skip (default: 0)
    /// * `limit` - Maximum number of recommendations to return (default: 50)
    ///
    /// # Returns
    ///
    /// Returns a paginated list of recommended tracks for the playlist.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let recommendations = client.playlist_recommendations(
    ///     "12345678-1234-1234-1234-123456789abc",
    ///     Some(0),
    ///     Some(50)
    /// ).await?;
    /// for track in recommendations.items {
    ///     println!(
    ///         "Recommended: {} by {}",
    ///         track.title,
    ///         track.artists[0].name
    ///     );
    /// }
    /// ```
    pub async fn playlist_recommendations(
        &self,
        playlist_id: &str,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<List<Track>, Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(50);
        let url = format!("{TIDAL_API_BASE_URL}/playlists/{playlist_id}/recommendations/items");
        let params = serde_json::json!({
            "offset": offset,
            "limit": limit,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: List<PlaylistRecommendationItem> = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        // Convert the internal recommendation wrapper type into a bare `List<Track>`
        let tracks: Vec<Track> = resp.items.into_iter().map(|rec| rec.item).collect();

        let track_list = List {
            items: tracks,
            offset: resp.offset,
            limit: resp.limit,
            total: resp.total,
            etag: resp.etag,
        };

        Ok(track_list)
    }
}
