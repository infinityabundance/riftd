use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub frame_duration_ms: u32,
    pub channels: u16,
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
    pub fn frame_samples(&self) -> usize {
        let per_channel = (self.sample_rate as usize * self.frame_duration_ms as usize) / 1000;
        per_channel * self.channels as usize
    }

    pub fn frame_duration(&self) -> Duration {
        Duration::from_millis(self.frame_duration_ms as u64)
    }
}

pub struct AudioIn {
    _stream: cpal::Stream,
}

impl AudioIn {
    pub fn new(config: &AudioConfig) -> Result<(Self, tokio::sync::mpsc::Receiver<Vec<i16>>)> {
        Self::new_with_device(config, None)
    }

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

pub struct AudioOut {
    _stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<i16>>>,
    frame_samples: usize,
    channels: u16,
}

impl AudioOut {
    pub fn new(config: &AudioConfig) -> Result<Self> {
        Self::new_with_device(config, None)
    }

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

    pub fn push_frame(&self, frame: &[i16]) {
        let mut queue = self.queue.lock().unwrap();
        for sample in frame {
            queue.push_back(*sample);
        }
    }

    pub fn frame_samples(&self) -> usize {
        self.frame_samples
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }
}

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

fn audio_out_callback_i16(data: &mut [i16], queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut q = queue.lock().unwrap();
    for sample in data.iter_mut() {
        *sample = q.pop_front().unwrap_or(0);
    }
}

fn audio_out_callback_f32(data: &mut [f32], queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut q = queue.lock().unwrap();
    for sample in data.iter_mut() {
        let v = q.pop_front().unwrap_or(0);
        *sample = v as f32 / i16::MAX as f32;
    }
}

fn audio_out_callback_u16(data: &mut [u16], queue: &Arc<Mutex<VecDeque<i16>>>) {
    let mut q = queue.lock().unwrap();
    for sample in data.iter_mut() {
        let v = q.pop_front().unwrap_or(0);
        *sample = (v as i32 + 32768) as u16;
    }
}

pub struct OpusEncoder {
    inner: opus::Encoder,
}

impl OpusEncoder {
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

    pub fn encode_i16(&mut self, frame: &[i16], out: &mut [u8]) -> Result<usize> {
        let len = self.inner.encode(frame, out)?;
        Ok(len)
    }

    pub fn encode_f32(&mut self, frame: &[f32], out: &mut [u8]) -> Result<usize> {
        let len = self.inner.encode_float(frame, out)?;
        Ok(len)
    }
}

pub struct OpusDecoder {
    inner: opus::Decoder,
}

impl OpusDecoder {
    pub fn new(config: &AudioConfig) -> Result<Self> {
        let channels = match config.channels {
            1 => opus::Channels::Mono,
            2 => opus::Channels::Stereo,
            _ => return Err(anyhow!("unsupported channel count")),
        };
        let decoder = opus::Decoder::new(config.sample_rate, channels)?;
        Ok(Self { inner: decoder })
    }

    pub fn decode_i16(&mut self, data: &[u8], out: &mut [i16]) -> Result<usize> {
        let len = self.inner.decode(data, out, false)?;
        Ok(len)
    }

    pub fn decode_f32(&mut self, data: &[u8], out: &mut [f32]) -> Result<usize> {
        let len = self.inner.decode_float(data, out, false)?;
        Ok(len)
    }
}

#[derive(Debug, Clone)]
pub struct VoiceFrame {
    pub seq: u32,
    pub timestamp: u64,
    pub payload: Vec<u8>,
}

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
    pub fn new(frame_samples: usize) -> Self {
        Self::with_prebuffer(frame_samples, 0)
    }

    pub fn with_prebuffer(frame_samples: usize, prebuffer_frames: usize) -> Self {
        Self {
            frame_samples,
            prebuffer_frames,
            streams: HashMap::new(),
        }
    }

    pub fn push(&mut self, stream_id: u64, frame: Vec<i16>) {
        let entry = self.streams.entry(stream_id).or_insert_with(|| StreamState {
            queue: VecDeque::new(),
            prebuffer: self.prebuffer_frames,
        });
        entry.queue.push_back(frame);
    }

    pub fn mix_next(&mut self) -> Vec<i16> {
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
        mix.into_iter()
            .map(|v| {
                let scaled = v / scale;
                scaled.clamp(i16::MIN as i32, i16::MAX as i32) as i16
            })
            .collect()
    }
}
