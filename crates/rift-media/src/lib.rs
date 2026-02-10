//! Audio capture, playback, and codec helpers.
//!
//! This module wraps CPAL for audio I/O and Opus for encoding/decoding.
//! It also provides a simple mixer for combining multiple streams.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rift_protocol::CodecId;

#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Frame duration in milliseconds.
    pub frame_duration_ms: u32,
    /// Channel count (mono/stereo).
    pub channels: u16,
    /// Target bitrate for Opus.
    pub bitrate: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            frame_duration_ms: 20,
            channels: 1,
            bitrate: 48_000,
        }
    }
}

impl AudioConfig {
    /// Number of samples per frame (all channels included).
    pub fn frame_samples(&self) -> usize {
        let per_channel = (self.sample_rate as usize * self.frame_duration_ms as usize) / 1000;
        per_channel * self.channels as usize
    }

    /// Frame duration as a `Duration`.
    pub fn frame_duration(&self) -> Duration {
        Duration::from_millis(self.frame_duration_ms as u64)
    }
}

/// Encode a single audio frame into the requested codec.
pub fn encode_frame(codec: CodecId, frame: &[i16], encoder: &mut OpusEncoder) -> Result<Vec<u8>> {
    match codec {
        CodecId::Opus => {
            let mut out = vec![0u8; 4000];
            let len = encoder.encode_i16(frame, &mut out)?;
            out.truncate(len);
            Ok(out)
        }
        CodecId::PCM16 => {
            let mut out = Vec::with_capacity(frame.len() * 2);
            for sample in frame {
                out.extend_from_slice(&sample.to_le_bytes());
            }
            Ok(out)
        }
        CodecId::Experimental(_) => Err(anyhow!("unsupported codec")),
    }
}

/// Decode a single audio frame payload into PCM samples.
pub fn decode_frame(
    codec: CodecId,
    payload: &[u8],
    decoder: &mut OpusDecoder,
    frame_samples: usize,
) -> Result<Vec<i16>> {
    match codec {
        CodecId::Opus => {
            let mut out = vec![0i16; frame_samples];
            let len = decoder.decode_i16(payload, &mut out)?;
            out.truncate(len);
            Ok(out)
        }
        CodecId::PCM16 => {
            let mut out = Vec::with_capacity(payload.len() / 2);
            for chunk in payload.chunks_exact(2) {
                out.push(i16::from_le_bytes([chunk[0], chunk[1]]));
            }
            Ok(out)
        }
        CodecId::Experimental(_) => Err(anyhow!("unsupported codec")),
    }
}

/// Active audio input stream wrapper.
pub struct AudioIn {
    _stream: cpal::Stream,
}

impl AudioIn {
    /// Open the default input device and start capturing.
    pub fn new(config: &AudioConfig) -> Result<(Self, tokio::sync::mpsc::Receiver<Vec<i16>>)> {
        Self::new_with_device(config, None)
    }

    /// Open a specific input device by name (or default if None).
    pub fn new_with_device(
        config: &AudioConfig,
        device_name: Option<&str>,
    ) -> Result<(Self, tokio::sync::mpsc::Receiver<Vec<i16>>)> {
        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            find_input_device(&host, name)?
        } else {
            host.default_input_device()
                .ok_or_else(|| anyhow!("no default input device"))?
        };

        let supported_config = device.default_input_config()?;
        let sample_format = supported_config.sample_format();
        let mut stream_config: cpal::StreamConfig = supported_config.into();
        stream_config.channels = config.channels;
        stream_config.sample_rate = cpal::SampleRate(config.sample_rate);

        let frame_samples = config.frame_samples();
        let (tx, rx) = tokio::sync::mpsc::channel::<Vec<i16>>(64);
        let buffer = Arc::new(Mutex::new(Vec::with_capacity(frame_samples * 2)));
        let buffer_clone = buffer.clone();

        let err_fn = |err| tracing::error!("audio input error: {err}");

        let stream = match sample_format {
            cpal::SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    audio_in_callback(data, frame_samples, &tx, &buffer_clone);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    let converted: Vec<i16> = data
                        .iter()
                        .map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                        .collect();
                    audio_in_callback(&converted, frame_samples, &tx, &buffer_clone);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let converted: Vec<i16> = data.iter().map(|s| (*s as i32 - 32768) as i16).collect();
                    audio_in_callback(&converted, frame_samples, &tx, &buffer_clone);
                },
                err_fn,
                None,
            )?,
            _ => return Err(anyhow!("unsupported input sample format")),
        };

        stream.play()?;
        Ok((Self { _stream: stream }, rx))
    }
}

