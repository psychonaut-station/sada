use std::{fs::File, io::BufWriter, path::Path};

use anyhow::{Context as _, Result};
use hound::{WavSpec, WavWriter};
use opus::Channels;
use str0m::media::MediaData;
use tracing::{error, info, warn};

pub struct AudioSink {
    frame_count: u64,
    byte_count: u64,
    dumper: Option<AudioDumper>,
}

impl AudioSink {
    pub fn new() -> Self {
        let dumper = match AudioDumper::create("audio_dump.wav") {
            Ok(d) => {
                info!("audio dumper: writing to audio_dump.wav");
                Some(d)
            },
            Err(e) => {
                warn!("audio dumper: disabled ({e:#})");
                None
            },
        };

        Self {
            frame_count: 0,
            byte_count: 0,
            dumper,
        }
    }

    pub fn handle_frame(&mut self, data: &MediaData) {
        self.frame_count += 1;
        self.byte_count += data.data.len() as u64;

        if self.frame_count <= 5 || self.frame_count % 500 == 0 {
            info!(
                "audio frame #{}: {} bytes, mid={:?} pt={:?} (total {} bytes)",
                self.frame_count,
                data.data.len(),
                data.mid,
                data.pt,
                self.byte_count,
            );
        }

        if let Some(dumper) = &mut self.dumper {
            if let Err(e) = dumper.write_frame(&data.data) {
                if self.frame_count <= 3 || self.frame_count % 100 == 0 {
                    warn!("audio dumper decode error: {e}");
                }
            }
        }
    }
}

struct AudioDumper {
    decoder: opus::Decoder,
    writer: WavWriter<BufWriter<File>>,
    sample_count: u64,
}

impl AudioDumper {
    fn create(path: impl AsRef<Path>) -> Result<Self> {
        let decoder = opus::Decoder::new(48000, Channels::Mono).context("failed to create Opus decoder")?;

        let spec = WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let writer = WavWriter::create(path.as_ref(), spec).context("failed to create WAV file")?;

        Ok(Self {
            decoder,
            writer,
            sample_count: 0,
        })
    }

    fn write_frame(&mut self, opus_data: &[u8]) -> Result<()> {
        let mut pcm_buf = [0i16; 5760];

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
            info!("audio dumper: {:.1}s of audio written", secs);
        }

        Ok(())
    }
}

impl Drop for AudioDumper {
    fn drop(&mut self) {
        let duration_secs = self.sample_count as f64 / 48000.0;
        info!(
            "audio dumper: finalizing WAV ({:.1}s, {} samples)",
            duration_secs, self.sample_count,
        );
        if let Err(e) = self.writer.flush() {
            error!("audio dumper: flush error: {e}");
        }
    }
}
