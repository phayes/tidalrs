//! Ready-to-play rodio sources that fall back across audio qualities and
//! streaming transports.

use crate::AudioQuality;
use crate::Error;
use crate::TidalClient;
use crate::Track;
use crate::track::spawn_playback;
use rodio::source::SeekError;
use rodio::{ChannelCount, Sample, SampleRate, Source};
use std::fmt;
use std::time::Duration;
use stream_download::StreamDownload;
use stream_download::storage::memory::MemoryStorageProvider;

/// Every quality, from lowest to highest.
const QUALITIES: [AudioQuality; 4] = [
    AudioQuality::Low,
    AudioQuality::High,
    AudioQuality::Lossless,
    AudioQuality::HiResLossless,
];

/// How a [`TrackSource`] receives its audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamTransport {
    /// MPEG-DASH, fetched fragment by fragment.
    Dash,
    /// A single progressive download from a streaming URL.
    Url,
}

/// A track opened for rodio playback.
///
/// Created by [`TidalClient::track_rodio_source`] or [`Track::rodio_source`],
/// which may fall back to a different quality or transport than requested.
/// Use [`TrackSource::audio_quality`] and [`TrackSource::transport`] to see
/// what was actually opened.
pub struct TrackSource {
    audio_quality: AudioQuality,
    inner: TrackSourceInner,
}

enum TrackSourceInner {
    Dash(playdash::DashSource),
    Url(rodio::Decoder<StreamDownload<MemoryStorageProvider>>),
}

impl TrackSource {
    /// The quality Tidal served, which may differ from the one requested.
    pub fn audio_quality(&self) -> AudioQuality {
        self.audio_quality
    }

    /// How the audio is being streamed.
    pub fn transport(&self) -> StreamTransport {
        match self.inner {
            TrackSourceInner::Dash(_) => StreamTransport::Dash,
            TrackSourceInner::Url(_) => StreamTransport::Url,
        }
    }

    /// A handle reporting download progress, for DASH streams only.
    ///
    /// Get it before handing the source to rodio; it stays usable from any
    /// thread afterwards.
    pub fn buffer(&self) -> Option<playdash::BufferHandle> {
        match &self.inner {
            TrackSourceInner::Dash(source) => Some(source.buffer()),
            TrackSourceInner::Url(_) => None,
        }
    }
}

impl fmt::Debug for TrackSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrackSource")
            .field("audio_quality", &self.audio_quality)
            .field("transport", &self.transport())
            .finish_non_exhaustive()
    }
}

impl Iterator for TrackSource {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        match &mut self.inner {
            TrackSourceInner::Dash(source) => source.next(),
            TrackSourceInner::Url(source) => source.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.inner {
            TrackSourceInner::Dash(source) => source.size_hint(),
            TrackSourceInner::Url(source) => source.size_hint(),
        }
    }
}

impl Source for TrackSource {
    fn current_span_len(&self) -> Option<usize> {
        match &self.inner {
            TrackSourceInner::Dash(source) => source.current_span_len(),
            TrackSourceInner::Url(source) => source.current_span_len(),
        }
    }

    fn is_exhausted(&self) -> bool {
        match &self.inner {
            TrackSourceInner::Dash(source) => source.is_exhausted(),
            TrackSourceInner::Url(source) => source.is_exhausted(),
        }
    }

    fn channels(&self) -> ChannelCount {
        match &self.inner {
            TrackSourceInner::Dash(source) => source.channels(),
            TrackSourceInner::Url(source) => source.channels(),
        }
    }

    fn sample_rate(&self) -> SampleRate {
        match &self.inner {
            TrackSourceInner::Dash(source) => source.sample_rate(),
            TrackSourceInner::Url(source) => source.sample_rate(),
        }
    }

