# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
