use std::sync::Mutex;

use super::embed::{Embedding, EmbeddingKind};

/// Spectrum vectors: a saved voice matches at or above this cosine.
pub const PROFILE_MATCH: f32 = 0.94;
/// Neural vectors (CAM++): a voice saved in another meeting matches at or
/// above this cosine. Higher than the in-meeting threshold in `book.rs`
/// because a wrong name is worse than "Pessoa 2", and another meeting means
/// another microphone and room. Not tuned on real meetings yet.
pub const NEURAL_PROFILE_MATCH: f32 = 0.62;
/// The same for ReDimNet2-B6 vectors, whose scores sit higher (see `book.rs`).
pub const REDIMNET_PROFILE_MATCH: f32 = 0.64;

fn neural_profile_match(kind: EmbeddingKind) -> f32 {
    match kind {
        EmbeddingKind::NeuralRedimnet => REDIMNET_PROFILE_MATCH,
        _ => NEURAL_PROFILE_MATCH,
    }
}

#[derive(Debug, Clone)]
pub struct VoiceProfile {
    pub name: String,
    /// At most one vector per kind.
    pub embeddings: Vec<Embedding>,
    pub updated_ms: i64,
}

impl VoiceProfile {
    fn of_kind(&self, kind: EmbeddingKind) -> Option<&Embedding> {
        self.embeddings.iter().find(|embedding| embedding.kind == kind)
    }
}

static PROFILES: Mutex<Vec<VoiceProfile>> = Mutex::new(Vec::new());
static EMBEDDINGS: Mutex<Vec<(String, Vec<Embedding>)>> = Mutex::new(Vec::new());

pub fn set_profiles(profiles: Vec<VoiceProfile>) {
    if let Ok(mut guard) = PROFILES.lock() {
        *guard = profiles;
    }
}

/// Keeps the current centroids of a speaker key, for `rename_speaker` to save.
pub fn remember_embeddings(key: &str, embeddings: Vec<Embedding>) {
    if embeddings.is_empty() {
        return;
    }
    if let Ok(mut guard) = EMBEDDINGS.lock() {
        if let Some(slot) = guard.iter_mut().find(|(existing, _)| existing == key) {
            slot.1 = embeddings;
        } else {
            guard.push((key.to_string(), embeddings));
        }
    }
}

pub fn embeddings_for(key: &str) -> Vec<Embedding> {
    EMBEDDINGS
        .lock()
        .ok()
        .and_then(|guard| guard.iter().find(|(existing, _)| existing == key).map(|(_, embeddings)| embeddings.clone()))
        .unwrap_or_default()
}

pub fn suggest_name(embeddings: &[Embedding]) -> Option<String> {
    best_profile(embeddings, &PROFILES.lock().ok()?.clone()).map(|(name, _)| name)
}

/// Neural first. The spectrum is consulted only for names that have no vector
/// of the neural model in use (saved before it was installed or chosen), so a
/// name the model knows is never handed out on the weaker evidence.
pub fn best_profile(embeddings: &[Embedding], profiles: &[VoiceProfile]) -> Option<(String, f32)> {
    let neural = embeddings.iter().find(|embedding| embedding.kind.is_neural());
    if let Some(probe) = neural {
        if let Some(found) = best_of_kind(probe, profiles, neural_profile_match(probe.kind), None) {
            return Some(found);
        }
    }
    let probe = embeddings.iter().find(|embedding| embedding.kind == EmbeddingKind::Spectrum)?;
    best_of_kind(probe, profiles, PROFILE_MATCH, neural.map(|embedding| embedding.kind))
}