/// Buffer callback for audio input streams.
fn audio_in_callback(
    data: &[i16],
    frame_samples: usize,
    tx: &tokio::sync::mpsc::Sender<Vec<i16>>,
    buffer: &Arc<Mutex<Vec<i16>>>,
) {
    let mut buf = buffer.lock().unwrap();
    buf.extend_from_slice(data);
    while buf.len() >= frame_samples {
        let frame: Vec<i16> = buf.drain(..frame_samples).collect();
        let _ = tx.try_send(frame);
    }
}

/// Active audio output stream wrapper.
pub struct AudioOut {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<i16>>>,
    frame_samples: usize,
    channels: u16,
}

impl AudioOut {
    /// Open the default output device and start playback.
    pub fn new(config: &AudioConfig) -> Result<Self> {
        Self::new_with_device(config, None)
    }

    /// Open a specific output device by name (or default if None).
    pub fn new_with_device(config: &AudioConfig, device_name: Option<&str>) -> Result<Self> {
        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            find_output_device(&host, name)?
        } else {
            host.default_output_device()
                .ok_or_else(|| anyhow!("no default output device"))?
        };

        let supported_config = device.default_output_config()?;
        let sample_format = supported_config.sample_format();
        let mut stream_config: cpal::StreamConfig = supported_config.into();
        stream_config.channels = config.channels;
        stream_config.sample_rate = cpal::SampleRate(config.sample_rate);

        let queue = Arc::new(Mutex::new(VecDeque::with_capacity(config.frame_samples() * 4)));
        let queue_clone = queue.clone();
        let frame_samples = config.frame_samples();
        let channels = config.channels;

        let err_fn = |err| tracing::error!("audio output error: {err}");

