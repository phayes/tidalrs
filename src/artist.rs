
use crate::Error;
use crate::TIDAL_API_BASE_URL;
use crate::TidalClient;
use crate::Order;
use crate::OrderDirection;
use crate::album::{Album, AlbumType};
use crate::List;
use std::collections::HashMap;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Represents an artist from the Tidal catalog.
///
/// This structure contains all available information about an artist,
/// including profile data, popularity metrics, and associated content.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    /// Unique artist identifier
    pub id: u64,
    /// Artist name
    pub name: String,
    /// Artist profile picture identifier
    /// 
    /// Use picture_url() to get the full URL of the picture
    pub picture: Option<String>,
    /// Tidal URL for the artist
    pub url: String,

    /// The Tidal user ID of the artist, will be None or zero if the artist is not a known user
    pub user_id: Option<u64>,

    /// Popularity score for the artist
    #[serde(default)]
    pub popularity: Option<u32>,

    /// Types/categories associated with the artist
    #[serde(default)]
    pub artist_types: Vec<String>,

    /// Roles the artist has in various contexts
    #[serde(default)]
    pub artist_roles: Vec<ArtistRole>,

    /// Fallback album cover to use when no artist picture is available
    #[serde(default)]
    pub selected_album_cover_fallback: Option<String>,

    /// Mix playlists associated with the artist
    #[serde(default)]
    pub mixes: HashMap<String, String>,

    /// Whether the artist is currently being spotlighted by Tidal
    pub spotlighted: bool
}

impl Artist {
    /// Generate a URL for the artist's profile picture at the specified dimensions.
    ///
    /// If no artist picture is available, falls back to the selected album cover.
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
    pub fn picture_url(&self, height: u16, width: u16) -> Option<String> {
        match &self.picture {
            Some(picture) => {
                let picture_path = picture.replace('-', "/");
                Some(format!("https://resources.tidal.com/images/{picture_path}/{height}x{width}.jpg"))
            }
            None => match &self.selected_album_cover_fallback {
                Some(selected_album_cover_fallback) => {
                    let picture_path = selected_album_cover_fallback.replace('-', "/");
                    Some(format!("https://resources.tidal.com/images/{picture_path}/{height}x{width}.jpg"))
                }
                None => None,
            },
        }
    }
}


#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct FavoriteArtist {
    pub created: String,
    pub item: Artist,
}

/// Represents a role or category that an artist has in the music industry.
///
/// This is used to categorize artists by their function (e.g., "Producer", "Songwriter").
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArtistRole {
    /// Name of the role category
    pub category: String,
    /// Unique identifier for the role category
    pub category_id: i64,
}

/// A simplified representation of an artist used in lists and summaries.
///
/// This structure contains only the basic information about an artist
/// and is commonly used in album credits, track listings, and search results.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArtistSummary {
    /// Unique artist identifier
    pub id: u64,
    /// Artist name
    pub name: String,
    /// Artist profile picture identifier
    /// 
    /// Use picture_url() to get the full URL of the picture
    pub picture: Option<String>,

    /// Whether the artist has cover art available
    #[serde(default)]
    pub contains_cover: bool,

    /// Popularity score for the artist
    #[serde(default)]
    pub popularity: Option<u32>,

    /// Type/category of the artist
    #[serde(rename = "type")]
    #[serde(default)]
    pub artist_type: Option<String>,
}

impl ArtistSummary {
    /// Generate a URL for the artist's profile picture at the specified dimensions.
    ///
    /// # Arguments
    ///
    /// * `height` - Height of the image in pixels
    /// * `width` - Width of the image in pixels
    ///
    /// # Returns
    ///
    /// Returns `Some(String)` with the full URL if a picture is available,
    /// or `None` if no picture is set.
    pub fn picture_url(&self, height: u16, width: u16) -> Option<String> {
        let picture_path = self.picture.as_ref().map(|picture| picture.replace('-', "/"));
        picture_path.map(|picture_path| format!("https://resources.tidal.com/images/{picture_path}/{height}x{width}.jpg"))
    }
}

impl TidalClient {
    /// Get artist information by ID.
    ///
    /// # Arguments
    ///
    /// * `artist_id` - The unique identifier of the artist
    ///
    /// # Returns
    ///
    /// Returns an `Artist` structure with all available metadata.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let artist = client.artist(123456789).await?;
    /// println!("Artist: {}", artist.name);
    /// ```
    pub async fn artist(
        &self,
        artist_id: u64,
    ) -> Result<Artist, Error> {
        let url = format!("{TIDAL_API_BASE_URL}/artists/{artist_id}");
        let params = serde_json::json!({
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });
        let resp: Artist = self.do_request(Method::GET, &url, Some(params), None).await?;
        Ok(resp)
    }

