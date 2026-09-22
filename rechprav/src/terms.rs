//! Experimental term-only pass. Scores are preferences, never probabilities.
use crate::{
    corpus::Edit,
    corrector::Baseline,
    pronunciation::{AliasOrigin, Pronunciation},
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

/// A rule may narrow global edit/weighted generation without changing ranking or gates.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Generation {
    #[default]
    Inherit,
    Exact,
}
impl Generation {
    fn is_inherit(&self) -> bool {
        *self == Self::Inherit
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnownWords {
    #[default]
    Never,
    Preserve,
    Context,
    Always,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TermKind {
    #[default]
    Word,
    Phrase,
}
impl TermKind {
    fn is_default(&self) -> bool {
        *self == Self::Word
    }
}

/// Tie-break only: gates, score and conflicting canonical outputs take priority.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpanTieBreak {
    #[default]
    Shortest,
    Distance,
    Longest,
}
impl SpanTieBreak {
    fn is_default(&self) -> bool {
        *self == Self::Shortest
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubstitutionCost {
    pub from: char,
    pub to: char,
    pub cost: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedDistance {
    pub max_cost: usize,
    pub insertion: usize,
    pub deletion: usize,
    pub substitutions: Vec<SubstitutionCost>,
}
impl WeightedDistance {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.max_cost <= 200
                && (50..=200).contains(&self.insertion)
                && (50..=200).contains(&self.deletion),
            "invalid weighted distance bounds"
        );
        ensure!(
            self.substitutions.len() <= 64,
            "too many weighted substitutions"
        );
        let mut seen = BTreeSet::new();
        for rule in &self.substitutions {
            ensure!(
                ('а'..='я').contains(&rule.from)
                    && ('а'..='я').contains(&rule.to)
                    && rule.from != rule.to
                    && (50..=200).contains(&rule.cost)
                    && seen.insert((rule.from, rule.to)),
                "invalid/duplicate directed substitution"
            );
        }
        Ok(())
    }

    // Direction is recognized source -> alias. Costs are integer hundredths.
    fn cost_at_most(&self, source: &str, alias: &str) -> Option<usize> {
        let a: Vec<_> = source.chars().collect();
        let b: Vec<_> = alias.chars().collect();
        let length_cost = if a.len() > b.len() {
            (a.len() - b.len()) * self.deletion
        } else {
            (b.len() - a.len()) * self.insertion
        };
        if length_cost > self.max_cost {
            return None;
        }
        let mut row: Vec<_> = (0..=b.len()).map(|j| j * self.insertion).collect();
        for (i, x) in a.iter().enumerate() {
            let mut diagonal = row[0];
            row[0] = (i + 1) * self.deletion;
            for (j, y) in b.iter().enumerate() {
                let substitution = if x == y {
                    0
                } else {
                    self.substitutions
                        .iter()
                        .find(|r| r.from == *x && r.to == *y)
                        .map_or(100, |r| r.cost)
                };
                let old = row[j + 1];
                row[j + 1] = (old + self.deletion)
                    .min(row[j] + self.insertion)
                    .min(diagonal + substitution);
                diagonal = old;
            }
            if row.iter().copied().min().unwrap() > self.max_cost {
                return None;
            }
        }
        (row[b.len()] <= self.max_cost).then_some(row[b.len()])
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    pub mode: MatchMode,
    pub max_distance: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weighted_distance: Option<WeightedDistance>,
    pub max_tokens: usize,
    pub max_comparisons: usize,
    pub context_window: usize,
    pub boost: f64,
    pub edit_penalty: f64,
    pub context_bonus: f64,
    pub known_penalty: f64,
    pub min_score: f64,
    pub min_margin: f64,
    #[serde(skip_serializing_if = "SpanTieBreak::is_default")]
    pub span_tie_break: SpanTieBreak,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            mode: MatchMode::Exact,
            max_distance: 1,
            weighted_distance: None,
            max_tokens: 3,
            max_comparisons: 20_000,
            context_window: 4,
            boost: 0.0,
            edit_penalty: 1.0,
            context_bonus: 2.0,
            known_penalty: 1.0,
            min_score: 3.0,
            min_margin: 1.0,
            span_tie_break: SpanTieBreak::Shortest,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub id: String,
    pub canonical: String,
    #[serde(default, skip_serializing_if = "Generation::is_inherit")]
    pub generation: Generation,
    #[serde(default, skip_serializing_if = "TermKind::is_default")]
    pub kind: TermKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forms: Vec<TermForm>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pronunciation: Option<Pronunciation>,
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
pub struct TermForm {
    pub canonical: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_patterns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_exclusions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_context: Option<bool>,
}

impl Term {
    fn variants(&self) -> Result<Vec<Self>> {
        ensure!(self.forms.len() <= 16, "at most 16 forms per term");
        ensure!(
            self.forms.is_empty() || (self.kind == TermKind::Word && self.pronunciation.is_none()),
            "forms require a word term without pronunciation generation"
        );
        let mut base = self.clone();
        base.forms.clear();
        let mut seen = BTreeSet::from([text::key(&self.canonical, false)]);
        let mut variants = vec![base.clone()];
        for form in &self.forms {
            ensure!(
                seen.insert(text::key(&form.canonical, false)),
                "duplicate canonical within term family"
            );
            let mut variant = base.clone();
            variant.canonical = form.canonical.clone();
            variant.aliases = if form.aliases.is_empty() {
                vec![form.canonical.clone()]
            } else {
                form.aliases.clone()
            };
            if let Some(value) = &form.context {
                variant.context = value.clone();
            }
            if let Some(value) = &form.context_patterns {
                variant.context_patterns = value.clone();
            }
            if let Some(value) = &form.context_exclusions {
                variant.context_exclusions = value.clone();
            }
            if let Some(value) = form.require_context {
                variant.require_context = value;
            }
            variants.push(variant);
        }
        Ok(variants)
    }
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

/// With generation disabled, preserve legacy alias ordering and work accounting.
fn expand_aliases(
    term: &Term,
    strict: bool,
) -> Result<(Vec<String>, BTreeMap<String, AliasOrigin>)> {
    let manual: Vec<_> = term.aliases.iter().map(|a| text::key(a, strict)).collect();
    let Some(generator) = term.pronunciation else {
        return Ok((manual, BTreeMap::new()));
    };
    let mut origins: BTreeMap<_, _> = manual
        .into_iter()
        .map(|a| (a, AliasOrigin::Manual))
        .collect();
    for alias in generator.generate(&term.canonical)? {
        let key = text::key(&alias, strict);
        origins
            .entry(key)
            .and_modify(|origin| {
                if *origin == AliasOrigin::Manual {
                    *origin = AliasOrigin::ManualAndGenerated;
                }
            })
            .or_insert(AliasOrigin::Generated);
    }
    ensure!(
        origins.len() <= 64,
        "expanded aliases exceed per-term limit 64"
    );
    Ok((origins.keys().cloned().collect(), origins))
}

/// A bounded disadvantage of keeping an exact input span, not a target probability.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePenalty {
    pub id: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub source: String,
    pub penalty: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_patterns: Vec<String>,
}

struct SourceRule {
    definition: SourcePenalty,
    key: String,
    patterns: Vec<Pattern>,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_penalties: Vec<SourcePenalty>,
}

fn bounded(value: f64, lower: f64, upper: f64) -> bool {
    value.is_finite() && (lower..=upper).contains(&value)
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        let p = &self.policy;
        if let Some(weighted) = &p.weighted_distance {
            ensure!(
                p.mode == MatchMode::Edit,
                "weighted distance requires edit mode"
            );
            weighted.validate()?;
        }
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
        ensure!(
            self.source_penalties.len() <= 128,
            "too many source penalties"
        );
        let mut penalty_ids = BTreeSet::new();
        for rule in &self.source_penalties {
            ensure!(
                valid_id(&rule.id) && penalty_ids.insert(&rule.id),
                "invalid/duplicate source penalty id"
            );
            let words: Vec<_> = rule.source.split(' ').collect();
            ensure!(
                (1..=3).contains(&words.len())
                    && words.iter().all(|w| russian(w))
                    && rule.source.chars().count() <= 128,
                "source penalty requires 1..3 Russian words with single spaces"
            );
            ensure!(bounded(rule.penalty, 0.0, 10.0), "invalid source penalty");
            ensure!(
                rule.context_patterns.len() <= 32,
                "too many source context patterns"
            );
            for pattern in &rule.context_patterns {
                Pattern::parse(pattern, false, p.context_window)?;
            }
        }
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
                for term in term.variants()? {
                    ensure!(bounded(term.boost, -10.0, 10.0), "invalid term boost");
                    let canonical = text::tokenize(&term.canonical, true);
                    if term.kind == TermKind::Phrase {
                        let words: Vec<_> = term.canonical.split(' ').collect();
                        ensure!(
                            (2..=3).contains(&words.len())
                                && words.iter().all(|w| russian(w))
                                && term.canonical.chars().count() <= 128
                                && term.pronunciation.is_none(),
                            "phrase canonical requires 2..3 Russian words with single spaces, no pronunciation generator"
                        );
                    } else {
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
                    }
                    ensure!(
                        (!term.aliases.is_empty() || term.pronunciation.is_some())
                            && term.aliases.len() <= 64,
                        "require 1..64 manual aliases or an enabled pronunciation generator"
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
                    alias_count += expand_aliases(&term, true)?.0.len();
                }
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
    family_canonical: Option<String>,
    boost: f64,
    aliases: Vec<(String, String)>,
    alias_origins: BTreeMap<String, AliasOrigin>,
    context: BTreeSet<String>,
    patterns: Vec<Pattern>,
    exclusions: Vec<Pattern>,
}

pub struct TermEngine {
    pub config: Config,
    baseline: Option<Baseline>,
    entries: Vec<Entry>,
    canonical: BTreeSet<String>,
    source_rules: Vec<SourceRule>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub set: String,
    pub term: String,
    pub canonical: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family_canonical: Option<String>,
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias_origin: Option<AliasOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement_edit: Option<Edit>,
    pub distance: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weighted_cost: Option<f64>,
    pub score: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub source_penalty: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub source_penalty_hits: Vec<String>,
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
        let mut metadata = serde_json::json!({"config_sha256":crate::hashing::sha256_hex(contents.as_bytes()),
            "effective_config":engine.config, "baseline_resources":base_metadata,
            "active_terms":engine.entries.len(), "scoring":"heuristic preferences, not probabilities; term-only pass"});
        let catalog: Vec<_> = engine
            .entries
            .iter()
            .filter(|e| e.term.pronunciation.is_some())
            .map(|e| {
                serde_json::json!({"set":e.set,"term":e.term.id,"canonical":e.term.canonical,
                "generator":e.term.pronunciation,"aliases":e.alias_origins})
            })
            .collect();
        if !catalog.is_empty() {
            metadata["pronunciation_aliases"] = serde_json::json!(catalog);
        }
        let mut families = BTreeMap::<(&str, &str), Vec<&Entry>>::new();
        for entry in &engine.entries {
            if entry.family_canonical.is_some() {
                families
                    .entry((&entry.set, &entry.term.id))
                    .or_default()
                    .push(entry);
            }
        }
        if !families.is_empty() {
            metadata["term_families"] =
                serde_json::json!(families.into_iter().map(|((set, id), entries)| {
                serde_json::json!({"set":set,"term":id,"canonical":entries[0].family_canonical,
                    "resolved_forms":entries.iter().map(|e| &e.term).collect::<Vec<_>>()})
            }).collect::<Vec<_>>());
        }
        Ok((engine, metadata))
    }

    pub fn new(config: Config, baseline: Option<Baseline>) -> Result<Self> {
        config.validate()?;
        let strict = baseline.as_ref().is_some_and(|b| b.strict_yo);
        let mut entries = Vec::new();
        let mut canonical = BTreeSet::new();
        if config.enabled {
            for set in config.sets.iter().filter(|s| s.enabled) {
                for definition in set.terms.iter().filter(|t| t.enabled) {
                    let family_canonical =
                        (!definition.forms.is_empty()).then(|| definition.canonical.clone());
                    for term in definition.variants()? {
                        ensure!(
                            baseline.is_some(),
                            "active terms require baseline membership/protection"
                        );
                        canonical.insert(text::key(&term.canonical, strict));
                        let (aliases, alias_origins) = expand_aliases(&term, strict)?;
                        entries.push(Entry {
                            set: set.id.clone(),
                            term: term.clone(),
                            family_canonical: family_canonical.clone(),
                            boost: set.boost + term.boost,
                            aliases: aliases
                                .into_iter()
                                .map(|a| {
                                    let compact = a.replace(' ', "");
                                    (a, compact)
                                })
                                .collect(),
                            alias_origins,
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
        }
        let source_rules = config
            .source_penalties
            .iter()
            .filter(|rule| config.enabled && rule.enabled)
            .map(|rule| {
                Ok(SourceRule {
                    definition: rule.clone(),
                    key: text::key(&rule.source, strict),
                    patterns: rule
                        .context_patterns
                        .iter()
                        .map(|p| Pattern::parse(p, strict, config.policy.context_window))
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            config,
            baseline,
            entries,
            canonical,
            source_rules,
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
                let mut source_penalty: f64 = 0.0;
                let mut source_penalty_hits = Vec::new();
                for rule in &self.source_rules {
                    if result.comparisons == p.max_comparisons {
                        return incomplete(result);
                    }
                    result.comparisons += 1;
                    if rule.key != spaced {
                        continue;
                    }
                    let mut matched = rule.patterns.is_empty();
                    for pattern in &rule.patterns {
                        if result.comparisons == p.max_comparisons {
                            return incomplete(result);
                        }
                        result.comparisons += 1;
                        matched |= pattern.matches(source, &tokens, start, end);
                    }
                    if matched {
                        source_penalty = source_penalty.max(rule.definition.penalty);
                        source_penalty_hits.push(rule.definition.id.clone());
                    }
                }
                source_penalty_hits.sort();
                let mut by_target = BTreeMap::<String, Candidate>::new();
                for entry in &self.entries {
                    let mut best: Option<(bool, usize, usize, &str)> = None;
                    for (alias, joined) in &entry.aliases {
                        if result.comparisons == p.max_comparisons {
                            return incomplete(result);
                        }
                        result.comparisons += 1;
                        let matched = if entry.term.generation == Generation::Exact {
                            (spaced == *alias).then_some((0, 0))
                        } else if let Some(weighted) = &p.weighted_distance {
                            weighted.cost_at_most(&compact, joined).map(|cost| {
                                let raw =
                                    distance_at_most(&compact, joined, weighted.max_cost / 50)
                                        .expect("positive costs bound ordinary distance");
                                (cost, raw)
                            })
                        } else {
                            match p.mode {
                                MatchMode::Exact => (spaced == *alias).then_some(0),
                                MatchMode::Edit => {
                                    distance_at_most(&compact, joined, p.max_distance)
                                }
                            }
                            .map(|d| (100 * d, d))
                        };
                        if let Some((cost, raw)) = matched {
                            let blocked =
                                known && entry.term.max_known_distance.is_some_and(|cap| raw > cap);
                            let key = (blocked, cost, raw, alias.as_str());
                            if best.is_none_or(|old| key < old) {
                                best = Some(key);
                            }
                        }
                    }
                    let Some((_, cost_units, distance, alias)) = best else {
                        continue;
                    };
                    let replacement_edit = if entry.term.kind == TermKind::Phrase {
                        let Some(edit) =
                            phrase_edit(&tokens, start, end, &entry.term.canonical, strict)
                        else {
                            // No-op or pure insertion/deletion; unsupported by lexical edit schema.
                            continue;
                        };
                        Some(edit)
                    } else {
                        None
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
                    let changed_known = entry.term.known_words == KnownWords::Preserve
                        && replacement_edit
                            .as_ref()
                            .map_or(start..end, |e| e.token_start..e.token_end)
                            .any(|i| {
                                self.baseline
                                    .as_ref()
                                    .unwrap()
                                    .dictionary
                                    .contains(&tokens.lexical[i].key)
                            });
                    let gate = if !context_exclusion_hits.is_empty() {
                        "context_excluded"
                    } else if known && entry.term.known_words == KnownWords::Never {
                        "known_word_blocked"
                    } else if changed_known {
                        "known_changed_word_blocked"
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
                    let score = 4.0 - p.edit_penalty * (cost_units as f64 / 100.0)
                        + p.boost
                        + entry.boost
                        + if !has_context { 0.0 } else { p.context_bonus }
                        - if known { p.known_penalty } else { 0.0 }
                        + source_penalty;
                    let candidate = Candidate {
                        set: entry.set.clone(),
                        term: entry.term.id.clone(),
                        canonical: entry.term.canonical.clone(),
                        family_canonical: entry.family_canonical.clone(),
                        alias: alias.into(),
                        alias_origin: entry.alias_origins.get(alias).copied(),
                        replacement_edit,
                        distance,
                        weighted_cost: p
                            .weighted_distance
                            .as_ref()
                            .filter(|_| entry.term.generation != Generation::Exact)
                            .map(|_| cost_units as f64 / 100.0),
                        score,
                        source_penalty,
                        source_penalty_hits: source_penalty_hits.clone(),
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
                .then_with(|| {
                    let lengths = (a.token_end - a.token_start).cmp(&(b.token_end - b.token_start));
                    match p.span_tie_break {
                        SpanTieBreak::Shortest => lengths,
                        SpanTieBreak::Distance => a.candidates[0]
                            .distance
                            .cmp(&b.candidates[0].distance)
                            .then(lengths),
                        SpanTieBreak::Longest => lengths.reverse(),
                    }
                })
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
                result
                    .edits
                    .push(
                        d.candidates[0]
                            .replacement_edit
                            .clone()
                            .unwrap_or_else(|| Edit {
                                byte_start: d.byte_start,
                                byte_end: d.byte_end,
                                token_start: d.token_start,
                                token_end: d.token_end,
                                replacement: d.selected.clone().unwrap(),
                            }),
                    );
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

/// Keep unchanged edge words (and their original spaces/case) outside the edit.
/// Conflict resolution still reserves the entire matched input phrase.
fn phrase_edit(
    tokens: &Tokens,
    start: usize,
    end: usize,
    canonical: &str,
    strict: bool,
) -> Option<Edit> {
    let target = text::tokenize(canonical, strict);
    let (mut left, mut right, mut a, mut b) = (start, end, 0, target.lexical.len());
    while left < right && a < b && tokens.lexical[left].key == target.lexical[a].key {
        left += 1;
        a += 1;
    }
    while left < right && a < b && tokens.lexical[right - 1].key == target.lexical[b - 1].key {
        right -= 1;
        b -= 1;
    }
    if left == right || a == b {
        return None;
    }
    Some(Edit {
        byte_start: tokens.lexical[left].byte_start,
        byte_end: tokens.lexical[right - 1].byte_end,
        token_start: left,
        token_end: right,
        replacement: canonical[target.lexical[a].byte_start..target.lexical[b - 1].byte_end].into(),
    })
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

#[cfg(test)]
mod weighted_tests {
    use super::*;

    // Full matrix oracle: no cutoffs or rolling rows; a map supplies directional costs.
    fn oracle(a: &str, b: &str, c: &WeightedDistance) -> usize {
        let a: Vec<_> = a.chars().collect();
        let b: Vec<_> = b.chars().collect();
        let costs: BTreeMap<_, _> = c
            .substitutions
            .iter()
            .map(|r| ((r.from, r.to), r.cost))
            .collect();
        let mut m = vec![vec![0; b.len() + 1]; a.len() + 1];
        for (i, row) in m.iter_mut().enumerate() {
            row[0] = i * c.deletion;
        }
        for (j, cell) in m[0].iter_mut().enumerate() {
            *cell = j * c.insertion;
        }
        for i in 1..=a.len() {
            for j in 1..=b.len() {
                let cost = if a[i - 1] == b[j - 1] {
                    0
                } else {
                    *costs.get(&(a[i - 1], b[j - 1])).unwrap_or(&100)
                };
                m[i][j] = (m[i - 1][j] + c.deletion)
                    .min(m[i][j - 1] + c.insertion)
                    .min(m[i - 1][j - 1] + cost);
            }
        }
        m[a.len()][b.len()]
    }
    #[test]
    fn weighted_cutoffs_match_full_matrix_exhaustively() {
        let mut strings = vec![String::new()];
        let mut frontier = strings.clone();
        for _ in 0..3 {
            frontier = frontier
                .iter()
                .flat_map(|s| ['б', 'п', 'а'].map(|c| format!("{s}{c}")))
                .collect();
            strings.extend(frontier.clone());
        }
        for (insertion, deletion) in [(100, 100), (50, 200), (75, 100)] {
            let mut c = WeightedDistance {
                max_cost: 0,
                insertion,
                deletion,
                substitutions: vec![SubstitutionCost {
                    from: 'б',
                    to: 'п',
                    cost: 50,
                }],
            };
            for a in &strings {
                for b in &strings {
                    let expected = oracle(a, b, &c);
                    for limit in [0, 49, 50, 75, 100, 150, 199, 200] {
                        c.max_cost = limit;
                        assert_eq!(
                            c.cost_at_most(a, b),
                            (expected <= limit).then_some(expected),
                            "{a}->{b} {limit}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn uniform_costs_equal_unicode_levenshtein_and_direction_is_explicit() {
        let mut c = WeightedDistance {
            max_cost: 100,
            insertion: 100,
            deletion: 100,
            substitutions: vec![],
        };
        for a in ["", "пээнзэ", "бээнсэ", "таб", "табы", "ё", "е"] {
            for b in ["", "пнз", "пээнзэ", "таб", "табы", "е"] {
                assert_eq!(
                    c.cost_at_most(a, b),
                    distance_at_most(a, b, 1).map(|d| 100 * d)
                );
            }
        }
        c.max_cost = 50;
        c.substitutions.push(SubstitutionCost {
            from: 'б',
            to: 'п',
            cost: 50,
        });
        assert_eq!(c.cost_at_most("б", "п"), Some(50));
        assert_eq!(c.cost_at_most("п", "б"), None);
        c.insertion = 50;
        c.deletion = 100;
        assert_eq!(c.cost_at_most("", "п"), Some(50));
        assert_eq!(c.cost_at_most("п", ""), None);
    }
}