        let stream = match sample_format {
            cpal::SampleFormat::I16 => device.build_output_stream(
                &stream_config,
                move |data: &mut [i16], _| {
                    audio_out_callback_i16(data, &queue_clone);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::F32 => device.build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| {
                    audio_out_callback_f32(data, &queue_clone);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_output_stream(
                &stream_config,
                move |data: &mut [u16], _| {
                    audio_out_callback_u16(data, &queue_clone);
                },
                err_fn,
                None,
            )?,
            _ => return Err(anyhow!("unsupported output sample format")),
        };

        stream.play()?;

        Ok(Self {
            _stream: stream,
            queue,
            frame_samples,
            channels,
        })
    }

    /// Push a PCM frame into the playback queue.
    pub fn push_frame(&self, frame: &[i16]) {
        let mut queue = self.queue.lock().unwrap();
        for sample in frame {
            queue.push_back(*sample);
        }
    }

    /// Number of samples per frame.
    pub fn frame_samples(&self) -> usize {
        self.frame_samples
    }

    /// Channel count.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Samples currently queued for playback.
    pub fn queued_samples(&self) -> usize {
        let queue = self.queue.lock().unwrap();
        queue.len()
    }
}

/// Find an input device by name.
fn find_input_device(host: &cpal::Host, name: &str) -> Result<cpal::Device> {
    for device in host.input_devices()? {
        if let Ok(dev_name) = device.name() {
            if dev_name == name {
                return Ok(device);
            }
        }
    }
    Err(anyhow!("input device not found: {}", name))
}

/// Find an output device by name.
fn find_output_device(host: &cpal::Host, name: &str) -> Result<cpal::Device> {
    for device in host.output_devices()? {
        if let Ok(dev_name) = device.name() {
            if dev_name == name {
                return Ok(device);
            }
        }
    }
    Err(anyhow!("output device not found: {}", name))
}

/// Write queued PCM data into an i16 output buffer.
fn audio_out_callback_i16(data: &mut [i16], queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut q = queue.lock().unwrap();
    for sample in data.iter_mut() {
        *sample = q.pop_front().unwrap_or(0);
    }
}

/// Write queued PCM data into an f32 output buffer.
fn audio_out_callback_f32(data: &mut [f32], queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut q = queue.lock().unwrap();
    for sample in data.iter_mut() {
        let v = q.pop_front().unwrap_or(0);
        *sample = v as f32 / i16::MAX as f32;
    }
}

/// Write queued PCM data into a u16 output buffer.
fn audio_out_callback_u16(data: &mut [u16], queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut q = queue.lock().unwrap();
    for sample in data.iter_mut() {
        let v = q.pop_front().unwrap_or(0);
        *sample = (v as i32 + 32768) as u16;
    }
}

/// Thin wrapper around Opus encoder with Rift-friendly defaults.
pub struct OpusEncoder {
    inner: opus::Encoder,
}

impl OpusEncoder {
    /// Construct an Opus encoder based on the audio config.
    pub fn new(config: &AudioConfig) -> Result<Self> {
        let channels = match config.channels {
            1 => opus::Channels::Mono,
            2 => opus::Channels::Stereo,
            _ => return Err(anyhow!("unsupported channel count")),
        };
        let mut encoder = opus::Encoder::new(config.sample_rate, channels, opus::Application::Voip)?;
        encoder.set_bitrate(opus::Bitrate::Bits(config.bitrate as i32))?;
        Ok(Self { inner: encoder })
    }

    /// Encode i16 PCM samples.
    pub fn encode_i16(&mut self, frame: &[i16], out: &mut [u8]) -> Result<usize> {
        let len = self.inner.encode(frame, out)?;
        Ok(len)
    }

    /// Encode f32 PCM samples.
    pub fn encode_f32(&mut self, frame: &[f32], out: &mut [u8]) -> Result<usize> {
        let len = self.inner.encode_float(frame, out)?;
        Ok(len)
    }

    /// Update target bitrate.
    pub fn set_bitrate(&mut self, bitrate: u32) -> Result<()> {
        self.inner
            .set_bitrate(opus::Bitrate::Bits(bitrate as i32))?;
        Ok(())
    }

    /// Enable or disable in-band FEC.
    pub fn set_fec(&mut self, enabled: bool) -> Result<()> {
        self.inner.set_inband_fec(enabled)?;
        Ok(())
    }

    /// Set expected packet loss percentage.
    pub fn set_packet_loss(&mut self, loss_pct: u8) -> Result<()> {
        let pct = loss_pct.min(100);
        self.inner.set_packet_loss_perc(pct as i32)?;
        Ok(())
    }
}

/// Thin wrapper around Opus decoder.
pub struct OpusDecoder {
    inner: opus::Decoder,
}

impl OpusDecoder {
    /// Construct an Opus decoder based on the audio config.
    pub fn new(config: &AudioConfig) -> Result<Self> {
        let channels = match config.channels {
            1 => opus::Channels::Mono,
            2 => opus::Channels::Stereo,
            _ => return Err(anyhow!("unsupported channel count")),
        };
        let decoder = opus::Decoder::new(config.sample_rate, channels)?;
        Ok(Self { inner: decoder })
    }

    /// Decode Opus payload into i16 PCM samples.
    pub fn decode_i16(&mut self, data: &[u8], out: &mut [i16]) -> Result<usize> {
        let len = self.inner.decode(data, out, false)?;
        Ok(len)
    }

    /// Decode Opus payload into f32 PCM samples.
    pub fn decode_f32(&mut self, data: &[u8], out: &mut [f32]) -> Result<usize> {
        let len = self.inner.decode_float(data, out, false)?;
        Ok(len)
    }
}

#[derive(Debug, Clone)]
pub struct VoiceFrame {
    /// Sequence number assigned by sender.
    pub seq: u32,
    /// Capture timestamp in milliseconds.
    pub timestamp: u64,
    /// Encoded audio payload.
    pub payload: Vec<u8>,
}

/// Simple audio mixer for combining multiple PCM streams.
pub struct AudioMixer {
    frame_samples: usize,
    prebuffer_frames: usize,
    streams: HashMap<u64, StreamState>,
}

struct StreamState {
    queue: VecDeque<Vec<i16>>,
    prebuffer: usize,
}

impl AudioMixer {
    /// Create a mixer with no prebuffer.
    pub fn new(frame_samples: usize) -> Self {
        Self::with_prebuffer(frame_samples, 0)
    }

    /// Create a mixer with a specified prebuffer in frames.
    pub fn with_prebuffer(frame_samples: usize, prebuffer_frames: usize) -> Self {
        Self {
            frame_samples,
            prebuffer_frames,
            streams: HashMap::new(),
        }
    }

    /// Push a new frame into a stream's queue.
    pub fn push(&mut self, stream_id: u64, frame: Vec<i16>) {
        let entry = self.streams.entry(stream_id).or_insert_with(|| StreamState {
            queue: VecDeque::new(),
            prebuffer: self.prebuffer_frames,
        });
        entry.queue.push_back(frame);
    }

    /// Mix the next frame without returning activity information.
    pub fn mix_next(&mut self) -> Vec<i16> {
        let (frame, _active) = self.mix_next_with_activity();
        frame
    }

    /// Mix the next frame and report whether any streams were active.
    pub fn mix_next_with_activity(&mut self) -> (Vec<i16>, bool) {
        let mut active = 0usize;
        let mut mix = vec![0i32; self.frame_samples];

        let mut remove = Vec::new();
        for (id, queue) in self.streams.iter_mut() {
            if queue.prebuffer > 0 {
                if queue.queue.len() >= queue.prebuffer {
                    queue.prebuffer = 0;
                } else {
                    continue;
                }
            }

            if let Some(frame) = queue.queue.pop_front() {
                active += 1;
                for (i, sample) in frame.iter().enumerate() {
                    mix[i] += *sample as i32;
                }
            }

            if queue.queue.is_empty() && queue.prebuffer == 0 {
                remove.push(*id);
            }
        }

        for id in remove {
            self.streams.remove(&id);
        }

        let scale = if active > 0 { active as i32 } else { 1 };
        let frame = mix.into_iter()
            .map(|v| {
                let scaled = v / scale;
                scaled.clamp(i16::MIN as i32, i16::MAX as i32) as i16
            })
            .collect();
        (frame, active > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_config_default_frame_samples() {
        let config = AudioConfig::default();
        // 48kHz * 20ms / 1000 * 1 channel = 960 samples
        assert_eq!(config.frame_samples(), 960);
    }

    #[test]
    fn audio_config_stereo_frame_samples() {
        let config = AudioConfig {
            sample_rate: 48_000,
            frame_duration_ms: 20,
            channels: 2,
            bitrate: 48_000,
        };
        // 48kHz * 20ms / 1000 * 2 channels = 1920 samples
        assert_eq!(config.frame_samples(), 1920);
    }

    #[test]
    fn audio_config_different_sample_rates() {
        // 8kHz mono 20ms
        let config8k = AudioConfig {
            sample_rate: 8_000,
            frame_duration_ms: 20,
            channels: 1,
            bitrate: 12_000,
        };
        assert_eq!(config8k.frame_samples(), 160);

        // 16kHz mono 20ms
        let config16k = AudioConfig {
            sample_rate: 16_000,
            frame_duration_ms: 20,
            channels: 1,
            bitrate: 24_000,
        };
        assert_eq!(config16k.frame_samples(), 320);
    }

    #[test]
    fn audio_config_frame_duration() {
        let config = AudioConfig {
            sample_rate: 48_000,
            frame_duration_ms: 10,
            channels: 1,
            bitrate: 48_000,
        };
        assert_eq!(config.frame_duration(), Duration::from_millis(10));
    }

    #[test]
    fn pcm16_encode_decode_roundtrip() {
        let config = AudioConfig::default();
        let mut encoder = OpusEncoder::new(&config).unwrap();
        let mut decoder = OpusDecoder::new(&config).unwrap();

        // Create a simple test frame with a sine wave pattern
        let frame: Vec<i16> = (0..config.frame_samples())
            .map(|i| ((i as f32 * 0.1).sin() * 10000.0) as i16)
            .collect();

        // PCM16 should be lossless
        let encoded = encode_frame(CodecId::PCM16, &frame, &mut encoder).unwrap();
        let decoded = decode_frame(CodecId::PCM16, &encoded, &mut decoder, config.frame_samples()).unwrap();

        assert_eq!(frame, decoded);
    }

    #[test]
    fn opus_encode_decode_roundtrip() {
        let config = AudioConfig::default();
        let mut encoder = OpusEncoder::new(&config).unwrap();
        let mut decoder = OpusDecoder::new(&config).unwrap();

        // Create test frame
        let frame: Vec<i16> = (0..config.frame_samples())
            .map(|i| ((i as f32 * 0.05).sin() * 5000.0) as i16)
            .collect();

        let encoded = encode_frame(CodecId::Opus, &frame, &mut encoder).unwrap();
        let decoded = decode_frame(CodecId::Opus, &encoded, &mut decoder, config.frame_samples()).unwrap();

        // Opus is lossy, so just verify we got the right number of samples
        assert_eq!(decoded.len(), config.frame_samples());
    }

    #[test]
    fn opus_encoder_bitrate_change() {
        let config = AudioConfig::default();
        let mut encoder = OpusEncoder::new(&config).unwrap();

        // Should not error
        encoder.set_bitrate(24_000).unwrap();
        encoder.set_bitrate(96_000).unwrap();
    }

    #[test]
    fn opus_encoder_fec_setting() {
        let config = AudioConfig::default();
        let mut encoder = OpusEncoder::new(&config).unwrap();

        encoder.set_fec(true).unwrap();
        encoder.set_fec(false).unwrap();
    }

    #[test]
    fn opus_encoder_packet_loss_setting() {
        let config = AudioConfig::default();
        let mut encoder = OpusEncoder::new(&config).unwrap();

        encoder.set_packet_loss(0).unwrap();
        encoder.set_packet_loss(10).unwrap();
        encoder.set_packet_loss(100).unwrap();
        // Values > 100 should be clamped
        encoder.set_packet_loss(255).unwrap();
    }

    #[test]
    fn mixer_single_stream() {
        let mut mixer = AudioMixer::new(4);

        mixer.push(1, vec![100, 200, 300, 400]);
        let (frame, active) = mixer.mix_next_with_activity();

        assert!(active);
        assert_eq!(frame, vec![100, 200, 300, 400]);
    }

    #[test]
    fn mixer_multiple_streams_averaging() {
        let mut mixer = AudioMixer::new(4);

        mixer.push(1, vec![100, 200, 300, 400]);
        mixer.push(2, vec![100, 200, 300, 400]);
        let (frame, active) = mixer.mix_next_with_activity();

        assert!(active);
        // Two streams with identical values, divided by 2
        assert_eq!(frame, vec![100, 200, 300, 400]);
    }

    #[test]
    fn mixer_empty_returns_zeros() {
        let mut mixer = AudioMixer::new(4);

        let (frame, active) = mixer.mix_next_with_activity();

        assert!(!active);
        assert_eq!(frame, vec![0, 0, 0, 0]);
    }

    #[test]
    fn mixer_stream_removal_when_empty() {
        let mut mixer = AudioMixer::new(4);

        mixer.push(1, vec![100, 200, 300, 400]);

        // First mix consumes the frame
        let (_, active) = mixer.mix_next_with_activity();
        assert!(active);

        // Second mix should find no active streams
        let (frame, active) = mixer.mix_next_with_activity();
        assert!(!active);
        assert_eq!(frame, vec![0, 0, 0, 0]);
    }

    #[test]
    fn mixer_prebuffer() {
        let mut mixer = AudioMixer::with_prebuffer(4, 2);

        // Push first frame - still prebuffering
        mixer.push(1, vec![100, 200, 300, 400]);
        let (frame, active) = mixer.mix_next_with_activity();
        assert!(!active);
        assert_eq!(frame, vec![0, 0, 0, 0]);

        // Push second frame - prebuffer satisfied
        mixer.push(1, vec![500, 600, 700, 800]);
        let (frame, active) = mixer.mix_next_with_activity();
        assert!(active);
        assert_eq!(frame, vec![100, 200, 300, 400]);
    }

    #[test]
    fn mixer_clipping_protection() {
        let mut mixer = AudioMixer::new(2);

        // Push values that would overflow if not averaged
        mixer.push(1, vec![i16::MAX, i16::MAX]);
        mixer.push(2, vec![i16::MAX, i16::MAX]);
        let (frame, _) = mixer.mix_next_with_activity();

        // Should be averaged, not clipped
        assert_eq!(frame, vec![i16::MAX, i16::MAX]);
    }

    #[test]
    fn voice_frame_construction() {
        let frame = VoiceFrame {
            seq: 42,
            timestamp: 1234567890,
            payload: vec![1, 2, 3, 4],
        };

        assert_eq!(frame.seq, 42);
        assert_eq!(frame.timestamp, 1234567890);
        assert_eq!(frame.payload, vec![1, 2, 3, 4]);
    }
}
