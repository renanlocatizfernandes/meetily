//! Which models to suggest for a machine. Pure: no I/O, no globals.
//!
//! The three functions run at the same time during a meeting (transcription,
//! voice vectors and the live assistant), so the suggestion is a set: the
//! best models whose estimated memory, added up, fits what the machine can
//! give to models. A model is never suggested alone if the set does not fit.

use serde::Serialize;

use super::catalog::{CatalogModel, Coverage, Role, CATALOG, PARAKEET_V3_LANGUAGES, ROLES};

/// What the recommendation needs to know about the machine.
#[derive(Debug, Clone, PartialEq)]
pub struct Machine {
    /// Installed memory in whole GB (8, 16, 24…).
    pub ram_gb: u32,
    pub apple_silicon: bool,
    /// Free space where the models are stored. `None` when unknown.
    pub free_disk_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    /// Less than 12 GB: the MacBook Air M2 with 8 GB is the reference.
    Light,
    /// 12 GB to less than 24 GB.
    Medium,
    /// 24 GB or more.
    High,
}

pub fn tier(ram_gb: u32) -> Tier {
    match ram_gb {
        0..=11 => Tier::Light,
        12..=23 => Tier::Medium,
        _ => Tier::High,
    }
}

/// Memory left for models, in MB: the total minus what macOS, this app and a
/// meeting app (Teams, Zoom, a browser) need. 3.5 GB on an 8 GB machine, plus
/// 30% of every GB above that, because people with more memory keep more open.
pub fn model_budget_mb(ram_gb: u32) -> u32 {
    let total = ram_gb * 1_000;
    let reserve = 3_500 + ram_gb.saturating_sub(8) * 300;
    total.saturating_sub(reserve)
}