    fn total_duration(&self) -> Option<Duration> {
        match &self.inner {
            TrackSourceInner::Dash(source) => source.total_duration(),
            TrackSourceInner::Url(source) => source.total_duration(),
        }
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        match &mut self.inner {
            TrackSourceInner::Dash(source) => source.try_seek(pos),
            TrackSourceInner::Url(source) => source.try_seek(pos),
        }
    }
}

impl TidalClient {
    /// Open a track for rodio playback, falling back across qualities and
    /// transports until one works.
    ///
    /// Fetches the track's metadata, then behaves like [`Track::rodio_source`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = tidalrs::TidalClient::new("client_id".to_string());
    /// let source = client
    ///     .track_rodio_source(123456789, tidalrs::AudioQuality::Lossless)
    ///     .await?;
    /// println!("Playing {:?} over {:?}", source.audio_quality(), source.transport());
    ///
    /// tokio::task::spawn_blocking(move || {
    ///     let handle = rodio::DeviceSinkBuilder::open_default_sink().unwrap();
    ///     let player = rodio::Player::connect_new(handle.mixer());
    ///     player.append(source);
    ///     player.sleep_until_end();
    /// })
    /// .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn track_rodio_source(
        &self,
        track_id: u64,
        audio_quality: AudioQuality,
    ) -> Result<TrackSource, Error> {
        self.track(track_id)
            .await?
            .rodio_source(self, audio_quality)
            .await
    }
}

impl Track {
    /// Open this track for rodio playback.
    ///
    /// Plays at `audio_quality` when it's available, and otherwise falls back
    /// to the nearest quality that is. It stays lossless (or lossy) when it
    /// can, and otherwise prefers lower over higher. Check
    /// [`TrackSource::audio_quality`] to see what you got.
    ///
    /// # Errors
    ///
    /// Fails if no quality can be played, or straight away on authentication,
    /// network or rate-limit errors.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = tidalrs::TidalClient::new("client_id".to_string());
    /// let track = client.track(123456789).await?;
    /// let source = track
    ///     .rodio_source(&client, tidalrs::AudioQuality::HiResLossless)
    ///     .await?;
    ///
    /// if source.audio_quality() != tidalrs::AudioQuality::HiResLossless {
    ///     println!("Fell back to {:?}", source.audio_quality());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn rodio_source(
        &self,
        client: &TidalClient,
        audio_quality: AudioQuality,
    ) -> Result<TrackSource, Error> {
        // Plan which streams to try from the track's metadata.
        let tags = self
            .media_metadata
            .as_ref()
            .map(|metadata| metadata.tags.as_slice())
            .unwrap_or_default();
        let attempts = plan_attempts(audio_quality, self.audio_quality, tags);

        // Try each in turn, moving on only when that stream is unavailable.
        let mut first_error = None;
        for (quality, transport) in attempts {
            match open(client, self.id, quality, transport).await {
                Ok(source) => return Ok(source),
                Err(error) if is_stream_error(&error) => {
                    log::debug!(
                        "Track {} unavailable at {quality:?} over {transport:?}: {error}",
                        self.id
                    );
                    first_error.get_or_insert(error);
                }
                Err(error) => return Err(error),
            }
        }

        // Nothing played; report why the preferred stream failed. `plan_attempts`
        // always includes `Low` over a URL, so an error was recorded.
        Err(first_error.unwrap_or(Error::TrackQualityNotAvailable))
    }
}

