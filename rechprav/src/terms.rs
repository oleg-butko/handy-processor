//! Experimental term-only pass. Scores are preferences, never probabilities.
use crate::{
    corpus::Edit,
    corrector::Baseline,
    term_context::{Pattern, horizontal},
    text::{self, Kind, Tokens},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

fn yes() -> bool {
    true
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    Exact,
    Edit,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnownWords {
    #[default]
    Never,
    Context,
    Always,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub mode: MatchMode,
    pub max_distance: usize,
    pub max_tokens: usize,
    pub max_comparisons: usize,
    pub context_window: usize,
    pub boost: f64,
    pub edit_penalty: f64,
    pub context_bonus: f64,
    pub known_penalty: f64,
    pub min_score: f64,
    pub min_margin: f64,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            mode: MatchMode::Exact,
            max_distance: 1,
            max_tokens: 3,
            max_comparisons: 20_000,
            context_window: 4,
            boost: 0.0,
            edit_penalty: 1.0,
            context_bonus: 2.0,
            known_penalty: 1.0,
            min_score: 3.0,
            min_margin: 1.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub id: String,
    pub canonical: String,
    pub aliases: Vec<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub boost: f64,
    #[serde(default)]
    pub known_words: KnownWords,
    #[serde(default)]
    pub context: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_known_distance: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_patterns: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_exclusions: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub require_context: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TermSet {
    pub id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub boost: f64,
    pub terms: Vec<Term>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Used only for membership, strict_yo and ignore/domain protection, not B2 edits.
    pub baseline_config: PathBuf,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub policy: Policy,
    pub sets: Vec<TermSet>,
}

fn bounded(value: f64, lower: f64, upper: f64) -> bool {
    value.is_finite() && (lower..=upper).contains(&value)
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        let p = &self.policy;
        ensure!(
            p.max_distance <= 2
                && (1..=3).contains(&p.max_tokens)
                && (1..=100_000).contains(&p.max_comparisons)
                && p.context_window <= 8,
            "invalid term search bounds"
        );
        ensure!(
            bounded(p.boost, -10.0, 10.0)
                && bounded(p.edit_penalty, 0.01, 10.0)
                && bounded(p.context_bonus, 0.0, 10.0)
                && bounded(p.known_penalty, 0.0, 10.0)
                && bounded(p.min_score, 0.01, 100.0)
                && bounded(p.min_margin, 0.01, 100.0),
            "invalid term scoring policy; KEEP and conflict margins must be positive"
        );
        ensure!(self.sets.len() <= 64, "too many term sets");
        let mut sets = BTreeSet::new();
        let mut alias_count = 0;
        for set in &self.sets {
            ensure!(
                valid_id(&set.id) && sets.insert(&set.id),
                "invalid/duplicate set id"
            );
            ensure!(bounded(set.boost, -10.0, 10.0), "invalid set boost");
            let mut terms = BTreeSet::new();
            ensure!(set.terms.len() <= 512, "too many terms");
            for term in &set.terms {
                ensure!(
                    valid_id(&term.id) && terms.insert(&term.id),
                    "invalid/duplicate term id"
                );
                ensure!(bounded(term.boost, -10.0, 10.0), "invalid term boost");
                let canonical = text::tokenize(&term.canonical, true);
                ensure!(
                    canonical.lexical.len() == 1
                        && canonical.lexical[0].byte_start == 0
                        && canonical.lexical[0].byte_end == term.canonical.len()
                        && (1..=64).contains(&term.canonical.chars().count())
                        && term
                            .canonical
                            .chars()
                            .next()
                            .is_some_and(char::is_alphabetic)
                        && term
                            .canonical
                            .chars()
                            .all(|c| c.is_alphanumeric() || "+#_-".contains(c)),
                    "canonical term must be one technical word/abbreviation"
                );
                ensure!(
                    !term.aliases.is_empty() && term.aliases.len() <= 64,
                    "require 1..64 aliases per term"
                );
                for alias in &term.aliases {
                    let parts: Vec<_> = alias.split(' ').collect();
                    ensure!(
                        (1..=3).contains(&parts.len())
                            && parts.iter().all(|w| russian(w))
                            && alias.chars().count() <= 128,
                        "alias must contain 1..3 Russian words separated by single spaces"
                    );
                }
                ensure!(
                    term.context.len() <= 64
                        && term.context.iter().all(|word| !word.is_empty()
                            && word.chars().count() <= 64
                            && word.chars().all(char::is_alphabetic)),
                    "context cues must be individual alphabetic words"
                );
                ensure!(
                    term.max_known_distance.is_none_or(|d| d <= 2),
                    "max_known_distance must be in 0..2"
                );
                ensure!(
                    term.context_patterns.len() <= 32 && term.context_exclusions.len() <= 32,
                    "too many context patterns"
                );
                for pattern in term.context_patterns.iter().chain(&term.context_exclusions) {
                    Pattern::parse(pattern, false, p.context_window)?;
                }
                ensure!(
                    !(term.known_words == KnownWords::Context || term.require_context)
                        || !term.context.is_empty()
                        || !term.context_patterns.is_empty(),
                    "context policy needs cues"
                );
                alias_count += term.aliases.len();
            }
        }
        ensure!(alias_count <= 4096, "too many aliases (limit 4096)");
        Ok(())
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn russian(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c) || c == 'ё' || c == 'Ё')
}
struct Entry {
    set: String,
    term: Term,
    boost: f64,
    aliases: Vec<(String, String)>,
    context: BTreeSet<String>,
    patterns: Vec<Pattern>,
    exclusions: Vec<Pattern>,
}

pub struct TermEngine {
    pub config: Config,
    baseline: Option<Baseline>,
    entries: Vec<Entry>,
    canonical: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub set: String,
    pub term: String,
    pub canonical: String,
    pub alias: String,
    pub distance: usize,
    pub score: f64,
    pub context_hits: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub context_pattern_hits: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub context_exclusion_hits: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_known_distance: Option<usize>,
    pub contains_known_word: bool,
    pub eligible: bool,
    pub gate: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Decision {
    pub token_start: usize,
    pub token_end: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub original: String,
    pub candidates: Vec<Candidate>,
    pub runner_up_margin: Option<f64>,
    pub selected: Option<String>,
    pub reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Analysis {
    pub corrected: String,
    pub edits: Vec<Edit>,
    pub decisions: Vec<Decision>,
    pub complete: bool,
    pub comparisons: usize,
}

impl TermEngine {
    pub fn load(path: &Path, disable: bool) -> Result<(Self, serde_json::Value)> {
        let contents = fs::read_to_string(path).context("read term config")?;
        let mut config: Config = toml::from_str(&contents).context("parse term config")?;
        if disable {
            config.enabled = false;
        }
        config.validate()?;
        let active = config.enabled
            && config
                .sets
                .iter()
                .any(|s| s.enabled && s.terms.iter().any(|t| t.enabled));
        let (baseline, base_metadata) = if active {
            let (b, m) = Baseline::load(
                &path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(&config.baseline_config),
            )?;
            (Some(b), Some(m))
        } else {
            (None, None)
        };
        let engine = Self::new(config, baseline)?;
        let metadata = serde_json::json!({"config_sha256":crate::hashing::sha256_hex(contents.as_bytes()),
            "effective_config":engine.config, "baseline_resources":base_metadata,
            "active_terms":engine.entries.len(), "scoring":"heuristic preferences, not probabilities; term-only pass"});
        Ok((engine, metadata))
    }

    pub fn new(config: Config, baseline: Option<Baseline>) -> Result<Self> {
        config.validate()?;
        let strict = baseline.as_ref().is_some_and(|b| b.strict_yo);
        let mut entries = Vec::new();
        let mut canonical = BTreeSet::new();
        if config.enabled {
            for set in config.sets.iter().filter(|s| s.enabled) {
                for term in set.terms.iter().filter(|t| t.enabled) {
                    ensure!(
                        baseline.is_some(),
                        "active terms require baseline membership/protection"
                    );
                    canonical.insert(text::key(&term.canonical, strict));
                    entries.push(Entry {
                        set: set.id.clone(),
                        term: term.clone(),
                        boost: set.boost + term.boost,
                        aliases: term
                            .aliases
                            .iter()
                            .map(|a| {
                                let a = text::key(a, strict);
                                let compact = a.replace(' ', "");
                                (a, compact)
                            })
                            .collect(),
                        context: term.context.iter().map(|w| text::key(w, strict)).collect(),
                        patterns: term
                            .context_patterns
                            .iter()
                            .map(|p| Pattern::parse(p, strict, config.policy.context_window))
                            .collect::<Result<_>>()?,
                        exclusions: term
                            .context_exclusions
                            .iter()
                            .map(|p| Pattern::parse(p, strict, config.policy.context_window))
                            .collect::<Result<_>>()?,
                    });
                }
            }
        }
        Ok(Self {
            config,
            baseline,
            entries,
            canonical,
        })
    }

    fn allowed(&self, source: &str, tokens: &Tokens, index: usize) -> bool {
        let token = &tokens.lexical[index];
        let word = &source[token.byte_start..token.byte_end];
        let Some(base) = &self.baseline else {
            return false;
        };
        tokens.text[token.text_token].kind == Kind::Word
            && russian(word)
            && !matches!(token.protection, Some("uppercase_or_mixed_case"))
            && !base.domain.contains(&token.key)
            && !base.ignore.contains(&token.key)
            && !self.canonical.contains(&token.key)
    }

    fn neighbors(
        &self,
        source: &str,
        tokens: &Tokens,
        start: usize,
        end: usize,
    ) -> BTreeSet<String> {
        let mut result = BTreeSet::new();
        let mut left = start;
        for _ in 0..self.config.policy.context_window {
            if left == 0
                || !horizontal(
                    &source[tokens.lexical[left - 1].byte_end..tokens.lexical[left].byte_start],
                )
            {
                break;
            }
            left -= 1;
            if tokens.text[tokens.lexical[left].text_token].kind != Kind::Word {
                break;
            }
            result.insert(tokens.lexical[left].key.clone());
        }
        for right in (end..).take(self.config.policy.context_window) {
            if right >= tokens.lexical.len()
                || !horizontal(
                    &source[tokens.lexical[right - 1].byte_end..tokens.lexical[right].byte_start],
                )
            {
                break;
            }
            if tokens.text[tokens.lexical[right].text_token].kind != Kind::Word {
                break;
            }
            result.insert(tokens.lexical[right].key.clone());
        }
        result
    }

    pub fn analyze(&self, source: &str) -> Analysis {
        let strict = self.baseline.as_ref().is_some_and(|b| b.strict_yo);
        let tokens = text::tokenize(source, strict);
        let mut result = Analysis {
            corrected: source.into(),
            edits: vec![],
            decisions: vec![],
            complete: true,
            comparisons: 0,
        };
        if self.entries.is_empty() {
            return result;
        }
        let p = &self.config.policy;
        for start in 0..tokens.lexical.len() {
            let mut keys = Vec::new();
            for end in start + 1..=(start + p.max_tokens).min(tokens.lexical.len()) {
                let index = end - 1;
                if !self.allowed(source, &tokens, index)
                    || (index > start
                        && !horizontal(
                            &source[tokens.lexical[index - 1].byte_end
                                ..tokens.lexical[index].byte_start],
                        ))
                {
                    break;
                }
                keys.push(tokens.lexical[index].key.as_str());
                let spaced = keys.join(" ");
                let compact = keys.concat();
                if compact.chars().count() > 128 {
                    break;
                }
                let known = keys
                    .iter()
                    .any(|w| self.baseline.as_ref().unwrap().dictionary.contains(w));
                let neighbors = self.neighbors(source, &tokens, start, end);
                let mut by_target = BTreeMap::<String, Candidate>::new();
                for entry in &self.entries {
                    let mut best: Option<(usize, &str)> = None;
                    for (alias, joined) in &entry.aliases {
                        if result.comparisons == p.max_comparisons {
                            return incomplete(result);
                        }
                        result.comparisons += 1;
                        let distance = match p.mode {
                            MatchMode::Exact => (spaced == *alias).then_some(0),
                            MatchMode::Edit => distance_at_most(&compact, joined, p.max_distance),
                        };
                        if let Some(d) = distance
                            && best.is_none_or(|old| (d, alias.as_str()) < old)
                        {
                            best = Some((d, alias));
                        }
                    }
                    let Some((distance, alias)) = best else {
                        continue;
                    };
                    let context_hits: Vec<_> =
                        entry.context.intersection(&neighbors).cloned().collect();
                    let mut context_pattern_hits = Vec::new();
                    let mut context_exclusion_hits = Vec::new();
                    for (patterns, hits) in [
                        (&entry.patterns, &mut context_pattern_hits),
                        (&entry.exclusions, &mut context_exclusion_hits),
                    ] {
                        for pattern in patterns {
                            if result.comparisons == p.max_comparisons {
                                return incomplete(result);
                            }
                            result.comparisons += 1;
                            if pattern.matches(source, &tokens, start, end) {
                                hits.push(pattern.label.clone());
                            }
                        }
                    }
                    let has_context = !context_hits.is_empty() || !context_pattern_hits.is_empty();
                    let gate = if !context_exclusion_hits.is_empty() {
                        "context_excluded"
                    } else if known && entry.term.known_words == KnownWords::Never {
                        "known_word_blocked"
                    } else if known
                        && entry
                            .term
                            .max_known_distance
                            .is_some_and(|limit| distance > limit)
                    {
                        "known_distance_limit"
                    } else if (entry.term.require_context
                        || (known && entry.term.known_words == KnownWords::Context))
                        && !has_context
                    {
                        "context_required"
                    } else {
                        "eligible"
                    };
                    let eligible = gate == "eligible";
                    let score = 4.0 - p.edit_penalty * distance as f64
                        + p.boost
                        + entry.boost
                        + if !has_context { 0.0 } else { p.context_bonus }
                        - if known { p.known_penalty } else { 0.0 };
                    let candidate = Candidate {
                        set: entry.set.clone(),
                        term: entry.term.id.clone(),
                        canonical: entry.term.canonical.clone(),
                        alias: alias.into(),
                        distance,
                        score,
                        context_hits,
                        context_pattern_hits,
                        context_exclusion_hits,
                        max_known_distance: entry.term.max_known_distance,
                        contains_known_word: known,
                        eligible,
                        gate,
                    };
                    // Identical outputs do not vote multiple times or count as competing goals.
                    let replace = by_target.get(&candidate.canonical).is_none_or(|old| {
                        (candidate.eligible && !old.eligible)
                            || (candidate.eligible == old.eligible
                                && (candidate.score > old.score
                                    || (candidate.score == old.score
                                        && (&candidate.set, &candidate.term)
                                            < (&old.set, &old.term))))
                    });
                    if replace {
                        by_target.insert(candidate.canonical.clone(), candidate);
                    }
                }
                if by_target.is_empty() {
                    continue;
                }
                let mut candidates: Vec<_> = by_target.into_values().collect();
                candidates.sort_by(|a, b| {
                    b.eligible
                        .cmp(&a.eligible)
                        .then_with(|| b.score.total_cmp(&a.score))
                        .then_with(|| a.canonical.cmp(&b.canonical))
                });
                let best = &candidates[0];
                let margin = candidates
                    .iter()
                    .skip(1)
                    .find(|c| c.eligible)
                    .map(|c| best.score - c.score);
                let reason = if !best.eligible {
                    best.gate
                } else if best.score < p.min_score {
                    "keep_score_margin"
                } else if margin.is_some_and(|m| m < p.min_margin) {
                    "term_conflict"
                } else {
                    "term_selected"
                };
                let byte_start = tokens.lexical[start].byte_start;
                let byte_end = tokens.lexical[end - 1].byte_end;
                result.decisions.push(Decision {
                    token_start: start,
                    token_end: end,
                    byte_start,
                    byte_end,
                    original: source[byte_start..byte_end].into(),
                    selected: (reason == "term_selected").then(|| best.canonical.clone()),
                    candidates,
                    runner_up_margin: margin,
                    reason,
                });
            }
        }
        // Resolve overlapping spans before applying any replacements; never cascade.
        let provisional: Vec<usize> = result
            .decisions
            .iter()
            .enumerate()
            .filter_map(|(i, d)| d.selected.as_ref().map(|_| i))
            .collect();
        let mut blocked = BTreeSet::new();
        for (pos, &i) in provisional.iter().enumerate() {
            let selected = &result.decisions[i];
            let lower = result
                .decisions
                .partition_point(|d| d.token_start + p.max_tokens <= selected.token_start);
            let upper = result
                .decisions
                .partition_point(|d| d.token_start < selected.token_end);
            if result.decisions[lower..upper].iter().any(|conflict| {
                conflict.reason == "term_conflict"
                    && selected.token_start < conflict.token_end
                    && conflict.token_start < selected.token_end
                    && selected.candidates[0].score - conflict.candidates[0].score < p.min_margin
            }) {
                blocked.insert(i);
            }
            for &j in &provisional[pos + 1..] {
                let (a, b) = (&result.decisions[i], &result.decisions[j]);
                if b.token_start >= a.token_end {
                    break;
                }
                if a.token_start < b.token_end
                    && a.selected != b.selected
                    && (a.candidates[0].score - b.candidates[0].score).abs() < p.min_margin
                {
                    blocked.insert(i);
                    blocked.insert(j);
                }
            }
        }
        let mut order = provisional;
        order.sort_by(|&i, &j| {
            let (a, b) = (&result.decisions[i], &result.decisions[j]);
            b.candidates[0]
                .score
                .total_cmp(&a.candidates[0].score)
                .then_with(|| (a.token_end - a.token_start).cmp(&(b.token_end - b.token_start)))
                .then_with(|| a.token_start.cmp(&b.token_start))
        });
        let mut occupied = BTreeSet::new();
        for i in order {
            let d = &mut result.decisions[i];
            if blocked.contains(&i) {
                d.selected = None;
                d.reason = "overlap_conflict";
            } else if (d.token_start..d.token_end).any(|t| occupied.contains(&t)) {
                d.selected = None;
                d.reason = "overlap_lower_priority";
            } else {
                occupied.extend(d.token_start..d.token_end);
                result.edits.push(Edit {
                    byte_start: d.byte_start,
                    byte_end: d.byte_end,
                    token_start: d.token_start,
                    token_end: d.token_end,
                    replacement: d.selected.clone().unwrap(),
                });
            }
        }
        result.edits.sort_by_key(|e| e.byte_start);
        result.corrected.clear();
        let mut cursor = 0;
        for e in &result.edits {
            result.corrected.push_str(&source[cursor..e.byte_start]);
            result.corrected.push_str(&e.replacement);
            cursor = e.byte_end;
        }
        result.corrected.push_str(&source[cursor..]);
        result
    }
}

fn incomplete(mut result: Analysis) -> Analysis {
    result.complete = false;
    for decision in &mut result.decisions {
        decision.selected = None;
        decision.reason = "comparison_limit";
    }
    result
}

/// Unicode Levenshtein with length and row cutoffs; bounded inputs validated above.
fn distance_at_most(a: &str, b: &str, limit: usize) -> Option<usize> {
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    if a.len().abs_diff(b.len()) > limit {
        return None;
    }
    let mut row: Vec<_> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (old + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(x != y));
            diagonal = old;
        }
        if row.iter().copied().min().unwrap() > limit {
            return None;
        }
    }
    (row[b.len()] <= limit).then_some(row[b.len()])
}
