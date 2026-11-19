//! Online clustering of one-speaker turns. Each channel keeps its own clusters
//! so the microphone is not merged with remote participants.
//!
//! A cluster keeps one centroid per embedding kind. A turn is compared in one
//! kind only: neural when the turn has a neural vector and every cluster of
//! that channel has one too, otherwise the spectrum. That is what lets the
//! meeting continue on the spectrum if the model stops working halfway.

use super::alias::display_name;
use super::embed::{cosine, Embedder, Embedding, EmbeddingKind};

/// Spectrum vectors: cosine at or above this is the same person.
const MATCH_THRESHOLD: f32 = 0.90;
/// Spectrum mode: a turn shorter than this never starts a new person.
const MIN_NEW_SECONDS: f32 = 0.8;

/// Neural vectors (CAM++, cosine against the cluster centroid): at or above
/// this is the same person. Published operating points for this model family
/// sit between 0.5 and 0.6. With five synthetic pt-BR voices of one speech
/// synthesizer (a hard case: they share most of their timbre), 3 s turns of
/// the same voice scored 0.83 or more and different voices 0.56 or less at
/// the 95th percentile (0.67 at worst); at 1.5 s the same voice dropped to
/// 0.61 at the 5th percentile. With 20 real Portuguese speakers (Multilingual
/// LibriSpeech, read speech, 12 turns each) 3 s turns scored 0.575 or more
/// against their own centroid and 0.549 at most against another one.
/// Not tuned on real meetings yet.
const NEURAL_MATCH: f32 = 0.58;
/// Neural mode: a turn shorter than `NEURAL_MIN_NEW_SECONDS` that misses
/// `NEURAL_MATCH` still goes to the closest person when it reaches this.
/// Short turns score lower against their own speaker, and starting a new
/// person for each of them would scatter one voice over many labels.
const NEURAL_LOOSE_MATCH: f32 = 0.40;
/// Neural mode: a turn shorter than this starts a new person only when the
/// channel has nobody yet.
const NEURAL_MIN_NEW_SECONDS: f32 = 1.5;

/// ReDimNet2-B6 scores sit higher than CAM++ for the same and for different
/// people. On the same 20 Portuguese speakers: 3 s turns scored 0.633 or more
/// against their own centroid (0.765 at 10 s) and 0.638 at most against
/// another one (99th percentile 0.52); pairs of single 3 s turns of one
/// person scored 0.545 at the 1st percentile, pairs of different people 0.48
/// at the 99th. 0.60 keeps both kinds of mistake rare there. At 1.5 s
/// different people reached 0.50 at the 99th percentile, hence the loose
/// threshold. Read speech from audiobooks, not meetings.
const REDIMNET_MATCH: f32 = 0.60;
const REDIMNET_LOOSE_MATCH: f32 = 0.50;

/// Thresholds of one neural model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeuralRule {
    pub same_person: f32,
    pub loose: f32,
    pub min_new_seconds: f32,
}

