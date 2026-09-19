use crate::{
    corpus::Edit,
    dictionary::{Candidate, Dictionary, protected_words},
    frequency::{FrequencyConfig, FrequencyScorer},
    text::{LexicalToken, Tokens, tokenize},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub max_distance: usize,
    pub min_word_length: usize,
    pub max_word_length: usize,
    pub max_candidates: usize,
    pub max_search_nodes: usize,
    pub protect_titlecase: bool,
    pub protect_uppercase: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            max_distance: 1,
            min_word_length: 5,
            max_word_length: 64,
            max_candidates: 20,
            max_search_nodes: 100_000,
            protect_titlecase: true,
            protect_uppercase: true,
        }
    }
}

impl Policy {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.max_distance == 1,
            "this baseline supports max_distance=1 only"
        );
        ensure!(
            (4..=128).contains(&self.min_word_length)
                && self.min_word_length <= self.max_word_length
                && self.max_word_length <= 128,
            "require 4 <= min_word_length <= max_word_length <= 128"
        );
        ensure!(
            (2..=1000).contains(&self.max_candidates),
            "require 2..1000 max_candidates"
        );
        ensure!(
            self.max_search_nodes > 0,
            "max_search_nodes must be positive"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub dictionary: PathBuf,
    pub domain: Option<PathBuf>,
    pub ignore: Option<PathBuf>,
    pub frequency: Option<FrequencyConfig>,
    #[serde(default)]
    pub strict_yo: bool,
    #[serde(default)]
    pub policy: Policy,
}

#[derive(Clone, Debug, Serialize)]
pub struct Decision {
    pub token: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub original: String,
    /// Selected output; equal to original for KEEP decisions.
    pub candidate: String,
    pub confidence: Option<f64>,
    pub candidate_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_margin: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_up_margin: Option<f64>,
    pub reason: &'static str,
    pub alternatives: Vec<Candidate>,
    pub search_complete: bool,
    pub visited_nodes: usize,
}

impl Decision {
    pub fn keep(source: &str, index: usize, token: &LexicalToken, reason: &'static str) -> Self {
        let original = source[token.byte_start..token.byte_end].to_owned();
        Self {
            token: index,
            byte_start: token.byte_start,
            byte_end: token.byte_end,
            candidate: original.clone(),
            original,
            confidence: None,
            candidate_score: None,
            keep_score: None,
            keep_margin: None,
            runner_up_margin: None,
            reason,
            alternatives: vec![],
            search_complete: true,
            visited_nodes: 0,
        }
    }
    pub fn changed(&self) -> bool {
        self.original != self.candidate
    }
}

pub trait Corrector {
    fn name(&self) -> &'static str;
    fn strict_yo(&self) -> bool;
    fn suggest(&self, source: &str, index: usize, token: &LexicalToken) -> Decision;
    fn known(&self, _key: &str) -> Option<bool> {
        None
    }
}

pub struct Keep {
    pub strict_yo: bool,
}
impl Corrector for Keep {
    fn name(&self) -> &'static str {
        "keep"
    }
    fn strict_yo(&self) -> bool {
        self.strict_yo
    }
    fn suggest(&self, source: &str, index: usize, token: &LexicalToken) -> Decision {
        Decision::keep(source, index, token, "keep_baseline")
    }
}

pub struct Baseline {
    pub dictionary: Dictionary,
    pub domain: BTreeSet<String>,
    pub ignore: BTreeSet<String>,
    pub policy: Policy,
    pub strict_yo: bool,
    pub frequency: Option<FrequencyScorer>,
}

impl Baseline {
    pub fn load(path: &Path) -> Result<(Self, serde_json::Value)> {
        let contents = fs::read_to_string(path).context("read baseline config")?;
        let config: Config = toml::from_str(&contents).context("parse baseline TOML")?;
        config.policy.validate()?;
        if let Some(frequency) = &config.frequency {
            frequency.scoring.validate()?;
        }
        let parent = path.parent().unwrap_or(Path::new("."));
        let mut hashes = BTreeMap::new();
        hashes.insert(
            "config",
            format!("{:x}", Sha256::digest(contents.as_bytes())),
        );
        let mut read = |name, resource: &Path| -> Result<String> {
            let data = fs::read_to_string(parent.join(resource))
                .with_context(|| format!("read {name}: {}", resource.display()))?;
            hashes.insert(name, format!("{:x}", Sha256::digest(data.as_bytes())));
            Ok(data)
        };
        let data = read("dictionary", &config.dictionary)?;
        let dictionary = if config.dictionary.extension().is_some_and(|s| s == "json") {
            Dictionary::from_json(&data, config.strict_yo)?
        } else {
            Dictionary::from_text(&data, config.strict_yo)?
        };
        let domain = if let Some(p) = &config.domain {
            protected_words(&read("domain", p)?, config.strict_yo)?
        } else {
            BTreeSet::new()
        };
        let ignore = if let Some(p) = &config.ignore {
            protected_words(&read("ignore", p)?, config.strict_yo)?
        } else {
            BTreeSet::new()
        };
        let frequency = config
            .frequency
            .as_ref()
            .map(|f| {
                FrequencyScorer::from_tsv(
                    &read("frequency", &f.path)?,
                    config.strict_yo,
                    f.scoring.clone(),
                )
            })
            .transpose()?;
        let metadata = serde_json::json!({"config":config,"resource_sha256":hashes,
            "dictionary_words":dictionary.len(),"trie_nodes":dictionary.node_count(),
            "domain_words":domain.len(),"ignore_words":ignore.len(),
            "frequency_words":frequency.as_ref().map(FrequencyScorer::len),
            "decision_policy": if frequency.is_some() {
                "OOV + complete distance-1 search + bounded frequency prior + KEEP and runner-up margins; uncalibrated"
            } else { "OOV + exactly one candidate in complete distance-1 search; uncalibrated" }});
        Ok((
            Self {
                dictionary,
                domain,
                ignore,
                policy: config.policy,
                strict_yo: config.strict_yo,
                frequency,
            },
            metadata,
        ))
    }
}

