//! Sound output: the emulator thread pushes 32 kHz stereo frames into a
//! lock-free ring, a `cpal` stream pulls them (linearly resampled to the
//! device rate), and the emulator paces itself on how full the ring is.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer, RingBuffer};

/// The DSP's output rate.
const SOURCE_RATE: f32 = 32_000.0;
/// Ring capacity in stereo frames (about 250 ms at 32 kHz).
const RING_FRAMES: usize = 8192;
/// Target queue depth the emulator keeps ahead of the device.
pub(crate) const TARGET_FRAMES: usize = 3200;

/// The producer side, owned by the emulator thread.
pub(crate) struct AudioOut {
    producer: Producer<f32>,
    _stream: cpal::Stream,
    /// Device frames per source frame.
    ratio: f32,
}

impl std::fmt::Debug for AudioOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioOut").field("ratio", &self.ratio).finish_non_exhaustive()
    }
}

/// Pulls device frames from the ring, resampling with linear interpolation.
struct Reader {
    consumer: Consumer<f32>,
    step: f32,
    position: f32,
    last: [f32; 2],
    next: [f32; 2],
}

impl Reader {
    fn pull_source(&mut self) -> bool {
        match (self.consumer.pop(), self.consumer.pop()) {
            (Ok(l), Ok(r)) => {
                self.last = self.next;
                self.next = [l, r];
                true
            }
            _ => false,
        }
    }

    #[allow(clippy::while_float, reason = "resampler phase accumulator in source frames")]
    fn fill(&mut self, out: &mut [f32]) {
        for frame in out.chunks_mut(2) {
            self.position += self.step;
            while self.position >= 1.0 {
                if !self.pull_source() {
                    // Underrun: hold the last sample.
                    self.position = 1.0;
                    break;
                }
                self.position -= 1.0;
            }
            let t = self.position.clamp(0.0, 1.0);
            for (ch, slot) in frame.iter_mut().enumerate() {
                let a = self.last.get(ch).copied().unwrap_or(0.0);
                let b = self.next.get(ch).copied().unwrap_or(0.0);
                *slot = (b - a).mul_add(t, a);
            }
        }
    }
}

impl AudioOut {
    /// Opens the default output device; `None` when there is none.
    pub(crate) fn open() -> Option<Self> {
        let device = cpal::default_host().default_output_device()?;
        let config = device.default_output_config().ok()?;
        let rate = config.sample_rate();
        let channels = config.channels();
        if channels < 2 {
            return None;
        }
        let (producer, consumer) = RingBuffer::<f32>::new(RING_FRAMES * 2);
        let step = SOURCE_RATE / f32::from(u16::try_from(rate).unwrap_or(48_000));
        let mut reader = Reader { consumer, step, position: 0.0, last: [0.0; 2], next: [0.0; 2] };
        let stream_config = cpal::StreamConfig {
            channels: 2,
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Default,
        };
        let stream = device
            .build_output_stream(
                stream_config,
                move |out: &mut [f32], _| reader.fill(out),
                |_err| {},
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some(Self { producer, _stream: stream, ratio: 1.0 / step })
    }

    /// Queues interleaved 16-bit frames; drops what does not fit.
    pub(crate) fn push(&mut self, samples: &[i16]) {
        for &s in samples {
            if self.producer.push(f32::from(s) / 32768.0).is_err() {
                break;
            }
        }
    }

    /// Stereo frames waiting in the ring.
    pub(crate) fn queued_frames(&self) -> usize {
        (RING_FRAMES * 2).saturating_sub(self.producer.slots()) / 2
    }
}