async fn open(
    client: &TidalClient,
    track_id: u64,
    audio_quality: AudioQuality,
    transport: StreamTransport,
) -> Result<TrackSource, Error> {
    match transport {
        StreamTransport::Dash => {
            let info = client
                .track_dash_playback_info(track_id, audio_quality)
                .await?;

            // Tidal may send a BTS manifest of plain URLs instead, sometimes at
            // a lower quality than requested. Play it only if it isn't.
            match info.bts_manifest() {
                Ok(_) if info.audio_quality < audio_quality => Err(Error::TrackQualityNotAvailable),
                Ok(bts) => Ok(TrackSource {
                    audio_quality: info.audio_quality,
                    inner: TrackSourceInner::Url(decode(bts.stream().await?).await?),
                }),
                Err(Error::NotBtsManifest(_)) => Ok(TrackSource {
                    audio_quality: info.audio_quality,
                    inner: TrackSourceInner::Dash(info.source().await?),
                }),
                Err(error) => Err(error),
            }
        }
        StreamTransport::Url => {
            let stream = client.track_stream(track_id, audio_quality).await?;
            let decoder = decode(stream.stream().await?).await?;

            Ok(TrackSource {
                audio_quality: stream.audio_quality,
                inner: TrackSourceInner::Url(decoder),
            })
        }
    }
}

/// Decode a progressive download for rodio.
async fn decode(
    download: StreamDownload<MemoryStorageProvider>,
) -> Result<rodio::Decoder<StreamDownload<MemoryStorageProvider>>, Error> {
    let byte_len = download.content_length();
    // Probing the format reads from the download, so it blocks.
    spawn_playback(move || {
        let builder = rodio::decoder::DecoderBuilder::new().with_data(download);
        // Rodio needs the length to seek.
        let builder = match byte_len {
            Some(len) => builder.with_byte_len(len).with_seekable(true),
            None => builder,
        };
        Ok(builder.build()?)
    })
    .await
}

/// The attempts to make for `requested`, in order, skipping those the track's
/// metadata rules out.
///
/// Qualities follow [`fallback_order`], and each is tried over DASH before a
/// URL. A lossless request tries every lossless quality over DASH before any
/// URL. A DASH attempt may play over a URL when Tidal sends a BTS manifest.
/// `url_max` is the track's `audio_quality`, the best it offers over a
/// URL. DASH can go higher, up to the best quality named in `tags`.
fn plan_attempts(
    requested: AudioQuality,
    url_max: AudioQuality,
    tags: &[String],
) -> Vec<(AudioQuality, StreamTransport)> {
    let dash_max = tags
        .iter()
        .filter_map(|tag| match tag.as_str() {
            "HIRES_LOSSLESS" => Some(AudioQuality::HiResLossless),
            "LOSSLESS" => Some(AudioQuality::Lossless),
            _ => None,
        })
        .fold(url_max, Ord::max);
    let dash = |quality| (quality <= dash_max).then_some((quality, StreamTransport::Dash));
    let url = |quality| (quality <= url_max).then_some((quality, StreamTransport::Url));

    // Tidal may answer a lossless URL request with lossy audio, so a lossless
    // request tries the lossless qualities over DASH first.
    let (lossless, rest): (Vec<_>, Vec<_>) = fallback_order(requested)
        .partition(|&quality| is_lossless(requested) && is_lossless(quality));

    let rest = rest
        .into_iter()
        .flat_map(|quality| [dash(quality), url(quality)]);
    lossless
        .iter()
        .copied()
        .map(dash)
        .chain(lossless.iter().copied().map(url))
        .chain(rest)
        .flatten()
        .collect()
}

/// `requested`, then the other qualities that are likewise lossless or lossy,
/// then the rest. Within each group, lower qualities come first from highest
/// to lowest, then higher ones from lowest to highest.
fn fallback_order(requested: AudioQuality) -> impl Iterator<Item = AudioQuality> {
    let lower = QUALITIES.into_iter().rev().filter(|&q| q < requested);
    let higher = QUALITIES.into_iter().filter(|&q| q > requested);
    let (same_kind, other_kind): (Vec<_>, Vec<_>) = lower
        .chain(higher)
        .partition(|&q| is_lossless(q) == is_lossless(requested));

    std::iter::once(requested)
        .chain(same_kind)
        .chain(other_kind)
}

fn is_lossless(quality: AudioQuality) -> bool {
    quality >= AudioQuality::Lossless
}

