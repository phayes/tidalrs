# Changelog

## [0.6.1] - 2026-10-15

### Added

- `TrackDashPlaybackInfo::bts_manifest` decodes the BTS manifest Tidal may send instead of MPEG-DASH, into a `BtsManifest` of plain URLs. It returns the new `Error::NotBtsManifest` for other formats. `BtsManifest::stream` downloads the track like `TrackStream::stream`.

### Changed

- `Track::rodio_source` and `TidalClient::track_rodio_source` play a BTS manifest returned by a DASH request when it is at the requested quality, instead of failing over to a second request for a progressive URL. A BTS manifest at a lower quality than requested still falls back, so a lossless request keeps looking for a lossless stream.

## [0.6.0] - 2026-10-15

### Breaking Changes

- **`Authz`**: `refresh_token` is now `Option<String>` and `user_id` is now `Option<u64>`. Added `expires_timestamp: Option<u64>`. `Authz::new` takes the optional fields, including expiry.
- **`AuthzToken::authz()`**: Now returns `Authz` instead of `Option<Authz>`.
- **`DeviceAuthorizationResponse`**: Added `interval` (RFC 8628 polling interval, default 5 seconds).
- **`Error` enum**: Removed `AuthorizationPending`. Added `ClientSecretRequired`, `PkceNotStarted`, `PkceRedirectMissingCode`, `DeviceAuthorizationTimeout`, `DeviceAuthorizationDenied`, `OAuth { error, description }`, and `Url`. Since the enum is exhaustive, this is a breaking change for code that matches on `Error`.
- **Dependencies**: Aligned with tidalv2 where possible (`reqwest` 0.13, `strum` 0.28, `base64` 0.23, `sha2` 0.11, `rand` 0.10). `stream-download` 0.24.4 is required for reqwest 0.13. The audio example uses `rodio` 0.22.
- **`Error` enum**: Added `NotDashManifest(String)`, plus `Playback(playdash::Error)` (with the `playback` feature) and `Decoder(rodio::decoder::DecoderError)` (with the `rodio` feature).
- **Default features**: The new `playback` feature is on by default. It adds the `playdash` dependency and enables tokio's `rt` feature.
- **`TrackStream::stream`**: Now requires the `playback` feature, which also gates the `stream-download` dependency.

### Added

- OAuth2 PKCE flow: `TidalClient::start_pkce` / `finish_pkce`.
- OAuth2 client-credentials grant: `TidalClient::client_credentials`, `with_client_secret`, and `with_access_token`.
- Built-in device-flow polling: `TidalClient::wait_for_authorization`.
- Isolated auth-token HTTP path so OAuth requests never attach a Bearer token or trigger refresh.
- Automatic token renewal prefers a refresh token and falls back to client credentials when a secret is configured. Refresh attempts are bounded.
- User-specific methods continue to require a user-authenticated session and return `UserAuthenticationRequired` for application tokens.
- Ignored live integration tests that search, walk catalog resources, and walk user collections against the v1 API.
- MPEG-DASH playback through the `playback` feature (on by default), built on `playdash`:
  - `TrackDashPlaybackInfo::dash_manifest` parses the manifest.
  - `TrackDashPlaybackInfo::stream` opens a seekable `Read + Seek` stream of fragmented MP4 (FLAC or AAC).
  - These methods, and `source` below, check `manifest_mime_type` and return `Error::NotDashManifest` when Tidal sent another manifest format.
  - `playdash` is re-exported as `tidalrs::playdash`.
- Rodio playback through the opt-in `rodio` feature, which also enables the symphonia FLAC, AAC and MP4 decoders:
  - `TrackDashPlaybackInfo::source` returns a ready-to-play `playdash::DashSource`.
  - `Track::rodio_source` and `TidalClient::track_rodio_source` return a `TrackSource`. They try the requested quality first, then the other lossless (or lossy) quality, then lower qualities, then higher ones. At each quality they try DASH before a progressive URL, and a lossless request tries both lossless qualities over DASH before any URL. They skip qualities the track's metadata rules out. Authentication, network, server and rate-limit errors are returned immediately instead of triggering a fallback.
  - `TrackSource::audio_quality`, `transport` (`StreamTransport::Dash` or `Url`) and `buffer` report what was opened and how much has downloaded.
- `AudioQuality` now implements `PartialOrd` and `Ord`, ordered from lowest to highest quality.
- `pkce_auth` example for browser login with PKCE, and `audio_streaming_rodio` example (requires the `rodio` feature).

### Changed

- Examples read their configuration from environment variables (`TIDAL_CLIENT_ID`, `TIDAL_CLIENT_SECRET`, `TIDAL_ACCESS_TOKEN`, `TIDAL_REFRESH_TOKEN`, `TIDAL_USER_ID`, `TIDAL_COUNTRY_CODE`, `TIDAL_REDIRECT_URI`) through a shared `examples/common` module instead of using hard-coded placeholders. The authentication examples print ready-to-paste `export` lines.

## [0.5.0] - 2026-08-31

### Breaking Changes

- **Error enum**: Added `Error::AuthorizationPending` variant for OAuth device-flow polling. Since the `Error` enum is exhaustive (not `#[non_exhaustive]`), this is a breaking change for code that exhaustively matches on `Error`. ([#7](https://github.com/phayes/tidalrs/pull/7))
- **Track struct**: Changed `Track.version` field from `String` to `Option<String>` to handle tracks without version information. ([#3](https://github.com/phayes/tidalrs/pull/3))
- **Artist and Album structs**: Changed `Artist.url` and `Album.url` fields from `String` to `Option<String>` to properly handle missing URLs. Fixes [#5](https://github.com/phayes/tidalrs/issues/5). ([#8](https://github.com/phayes/tidalrs/pull/8))

### Added

- `TidalClient::with_auth_base_url()` method to override the authentication base URL for testing or staging environments. ([#7](https://github.com/phayes/tidalrs/pull/7))
- Additional test coverage for OAuth device flow and error handling. ([#7](https://github.com/phayes/tidalrs/pull/7))

## [0.4.1] - 2025-11-24

Initial tracked release on crates.io.
