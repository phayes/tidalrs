use crate::AudioQuality;
use crate::Error;
use crate::List;
use crate::MediaMetadata;
use crate::Order;
use crate::OrderDirection;
use crate::TIDAL_API_BASE_URL;
use crate::TidalClient;
use crate::artist::ArtistSummary;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stream_download::storage::memory::MemoryStorageProvider;
use stream_download::{Settings, StreamDownload};

/// Represents a track from the Tidal catalog.
///
/// This structure contains all available information about a track,
/// including metadata, audio quality, and associated album/artist data.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Unique track identifier
    pub id: u64,
    /// Track number within the album
    pub track_number: u32,
    /// List of artists who contributed to this track
    #[serde(default = "Default::default")]
    pub artists: Vec<ArtistSummary>,

    /// Album information for this track
    pub album: AlbumSummary,

    /// Audio quality level available for this album for standard streaming
    ///
    /// Higher quality streams may be available than is indicated here when using MPEG-DASH for playback.
    pub audio_quality: AudioQuality,

    /// Duration of the track in seconds
    pub duration: u32,

    /// Whether the track contains explicit content
    pub explicit: bool,

    /// International Standard Recording Code (ISRC)
    pub isrc: Option<String>,
    /// Popularity score for the track
    pub popularity: u32,
    /// Track title
    pub title: String,
    /// Version or remix information
    pub version: Option<String>,

    /// Additional media metadata and tags
    #[serde(rename = "mediaMetadata")]
    pub media_metadata: Option<MediaMetadata>,

    /// Copyright information
    pub copyright: Option<String>,
    /// Tidal URL for the track
    pub url: Option<String>,
    /// Beats per minute (BPM) of the track
    pub bpm: Option<u32>,

    pub upload: Option<bool>,
}

/// A simplified representation of an album used in track listings.
///
/// This structure contains only the basic album information
/// and is commonly used in track metadata and search results.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumSummary {
    /// Unique album identifier
    pub id: u64,
    /// Album title
    pub title: String,
    /// Album cover image identifier
    pub cover: Option<String>,
    /// Album release date
    pub release_date: Option<String>,
    /// Dominant color extracted from the cover art
    pub vibrant_color: Option<String>,
    /// Video cover identifier (if available)
    pub video_cover: Option<String>,
}

/// Represents a track that has been added to a user's favorites.
///
/// This structure includes the track data along with metadata
/// about when it was added to favorites.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteTrack {
    /// ISO timestamp when the track was added to favorites
    pub created: String,
    /// The track data
    pub item: Track,
}

/// A suggested track from a track recommendations response.
///
/// This structure wraps a recommended track along with information
/// about the sources that suggested it.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SuggestedTrack {
    /// The recommended track
    pub track: Track,
    /// Sources that suggested this track (e.g., "SUGGESTED_TRACKS")
    pub sources: Vec<String>,
}

impl TidalClient {
    /// Get streaming information for a track at the specified audio quality.
    ///
    /// This method retrieves the streaming URLs and metadata needed to
    /// play a track at the requested quality level.
    ///
    /// # Arguments
    ///
    /// * `track_id` - The unique identifier of the track
    /// * `audio_quality` - The desired audio quality level
    ///
    /// # Returns
    ///
    /// Returns a `TrackStream` containing streaming URLs and metadata.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let stream = client.track_stream(123456789, tidalrs::AudioQuality::Lossless).await?;
    /// println!("Stream URL: {}", stream.primary_url().unwrap());
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub async fn track_stream(
        &self,
        track_id: u64,
        audio_quality: AudioQuality,
    ) -> Result<TrackStream, Error> {
        let url = format!("{TIDAL_API_BASE_URL}/tracks/{track_id}/urlpostpaywall");

        let audio_quality = match audio_quality {
            AudioQuality::Low => "LOW",
            AudioQuality::High => "HIGH",
            AudioQuality::Lossless => "LOSSLESS",
            AudioQuality::HiResLossless => "HI_RES_LOSSLESS", // HI_RES_LOSSLESS here, but HIRES_LOSSLESS elsewhere
        };

        let params = serde_json::json!({
            "audioquality": audio_quality,
            "urlusagemode": "STREAM",
            "assetpresentation": "FULL"
        });

        let resp: TrackStream = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        Ok(resp)
    }