/// Whether `error` is specific to one stream, so another quality or transport
/// might still work.
fn is_stream_error(error: &Error) -> bool {
    match error {
        Error::TidalApiError(api) => {
            (400..500).contains(&api.status) && api.status != 429 && !api.is_expired_token()
        }
        Error::Playback(_)
        | Error::NotDashManifest(_)
        | Error::Decoder(_)
        | Error::StreamInitializationError(_)
        | Error::NoPrimaryUrl
        | Error::TrackQualityNotAvailable => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TidalApiError;
    use AudioQuality::*;
    use StreamTransport::*;

    fn tags(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|tag| tag.to_string()).collect()
    }

    fn api_error(status: u16, sub_status: u64) -> Error {
        Error::TidalApiError(TidalApiError {
            status,
            sub_status,
            user_message: String::new(),
        })
    }

    #[test]
    fn fallback_order_stays_lossless_or_lossy_then_goes_down_then_up() {
        let order = |q| fallback_order(q).collect::<Vec<_>>();
        assert_eq!(order(Low), [Low, High, Lossless, HiResLossless]);
        assert_eq!(order(High), [High, Low, Lossless, HiResLossless]);
        assert_eq!(order(Lossless), [Lossless, HiResLossless, High, Low]);
        assert_eq!(order(HiResLossless), [HiResLossless, Lossless, High, Low]);
    }

    #[test]
    fn plan_tries_dash_before_url_at_each_quality() {
        let plan = plan_attempts(High, HiResLossless, &[]);
        assert_eq!(
            plan,
            [
                (High, Dash),
                (High, Url),
                (Low, Dash),
                (Low, Url),
                (Lossless, Dash),
                (Lossless, Url),
                (HiResLossless, Dash),
                (HiResLossless, Url),
            ]
        );
    }

    #[test]
    fn plan_tries_lossless_over_dash_first_for_lossless_requests() {
        let plan = plan_attempts(Lossless, HiResLossless, &[]);
        assert_eq!(
            plan,
            [
                (Lossless, Dash),
                (HiResLossless, Dash),
                (Lossless, Url),
                (HiResLossless, Url),
                (High, Dash),
                (High, Url),
                (Low, Dash),
                (Low, Url),
            ]
        );
    }

    #[test]
    fn plan_allows_dash_above_url_max_when_tagged() {
        let plan = plan_attempts(Lossless, Lossless, &tags(&["LOSSLESS", "HIRES_LOSSLESS"]));
        assert_eq!(
            plan,
            [
                (Lossless, Dash),
                (HiResLossless, Dash),
                (Lossless, Url),
                (High, Dash),
                (High, Url),
                (Low, Dash),
                (Low, Url),
            ]
        );
    }

    #[test]
    fn plan_skips_qualities_the_track_lacks() {
        let plan = plan_attempts(HiResLossless, High, &tags(&["DOLBY_ATMOS"]));
        assert_eq!(plan, [(High, Dash), (High, Url), (Low, Dash), (Low, Url)]);
    }

    #[test]
    fn stream_errors_fall_back() {
        assert!(is_stream_error(&api_error(404, 2001)));
        assert!(is_stream_error(&api_error(401, 4005)));
        assert!(is_stream_error(&Error::NoPrimaryUrl));
        assert!(is_stream_error(&Error::NotDashManifest(
            "application/vnd.tidal.bts".to_string()
        )));
        assert!(is_stream_error(&Error::Playback(
            playdash::Error::DashManifestMissingRepresentations
        )));
    }

    #[test]
    fn auth_server_and_rate_limit_errors_fail_fast() {
        assert!(!is_stream_error(&api_error(401, 11003)));
        assert!(!is_stream_error(&api_error(429, 0)));
        assert!(!is_stream_error(&api_error(500, 0)));
        assert!(!is_stream_error(&Error::NoAccessTokenAvailable));
        assert!(!is_stream_error(&Error::RateLimitBackoffExceeded(1000)));
    }
}
