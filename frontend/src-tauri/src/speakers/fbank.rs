//! Kaldi-style log-mel filterbank features for the neural speaker model.
//!
//! Matches `kaldi-native-fbank` / `torchaudio.compliance.kaldi.fbank` with the
//! settings the CAM++ model was trained on: 16 kHz, 25 ms frames, 10 ms hop,
//! DC removal, pre-emphasis 0.97, povey window, 512-point FFT, power spectrum,
//! 80 mel bins from 20 Hz to Nyquist, natural log, `snip_edges = true`, no
//! dither, then per-utterance mean normalization (the model metadata says
//! `feature_normalize_type = global-mean`).

use realfft::RealFftPlanner;

pub const SAMPLE_RATE: u32 = 16_000;
pub const MEL_BINS: usize = 80;
const FRAME_LEN: usize = 400;
const FRAME_SHIFT: usize = 160;
const FFT_LEN: usize = 512;
const PREEMPHASIS: f32 = 0.97;
const LOW_HZ: f32 = 20.0;
const HIGH_HZ: f32 = 8_000.0;

/// Number of frames produced for `samples` 16 kHz samples.
pub fn frame_count(samples: usize) -> usize {
    if samples < FRAME_LEN {
        0
    } else {
        1 + (samples - FRAME_LEN) / FRAME_SHIFT
    }
}

/// Returns `frames * MEL_BINS` values in row-major order (frame, bin), already
/// mean-normalized per bin. `None` when the audio is shorter than one frame.
pub fn fbank(samples: &[f32]) -> Option<Vec<f32>> {
    let frames = frame_count(samples.len());
    if frames == 0 {
        return None;
    }
    let window = povey_window();
    let filters = mel_filters();
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_LEN);
    let mut input = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let mut power = vec![0.0f32; FFT_LEN / 2];
    let mut features = vec![0.0f32; frames * MEL_BINS];

    for frame in 0..frames {
        let start = frame * FRAME_SHIFT;
        let chunk = &samples[start..start + FRAME_LEN];
        let mean = chunk.iter().sum::<f32>() / FRAME_LEN as f32;
        for (slot, sample) in input.iter_mut().zip(chunk) {
            *slot = sample - mean;
        }
        for index in (1..FRAME_LEN).rev() {
            input[index] -= PREEMPHASIS * input[index - 1];
        }
        input[0] -= PREEMPHASIS * input[0];
        for (slot, weight) in input.iter_mut().zip(&window) {
            *slot *= weight;
        }
        for slot in &mut input[FRAME_LEN..] {
            *slot = 0.0;
        }
        fft.process(&mut input, &mut spectrum).ok()?;
        for (slot, value) in power.iter_mut().zip(&spectrum) {
            *slot = value.norm_sqr();
        }
        let row = &mut features[frame * MEL_BINS..(frame + 1) * MEL_BINS];
        for (slot, (first_bin, weights)) in row.iter_mut().zip(&filters) {
            let energy: f32 = weights
                .iter()
                .zip(&power[*first_bin..])
                .map(|(weight, value)| weight * value)
                .sum();
            *slot = energy.max(f32::EPSILON).ln();
        }
    }

    for bin in 0..MEL_BINS {
        let mean = (0..frames).map(|frame| features[frame * MEL_BINS + bin]).sum::<f32>() / frames as f32;
        for frame in 0..frames {
            features[frame * MEL_BINS + bin] -= mean;
        }
    }
    features.iter().all(|value| value.is_finite()).then_some(features)
}

fn povey_window() -> Vec<f32> {
    (0..FRAME_LEN)
        .map(|index| {
            let phase = 2.0 * std::f64::consts::PI * index as f64 / (FRAME_LEN - 1) as f64;
            (0.5 - 0.5 * phase.cos()).powf(0.85) as f32
        })
        .collect()
}

fn mel(hz: f32) -> f32 {
    1127.0 * (1.0 + hz / 700.0).ln()
}

/// Triangular filters in the mel domain, as Kaldi builds them. Each entry is
/// the first FFT bin with a non-zero weight and the run of weights from there.
fn mel_filters() -> Vec<(usize, Vec<f32>)> {
    let bin_width = SAMPLE_RATE as f32 / FFT_LEN as f32;
    let mel_low = mel(LOW_HZ);
    let mel_high = mel(HIGH_HZ);
    let delta = (mel_high - mel_low) / (MEL_BINS + 1) as f32;
    (0..MEL_BINS)
        .map(|bin| {
            let left = mel_low + bin as f32 * delta;
            let center = left + delta;
            let right = center + delta;
            let mut first = None;
            let mut weights = Vec::new();
            for index in 0..FFT_LEN / 2 {
                let point = mel(bin_width * index as f32);
                if point > left && point < right {
                    let weight = if point <= center {
                        (point - left) / (center - left)
                    } else {
                        (right - point) / (right - center)
                    };
                    if first.is_none() {
                        first = Some(index);
                    }
                    weights.push(weight);
                }
            }
            (first.unwrap_or(0), weights)
        })
        .collect()
}

