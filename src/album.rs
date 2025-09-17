
use crate::Error;
use crate::TIDAL_API_BASE_URL;
use crate::TidalClient;
use crate::track::Track;
use crate::Order;
use crate::OrderDirection;
use crate::artist::ArtistSummary;
use crate::MediaMetadata;
use crate::List;
use reqwest::Method;
use serde_json::Value;
use serde::{Deserialize, Serialize};
use strum_macros::AsRefStr;
use crate::AudioQuality;
use strum_macros::EnumString;

/// Types of albums available in the Tidal catalog.
///
/// This enum represents different album formats and categories
/// that can be used for filtering album searches.
#[derive(Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum AlbumType {
    /// Standard album release
    #[default]
    ALBUM,
    /// Long play album
    Lp,
    /// Extended play album
    Ep,
    /// Single track release
    Single,
    /// Collection of EPs and singles
    EpsAndSingles,
    /// Compilation album
    Compilations,
}

/// Represents an album from the Tidal catalog.
///
/// This structure contains all available information about an album,
/// including metadata, artwork, streaming availability, and track counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    /// Unique album identifier
    pub id: u64,
    /// List of artists who contributed to this album
    pub artists: Vec<ArtistSummary>,

    /// Audio quality level available for this album for standard streaming
    /// 
    /// Higher quality streams may be available than is indicated here when using MPEG-DASH for playback.
    pub audio_quality: AudioQuality,
    /// Total duration of the album in seconds
    pub duration: u32,
    /// Whether the album contains explicit content
    pub explicit: bool,
    /// Album title
    pub title: String,
    /// Popularity score for the album
    pub popularity: u32,

    /// Additional media metadata and tags
    pub media_metadata: Option<MediaMetadata>,

    /// Album cover image identifier
    /// 
    /// Use cover_url() to get the full URL of the cover image.
    pub cover: Option<String>,
    /// Video cover identifier (if available)
    pub video_cover: Option<String>,
    /// Dominant color extracted from the cover art
    pub vibrant_color: Option<String>,
    /// Original release date of the album
    pub release_date: Option<String>,
    /// Date when the album became available for streaming
    pub stream_start_date: Option<String>,

    /// Copyright information
    pub copyright: Option<String>,
    /// Total number of tracks on the album
    pub number_of_tracks: u32,
    /// Number of videos included with the album
    pub number_of_videos: u32,
    /// Number of volumes (for multi-disc albums)
    pub number_of_volumes: u32,
    /// Universal Product Code (UPC) for the album
    pub upc: Option<String>,
    /// Tidal URL for the album
    pub url: String,
    /// Album version or edition
    pub version: Option<String>,

    /// Type of album (ALBUM, EP, Single, etc.)
    #[serde(rename = "type")]
    pub album_type: AlbumType,

    /// Whether the album is ready for ad-supported streaming
    pub ad_supported_stream_ready: bool,
    /// Whether streaming is allowed for this album
    pub allow_streaming: bool,
    /// Whether the album is ready for DJ use
    pub dj_ready: bool,
    /// Whether the album requires payment to stream
    pub pay_to_stream: bool,
    /// Whether the album is only available to premium subscribers
    pub premium_streaming_only: bool,
    /// Whether the album supports stem separation
    pub stem_ready: bool,
    /// Whether the album is ready for streaming
    pub stream_ready: bool,

    /// Available audio modes for this album
    pub audio_modes: Vec<String>,
}

impl Album {
    /// Generate a URL for the album cover image at the specified dimensions.
    ///
    /// # Arguments
    ///
    /// * `height` - Height of the image in pixels
    /// * `width` - Width of the image in pixels
    ///
    /// # Returns
    ///
    /// Returns `Some(String)` with the full URL if a cover is available,
    /// or `None` if no cover image is set.
    pub fn cover_url(&self, height: u16, width: u16) -> Option<String> {
        self.cover.as_ref().map(|cover| {
            let cover_path = cover.replace('-', "/");
            format!("https://resources.tidal.com/images/{cover_path}/{height}x{width}.jpg")
        })
    }
}

/// Represents an album that has been added to a user's favorites.
///
/// This structure includes the album data along with metadata
/// about when it was added to favorites.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteAlbum {
    /// ISO timestamp when the album was added to favorites
    pub created: String,
    /// The album data
    pub item: Album,
}

