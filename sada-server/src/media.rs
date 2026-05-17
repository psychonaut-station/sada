//! Media frame handling.

use std::{fs::File, io::BufWriter, path::Path};

use anyhow::{Context as _, Result};
use hound::{WavSpec, WavWriter};
use opus::Channels;
use str0m::media::MediaData;

/// Consumes incoming audio media frames.
pub struct AudioSink {
    /// Number of media frames observed.
    frame_count: u64,
    /// Total number of encoded bytes observed.
    byte_count: u64,
    /// Optional WAV dumper used for local debugging.
    dumper: Option<AudioDumper>,
}

impl AudioSink {
    /// Create a new audio sink.
    pub fn new() -> Self {
        let dumper = match AudioDumper::create("audio_dump.wav") {
            Ok(d) => {
                info!("audio dumper: writing to audio_dump.wav");
                Some(d)
            },
            Err(err) => {
                warn!(?err, "audio dumper: disabled");
                None
            },
        };

        Self {
            frame_count: 0,
            byte_count: 0,
            dumper,
        }
    }

    /// Process one encoded media frame.
    pub fn handle_frame(&mut self, data: &MediaData) {
        self.frame_count += 1;
        self.byte_count += data.data.len() as u64;

        if self.frame_count <= 5 || self.frame_count.is_multiple_of(500) {
            info!(
                frame = self.frame_count,
                bytes = data.data.len(),
                mid = ?data.mid,
                pt = ?data.pt,
                total_bytes = self.byte_count,
                "audio frame",
            );
        }

        if let Some(dumper) = &mut self.dumper
            && let Err(err) = dumper.write_frame(&data.data)
            && (self.frame_count <= 3 || self.frame_count.is_multiple_of(100))
        {
            warn!(?err, "audio dumper: decode error");
        }
    }
}

/// Decodes Opus frames and writes the resulting PCM samples to a WAV file.
struct AudioDumper {
    /// Opus decoder configured for the negotiated audio stream.
    decoder: opus::Decoder,
    /// WAV writer for decoded samples.
    writer: WavWriter<BufWriter<File>>,
    /// Number of decoded PCM samples written.
    sample_count: u64,
}

impl AudioDumper {
    /// Create a dumper that writes to `path`.
    fn create(path: impl AsRef<Path>) -> Result<Self> {
        let decoder = opus::Decoder::new(48000, Channels::Mono).context("failed to create Opus decoder")?;

        let spec = WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let writer = WavWriter::create(path, spec).context("failed to create WAV file")?;

        Ok(Self {
            decoder,
            writer,
            sample_count: 0,
        })
    }

    /// Decode one Opus packet and append its samples to the WAV file.
    fn write_frame(&mut self, opus_data: &[u8]) -> Result<()> {
        let mut pcm_buf = [0; 5760];

        let samples = self
            .decoder
            .decode(opus_data, &mut pcm_buf, false)
            .context("Opus decode failed")?;

        for &sample in &pcm_buf[..samples] {
            self.writer.write_sample(sample).context("WAV write failed")?;
        }

        self.sample_count += samples as u64;

        if self.sample_count % 240_000 < samples as u64 {
            let secs = self.sample_count as f64 / 48000.0;
            info!(duration_secs = secs, "audio dumper: audio written");
        }

        Ok(())
    }
}

impl Drop for AudioDumper {
    fn drop(&mut self) {
        let secs = self.sample_count as f64 / 48000.0;
        info!(
            duration_secs = secs,
            samples = self.sample_count,
            "audio dumper: finalizing WAV",
        );
        if let Err(err) = self.writer.flush() {
            error!(?err, "audio dumper: flush error");
        }
    }
}