/// Space kept free on the disk after a download.
const DISK_MARGIN_BYTES: u64 = 2_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    /// Fits together with the suggested models of the other two functions.
    Fits,
    /// The machine has less memory than the model needs to be worth offering.
    TooLittleMemory,
    /// It would not fit next to the suggested models of the other functions.
    DoesNotFitTogether,
    /// Needs the GPU of Apple Silicon.
    NeedsAppleSilicon,
    /// Not made for the meeting language.
    OtherLanguage,
    /// Not enough free disk space to download it.
    NoDiskSpace,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Option_ {
    pub model: &'static CatalogModel,
    pub memory_mb: u32,
    pub fit: Fit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RolePick {
    pub role: Role,
    /// `None` when nothing in the catalog fits this machine.
    pub model: Option<&'static CatalogModel>,
    pub memory_mb: u32,
    /// One sentence in Portuguese.
    pub reason: String,
    /// The other models of the function, best first, with whether they fit.
    pub others: Vec<Option_>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recommendation {
    pub tier: Tier,
    pub budget_mb: u32,
    /// Sum of the suggested set.
    pub total_memory_mb: u32,
    pub picks: Vec<RolePick>,
}

fn is_portuguese(language: &str) -> bool {
    let language = language.trim().to_ascii_lowercase();
    language == "pt" || language.starts_with("pt-") || language.starts_with("pt_")
}

/// `auto`, empty and unknown codes count as "any language".
fn base_language(language: &str) -> String {
    let language = language.trim().to_ascii_lowercase();
    language.split(['-', '_']).next().unwrap_or_default().to_string()
}

fn rank(model: &CatalogModel, language: &str) -> u8 {
    if is_portuguese(language) {
        model.rank_pt
    } else {
        model.rank_other
    }
}

fn speaks(model: &CatalogModel, language: &str) -> bool {
    let base = base_language(language);
    match model.coverage {
        Coverage::PortugueseTuned => base == "pt",
        // With automatic detection Parakeet v3 picks among its own languages.
        Coverage::European => base.is_empty() || base == "auto" || PARAKEET_V3_LANGUAGES.contains(&base.as_str()),
        Coverage::Multilingual | Coverage::ChineseEnglish => true,
    }
}

/// Why a model cannot be part of the set on this machine, ignoring the sum.
fn own_limit(model: &CatalogModel, machine: &Machine, language: &str, installed: &[&str]) -> Option<Fit> {
    if !speaks(model, language) || rank(model, language) == 0 {
        return Some(Fit::OtherLanguage);
    }
    if model.needs_apple_gpu && !machine.apple_silicon {
        return Some(Fit::NeedsAppleSilicon);
    }
    if machine.ram_gb < model.min_ram_gb {
        return Some(Fit::TooLittleMemory);
    }
    if !installed.contains(&model.id) {
        if let Some(free) = machine.free_disk_bytes {
            if model.download_bytes.saturating_add(DISK_MARGIN_BYTES) > free {
                return Some(Fit::NoDiskSpace);
            }
        }
    }
    None
}

fn gigabytes(megabytes: u32) -> String {
    format!("{:.1}", megabytes as f32 / 1_000.0).replace('.', ",")
}

/// `installed` holds the ids of models already on disk: they need no free
/// space. `language` is the meeting language code ("pt", "pt-BR", "en",
/// "auto"…).
pub fn recommend(machine: &Machine, language: &str, installed: &[&str]) -> Recommendation {
    let ram = machine.ram_gb;
    let budget = model_budget_mb(ram);
    let candidates = |role: Role| -> Vec<&'static CatalogModel> {
        CATALOG
            .iter()
            .filter(|model| model.role == role && own_limit(model, machine, language, installed).is_none())
            .collect()
    };
    let (speech, voices, assistant) = (candidates(Role::Speech), candidates(Role::Voices), candidates(Role::Assistant));

    // The best set that fits: speech first, then voices, then the assistant.
    // Ties go to the set that uses less memory.
    let mut best: Option<([&'static CatalogModel; 3], (u8, u8, u8), u32)> = None;
    for &s in &speech {
        for &v in &voices {
            for &a in &assistant {
                let total = s.memory_mb_on(ram) + v.memory_mb_on(ram) + a.memory_mb_on(ram);
                if total > budget {
                    continue;
                }
                let score = (rank(s, language), rank(v, language), rank(a, language));
                let better = match &best {
                    None => true,
                    Some((_, best_score, best_total)) => score > *best_score || (score == *best_score && total < *best_total),
                };
                if better {
                    best = Some(([s, v, a], score, total));
                }
            }
        }
    }

    let chosen = best.as_ref().map(|(set, _, _)| *set);
    let total_memory_mb = best.as_ref().map(|(_, _, total)| *total).unwrap_or(0);
    let picks = ROLES
        .iter()
        .enumerate()
        .map(|(index, &role)| {
            let model = chosen.map(|set| set[index]);
            let memory_mb = model.map(|model| model.memory_mb_on(ram)).unwrap_or(0);
            // What the other two suggested models leave for this function.
            let room = budget.saturating_sub(total_memory_mb - memory_mb);
            let mut others: Vec<Option_> = CATALOG
                .iter()
                .filter(|other| other.role == role && Some(other.id) != model.map(|model| model.id))
                .map(|other| {
                    let memory = other.memory_mb_on(ram);
                    let fit = own_limit(other, machine, language, installed).unwrap_or(if chosen.is_none() || memory > room {
                        Fit::DoesNotFitTogether
                    } else {
                        Fit::Fits
                    });
                    Option_ { model: other, memory_mb: memory, fit }
                })
                .collect();
            others.sort_by_key(|other| (other.fit != Fit::Fits, std::cmp::Reverse(rank(other.model, language))));
            let reason = match model {
                Some(model) => format!(
                    "{} {}; usa cerca de {} GB e o conjunto das três funções soma {} GB dos {} GB que este computador de {} GB pode dar aos modelos.",
                    model.name,
                    model.why,
                    gigabytes(memory_mb),
                    gigabytes(total_memory_mb),
                    gigabytes(budget),
                    ram
                ),
                None => format!(
                    "Nenhum conjunto de modelos locais cabe com segurança em {ram} GB de memória junto com a reunião."
                ),
            };
            RolePick { role, model, memory_mb, reason, others }
        })
        .collect();

    Recommendation { tier: tier(ram), budget_mb: budget, total_memory_mb, picks }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac(ram_gb: u32) -> Machine {
        Machine { ram_gb, apple_silicon: true, free_disk_bytes: Some(200_000_000_000) }
    }

    fn ids(recommendation: &Recommendation) -> Vec<&'static str> {
        recommendation.picks.iter().map(|pick| pick.model.map(|model| model.id).unwrap_or("-")).collect()
    }

    #[test]
    fn a_macbook_air_m2_with_8_gb_gets_a_light_set_for_the_three_functions() {
        let recommendation = recommend(&mac(8), "pt-BR", &[]);
        assert_eq!(recommendation.tier, Tier::Light);
        assert_eq!(ids(&recommendation), ["parakeet-tdt-0.6b-v3-int8", "campplus-zh-en", "llama3.2:latest"]);
        assert!(recommendation.total_memory_mb <= recommendation.budget_mb);
        assert_eq!(recommendation.budget_mb, 4_500);
    }

    #[test]
    fn sixteen_gb_moves_up_in_voices_and_assistant() {
        let recommendation = recommend(&mac(16), "pt", &[]);
        assert_eq!(recommendation.tier, Tier::Medium);
        assert_eq!(ids(&recommendation), ["parakeet-tdt-0.6b-v3-int8", "redimnet2-b6", "gemma4:e4b"]);
        assert!(recommendation.total_memory_mb <= recommendation.budget_mb);
    }

    #[test]
    fn twenty_four_gb_gets_the_medium_gemma() {
        let recommendation = recommend(&mac(24), "pt", &[]);
        assert_eq!(recommendation.tier, Tier::High);
        assert_eq!(ids(&recommendation), ["parakeet-tdt-0.6b-v3-int8", "redimnet2-b6", "gemma4:12b"]);
        assert!(recommendation.total_memory_mb <= recommendation.budget_mb);
    }

    #[test]
    fn the_set_always_fits_together_from_8_gb_up() {
        for ram in [8, 12, 16, 18, 24, 32, 36, 48, 64, 96, 128] {
            for language in ["pt-BR", "en", "ja", "auto", ""] {
                for apple_silicon in [true, false] {
                    let machine = Machine { ram_gb: ram, apple_silicon, free_disk_bytes: None };
                    let recommendation = recommend(&machine, language, &[]);
                    let sum: u32 = recommendation.picks.iter().map(|pick| pick.memory_mb).sum();
                    assert_eq!(sum, recommendation.total_memory_mb);
                    assert!(sum <= model_budget_mb(ram), "{ram} GB {language}");
                    if language == "ja" && !apple_silicon {
                        // Japanese needs Whisper, which needs the Apple GPU: nothing is suggested.
                        assert!(recommendation.picks.iter().all(|pick| pick.model.is_none()));
                        continue;
                    }
                    for pick in &recommendation.picks {
                        let model = pick.model.unwrap_or_else(|| panic!("{ram} GB {language} {:?}", pick.role));
                        assert!(model.min_ram_gb <= ram);
                        assert!(apple_silicon || !model.needs_apple_gpu);
                        assert!(!pick.reason.contains('—') && pick.reason.ends_with('.'));
                        assert_eq!(pick.reason.matches(". ").count(), 0, "one sentence: {}", pick.reason);
                        // Nothing marked as fitting would break the budget in place of the pick.
                        for other in pick.others.iter().filter(|other| other.fit == Fit::Fits) {
                            assert!(sum - pick.memory_mb + other.memory_mb <= model_budget_mb(ram));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn more_memory_never_gives_a_worse_set() {
        let mut last = (0u8, 0u8, 0u8);
        for ram in [8, 16, 24, 36, 48, 64] {
            let recommendation = recommend(&mac(ram), "pt", &[]);
            let score = (
                recommendation.picks[0].model.unwrap().rank_pt,
                recommendation.picks[1].model.unwrap().rank_pt,
                recommendation.picks[2].model.unwrap().rank_pt,
            );
            assert!(score >= last, "{ram} GB");
            last = score;
        }
        assert_eq!(ids(&recommend(&mac(48), "pt", &[]))[2], "gemma4:26b");
    }

    #[test]
    fn too_little_memory_recommends_nothing_instead_of_something_that_swaps() {
        let recommendation = recommend(&mac(4), "pt", &[]);
        assert!(recommendation.picks.iter().all(|pick| pick.model.is_none()));
        assert_eq!(recommendation.total_memory_mb, 0);
        assert!(recommendation.picks[0].reason.contains("4 GB"));
        assert!(recommendation.picks.iter().flat_map(|pick| &pick.others).all(|other| other.fit != Fit::Fits));
    }

    #[test]
    fn the_language_chooses_the_speech_model() {
        // English and the other Parakeet v3 languages keep Parakeet.
        assert_eq!(ids(&recommend(&mac(16), "en", &[]))[0], "parakeet-tdt-0.6b-v3-int8");
        assert_eq!(ids(&recommend(&mac(16), "auto", &[]))[0], "parakeet-tdt-0.6b-v3-int8");
        // Japanese is outside them: Whisper, sized by the memory.
        assert_eq!(ids(&recommend(&mac(8), "ja", &[]))[0], "large-v3-turbo-q5_0");
        assert_eq!(ids(&recommend(&mac(16), "ja", &[]))[0], "large-v3-turbo-q8_0");
        assert_eq!(ids(&recommend(&mac(32), "ja", &[]))[0], "large-v3");
        // The model tuned for Brazil is never offered for another language.
        let english = recommend(&mac(16), "en", &[]);
        let tuned = english.picks[0].others.iter().find(|other| other.model.id.contains("tagarela-int8")).unwrap();
        assert_eq!(tuned.fit, Fit::OtherLanguage);
        // For Portuguese it is an alternative that fits.
        let portuguese = recommend(&mac(8), "pt-BR", &[]);
        let tuned = portuguese.picks[0].others.iter().find(|other| other.model.id.contains("tagarela-int8")).unwrap();
        assert_eq!(tuned.fit, Fit::Fits);
    }

    #[test]
    fn without_apple_silicon_whisper_is_not_suggested() {
        let machine = Machine { ram_gb: 16, apple_silicon: false, free_disk_bytes: None };
        assert_eq!(ids(&recommend(&machine, "pt", &[]))[0], "parakeet-tdt-0.6b-v3-int8");
        let japanese = recommend(&machine, "ja", &[]);
        assert!(japanese.picks.iter().all(|pick| pick.model.is_none()));
        assert!(japanese.picks[0].others.iter().any(|other| other.fit == Fit::NeedsAppleSilicon));
    }

    #[test]
    fn a_full_disk_keeps_only_what_is_installed_or_small() {
        let machine = Machine { ram_gb: 24, apple_silicon: true, free_disk_bytes: Some(5_000_000_000) };
        let recommendation = recommend(&machine, "pt", &[]);
        // 8 GB of gemma4:12b do not fit in 5 GB free; llama3.2 (2 GB) does.
        assert_eq!(ids(&recommendation)[2], "llama3.2:latest");
        let big = recommendation.picks[2].others.iter().find(|other| other.model.id == "gemma4:12b").unwrap();
        assert_eq!(big.fit, Fit::NoDiskSpace);
        // Already installed: the disk does not matter.
        let recommendation = recommend(&machine, "pt", &["gemma4:12b"]);
        assert_eq!(ids(&recommendation)[2], "gemma4:12b");
    }

    #[test]
    fn the_budget_grows_with_the_memory() {
        assert_eq!(model_budget_mb(8), 4_500);
        assert_eq!(model_budget_mb(16), 10_100);
        assert_eq!(model_budget_mb(24), 15_700);
        assert_eq!(model_budget_mb(2), 0);
        assert_eq!(tier(8), Tier::Light);
        assert_eq!(tier(16), Tier::Medium);
        assert_eq!(tier(18), Tier::Medium);
        assert_eq!(tier(24), Tier::High);
    }
}