impl Corrector for Baseline {
    fn name(&self) -> &'static str {
        if self.frequency.is_some() {
            "levenshtein_frequency"
        } else {
            "levenshtein_unique"
        }
    }
    fn strict_yo(&self) -> bool {
        self.strict_yo
    }
    fn known(&self, key: &str) -> Option<bool> {
        Some(self.dictionary.contains(key))
    }
    fn suggest(&self, source: &str, index: usize, token: &LexicalToken) -> Decision {
        let mut result = Decision::keep(source, index, token, "no_candidates");
        let word = &result.original;
        let all_upper = word
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase);
        let protected = if self.ignore.contains(&token.key) {
            Some("ignore_dictionary")
        } else if self.domain.contains(&token.key) {
            Some("domain_dictionary")
        } else if self.dictionary.contains(&token.key) {
            Some("known_word")
        } else {
            token.protection.filter(|&reason| {
                !(reason == "uppercase_or_mixed_case"
                    && all_upper
                    && !self.policy.protect_uppercase)
            })
        };
        let protected = protected.or_else(|| {
            let len = token.key.chars().count();
            if len < self.policy.min_word_length {
                Some("below_min_length")
            } else if len > self.policy.max_word_length {
                Some("above_max_length")
            } else if self.policy.protect_titlecase
                && !all_upper
                && word.chars().next().is_some_and(char::is_uppercase)
            {
                Some("titlecase")
            } else {
                None
            }
        });
        if let Some(reason) = protected {
            result.reason = reason;
            return result;
        }
        let mut search = self.dictionary.lookup(
            &token.key,
            self.policy.max_distance,
            self.policy.max_candidates,
            self.policy.max_search_nodes,
        );
        result.search_complete = search.complete;
        result.visited_nodes = search.visited_nodes;
        result.reason = if !search.complete {
            search.stop_reason.unwrap()
        } else if search.candidates.len() > 1 {
            "ambiguous_candidates"
        } else if search.candidates.is_empty() {
            "no_candidates"
        } else {
            "unique_distance_one"
        };
        if let Some(frequency) = &self.frequency {
            for candidate in &mut search.candidates {
                candidate.frequency_per_billion = frequency.get(&candidate.key);
                candidate.score = frequency.score(&candidate.key, candidate.distance);
            }
            search.candidates.sort_by(|a, b| {
                b.score
                    .total_cmp(&a.score)
                    .then_with(|| a.key.cmp(&b.key))
                    .then_with(|| a.word.cmp(&b.word))
            });
            let keep_score = frequency.keep_score(&token.key);
            result.keep_score = Some(keep_score);
            if let Some(best) = search.candidates.first() {
                let keep_margin = best.score - keep_score;
                let runner_margin = search
                    .candidates
                    .get(1)
                    .map(|other| best.score - other.score);
                result.keep_margin = Some(keep_margin);
                result.runner_up_margin = runner_margin;
                if search.complete {
                    result.reason = if !best.frequency_per_billion.is_some_and(|v| v > 0.0) {
                        "candidate_frequency_unknown"
                    } else if keep_margin < frequency.policy.min_keep_margin {
                        "keep_score_margin"
                    } else if runner_margin
                        .is_some_and(|v| v < frequency.policy.min_runner_up_margin)
                    {
                        "runner_up_score_margin"
                    } else {
                        "frequency_margins_passed"
                    };
                }
            }
        }
        result.candidate_score = search.candidates.first().map(|c| c.score);
        if matches!(
            result.reason,
            "unique_distance_one" | "frequency_margins_passed"
        ) {
            let chosen = &search.candidates[0].word;
            result.candidate = if all_upper {
                chosen.to_uppercase()
            } else if word.chars().next().is_some_and(char::is_uppercase) {
                let mut chars = chosen.chars();
                chars.next().unwrap().to_uppercase().collect::<String>() + chars.as_str()
            } else {
                chosen.clone()
            };
        }
        result.alternatives = search.candidates;
        result
    }
}

pub struct Analysis {
    pub tokens: Tokens,
    pub decisions: Vec<Decision>,
    pub edits: Vec<Edit>,
    pub corrected: String,
}

pub fn analyze(source: &str, engine: &dyn Corrector) -> Analysis {
    let tokens = tokenize(source, engine.strict_yo());
    let decisions: Vec<_> = tokens
        .lexical
        .iter()
        .enumerate()
        .map(|(i, t)| engine.suggest(source, i, t))
        .collect();
    let mut edits = Vec::new();
    let mut corrected = String::new();
    let mut cursor = 0;
    for decision in &decisions {
        if decision.changed() {
            corrected.push_str(&source[cursor..decision.byte_start]);
            corrected.push_str(&decision.candidate);
            cursor = decision.byte_end;
            edits.push(Edit {
                byte_start: decision.byte_start,
                byte_end: decision.byte_end,
                token_start: decision.token,
                token_end: decision.token + 1,
                replacement: decision.candidate.clone(),
            });
        }
    }
    corrected.push_str(&source[cursor..]);
    Analysis {
        tokens,
        decisions,
        edits,
        corrected,
    }
}