    /// Get the authenticated user's favorite artists with pagination and sorting.
    ///
    /// # Arguments
    ///
    /// * `offset` - Number of artists to skip (default: 0)
    /// * `limit` - Maximum number of artists to return (default: 100)
    /// * `order` - Sort order (default: Date)
    /// * `order_direction` - Sort direction (default: Desc)
    ///
    /// # Returns
    ///
    /// Returns a paginated list of artists the user has favorited.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let favorites = client.favorite_artists(None, Some(10), None, None).await?;
    /// for artist in favorites.items {
    ///     println!("Favorite: {}", artist.name);
    /// }
    /// ```
    pub async fn favorite_artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<Order>,
        order_direction: Option<OrderDirection>,
    ) -> Result<List<FavoriteArtist>, Error> {
        let user_id = self.get_user_id().ok_or(Error::UserAuthenticationRequired)?;
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/favorites/artists");

        let params = serde_json::json!({
            "offset": offset,
            "limit": limit,
            "order": order.unwrap_or(Order::Date).as_ref(),
            "orderDirection": order_direction.unwrap_or(OrderDirection::Desc).as_ref(),
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let resp: List<FavoriteArtist> = self.do_request(Method::GET, &url, Some(params), None).await?;

        Ok(resp)
    }

    /// Get all albums for a specific artist with pagination and filtering.
    ///
    /// # Arguments
    ///
    /// * `artist_id` - The unique identifier of the artist
    /// * `offset` - Number of albums to skip (default: 0)
    /// * `limit` - Maximum number of albums to return (default: 100)
    /// * `album_type` - Filter by album type (optional)
    ///
    /// # Returns
    ///
    /// Returns a paginated list of albums by the specified artist.
    ///
    /// # Example
    ///
    /// ```no_run
    /// let albums = client.artist_albums(123456789, None, Some(20), None).await?;
    /// for album in albums.items {
    ///     println!("Album: {}", album.title);
    /// }
    /// ```
    pub async fn artist_albums(
        &self,
        artist_id: u64,
        album_type: Option<AlbumType>,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<List<Album>, Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        let url = format!("{TIDAL_API_BASE_URL}/artists/{artist_id}/albums");

        let mut params = serde_json::json!({
            "offset": offset,
            "limit": limit,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        if let Some(album_type) = album_type {
            params["filter"] = serde_json::Value::String(album_type.as_ref().to_string());
        }

        let resp: List<Album> = self.do_request(Method::GET, &url, Some(params), None).await?;

        Ok(resp)
    }

    /// Add an artist to the authenticated user's favorites.
    ///
    /// # Arguments
    ///
    /// * `artist_id` - The unique identifier of the artist to favorite
    ///
    /// # Example
    ///
    /// ```no_run
    /// client.add_favorite_artist(123456789).await?;
    /// println!("Artist added to favorites!");
    /// ```
    pub async fn add_favorite_artist(
        &self,
        artist_id: u64,
    ) -> Result<(), Error> {
        let user_id = self.get_user_id().ok_or(Error::UserAuthenticationRequired)?;
        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/favorites/artists");

        let params = serde_json::json!({
            "artistId": artist_id,
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let _: Value = self.do_request(Method::POST, &url, Some(params), None).await?;

        Ok(())
    }

    /// Remove an artist from the authenticated user's favorites.
    ///
    /// # Arguments
    ///
    /// * `artist_id` - The unique identifier of the artist to remove from favorites
    ///
    /// # Example
    ///
    /// ```no_run
    /// client.remove_favorite_artist(123456789).await?;
    /// println!("Artist removed from favorites!");
    /// ```
    pub async fn remove_favorite_artist(
        &self,
        artist_id: u64,
    ) -> Result<(), Error> {
        let user_id = self.get_user_id().ok_or(Error::UserAuthenticationRequired)?;
        let url = format!("{TIDAL_API_BASE_URL}/users/{user_id}/favorites/artists/{artist_id}");

        let params = serde_json::json!({
            "countryCode": self.get_country_code(),
            "locale": self.get_locale(),
            "deviceType": self.get_device_type().as_ref(),
        });

        let _: Value = self.do_request(Method::DELETE, &url, Some(params), None).await?;

        Ok(())
    }
}
