//! Online clustering of one-speaker turns. Each channel keeps its own clusters
//! so the microphone is not merged with remote participants.

use super::alias::display_name;
use super::embed::{cosine, Embedder};

const MATCH_THRESHOLD: f32 = 0.90;
const MIN_NEW_SECONDS: f32 = 0.8;

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
    pub embedding: Option<Vec<f32>>,
}

struct Cluster {
    key: String,
    channel: Channel,
    embedding: Vec<f32>,
    samples: u32,
    default_label: String,
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
    pub fn assign(&mut self, channel: Channel, samples: &[f32], sample_rate: u32) -> Option<Assignment> {
        let seconds = samples.len() as f32 / sample_rate.max(1) as f32;
        let embedding = self.embedder.embed(samples, sample_rate);
        let Some(embedding) = embedding else {
            return self.inherit(channel);
        };

        if let Some(index) = self.best_match(channel, &embedding) {
            self.update_centroid(index, &embedding);
            let key = self.clusters[index].key.clone();
            let label = self.clusters[index].default_label.clone();
            let embedding = self.clusters[index].embedding.clone();
            self.remember(channel, &key);
            return Some(self.finish(key, label, Some(embedding)));
        }

        if seconds < MIN_NEW_SECONDS {
            return self.inherit(channel);
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
            key: key.clone(),
            channel,
            embedding: embedding.clone(),
            samples: 1,
            default_label: default_label.clone(),
        });
        self.remember(channel, &key);
        Some(self.finish(key, default_label, Some(embedding)))
    }

    fn finish(&self, key: String, default_label: String, embedding: Option<Vec<f32>>) -> Assignment {
        Assignment {
            label: display_name(&key, &default_label),
            key,
            embedding,
        }
    }

    fn best_match(&self, channel: Channel, embedding: &[f32]) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (index, cluster) in self.clusters.iter().enumerate() {
            if cluster.channel != channel {
                continue;
            }
            let score = cosine(&cluster.embedding, embedding);
            if score >= MATCH_THRESHOLD && best.map(|(_, current)| score > current).unwrap_or(true) {
                best = Some((index, score));
            }
        }
        best.map(|(index, _)| index)
    }

    fn update_centroid(&mut self, index: usize, embedding: &[f32]) {
        let cluster = &mut self.clusters[index];
        let count = cluster.samples.min(8) as f32;
        for (slot, value) in cluster.embedding.iter_mut().zip(embedding) {
            *slot = (*slot * count + value) / (count + 1.0);
        }
        let norm = cluster.embedding.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 1.0e-6 {
            for value in &mut cluster.embedding {
                *value /= norm;
            }
        }
        cluster.samples = cluster.samples.saturating_add(1);
    }

    fn remember(&mut self, channel: Channel, key: &str) {
        match channel {
            Channel::Mic => self.last_mic = Some(key.to_string()),
            Channel::System => self.last_system = Some(key.to_string()),
        }
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
        let embedding = found.map(|cluster| cluster.embedding.clone());
        Some(self.finish(key, label, embedding))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::speakers::alias::{clear_aliases, rename};
    use crate::speakers::embed::tone;

    static LOCK: Mutex<()> = Mutex::new(());

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
}
