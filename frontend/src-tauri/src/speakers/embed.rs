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

/// Where a voice vector came from. Vectors of different kinds (or sizes) live
/// in unrelated spaces and are never compared with each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EmbeddingKind {
    /// 16-band log spectrum from this file. Stored as a plain JSON array.
    Spectrum,
    /// CAM++ speaker model (`speakers::neural`), 192 values.
    Neural,
    /// ReDimNet2-B6 speaker model (`speakers::neural`), 192 values. Same size
    /// as CAM++ but another space, hence its own kind.
    NeuralRedimnet,
}

/// Tag written to `voice_profiles.embedding` for neural vectors. Bump it when
/// the model changes, so vectors from the old model are ignored.
pub const NEURAL_TAG: &str = "neural-v1";
/// The same for ReDimNet2-B6 vectors. Older builds do not know this tag and
/// read such a cell as "no neural vector".
pub const NEURAL_REDIMNET_TAG: &str = "redimnet2-b6-v1";

impl EmbeddingKind {
    pub fn is_neural(self) -> bool {
        self != EmbeddingKind::Spectrum
    }

    /// Storage tag of a neural kind.
    pub fn tag(self) -> Option<&'static str> {
        match self {
            EmbeddingKind::Spectrum => None,
            EmbeddingKind::Neural => Some(NEURAL_TAG),
            EmbeddingKind::NeuralRedimnet => Some(NEURAL_REDIMNET_TAG),
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        [EmbeddingKind::Neural, EmbeddingKind::NeuralRedimnet].into_iter().find(|kind| kind.tag() == Some(tag))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Embedding {
    pub kind: EmbeddingKind,
    pub values: Vec<f32>,
}

impl Embedding {
    pub fn spectrum(values: Vec<f32>) -> Self {
        Self { kind: EmbeddingKind::Spectrum, values }
    }

    /// Normalizes to unit length. `None` when the vector is empty, has a
    /// non-finite value or is all zeros.
    pub fn neural(values: Vec<f32>) -> Option<Self> {
        Self::neural_of(EmbeddingKind::Neural, values)
    }

    /// The same for the given neural kind. `None` for `Spectrum`.
    pub fn neural_of(kind: EmbeddingKind, mut values: Vec<f32>) -> Option<Self> {
        if !kind.is_neural() || values.is_empty() || values.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm <= 1.0e-6 {
            return None;
        }
        for value in &mut values {
            *value /= norm;
        }
        Some(Self { kind, values })
    }

    /// Cosine similarity, or `None` when kind or dimension differ.
    pub fn similarity(&self, other: &Embedding) -> Option<f32> {
        if self.kind != other.kind || self.values.len() != other.values.len() || self.values.is_empty() {
            return None;
        }
        Some(cosine(&self.values, &other.values))
    }
}

/// Reads one `voice_profiles.embedding` cell. A plain array is the legacy
/// spectrum vector. An object carries a tagged neural vector and, optionally,
/// the spectrum vector saved for the same name and the vectors of other
/// neural models:
/// `{"kind":"neural-v1","dim":192,"v":[...],"spectrum":[...],"more":[{"kind":"redimnet2-b6-v1","dim":192,"v":[...]}]}`.
/// Unknown tags and wrong dimensions are dropped.
pub fn parse_stored(text: &str) -> Vec<Embedding> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let floats = |value: &serde_json::Value| -> Vec<f32> {
        serde_json::from_value::<Vec<f32>>(value.clone()).unwrap_or_default()
    };
    let tagged = |map: &serde_json::Map<String, serde_json::Value>| -> Option<Embedding> {
        let values = map.get("v").map(floats).unwrap_or_default();
        let dim = map.get("dim").and_then(|dim| dim.as_u64()).unwrap_or(0) as usize;
        let kind = map.get("kind").and_then(|kind| kind.as_str()).and_then(EmbeddingKind::from_tag)?;
        if dim != values.len() {
            return None;
        }
        Embedding::neural_of(kind, values)
    };
    let mut found: Vec<Embedding> = Vec::new();
    match &value {
        serde_json::Value::Array(_) => {
            let values = floats(&value);
            if !values.is_empty() {
                found.push(Embedding::spectrum(values));
            }
        }
        serde_json::Value::Object(map) => {
            found.extend(tagged(map));
            let more = map.get("more").and_then(|more| more.as_array()).cloned().unwrap_or_default();
            for entry in more.iter().filter_map(|entry| entry.as_object()) {
                if let Some(embedding) = tagged(entry) {
                    if !found.iter().any(|existing| existing.kind == embedding.kind) {
                        found.push(embedding);
                    }
                }
            }
            let spectrum = map.get("spectrum").map(floats).unwrap_or_default();
            if !spectrum.is_empty() {
                found.push(Embedding::spectrum(spectrum));
            }
        }
        _ => {}
    }
    found
}

/// Writes the cell `parse_stored` reads. With only a spectrum vector the
/// output is the legacy plain array, exactly what older builds wrote. With
/// vectors of more than one neural model, CAM++ goes first: it is the one
/// older builds can read.
pub fn to_stored(embeddings: &[Embedding]) -> Option<String> {
    let pick = |kind: EmbeddingKind| {
        embeddings
            .iter()
            .find(|embedding| embedding.kind == kind && !embedding.values.is_empty())
    };
    let spectrum = pick(EmbeddingKind::Spectrum);
    let neural: Vec<(&'static str, &Embedding)> = [EmbeddingKind::Neural, EmbeddingKind::NeuralRedimnet]
        .into_iter()
        .filter_map(|kind| Some((kind.tag()?, pick(kind)?)))
        .collect();
    match neural.split_first() {
        None => spectrum.and_then(|embedding| serde_json::to_string(&embedding.values).ok()),
        Some(((kind, first), rest)) => serde_json::to_string(&StoredNeural {
            kind,
            dim: first.values.len(),
            v: &first.values,
            spectrum: spectrum.map(|embedding| embedding.values.as_slice()),
            more: rest
                .iter()
                .map(|(kind, embedding)| StoredMore { kind, dim: embedding.values.len(), v: &embedding.values })
                .collect(),
        })
        .ok(),
    }
}

#[derive(serde::Serialize)]
struct StoredNeural<'a> {
    kind: &'static str,
    dim: usize,
    v: &'a [f32],
    #[serde(skip_serializing_if = "Option::is_none")]
    spectrum: Option<&'a [f32]>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    more: Vec<StoredMore<'a>>,
}

#[derive(serde::Serialize)]
struct StoredMore<'a> {
    kind: &'static str,
    dim: usize,
    v: &'a [f32],
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