fn best_of_kind(
    probe: &Embedding,
    profiles: &[VoiceProfile],
    threshold: f32,
    skip_names_with: Option<EmbeddingKind>,
) -> Option<(String, f32)> {
    let mut best: Option<(String, f32, i64)> = None;
    for profile in profiles {
        if skip_names_with.is_some_and(|kind| profile.of_kind(kind).is_some()) {
            continue;
        }
        let Some(score) = profile.of_kind(probe.kind).and_then(|saved| probe.similarity(saved)) else {
            continue;
        };
        if score < threshold {
            continue;
        }
        let replace = match &best {
            None => true,
            Some((_, current, updated)) => score > *current + 0.001 || ((score - current).abs() <= 0.001 && profile.updated_ms >= *updated),
        };
        if replace {
            best = Some((profile.name.clone(), score, profile.updated_ms));
        }
    }
    best.map(|(name, score, _)| (name, score))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(seed: f32) -> Vec<f32> {
        let mut values = vec![seed, 1.0 - seed, 0.2, 0.1];
        let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        for value in &mut values {
            *value /= norm;
        }
        values
    }

    fn spectrum(seed: f32) -> Embedding {
        Embedding::spectrum(vector(seed))
    }

    fn neural(seed: f32) -> Embedding {
        Embedding::neural(vector(seed)).unwrap()
    }

    fn profile(name: &str, embeddings: Vec<Embedding>, updated_ms: i64) -> VoiceProfile {
        VoiceProfile { name: name.into(), embeddings, updated_ms }
    }

    #[test]
    fn a_close_profile_matches_and_a_distant_one_does_not() {
        let ana = spectrum(0.2);
        let profiles = vec![profile("Ana", vec![ana.clone()], 1)];
        let (name, score) = best_profile(&[ana], &profiles).unwrap();
        assert_eq!(name, "Ana");
        assert!(score > 0.99);
        assert!(best_profile(&[spectrum(0.9)], &profiles).is_none());
    }

    #[test]
    fn the_newer_profile_wins_a_tie() {
        let voice = spectrum(0.3);
        let profiles = vec![
            profile("Ana", vec![voice.clone()], 1),
            profile("Ana Costa", vec![voice.clone()], 5),
        ];
        assert_eq!(best_profile(&[voice], &profiles).unwrap().0, "Ana Costa");
    }

    #[test]
    fn a_neural_voice_is_not_compared_with_a_spectrum_profile_of_the_same_numbers() {
        // Same four numbers, different kinds: no match either way.
        let profiles = vec![profile("Ana", vec![spectrum(0.2)], 1)];
        assert!(best_profile(&[neural(0.2)], &profiles).is_none());
        let profiles = vec![profile("Ana", vec![neural(0.2)], 1)];
        assert!(best_profile(&[spectrum(0.2)], &profiles).is_none());
    }

    #[test]
    fn neural_profiles_use_their_own_lower_threshold() {
        // cos(vector(0.2), vector(0.45)) is about 0.91: a neural match, not a spectrum one.
        let profiles = vec![profile("Ana", vec![neural(0.2)], 1)];
        let (name, score) = best_profile(&[neural(0.45)], &profiles).unwrap();
        assert_eq!(name, "Ana");
        assert!(score < PROFILE_MATCH && score >= NEURAL_PROFILE_MATCH);
        let profiles = vec![profile("Ana", vec![spectrum(0.2)], 1)];
        assert!(best_profile(&[spectrum(0.45)], &profiles).is_none());
    }

    #[test]
    fn the_spectrum_names_only_people_saved_before_the_model() {
        let turn = vec![neural(0.9), spectrum(0.2)];
        // Bia has a neural vector that does not match: her spectrum is not consulted.
        let bia = profile("Bia", vec![neural(0.1), spectrum(0.2)], 9);
        assert!(best_profile(&turn, &[bia.clone()]).is_none());
        // Ana was saved before the model, with the spectrum only.
        let ana = profile("Ana", vec![spectrum(0.2)], 1);
        assert_eq!(best_profile(&turn, &[bia, ana]).unwrap().0, "Ana");
        // A neural match wins over a spectrum one.
        let carla = profile("Carla", vec![neural(0.9)], 1);
        let ana = profile("Ana", vec![spectrum(0.2)], 5);
        assert_eq!(best_profile(&turn, &[ana, carla]).unwrap().0, "Carla");
    }

    #[test]
    fn a_name_saved_with_one_neural_model_is_still_found_by_the_spectrum_when_the_other_model_is_in_use() {
        let redimnet = |seed: f32| Embedding::neural_of(EmbeddingKind::NeuralRedimnet, vector(seed)).unwrap();
        let profiles = vec![profile("Ana", vec![neural(0.9), spectrum(0.5)], 1)];
        // The probe comes from ReDimNet2: the CAM++ vector of Ana says nothing about it.
        let found = best_profile(&[redimnet(0.1), spectrum(0.5)], &profiles);
        assert_eq!(found.map(|(name, _)| name).as_deref(), Some("Ana"));
        // Once Ana has a ReDimNet2 vector, it decides, with its own threshold.
        let profiles = vec![profile("Ana", vec![redimnet(0.9), spectrum(0.5)], 1)];
        assert!(best_profile(&[redimnet(0.1), spectrum(0.5)], &profiles).is_none());
        assert_eq!(best_profile(&[redimnet(0.9)], &profiles).map(|(name, _)| name).as_deref(), Some("Ana"));
        assert!(REDIMNET_PROFILE_MATCH > NEURAL_PROFILE_MATCH);
    }
}