/// Mel bin whose triangle is centered closest to `hz`.
#[cfg(test)]
fn bin_for(hz: f32) -> usize {
    let delta = (mel(HIGH_HZ) - mel(LOW_HZ)) / (MEL_BINS + 1) as f32;
    (((mel(hz) - mel(LOW_HZ)) / delta).round() as usize).clamp(1, MEL_BINS) - 1
}

/// Same as `fbank` without the mean normalization, for tests that look at
/// where the energy lands.
#[cfg(test)]
fn raw_fbank_mean(samples: &[f32]) -> Vec<f32> {
    let frames = frame_count(samples.len());
    let window = povey_window();
    let filters = mel_filters();
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_LEN);
    let mut input = fft.make_input_vec();
    let mut spectrum = fft.make_output_vec();
    let mut mean = vec![0.0f32; MEL_BINS];
    for frame in 0..frames {
        let start = frame * FRAME_SHIFT;
        input[..FRAME_LEN].copy_from_slice(&samples[start..start + FRAME_LEN]);
        for (slot, weight) in input.iter_mut().zip(&window) {
            *slot *= weight;
        }
        for slot in &mut input[FRAME_LEN..] {
            *slot = 0.0;
        }
        fft.process(&mut input, &mut spectrum).unwrap();
        for (slot, (first_bin, weights)) in mean.iter_mut().zip(&filters) {
            let energy: f32 = weights
                .iter()
                .zip(&spectrum[*first_bin..])
                .map(|(weight, value)| weight * value.norm_sqr())
                .sum();
            *slot += energy.max(f32::EPSILON).ln() / frames as f32;
        }
    }
    mean
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speakers::embed::tone;

    #[test]
    fn one_second_gives_98_frames_of_80_finite_bins() {
        let features = fbank(&tone(440.0, 1.0, SAMPLE_RATE)).unwrap();
        assert_eq!(frame_count(16_000), 98);
        assert_eq!(features.len(), 98 * MEL_BINS);
        assert!(features.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn audio_shorter_than_one_frame_has_no_features_and_silence_stays_finite() {
        assert!(fbank(&vec![0.1f32; FRAME_LEN - 1]).is_none());
        let silence = fbank(&vec![0.0f32; 8_000]).unwrap();
        assert!(silence.iter().all(|value| value.is_finite() && value.abs() < 1.0e-4));
    }

    #[test]
    fn mean_normalization_leaves_every_bin_centered_on_zero() {
        let mut samples = tone(300.0, 0.5, SAMPLE_RATE);
        samples.extend(tone(2500.0, 0.5, SAMPLE_RATE));
        let features = fbank(&samples).unwrap();
        let frames = features.len() / MEL_BINS;
        for bin in 0..MEL_BINS {
            let mean = (0..frames).map(|frame| features[frame * MEL_BINS + bin]).sum::<f32>() / frames as f32;
            assert!(mean.abs() < 1.0e-3, "bin {bin} mean {mean}");
        }
    }

    #[test]
    fn a_tone_puts_its_energy_in_the_matching_mel_bin() {
        for hz in [250.0f32, 1000.0, 3000.0, 6000.0] {
            let mean = raw_fbank_mean(&tone(hz, 1.0, SAMPLE_RATE));
            let loudest = mean
                .iter()
                .enumerate()
                .max_by(|left, right| left.1.total_cmp(right.1))
                .map(|(index, _)| index)
                .unwrap();
            let expected = bin_for(hz);
            assert!(loudest.abs_diff(expected) <= 1, "{hz} Hz landed in bin {loudest}, expected {expected}");
        }
    }

    #[test]
    fn the_filters_cover_the_band_and_peak_near_one() {
        let filters = mel_filters();
        assert_eq!(filters.len(), MEL_BINS);
        for (first, weights) in &filters {
            assert!(!weights.is_empty());
            assert!(first + weights.len() <= FFT_LEN / 2);
            assert!(weights.iter().all(|weight| (0.0..=1.0).contains(weight)));
        }
        assert!(filters[40].1.iter().cloned().fold(0.0f32, f32::max) > 0.5);
    }
}
