use std::sync::Mutex;

use super::embed::cosine;

pub const PROFILE_MATCH: f32 = 0.94;

#[derive(Debug, Clone)]
pub struct VoiceProfile {
    pub name: String,
    pub embedding: Vec<f32>,
    pub updated_ms: i64,
}

static PROFILES: Mutex<Vec<VoiceProfile>> = Mutex::new(Vec::new());
static EMBEDDINGS: Mutex<Vec<(String, Vec<f32>)>> = Mutex::new(Vec::new());

pub fn set_profiles(profiles: Vec<VoiceProfile>) {
    if let Ok(mut guard) = PROFILES.lock() {
        *guard = profiles;
    }
}

pub fn remember_embedding(key: &str, embedding: Vec<f32>) {
    if let Ok(mut guard) = EMBEDDINGS.lock() {
        if let Some(slot) = guard.iter_mut().find(|(existing, _)| existing == key) {
            slot.1 = embedding;
        } else {
            guard.push((key.to_string(), embedding));
        }
    }
}

pub fn embedding_for(key: &str) -> Option<Vec<f32>> {
    EMBEDDINGS
        .lock()
        .ok()
        .and_then(|guard| guard.iter().find(|(existing, _)| existing == key).map(|(_, embedding)| embedding.clone()))
}

pub fn suggest_name(embedding: &[f32]) -> Option<String> {
    best_profile(embedding, &PROFILES.lock().ok()?.clone()).map(|(name, _)| name)
}

pub fn best_profile(embedding: &[f32], profiles: &[VoiceProfile]) -> Option<(String, f32)> {
    let mut best: Option<(String, f32, i64)> = None;
    for profile in profiles {
        let score = cosine(embedding, &profile.embedding);
        if score < PROFILE_MATCH {
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

    #[test]
    fn a_close_profile_matches_and_a_distant_one_does_not() {
        let ana = vector(0.2);
        let profiles = vec![VoiceProfile {
            name: "Ana".into(),
            embedding: ana.clone(),
            updated_ms: 1,
        }];
        let (name, score) = best_profile(&ana, &profiles).unwrap();
        assert_eq!(name, "Ana");
        assert!(score > 0.99);
        assert!(best_profile(&vector(0.9), &profiles).is_none());
    }

    #[test]
    fn the_newer_profile_wins_a_tie() {
        let voice = vector(0.3);
        let profiles = vec![
            VoiceProfile { name: "Ana".into(), embedding: voice.clone(), updated_ms: 1 },
            VoiceProfile { name: "Ana Costa".into(), embedding: voice.clone(), updated_ms: 5 },
        ];
        assert_eq!(best_profile(&voice, &profiles).unwrap().0, "Ana Costa");
    }
}