    #[test]
    fn a_legacy_array_is_a_spectrum_vector_and_is_written_back_unchanged() {
        let parsed = parse_stored("[0.6,0.8]");
        assert_eq!(parsed, vec![Embedding::spectrum(vec![0.6, 0.8])]);
        assert_eq!(to_stored(&parsed).unwrap(), "[0.6,0.8]");
    }

    #[test]
    fn vectors_of_two_neural_models_are_stored_side_by_side_and_never_compared() {
        let campplus = Embedding::neural(vec![1.0, 0.0]).unwrap();
        let redimnet = Embedding::neural_of(EmbeddingKind::NeuralRedimnet, vec![0.0, 2.0]).unwrap();
        assert_eq!(campplus.similarity(&redimnet), None);
        assert!(Embedding::neural_of(EmbeddingKind::Spectrum, vec![1.0]).is_none());

        let alone = to_stored(&[redimnet.clone()]).unwrap();
        assert!(alone.contains("\"kind\":\"redimnet2-b6-v1\"") && !alone.contains("more"));
        assert_eq!(parse_stored(&alone), vec![redimnet.clone()]);

        let both = to_stored(&[redimnet.clone(), Embedding::spectrum(vec![1.0, 0.0]), campplus.clone()]).unwrap();
        // CAM++ first, so a build that only knows "neural-v1" still reads it.
        assert!(both.starts_with("{\"kind\":\"neural-v1\""));
        let parsed = parse_stored(&both);
        assert_eq!(parsed.len(), 3);
        assert!(parsed.contains(&campplus) && parsed.contains(&redimnet));
        assert!(parsed.contains(&Embedding::spectrum(vec![1.0, 0.0])));
        // An unknown extra model is dropped, the rest stays.
        let unknown = r#"{"kind":"neural-v1","dim":2,"v":[1.0,0.0],"more":[{"kind":"other-v1","dim":2,"v":[0.0,1.0]}]}"#;
        assert_eq!(parse_stored(unknown), vec![campplus]);
    }

    #[test]
    fn a_neural_vector_round_trips_tagged_and_older_builds_read_it_as_nothing() {
        let neural = Embedding::neural(vec![3.0, 4.0]).unwrap();
        assert!((neural.values[0] - 0.6).abs() < 1.0e-6);
        let text = to_stored(&[neural.clone()]).unwrap();
        assert!(text.contains("\"kind\":\"neural-v1\""));
        assert!(text.contains("\"dim\":2"));
        assert_eq!(parse_stored(&text), vec![neural.clone()]);
        // What the previous build did with the cell: parse as Vec<f32>, skip on failure.
        assert!(serde_json::from_str::<Vec<f32>>(&text).is_err());

        let both = to_stored(&[Embedding::spectrum(vec![1.0, 0.0]), neural.clone()]).unwrap();
        let parsed = parse_stored(&both);
        assert_eq!(parsed.len(), 2);
        assert!(parsed.contains(&neural));
        assert!(parsed.contains(&Embedding::spectrum(vec![1.0, 0.0])));
    }

    #[test]
    fn unknown_tags_wrong_sizes_and_junk_are_dropped() {
        assert!(parse_stored(r#"{"kind":"neural-v9","dim":2,"v":[1.0,0.0]}"#).is_empty());
        assert!(parse_stored(r#"{"kind":"neural-v1","dim":3,"v":[1.0,0.0]}"#).is_empty());
        assert!(parse_stored(r#"{"kind":"neural-v1","dim":2,"v":[0.0,0.0]}"#).is_empty());
        assert!(parse_stored("not json").is_empty());
        assert!(parse_stored("[]").is_empty());
        assert!(to_stored(&[]).is_none());
        assert!(Embedding::neural(vec![f32::NAN, 1.0]).is_none());
    }

    #[test]
    fn different_kinds_or_sizes_are_never_compared() {
        let spectrum = Embedding::spectrum(vec![1.0, 0.0]);
        let neural = Embedding::neural(vec![1.0, 0.0]).unwrap();
        let wider = Embedding::neural(vec![1.0, 0.0, 0.0]).unwrap();
        assert_eq!(spectrum.similarity(&neural), None);
        assert_eq!(neural.similarity(&wider), None);
        assert!(neural.similarity(&neural).unwrap() > 0.999);
    }
}