    /// Get track information by ID.
    ///
    /// # Arguments
    ///
    /// * `track_id` - The unique identifier of the track
    ///
    /// # Returns
    ///
    /// Returns a `Track` structure with all available metadata.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let track = client.track(123456789).await?;
    /// println!("Track: {} by {}", track.title, track.artists[0].name);
    /// ```
    pub async fn track(&self, track_id: u64) -> Result<Track, Error> {
        let url = format!("{TIDAL_API_BASE_URL}/tracks/{track_id}");

        let params = serde_json::json!({
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: Track = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        Ok(resp)
    }

    /// Get recommended tracks for a specific track with pagination support.
    ///
    /// This method retrieves tracks that Tidal recommends based on the
    /// specified track's characteristics and user preferences.
    ///
    /// # Arguments
    ///
    /// * `track_id` - The unique identifier of the track
    /// * `offset` - Number of recommendations to skip (default: 0)
    /// * `limit` - Maximum number of recommendations to return (default: 20)
    ///
    /// # Returns
    ///
    /// Returns a paginated list of recommended tracks.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let recommendations = client.track_recommendations(123456789, Some(0), Some(20)).await?;
    /// for track in recommendations.items {
    ///     println!("Suggested: {} by {}", 
    ///         track.title,
    ///         track.artists[0].name
    ///     );
    /// }
    /// ```
    pub async fn track_recommendations(
        &self,
        track_id: u64,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<List<Track>, Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(5);
        let url = format!("{TIDAL_API_BASE_URL}/tracks/{track_id}/recommendations");
        let params = serde_json::json!({
            "offset": offset,
            "limit": limit,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: List<SuggestedTrack> = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        let tracks = List {
            items: resp.items.into_iter().map(|s| s.track).collect(),
            offset: resp.offset,
            limit: resp.limit,
            total: resp.total,
            etag: resp.etag,
        };

        Ok(tracks)
    }

    /// Get detailed playback information for a track.
    ///
    /// This method provides technical details about the track's audio
    /// including manifest information, replay gain, and peak amplitude.
    ///
    /// # Arguments
    ///
    /// * `track_id` - The unique identifier of the track
    /// * `audio_quality` - The desired audio quality level
    ///
    /// # Returns
    ///
    /// Returns a `TrackPlaybackInfo` containing technical playback metadata.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let playback_info = client.track_playback_info(123456789, tidalrs::AudioQuality::Lossless).await?;
    /// println!("Sample rate: {} Hz", playback_info.sample_rate.unwrap_or(0));
    /// ```
    pub async fn track_playback_info(
        &self,
        track_id: u64,
        audio_quality: AudioQuality,
    ) -> Result<TrackPlaybackInfo, Error> {
        let url = format!("{TIDAL_API_BASE_URL}/tracks/{track_id}/playbackinfo");

        let params = serde_json::json!({
            "audioquality": audio_quality.as_ref(),
            "playbackmode": "STREAM",
            "assetpresentation": "FULL"
        });

        let resp: TrackPlaybackInfo = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        Ok(resp)
    }

    /// Get DASH playback information for a track.
    ///
    /// This method provides DASH-specific playback information including
    /// manifest data and audio quality details for streaming.
    ///
    /// # Arguments
    ///
    /// * `track_id` - The unique identifier of the track
    /// * `audio_quality` - The desired audio quality level
    ///
    /// # Returns
    ///
    /// Returns a `TrackDashPlaybackInfo` containing DASH streaming metadata.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let dash_info = client.track_dash_playback_info(123456789, tidalrs::AudioQuality::Lossless).await?;
    /// let manifest = dash_info.unpack_manifest()?;
    /// println!("DASH manifest: {}", manifest);
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub async fn track_dash_playback_info(
        &self,
        track_id: u64,
        audio_quality: AudioQuality,
    ) -> Result<TrackDashPlaybackInfo, Error> {
        let url = format!("{TIDAL_API_BASE_URL}/tracks/{track_id}/playbackinfopostpaywall");

        let audio_quality = match audio_quality {
            AudioQuality::Low => "LOW",
            AudioQuality::High => "HIGH",
            AudioQuality::Lossless => "LOSSLESS",
            AudioQuality::HiResLossless => "HI_RES_LOSSLESS", // HI_RES_LOSSLESS here, but HIRES_LOSSLESS elsewhere
        };

        let params = serde_json::json!({
            "audioquality": audio_quality,
            "playbackmode": "STREAM",
            "assetpresentation": "FULL",
            "countryCode": self.get_country_code(),
        });

        let resp: TrackDashPlaybackInfo = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        Ok(resp)
    }

    /// Get the authenticated user's favorite tracks with pagination and sorting.
    ///
    /// # Arguments
    ///
    /// * `offset` - Number of tracks to skip (default: 0)
    /// * `limit` - Maximum number of tracks to return (default: 100)
    /// * `order` - Sort order (default: Date)
    /// * `order_direction` - Sort direction (default: Desc)
    ///
    /// # Returns
    ///
    /// Returns a paginated list of tracks the user has favorited.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let favorites = client.favorite_tracks(None, Some(10), None, None).await?;
    /// for favorite in favorites.items {
    ///     println!("Favorite: {}", favorite.item.title);
    /// }
    /// ```
    pub async fn favorite_tracks(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<Order>,
        order_direction: Option<OrderDirection>,
    ) -> Result<List<FavoriteTrack>, Error> {
        let user_id = self
            .get_user_id()
            .ok_or(Error::UserAuthenticationRequired)?;
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/favorites/tracks");

        let params = serde_json::json!({
            "offset": offset,
            "limit": limit,
            "order": order.unwrap_or(Order::Date).as_ref(),
            "orderDirection": order_direction.unwrap_or(OrderDirection::Desc).as_ref(),
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: List<FavoriteTrack> = self
            .do_request(Method::GET, &url, Some(params), None)
            .await?;

        Ok(resp)
    }

    /// Add a track to the authenticated user's favorites.
    ///
    /// # Arguments
    ///
    /// * `track_id` - The unique identifier of the track to favorite
    ///
    /// # Example
    ///
    /// ```no_run
    /// client.add_favorite_track(123456789).await?;
    /// println!("Track added to favorites!");
    /// ```
    pub async fn add_favorite_track(&self, track_id: u64) -> Result<(), Error> {
        let user_id = self
            .get_user_id()
            .ok_or(Error::UserAuthenticationRequired)?;
        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/favorites/tracks");

        let params = serde_json::json!({
            "trackId": track_id,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let _: Value = self
            .do_request(Method::POST, &url, Some(params), None)
            .await?;

        Ok(())
    }

    /// Remove a track from the authenticated user's favorites.
    ///
    /// # Arguments
    ///
    /// * `track_id` - The unique identifier of the track to remove from favorites
    ///
    /// # Example
    ///
    /// ```no_run
    /// client.remove_favorite_track(123456789).await?;
    /// println!("Track removed from favorites!");
    /// ```
    pub async fn remove_favorite_track(&self, track_id: u64) -> Result<(), Error> {
        let user_id = self
            .get_user_id()
            .ok_or(Error::UserAuthenticationRequired)?;
        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/favorites/tracks/{track_id}");

        let params = serde_json::json!({
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let _: Value = self
            .do_request(Method::DELETE, &url, Some(params), None)
            .await?;

        Ok(())
    }
}

/// Streaming information for a track.
///
/// This structure contains all the data needed to stream a track,
/// including URLs, codec information, and security tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackStream {
    /// Asset presentation format
    pub asset_presentation: String,
    /// Audio mode (e.g., "STEREO", "SURROUND")
    pub audio_mode: String,
    /// Audio quality level
    pub audio_quality: AudioQuality,
    /// Audio codec used for streaming
    pub codec: String,
    /// Security token for accessing the stream
    pub security_token: Option<String>,
    /// Type of security applied to the stream
    pub security_type: Option<String>,
    /// Session ID for the streaming session
    pub streaming_session_id: Option<String>,
    /// Track identifier
    pub track_id: u64,
    /// List of streaming URLs (primary URL is typically first)
    pub urls: Vec<String>,
}

/// Playback information for a track.
///
/// This structure contains technical audio information including
/// replay gain, peak amplitude, and manifest data.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrackPlaybackInfo {
    /// Peak amplitude for the entire album
    pub album_peak_amplitude: f64,
    /// Replay gain adjustment for the album
    pub album_replay_gain: f64,
    /// Asset presentation format
    pub asset_presentation: String,
    /// Audio mode (e.g., "STEREO", "SURROUND")
    pub audio_mode: String,
    /// Audio quality as a string
    pub audio_quality: String,
    /// Bit depth of the audio (if available)
    pub bit_depth: Option<u8>,
    /// Base64-encoded manifest data
    ///
    /// Use unpack_manifest() to get the decoded manifest. Crates that support MPEG-DASH playback should be able to use this manifest to play the track.
    pub manifest: String,
    /// Hash of the manifest for verification
    pub manifest_hash: String,
    /// MIME type of the manifest
    pub manifest_mime_type: String,
    /// Sample rate in Hz (if available)
    pub sample_rate: Option<u32>,
    /// Track identifier
    pub track_id: u64,
    /// Peak amplitude for this specific track
    pub track_peak_amplitude: f64,
    /// Replay gain adjustment for this track
    pub track_replay_gain: f64,
}

/// DASH-specific playback information for a track.
///
/// This structure contains DASH streaming metadata including
/// manifest information and audio quality details.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrackDashPlaybackInfo {
    /// Peak amplitude for the entire album
    pub album_peak_amplitude: f64,
    /// Replay gain adjustment for the album
    pub album_replay_gain: f64,
    /// Asset presentation format
    pub asset_presentation: String,
    /// Audio mode (e.g., "STEREO", "SURROUND")
    pub audio_mode: String,
    /// Audio quality level
    pub audio_quality: AudioQuality,
    /// Bit depth of the audio - may be None if format is lossy
    pub bit_depth: Option<u32>,
    /// Base64-encoded manifest data
    ///
    /// Use unpack_manifest() to get the decoded manifest. Crates that support MPEG-DASH playback should be able to use this manifest to play the track.
    pub manifest: String,
    /// Hash of the manifest for verification
    pub manifest_hash: String,
    /// MIME type of the manifest
    pub manifest_mime_type: String,
    /// Sample rate in Hz - may be None if format is lossy
    pub sample_rate: Option<u32>,
    /// Track identifier
    pub track_id: u64,
    /// Peak amplitude for this specific track
    pub track_peak_amplitude: f64,
    /// Replay gain adjustment for this track
    pub track_replay_gain: f64,
}

impl TrackPlaybackInfo {
    /// Decode the base64-encoded manifest, which may be either XML or JSON string
    ///
    /// # Returns
    ///
    /// Returns the decoded manifest as a UTF-8 string.
    ///
    /// # Errors
    ///
    /// Returns a `base64::DecodeError` if the manifest cannot be decoded.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let dash_info = client.track_dash_playback_info(123456789, tidalrs::AudioQuality::Lossless).await?;
    /// let manifest = dash_info.unpack_manifest()?;
    /// println!("DASH manifest: {}", manifest);
    /// ```
    pub fn unpack_manifest(&self) -> Result<String, base64::DecodeError> {
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD.decode(self.manifest.as_bytes())?;

        Ok(String::from_utf8(decoded).expect("tidalrs: Failed to decode manifest"))
    }
}

impl TrackDashPlaybackInfo {
    /// Decode the base64-encoded DASH manifest.
    ///
    /// # Returns
    ///
    /// Returns the decoded manifest as a UTF-8 string.
    ///
    /// # Errors
    ///
    /// Returns a `base64::DecodeError` if the manifest cannot be decoded.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let dash_info = client.track_dash_playback_info(123456789, tidalrs::AudioQuality::Lossless).await?;
    /// let manifest = dash_info.unpack_manifest()?;
    /// println!("DASH manifest: {}", manifest);
    /// ```
    pub fn unpack_manifest(&self) -> Result<String, base64::DecodeError> {
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD.decode(self.manifest.as_bytes())?;

        Ok(String::from_utf8(decoded).expect("Failed to decode manifest, not UTF-8 XML"))
    }
}

impl TrackStream {
    /// Get the primary streaming URL for the track.
    ///
    /// # Returns
    ///
    /// Returns the first URL from the URLs list, which is typically
    /// the primary streaming endpoint.
    pub fn primary_url(&self) -> Option<&str> {
        self.urls.get(0).map(|s| s.as_str())
    }

    /// Get a buffered, seekable stream of the track.
    ///
    /// This method downloads the track to memory and provides a seekable
    /// stream that can be used with audio libraries like rodio.
    ///
    /// While this function is async, the returned stream is sync.
    ///
    /// # Returns
    ///
    /// Returns a `StreamDownload` that can be used to read the audio data.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let track_stream = client.track_stream(123456789, tidalrs::AudioQuality::Lossless).await?;
    /// let stream = track_stream.stream().await?;
    ///
    /// tokio::task::spawn_blocking(move || {
    ///     let device_handle = rodio::OutputStreamBuilder::open_default_stream().unwrap();
    ///     let sink = rodio::Sink::connect_new(device_handle.mixer());
    ///     sink.append(rodio::Decoder::new(stream).unwrap());
    ///     sink.play();
    ///     sink.sleep_until_end();
    /// })
    /// .await
    /// .unwrap();
    /// ```
    pub async fn stream(&self) -> Result<StreamDownload<MemoryStorageProvider>, Error> {
        let url: reqwest::Url = match self.primary_url() {
            Some(url) => url.parse().expect("Failed to parse stream URL"),
            None => return Err(Error::NoPrimaryUrl),
        };

        let reader =
            match StreamDownload::new_http(url, MemoryStorageProvider, Settings::default()).await {
                Ok(reader) => reader,
                Err(e) => {
                    return Err(Error::StreamInitializationError(e.to_string()));
                }
            };

        Ok(reader)
    }
}
