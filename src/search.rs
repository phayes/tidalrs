use crate::Error;
use crate::ResourceType;
use crate::TIDAL_API_BASE_URL;
use crate::TidalClient;
use crate::album::Album;
use crate::artist::Artist;
use crate::track::Track;
use crate::Playlist;
use crate::List;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::Resource;

/// A search query for finding content in the Tidal catalog.
///
/// This structure contains all the parameters needed to perform
/// a search operation with various filtering and pagination options.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchQuery<'a> {
    /// The search query string
    pub query: &'a str,
    /// Number of results to skip (for pagination)
    pub offset: Option<u32>,
    /// Maximum number of results to return
    pub limit: Option<u32>,
    /// Whether to include contribution information in results
    pub include_contributions: Option<bool>,
    /// Whether to include "did you mean" suggestions
    pub include_did_you_mean: Option<bool>,
    /// Whether to include user-created playlists in results
    pub include_user_playlists: Option<bool>,
    /// Whether the search supports user-specific data
    pub supports_user_data: Option<bool>,
    /// Types of content to search for
    pub search_types: Option<Vec<ResourceType>>,
}

impl<'a> SearchQuery<'a> {
    /// Create a new search query with the specified search string.
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tidalrs::SearchQuery;
    ///
    /// let search = SearchQuery::new("The Beatles");
    /// ```
    pub fn new(query: &'a str) -> Self {
        Self {
            query,
            offset: None,
            limit: None,
            include_contributions: None,
            include_did_you_mean: None,
            include_user_playlists: None,
            supports_user_data: None,
            search_types: None,
        }
    }
}

impl TidalClient {
    /// Search for content in the Tidal catalog.
    ///
    /// This method performs a search across multiple content types
    /// and returns results organized by type.
    ///
    /// # Arguments
    ///
    /// * `search` - The search query parameters
    ///
    /// # Returns
    ///
    /// Returns a `SearchResults` structure containing all matching content
    /// organized by type (artists, albums, tracks, playlists, etc.).
    ///
    /// # Example
    ///
    /// ```no_run
    /// let search_query = tidalrs::SearchQuery::new("The Beatles");
    /// let results = client.search(search_query).await?;
    /// 
    /// for artist in results.artists.items {
    ///     println!("Artist: {}", artist.name);
    /// }
    /// for album in results.albums.items {
    ///     println!("Album: {}", album.title);
    /// }
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub async fn search<'a>(&self, search: SearchQuery<'a>) -> Result<SearchResults, Error> {
        let url = format!("{TIDAL_API_BASE_URL}/search/top-hits");

        let mut params = serde_json::json!({ "query": search.query });

        // Requires fields:
        let search_types_string = {
            match search.search_types {
                Some(types) => {
                    let mut types_str = String::new();
                    for resource_type in types {
                        match resource_type {
                            ResourceType::Artist => types_str.push_str("ARTISTS"),
                            ResourceType::Album => types_str.push_str("ALBUMS"),
                            ResourceType::Track => types_str.push_str("TRACKS"),
                            ResourceType::Video => types_str.push_str("VIDEOS"),
                            ResourceType::Playlist => types_str.push_str("PLAYLISTS"),
                            ResourceType::UserProfile => types_str.push_str("USER_PROFILES"),
                        }
                        types_str.push(',');
                    }
                    types_str.pop();
                    types_str
                }
                None => "ARTISTS,ALBUMS,TRACKS,PLAYLISTS".to_string(),
            }
        };
        params["types"] = Value::String(search_types_string.clone());
        params["countryCode"] = Value::String(self.get_country_code());
        params["locale"] = Value::String(self.get_locale());
        params["deviceType"] = Value::String(self.get_device_type().as_ref().to_string());

        // Optional fields:
        if let Some(offset) = search.offset {
            params["offset"] = Value::Number(offset.into());
        }
        if let Some(limit) = search.limit {
            params["limit"] = Value::Number(limit.into());
        }
        if let Some(include_contributions) = search.include_contributions {
            params["includeContributions"] = Value::Bool(include_contributions);
        }
        if let Some(include_did_you_mean) = search.include_did_you_mean {
            params["includeDidYouMean"] = Value::Bool(include_did_you_mean);
        }
        if let Some(include_user_playlists) = search.include_user_playlists {
            params["includeUserPlaylists"] = Value::Bool(include_user_playlists);
        }
        if let Some(supports_user_data) = search.supports_user_data {
            params["supportsUserData"] = Value::Bool(supports_user_data);
        }

        let resp: SearchResults = self.do_request(Method::GET, &url, Some(params), None).await?;

        Ok(resp)
    }
}



/// Results from a search operation in the Tidal catalog.
///
/// This structure contains all matching content organized by type,
/// along with pagination information and top hits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    /// Matching albums
    #[serde(skip_serializing_if = "List::is_empty")]
    #[serde(default)]
    pub albums: List<Album>,

    /// Matching artists
    #[serde(skip_serializing_if = "List::is_empty")]
    #[serde(default)]
    pub artists: List<Artist>,
    
    /// Matching tracks
    #[serde(skip_serializing_if = "List::is_empty")]
    #[serde(default)]
    pub tracks: List<Track>,

    /// Matching playlists
    #[serde(skip_serializing_if = "List::is_empty")]
    #[serde(default)]
    pub playlists: List<Playlist>,

    /// Matching user profiles (currently as raw JSON)
    #[serde(skip_serializing_if = "List::is_empty")]
    #[serde(default)]
    pub user_profiles: List<serde_json::Value>,

    /// Matching videos (currently as raw JSON)
    #[serde(skip_serializing_if = "List::is_empty")]
    #[serde(default)]
    pub videos: List<serde_json::Value>,

    /// Top hits across all content types
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(default)]
    #[serde(rename = "topHits")]
    pub top_hits: Vec<Resource>,
}
