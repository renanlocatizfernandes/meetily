//! Turn embedding from log-magnitude bands of a real FFT.
//!
//! This is not a neural speaker model. It separates sequential turns whose
//! spectra differ. Overlapping voices inside one VAD segment stay one vector.

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

const FRAME: usize = 512;
const HOP: usize = 256;
const BANDS: usize = 16;

pub struct Embedder {
    fft: Arc<dyn RealToComplex<f32>>,
    spectrum: Vec<Complex<f32>>,
}

impl Default for Embedder {
    fn default() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(FRAME);
        let spectrum = fft.make_output_vec();
        Self { fft, spectrum }
    }
}

impl Embedder {
    pub fn embed(&mut self, samples: &[f32], sample_rate: u32) -> Option<Vec<f32>> {
        if sample_rate < 8_000 || samples.len() < FRAME {
            return None;
        }
        let filters = band_filters(sample_rate);
        let mut accum = vec![0.0f32; BANDS];
        let mut frames = 0usize;
        let mut start = 0usize;
        while start + FRAME <= samples.len() {
            let mut frame = samples[start..start + FRAME].to_vec();
            for (index, sample) in frame.iter_mut().enumerate() {
                let hann = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * index as f32 / FRAME as f32).cos();
                *sample *= hann;
            }
            self.fft.process(&mut frame, &mut self.spectrum).ok()?;
            for (band, weights) in filters.iter().enumerate() {
                let mut energy = 0.0f32;
                for (bin, weight) in weights.iter().enumerate() {
                    if *weight == 0.0 {
                        continue;
                    }
                    let value = self.spectrum.get(bin)?;
                    energy += weight * value.norm();
                }
                accum[band] += (energy + 1.0e-8).ln();
            }
            frames += 1;
            start += HOP;
        }
        if frames == 0 {
            return None;
        }
        for value in &mut accum {
            *value /= frames as f32;
        }
        normalize(&mut accum);
        if accum.iter().all(|value| value.is_finite()) {
            Some(accum)
        } else {
            None
        }
    }
}

fn band_filters(sample_rate: u32) -> Vec<Vec<f32>> {
    let nyquist = sample_rate as f32 / 2.0;
    let low = 80.0f32;
    let high = (4000.0f32).min(nyquist * 0.95);
    let edges: Vec<f32> = (0..=BANDS + 1)
        .map(|index| {
            let mel = hz_to_mel(low) + (hz_to_mel(high) - hz_to_mel(low)) * index as f32 / (BANDS + 1) as f32;
            mel_to_hz(mel)
        })
        .collect();
    let bins = FRAME / 2 + 1;
    (0..BANDS)
        .map(|band| {
            let left = edges[band];
            let center = edges[band + 1];
            let right = edges[band + 2];
            (0..bins)
                .map(|bin| {
                    let hz = bin as f32 * sample_rate as f32 / FRAME as f32;
                    if hz <= left || hz >= right || right <= left {
                        0.0
                    } else if hz <= center {
                        (hz - left) / (center - left).max(1.0)
                    } else {
                        (right - hz) / (right - center).max(1.0)
                    }
                })
                .collect()
        })
        .collect()
}

fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0f32.powf(mel / 2595.0) - 1.0)
}

pub fn cosine(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let dot: f32 = left.iter().zip(right).map(|(a, b)| a * b).sum();
    dot.clamp(-1.0, 1.0)
}

fn normalize(values: &mut [f32]) {
    let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 1.0e-6 {
        for value in values {
            *value /= norm;
        }
    }
}

pub fn tone(frequency: f32, seconds: f32, sample_rate: u32) -> Vec<f32> {
    let count = (seconds * sample_rate as f32) as usize;
    (0..count)
        .map(|index| (2.0 * std::f32::consts::PI * frequency * index as f32 / sample_rate as f32).sin())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_tone_matches_and_a_distant_tone_does_not() {
        let mut embedder = Embedder::default();
        let low = embedder.embed(&tone(180.0, 1.2, 16_000), 16_000).unwrap();
        let low_again = embedder.embed(&tone(180.0, 1.2, 16_000), 16_000).unwrap();
        let high = embedder.embed(&tone(2200.0, 1.2, 16_000), 16_000).unwrap();
        assert!(cosine(&low, &low_again) > 0.98);
        assert!(cosine(&low, &high) < 0.85);
    }
}
