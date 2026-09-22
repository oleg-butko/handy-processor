//! Conservative offline composition; never mutates manual definitions or the runtime.
use crate::feedback_registry::Registry;
use anyhow::{Result, ensure};
use rechprav::{
    hashing::sha256_hex,
    terms::{Config, Generation, KnownWords, Term, TermSet},
    text,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Owner {
    pub set_id: String,
    pub term_id: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub proposal_id: String,
    pub enabled: bool,
    #[serde(deserialize_with = "explicit_owner")]
    pub owner: Option<Owner>,
    pub boost: f64,
}
fn explicit_owner<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Owner>, D::Error> {
    Option::<Owner>::deserialize(deserializer)
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub enabled: bool,
    pub bindings: Vec<Binding>,
}
#[derive(Debug, Serialize)]
pub struct Audit {
    pub proposal_id: String,
    pub status: &'static str,
    pub owner: Option<Owner>,
    pub output_set: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Composition {
    pub registry: Registry,
    pub config: Config,
    pub audit: Vec<Audit>,
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}

// Conjunction of two adjacent templates. A longer left suffix/right prefix is stricter.
fn intersection(a: &str, b: &str, strict: bool) -> Option<String> {
    let split = |s: &str| {
        let (l, r) = s.split_once("{term}").unwrap(); // Inputs are validated Config patterns.
        (
            l.split_whitespace().map(str::to_owned).collect::<Vec<_>>(),
            r.split_whitespace().map(str::to_owned).collect::<Vec<_>>(),
        )
    };
    let (al, ar) = split(a);
    let (bl, br) = split(b);
    let side = |a: Vec<String>, b: Vec<String>, left: bool| -> Option<Vec<String>> {
        let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
        let start = if left { long.len() - short.len() } else { 0 };
        short
            .iter()
            .zip(&long[start..])
            .all(|(x, y)| text::key(x, strict) == text::key(y, strict))
            .then_some(long)
    };
    let l = side(al, bl, true)?;
    let r = side(ar, br, false)?;
    Some(
        l.into_iter()
            .chain(["{term}".into()])
            .chain(r)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn variant(owner: &Term, canonical: &str) -> Option<Term> {
    let mut result = owner.clone();
    if owner.canonical != canonical {
        let form = owner.forms.iter().find(|f| f.canonical == canonical)?;
        result.canonical = form.canonical.clone();
        if let Some(v) = &form.context {
            result.context = v.clone();
        }
        if let Some(v) = &form.context_patterns {
            result.context_patterns = v.clone();
        }
        if let Some(v) = &form.context_exclusions {
            result.context_exclusions = v.clone();
        }
        if let Some(v) = form.require_context {
            result.require_context = v;
        }
    }
    result.forms.clear();
    Some(result)
}
fn known_rank(k: KnownWords) -> u8 {
    match k {
        KnownWords::Never => 0,
        KnownWords::Preserve => 1,
        KnownWords::Context => 2,
        KnownWords::Always => 3,
    }
}

pub fn compose(
    manual: &Config,
    registry: Registry,
    settings: &Settings,
    strict: bool,
) -> Result<Composition> {
    manual.validate()?;
    ensure!(settings.bindings.len() <= 512, "too many feedback bindings");
    let mut bindings = BTreeMap::new();
    for b in &settings.bindings {
        ensure!(
            valid_id(&b.proposal_id) && bindings.insert(b.proposal_id.as_str(), b).is_none(),
            "invalid/duplicate binding id"
        );
        ensure!(
            b.boost.is_finite() && (-10.0..=10.0).contains(&b.boost),
            "invalid binding boost"
        );
        if let Some(o) = &b.owner {
            ensure!(
                valid_id(&o.set_id) && valid_id(&o.term_id) && b.boost == 0.0,
                "owner binding uses owner's boost, not an override"
            );
        }
    }
    let accepted: BTreeMap<_, _> = registry
        .config
        .sets
        .iter()
        .flat_map(|s| &s.terms)
        .map(|t| (t.id.as_str(), t))
        .collect();
    let ids: BTreeSet<_> = registry
        .draft
        .proposals
        .iter()
        .map(|p| p.id.as_str())
        .chain(bindings.keys().copied())
        .collect();
    let mut config = manual.clone();
    let mut audit = Vec::new();
    let new_set_id = format!(
        "fb_new_{}",
        &sha256_hex(registry.draft.scope.as_bytes())[..16]
    );
    let mut new_terms = Vec::new();
    for id in ids {
        let binding = bindings.get(id).copied();
        let mut row = Audit {
            proposal_id: id.into(),
            status: "not_accepted",
            owner: binding.and_then(|b| b.owner.clone()),
            output_set: None,
        };
        let status = (|| -> &'static str {
            let Some(generated) = accepted.get(id) else {
                return "not_accepted";
            };
            let Some(binding) = binding else {
                return "unbound";
            };
            if !manual.enabled {
                return "manual_disabled";
            }
            if !settings.enabled {
                return "feedback_disabled";
            }
            if !binding.enabled {
                return "binding_disabled";
            }
            let mut term = (*generated).clone();
            term.generation = Generation::Exact;
            if let Some(owner) = &binding.owner {
                let Some(set_index) = manual.sets.iter().position(|s| s.id == owner.set_id) else {
                    return "owner_missing";
                };
                let set = &manual.sets[set_index];
                let Some(definition) = set.terms.iter().find(|t| t.id == owner.term_id) else {
                    return "owner_missing";
                };
                if !set.enabled {
                    return "owner_set_disabled";
                }
                if !definition.enabled {
                    return "owner_term_disabled";
                }
                let Some(v) = variant(definition, &term.canonical) else {
                    return "owner_target_mismatch";
                };
                if !v.context.is_empty() {
                    return "unsupported_context_cues";
                }
                if !v.context_patterns.is_empty() {
                    let patterns: BTreeSet<_> = if term.context_patterns.is_empty() {
                        v.context_patterns.iter().cloned().collect()
                    } else {
                        term.context_patterns
                            .iter()
                            .flat_map(|a| {
                                v.context_patterns
                                    .iter()
                                    .filter_map(|b| intersection(a, b, strict))
                            })
                            .collect()
                    };
                    if patterns.is_empty() {
                        return "disjoint_context";
                    }
                    term.context_patterns = patterns.into_iter().collect();
                    term.require_context = true;
                }
                term.context_exclusions = term
                    .context_exclusions
                    .iter()
                    .chain(&v.context_exclusions)
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                if known_rank(v.known_words) < known_rank(term.known_words) {
                    term.known_words = v.known_words;
                }
                term.boost = v.boost;
                // Only the accepted alias and target are copied; no pronunciation/forms are added.
                config.sets[set_index].terms.push(term);
                row.output_set = Some(set.id.clone());
            } else {
                let key = text::key(&term.canonical, strict);
                if manual.sets.iter().flat_map(|s| &s.terms).any(|t| {
                    text::key(&t.canonical, strict) == key
                        || t.forms
                            .iter()
                            .any(|f| text::key(&f.canonical, strict) == key)
                }) {
                    return "existing_target_requires_owner";
                }
                term.boost = binding.boost;
                new_terms.push(term);
                row.output_set = Some(new_set_id.clone());
            }
            "added"
        })();
        row.status = status;
        audit.push(row);
    }
    if !new_terms.is_empty() {
        ensure!(
            !manual.sets.iter().any(|s| s.id == new_set_id),
            "feedback namespace collision"
        );
        config.sets.push(TermSet {
            id: new_set_id,
            enabled: true,
            boost: 0.0,
            terms: new_terms,
        });
    }
    config.validate()?;
    Ok(Composition {
        registry,
        config,
        audit,
    })
}
