//! Bounded frequency prior, separate from the dictionary of valid forms.
use crate::packed_frequency::PackedFrequency;
use crate::text::key;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrequencyConfig {
    pub path: PathBuf,
    #[serde(default)]
    pub scoring: FrequencyPolicy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct FrequencyPolicy {
    pub weight: f64,
    pub log_cap: f64,
    pub edit_penalty: f64,
    pub keep_bias: f64,
    pub min_keep_margin: f64,
    pub min_runner_up_margin: f64,
}

impl Default for FrequencyPolicy {
    fn default() -> Self {
        Self {
            weight: 1.0,
            log_cap: 6.0,
            edit_penalty: 1.0,
            keep_bias: 1.0,
            min_keep_margin: 0.5,
            min_runner_up_margin: 0.5,
        }
    }
}

impl FrequencyPolicy {
    pub fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("weight", self.weight),
            ("log_cap", self.log_cap),
            ("edit_penalty", self.edit_penalty),
            ("keep_bias", self.keep_bias),
            ("min_keep_margin", self.min_keep_margin),
            ("min_runner_up_margin", self.min_runner_up_margin),
        ] {
            ensure!(
                value.is_finite() && (0.0..=100.0).contains(&value),
                "frequency {name} must be finite and in 0..100"
            );
        }
        ensure!(
            self.log_cap > 0.0
                && self.edit_penalty > 0.0
                && self.min_keep_margin > 0.0
                && self.min_runner_up_margin > 0.0,
            "cap, edit penalty and both margins must be positive (ties must KEEP)"
        );
        Ok(())
    }
    pub fn prior(&self, frequency: Option<f64>) -> f64 {
        self.weight * (frequency.unwrap_or(0.0).ln_1p() / std::f64::consts::LN_10).min(self.log_cap)
    }
}

pub struct FrequencyScorer {
    pub policy: FrequencyPolicy,
    values: Values,
    strict_yo: bool,
}

enum Values {
    Text(BTreeMap<String, f64>),
    Packed(PackedFrequency),
}

impl FrequencyScorer {
    /// TSV: Russian spelling<TAB>occurrences per billion tokens. No header.
    /// Missing is unknown, not evidence that a word is invalid. ё collisions use max.
    pub fn from_tsv(data: &str, strict_yo: bool, policy: FrequencyPolicy) -> Result<Self> {
        policy.validate()?;
        let mut values = BTreeMap::<String, f64>::new();
        for (line, row) in data.lines().enumerate() {
            let (word, number) = row.split_once('\t').with_context(|| {
                format!("frequency TSV line {}: expected word TAB value", line + 1)
            })?;
            ensure!(
                !word.is_empty()
                    && word.chars().count() <= 128
                    && word.chars().all(|c| ('а'..='я').contains(&c) || c == 'ё'),
                "frequency TSV line {}: expected lowercase Russian word",
                line + 1
            );
            let value: f64 = number.parse().context("parse frequency per billion")?;
            ensure!(
                value.is_finite() && (0.0..=1e9).contains(&value),
                "frequency must be finite and in 0..1e9 per billion"
            );
            values
                .entry(key(word, strict_yo))
                .and_modify(|old| *old = old.max(value))
                .or_insert(value);
        }
        ensure!(!values.is_empty(), "frequency table is empty");
        Ok(Self {
            policy,
            values: Values::Text(values),
            strict_yo,
        })
    }
    pub fn from_packed(data: Vec<u8>, strict_yo: bool, policy: FrequencyPolicy) -> Result<Self> {
        policy.validate()?;
        Ok(Self {
            policy,
            values: Values::Packed(PackedFrequency::from_bytes(data, strict_yo)?),
            strict_yo,
        })
    }
    pub fn pack(&self, source_sha256: [u8; 32]) -> Result<Vec<u8>> {
        match &self.values {
            Values::Text(values) => {
                crate::packed_frequency::encode(values, self.strict_yo, source_sha256)
            }
            Values::Packed(_) => anyhow::bail!("pack requires the original TSV table"),
        }
    }
    pub fn storage_metadata(&self) -> serde_json::Value {
        match &self.values {
            Values::Text(_) => serde_json::json!({"format":"tsv_btree"}),
            Values::Packed(table) => {
                serde_json::json!({"format":"packed_frequency_v1", "bytes":table.as_bytes().len(), "source_sha256":table.source_sha256()})
            }
        }
    }
    pub fn get(&self, key: &str) -> Option<f64> {
        match &self.values {
            Values::Text(values) => values.get(key).copied(),
            Values::Packed(table) => table.get(key),
        }
    }
    pub fn len(&self) -> usize {
        match &self.values {
            Values::Text(values) => values.len(),
            Values::Packed(table) => table.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn score(&self, key: &str, distance: usize) -> f64 {
        self.policy.prior(self.get(key)) - self.policy.edit_penalty * distance as f64
    }
    pub fn keep_score(&self, key: &str) -> f64 {
        self.policy.prior(self.get(key)) + self.policy.keep_bias
    }
}