impl TidalClient {
    /// Get album information by ID.
    ///
    /// # Arguments
    ///
    /// * `album_id` - The unique identifier of the album
    ///
    /// # Returns
    ///
    /// Returns an `Album` structure with all available metadata.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let album = client.album(123456789).await?;
    /// println!("Album: {} by {}", album.title, album.artists[0].name);
    /// ```
    pub async fn album(
        &self,
        album_id: u64,
    ) -> Result<Album, Error> {
        let url = format!("{TIDAL_API_BASE_URL}/albums/{album_id}");

        let params = serde_json::json!({
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: Album = self.do_request(Method::GET, &url, Some(params), None).await?;

        Ok(resp)
    }

    /// Get all tracks for a specific album with pagination support.
    ///
    /// # Arguments
    ///
    /// * `album_id` - The unique identifier of the album
    /// * `offset` - Number of tracks to skip (default: 0)
    /// * `limit` - Maximum number of tracks to return (default: 100)
    ///
    /// # Returns
    ///
    /// Returns a paginated list of tracks belonging to the album.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let tracks = client.album_tracks(123456789, Some(0), Some(20)).await?;
    /// for track in tracks.items {
    ///     println!("Track: {}", track.title);
    /// }
    /// ```
    pub async fn album_tracks(
        &self,
        album_id: u64,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<List<Track>, Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        let url = format!("{TIDAL_API_BASE_URL}/albums/{album_id}/tracks");

        let params = serde_json::json!({
            "offset": offset,
            "limit": limit,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: List<Track> = self.do_request(Method::GET, &url, Some(params), None).await?;
        Ok(resp)
    }

    /// Get the authenticated user's favorite albums with pagination and sorting.
    ///
    /// # Arguments
    ///
    /// * `offset` - Number of albums to skip (default: 0)
    /// * `limit` - Maximum number of albums to return (default: 100)
    /// * `order` - Sort order (default: Date)
    /// * `order_direction` - Sort direction (default: Desc)
    ///
    /// # Returns
    ///
    /// Returns a paginated list of albums the user has favorited.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let favorites = client.favorite_albums(None, Some(10), None, None).await?;
    /// for favorite in favorites.items {
    ///     println!("Favorite: {}", favorite.item.title);
    /// }
    /// ```
    pub async fn favorite_albums(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<Order>,
        order_direction: Option<OrderDirection>,
    ) -> Result<List<FavoriteAlbum>, Error> {
        let user_id = self.get_user_id().ok_or(Error::UserAuthenticationRequired)?;
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/favorites/albums");

        let params = serde_json::json!({
            "offset": offset,
            "limit": limit,
            "order": order.unwrap_or(Order::Date).as_ref(),
            "orderDirection": order_direction.unwrap_or(OrderDirection::Desc).as_ref(),
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: List<FavoriteAlbum> = self.do_request(Method::GET, &url, Some(params), None).await?;

        Ok(resp)
    }

    /// Add an album to the authenticated user's favorites.
    ///
    /// # Arguments
    ///
    /// * `album_id` - The unique identifier of the album to favorite
    ///
    /// # Example
    ///
    /// ```no_run
    /// client.add_favorite_album(123456789).await?;
    /// println!("Album added to favorites!");
    /// ```
    pub async fn add_favorite_album(
        &self,
        album_id: u64,
    ) -> Result<(), Error> {
        let user_id = self.get_user_id().ok_or(Error::UserAuthenticationRequired)?;
        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/favorites/albums");

        let params = serde_json::json!({
            "albumId": album_id,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let _: Value = self.do_request(Method::POST, &url, Some(params), None).await?;

        Ok(())
    }

    /// Remove an album from the authenticated user's favorites.
    ///
    /// # Arguments
    ///
    /// * `album_id` - The unique identifier of the album to remove from favorites
    ///
    /// # Example
    ///
    /// ```no_run
    /// client.remove_favorite_album(123456789).await?;
    /// println!("Album removed from favorites!");
    /// ```
    pub async fn remove_favorite_album(
        &self,
        album_id: u64,
    ) -> Result<(), Error> {
        let user_id = self.get_user_id().ok_or(Error::UserAuthenticationRequired)?;
        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/favorites/albums/{album_id}");

        let params = serde_json::json!({
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let _: Value = self.do_request(Method::DELETE, &url, Some(params), None).await?;

        Ok(())
    }
}