/// `None` for the spectrum, which has its own constants above.
pub fn neural_rule(kind: EmbeddingKind) -> Option<NeuralRule> {
    match kind {
        EmbeddingKind::Spectrum => None,
        EmbeddingKind::Neural => Some(NeuralRule {
            same_person: NEURAL_MATCH,
            loose: NEURAL_LOOSE_MATCH,
            min_new_seconds: NEURAL_MIN_NEW_SECONDS,
        }),
        EmbeddingKind::NeuralRedimnet => Some(NeuralRule {
            same_person: REDIMNET_MATCH,
            loose: REDIMNET_LOOSE_MATCH,
            min_new_seconds: NEURAL_MIN_NEW_SECONDS,
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Mic,
    System,
}

impl Channel {
    pub fn key_prefix(self) -> &'static str {
        match self {
            Channel::Mic => "mic",
            Channel::System => "system",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub key: String,
    pub label: String,
    /// Centroids of this person, neural first when there is one.
    pub embeddings: Vec<Embedding>,
}

#[derive(Default)]
struct Centroid {
    values: Vec<f32>,
    samples: u32,
}

impl Centroid {
    fn new(values: Vec<f32>) -> Self {
        Self { values, samples: 1 }
    }

    fn add(&mut self, embedding: &[f32]) {
        if self.values.len() != embedding.len() {
            return;
        }
        let count = self.samples.min(8) as f32;
        for (slot, value) in self.values.iter_mut().zip(embedding) {
            *slot = (*slot * count + value) / (count + 1.0);
        }
        let norm = self.values.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 1.0e-6 {
            for value in &mut self.values {
                *value /= norm;
            }
        }
        self.samples = self.samples.saturating_add(1);
    }
}

struct Cluster {
    key: String,
    channel: Channel,
    spectrum: Option<Centroid>,
    /// Centroid of one neural model, with the model it came from.
    neural: Option<(EmbeddingKind, Centroid)>,
    default_label: String,
}

impl Cluster {
    fn centroid(&self, kind: EmbeddingKind) -> Option<&Centroid> {
        match kind {
            EmbeddingKind::Spectrum => self.spectrum.as_ref(),
            _ => self.neural.as_ref().filter(|(own, _)| *own == kind).map(|(_, centroid)| centroid),
        }
    }

    /// A neural vector of another model than the one this person already has
    /// is left out: the two spaces cannot be averaged.
    fn absorb(&mut self, spectrum: Option<&[f32]>, neural: Option<(EmbeddingKind, &[f32])>) {
        if let Some(values) = spectrum {
            match &mut self.spectrum {
                Some(centroid) => centroid.add(values),
                None => self.spectrum = Some(Centroid::new(values.to_vec())),
            }
        }
        if let Some((kind, values)) = neural {
            match &mut self.neural {
                Some((own, centroid)) if *own == kind => centroid.add(values),
                Some(_) => {}
                None => self.neural = Some((kind, Centroid::new(values.to_vec()))),
            }
        }
    }

    fn embeddings(&self) -> Vec<Embedding> {
        let mut found = Vec::new();
        if let Some((kind, centroid)) = &self.neural {
            found.push(Embedding { kind: *kind, values: centroid.values.clone() });
        }
        if let Some(centroid) = &self.spectrum {
            found.push(Embedding { kind: EmbeddingKind::Spectrum, values: centroid.values.clone() });
        }
        found
    }
}

#[derive(Default)]
pub struct SpeakerBook {
    embedder: Embedder,
    clusters: Vec<Cluster>,
    next_person: u32,
    last_mic: Option<String>,
    last_system: Option<String>,
    mic_clusters: u32,
}

impl SpeakerBook {
    /// Spectrum only, as before the neural model existed.
    pub fn assign(&mut self, channel: Channel, samples: &[f32], sample_rate: u32) -> Option<Assignment> {
        self.assign_with(channel, samples, sample_rate, None)
    }

    /// `neural` is the model's vector for this turn, when the model is in use
    /// and produced one. The spectrum is always computed as well.
    pub fn assign_with(
        &mut self,
        channel: Channel,
        samples: &[f32],
        sample_rate: u32,
        neural: Option<Embedding>,
    ) -> Option<Assignment> {
        let seconds = samples.len() as f32 / sample_rate.max(1) as f32;
        let spectrum = self.embedder.embed(samples, sample_rate);
        let neural = neural.filter(|embedding| embedding.kind.is_neural() && !embedding.values.is_empty());
        let neural_kind = neural.as_ref().map(|embedding| embedding.kind);
        let neural = neural.map(|embedding| embedding.values);

        let neural_mode = neural_kind.zip(neural.as_ref()).is_some_and(|(kind, values)| {
            self.clusters
                .iter()
                .filter(|cluster| cluster.channel == channel)
                .all(|cluster| cluster.centroid(kind).is_some_and(|centroid| centroid.values.len() == values.len()))
        });
        let rule = neural_kind.filter(|_| neural_mode).and_then(|kind| Some((kind, neural_rule(kind)?)));
        let (kind, probe, threshold, min_new_seconds) = match (rule, &neural, &spectrum) {
            (Some((kind, rule)), Some(values), _) => (kind, values, rule.same_person, rule.min_new_seconds),
            (_, _, Some(values)) => (EmbeddingKind::Spectrum, values, MATCH_THRESHOLD, MIN_NEW_SECONDS),
            _ => return self.inherit(channel),
        };
        let loose = rule.map(|(_, rule)| rule.loose);

        let best = self.best_match(channel, kind, probe);
        if let Some((index, _)) = best.filter(|(_, score)| *score >= threshold) {
            self.clusters[index].absorb(spectrum.as_deref(), neural_kind.zip(neural.as_deref()));
            return Some(self.settle(channel, index));
        }

        if seconds < min_new_seconds {
            let channel_is_empty = !self.clusters.iter().any(|cluster| cluster.channel == channel);
            if kind.is_neural() && channel_is_empty && seconds >= MIN_NEW_SECONDS {
                // First voice of the channel: nobody to inherit from.
            } else if let Some((index, _)) =
                best.filter(|(_, score)| loose.is_some_and(|loose| *score >= loose))
            {
                return Some(self.settle(channel, index));
            } else {
                return self.inherit(channel);
            }
        }

        let index = self.clusters.len();
        let key = format!("{}:{index}", channel.key_prefix());
        let default_label = if channel == Channel::Mic && self.mic_clusters == 0 {
            self.mic_clusters += 1;
            "Você".to_string()
        } else {
            if channel == Channel::Mic {
                self.mic_clusters += 1;
            }
            self.next_person += 1;
            format!("Pessoa {}", self.next_person)
        };
        self.clusters.push(Cluster {
            key,
            channel,
            spectrum: spectrum.map(Centroid::new),
            neural: neural_kind.zip(neural).map(|(kind, values)| (kind, Centroid::new(values))),
            default_label,
        });
        Some(self.settle(channel, index))
    }

    fn settle(&mut self, channel: Channel, index: usize) -> Assignment {
        let key = self.clusters[index].key.clone();
        match channel {
            Channel::Mic => self.last_mic = Some(key.clone()),
            Channel::System => self.last_system = Some(key.clone()),
        }
        let cluster = &self.clusters[index];
        Assignment {
            label: display_name(&key, &cluster.default_label),
            key,
            embeddings: cluster.embeddings(),
        }
    }

    /// Closest cluster of the channel in one embedding kind, whatever the score.
    fn best_match(&self, channel: Channel, kind: EmbeddingKind, embedding: &[f32]) -> Option<(usize, f32)> {
        let mut best: Option<(usize, f32)> = None;
        for (index, cluster) in self.clusters.iter().enumerate() {
            if cluster.channel != channel {
                continue;
            }
            let Some(centroid) = cluster.centroid(kind) else { continue };
            if centroid.values.len() != embedding.len() {
                continue;
            }
            let score = cosine(&centroid.values, embedding);
            if best.map(|(_, current)| score > current).unwrap_or(true) {
                best = Some((index, score));
            }
        }
        best
    }

    fn inherit(&self, channel: Channel) -> Option<Assignment> {
        let key = match channel {
            Channel::Mic => self.last_mic.clone(),
            Channel::System => self.last_system.clone(),
        }?;
        let found = self.clusters.iter().find(|cluster| cluster.key == key);
        let label = found
            .map(|cluster| cluster.default_label.clone())
            .unwrap_or_else(|| "Pessoa".to_string());
        Some(Assignment {
            label: display_name(&key, &label),
            key,
            embeddings: found.map(Cluster::embeddings).unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::speakers::alias::{clear_aliases, rename};
    use crate::speakers::embed::tone;

    static LOCK: Mutex<()> = Mutex::new(());

    /// A unit vector mostly along `axis`, tilted by `tilt` toward the next axis.
    fn voice(axis: usize, tilt: f32) -> Option<Embedding> {
        let mut values = vec![0.0f32; 192];
        values[axis] = 1.0;
        values[axis + 1] = tilt;
        Embedding::neural(values)
    }

    #[test]
    fn distinct_tones_on_system_audio_become_different_people() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let mut book = SpeakerBook::default();
        let first = book.assign(Channel::System, &tone(180.0, 1.2, 16_000), 16_000).unwrap();
        let again = book.assign(Channel::System, &tone(180.0, 1.2, 16_000), 16_000).unwrap();
        let second = book.assign(Channel::System, &tone(2200.0, 1.2, 16_000), 16_000).unwrap();
        assert_eq!(first.key, again.key);
        assert_eq!(first.label, "Pessoa 1");
        assert_ne!(first.key, second.key);
        assert_eq!(second.label, "Pessoa 2");
        assert!(first.embeddings.iter().all(|embedding| embedding.kind == EmbeddingKind::Spectrum));
    }

    #[test]
    fn the_first_microphone_voice_is_you_and_a_second_voice_is_numbered() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let mut book = SpeakerBook::default();
        let you = book.assign(Channel::Mic, &tone(150.0, 1.0, 16_000), 16_000).unwrap();
        let other = book.assign(Channel::Mic, &tone(1800.0, 1.0, 16_000), 16_000).unwrap();
        assert_eq!(you.label, "Você");
        assert_eq!(other.label, "Pessoa 1");
    }

    #[test]
    fn a_short_follow_up_keeps_the_previous_person_and_a_rename_sticks() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let mut book = SpeakerBook::default();
        let first = book.assign(Channel::System, &tone(200.0, 1.0, 16_000), 16_000).unwrap();
        rename(first.key.clone(), "Ana".to_string());
        let short = vec![0.0f32; 1600];
        let inherited = book.assign(Channel::System, &short, 16_000).unwrap();
        assert_eq!(inherited.key, first.key);
        assert_eq!(inherited.label, "Ana");
        rename(first.key, String::new());
    }

    #[test]
    fn neural_vectors_separate_voices_the_spectrum_would_merge() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let same_sound = tone(180.0, 2.0, 16_000);
        let mut book = SpeakerBook::default();
        let ana = book.assign_with(Channel::System, &same_sound, 16_000, voice(0, 0.0)).unwrap();
        let bia = book.assign_with(Channel::System, &same_sound, 16_000, voice(10, 0.0)).unwrap();
        let ana_again = book.assign_with(Channel::System, &same_sound, 16_000, voice(0, 0.5)).unwrap();
        assert_ne!(ana.key, bia.key);
        assert_eq!(ana.key, ana_again.key);
        assert_eq!(ana.label, "Pessoa 1");
        assert_eq!(bia.label, "Pessoa 2");
        assert_eq!(ana_again.embeddings[0].kind, EmbeddingKind::Neural);
        assert_eq!(ana_again.embeddings[0].values.len(), 192);
        assert_eq!(ana_again.embeddings[1].kind, EmbeddingKind::Spectrum);
    }

    #[test]
    fn neural_vectors_merge_one_voice_whose_spectrum_changed() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let mut book = SpeakerBook::default();
        let low = book.assign_with(Channel::System, &tone(180.0, 2.0, 16_000), 16_000, voice(3, 0.0)).unwrap();
        let high = book.assign_with(Channel::System, &tone(2200.0, 2.0, 16_000), 16_000, voice(3, 0.3)).unwrap();
        assert_eq!(low.key, high.key);
    }

    #[test]
    fn a_similarity_just_under_the_neural_threshold_is_another_person() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let sound = tone(180.0, 2.0, 16_000);
        let mut book = SpeakerBook::default();
        let first = book.assign_with(Channel::System, &sound, 16_000, voice(0, 0.0)).unwrap();
        // cos = 1 / sqrt(1 + tilt^2): tilt 1.6 gives 0.53, tilt 1.3 gives 0.61.
        let below = book.assign_with(Channel::System, &sound, 16_000, voice(0, 1.6)).unwrap();
        assert_ne!(first.key, below.key);
        let mut book = SpeakerBook::default();
        let first = book.assign_with(Channel::System, &sound, 16_000, voice(0, 0.0)).unwrap();
        let above = book.assign_with(Channel::System, &sound, 16_000, voice(0, 1.3)).unwrap();
        assert_eq!(first.key, above.key);
    }

    #[test]
    fn a_short_neural_turn_goes_to_the_closest_person_or_the_previous_one() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let long = tone(180.0, 2.0, 16_000);
        let short = tone(180.0, 1.0, 16_000);
        let mut book = SpeakerBook::default();
        let ana = book.assign_with(Channel::System, &long, 16_000, voice(0, 0.0)).unwrap();
        let bia = book.assign_with(Channel::System, &long, 16_000, voice(10, 0.0)).unwrap();
        // cos 0.45 with Ana: under the match threshold, over the loose one.
        let near_ana = book.assign_with(Channel::System, &short, 16_000, voice(0, 2.0)).unwrap();
        assert_eq!(near_ana.key, ana.key);
        // Close to nobody: keeps the previous person (Ana), no third person.
        let nobody = book.assign_with(Channel::System, &short, 16_000, voice(50, 0.0)).unwrap();
        assert_eq!(nobody.key, ana.key);
        // The same vector on a long turn does start a new person.
        let carla = book.assign_with(Channel::System, &long, 16_000, voice(50, 0.0)).unwrap();
        assert_ne!(carla.key, ana.key);
        assert_ne!(carla.key, bia.key);
        assert_eq!(carla.label, "Pessoa 3");
    }

    #[test]
    fn the_first_voice_of_a_channel_is_created_from_a_one_second_neural_turn() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let mut book = SpeakerBook::default();
        let you = book.assign_with(Channel::Mic, &tone(150.0, 1.0, 16_000), 16_000, voice(0, 0.0)).unwrap();
        assert_eq!(you.label, "Você");
    }

    #[test]
    fn when_the_model_stops_mid_meeting_the_spectrum_takes_over_without_mixing() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let mut book = SpeakerBook::default();
        let ana = book.assign_with(Channel::System, &tone(180.0, 2.0, 16_000), 16_000, voice(0, 0.0)).unwrap();
        let bia = book.assign_with(Channel::System, &tone(2200.0, 2.0, 16_000), 16_000, voice(10, 0.0)).unwrap();
        // No neural vector any more: matched on the spectrum centroids.
        let ana_again = book.assign_with(Channel::System, &tone(180.0, 2.0, 16_000), 16_000, None).unwrap();
        let bia_again = book.assign_with(Channel::System, &tone(2200.0, 2.0, 16_000), 16_000, None).unwrap();
        assert_eq!(ana_again.key, ana.key);
        assert_eq!(bia_again.key, bia.key);
    }

    #[test]
    fn a_person_created_without_the_model_is_matched_on_the_spectrum_and_then_gains_a_neural_centroid() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let sound = tone(180.0, 2.0, 16_000);
        let mut book = SpeakerBook::default();
        let before = book.assign(Channel::System, &sound, 16_000).unwrap();
        assert_eq!(before.embeddings.len(), 1);
        // The channel has a person without a neural centroid: spectrum decides.
        let after = book.assign_with(Channel::System, &sound, 16_000, voice(0, 0.0)).unwrap();
        assert_eq!(after.key, before.key);
        assert_eq!(after.embeddings[0].kind, EmbeddingKind::Neural);
        // Now every person has one, so a different neural voice with the same sound is new.
        let other = book.assign_with(Channel::System, &sound, 16_000, voice(10, 0.0)).unwrap();
        assert_ne!(other.key, before.key);
    }

    #[test]
    fn a_neural_vector_of_another_size_is_not_compared_with_the_existing_people() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let sound = tone(180.0, 2.0, 16_000);
        let mut book = SpeakerBook::default();
        let first = book.assign_with(Channel::System, &sound, 16_000, voice(0, 0.0)).unwrap();
        let odd = Embedding::neural(vec![1.0, 0.0, 0.0]);
        // Falls back to the spectrum, where the sound is the same.
        let second = book.assign_with(Channel::System, &sound, 16_000, odd).unwrap();
        assert_eq!(first.key, second.key);
    }

    fn redimnet_voice(axis: usize, tilt: f32) -> Option<Embedding> {
        voice(axis, tilt).and_then(|embedding| Embedding::neural_of(EmbeddingKind::NeuralRedimnet, embedding.values))
    }

    #[test]
    fn each_neural_model_uses_its_own_threshold() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let sound = tone(180.0, 2.0, 16_000);
        // tilt 1.36 gives cos 0.592: the same person for CAM++ (0.58), another one for ReDimNet2 (0.60).
        let mut book = SpeakerBook::default();
        let first = book.assign_with(Channel::System, &sound, 16_000, voice(0, 0.0)).unwrap();
        let second = book.assign_with(Channel::System, &sound, 16_000, voice(0, 1.36)).unwrap();
        assert_eq!(first.key, second.key);
        let mut book = SpeakerBook::default();
        let first = book.assign_with(Channel::System, &sound, 16_000, redimnet_voice(0, 0.0)).unwrap();
        let second = book.assign_with(Channel::System, &sound, 16_000, redimnet_voice(0, 1.36)).unwrap();
        assert_ne!(first.key, second.key);
        assert_eq!(second.embeddings[0].kind, EmbeddingKind::NeuralRedimnet);
        // tilt 1.2 gives 0.64: the same person for both.
        let third = book.assign_with(Channel::System, &sound, 16_000, redimnet_voice(0, 0.3)).unwrap();
        assert_eq!(first.key, third.key);
        assert!(neural_rule(EmbeddingKind::Spectrum).is_none());
        let (campplus, redimnet) = (neural_rule(EmbeddingKind::Neural).unwrap(), neural_rule(EmbeddingKind::NeuralRedimnet).unwrap());
        assert!(redimnet.same_person > campplus.same_person && redimnet.loose > campplus.loose);
        assert!(redimnet.loose < redimnet.same_person && campplus.loose < campplus.same_person);
    }

    #[test]
    fn a_vector_of_another_neural_model_is_not_compared_with_the_existing_people() {
        let _lock = LOCK.lock().unwrap();
        clear_aliases();
        let sound = tone(180.0, 2.0, 16_000);
        let mut book = SpeakerBook::default();
        let first = book.assign_with(Channel::System, &sound, 16_000, voice(0, 0.0)).unwrap();
        // Same numbers, other model: falls back to the spectrum, where the sound is the same,
        // and the person keeps the centroid of the first model.
        let second = book.assign_with(Channel::System, &sound, 16_000, redimnet_voice(5, 0.0)).unwrap();
        assert_eq!(first.key, second.key);
        assert_eq!(second.embeddings[0].kind, EmbeddingKind::Neural);
    }
}
