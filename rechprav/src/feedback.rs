//! Offline editor workbench, compiled by feedback_draft example; does not alter runtime rules.
use anyhow::{Result, ensure};
use rechprav::{
    corrector::Baseline,
    hashing::sha256_hex,
    terms,
    text::{self, Kind, Tokens},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub byte_start: usize,
    pub byte_end: usize,
    pub replacement: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Correction {
        schema_version: u32,
        event_id: String,
        scope: String,
        before: String,
        after: String,
        confirmed_asr: bool,
        #[serde(default)]
        selection: Option<Selection>,
    },
    Retract {
        schema_version: u32,
        event_id: String,
        target_event_id: String,
    },
}
impl Event {
    fn id(&self) -> &str {
        match self {
            Self::Correction { event_id, .. } | Self::Retract { event_id, .. } => event_id,
        }
    }
    fn version(&self) -> u32 {
        match self {
            Self::Correction { schema_version, .. } | Self::Retract { schema_version, .. } => {
                *schema_version
            }
        }
    }
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn russian(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| ('а'..='я').contains(&c) || ('А'..='Я').contains(&c) || c == 'ё' || c == 'Ё')
}
fn horizontal(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| matches!(c, ' ' | '\t' | '\u{a0}'))
}

#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub byte_start: usize,
    pub byte_end: usize,
    pub source: String,
    pub alias: String,
    pub replacement: String,
    pub context_pattern: Option<String>,
}
fn range(
    before: &str,
    t: &Tokens,
    start: usize,
    end: usize,
    replacement: &str,
    strict: bool,
    explicit: bool,
) -> Option<Change> {
    let left = t.lexical.iter().position(|x| x.byte_start == start)?;
    let right = t.lexical.iter().position(|x| x.byte_end == end)? + 1;
    if !(1..=3).contains(&right.checked_sub(left)?) {
        return None;
    }
    if before[start..end].chars().count() > 128 {
        return None;
    }
    let selected = &t.lexical[left..right];
    if selected.iter().any(|w| {
        t.text[w.text_token].kind != Kind::Word
            || w.protection.is_some_and(|reason| reason != "short_word")
            || !russian(&before[w.byte_start..w.byte_end])
    }) {
        return None;
    }
    if selected
        .windows(2)
        .any(|w| !horizontal(&before[w[0].byte_end..w[1].byte_start]))
    {
        return None;
    }
    let out = text::tokenize(replacement, strict);
    if !(1..=3).contains(&out.lexical.len()) {
        return None;
    }
    let words: Vec<_> = out
        .lexical
        .iter()
        .map(|w| &replacement[w.byte_start..w.byte_end])
        .collect();
    if words.join(" ") != replacement
        || words.iter().any(|w| !russian(w) || w.chars().count() > 64)
        || replacement.chars().count() > 128
    {
        return None;
    }
    let source_keys: Vec<_> = selected.iter().map(|w| w.key.as_str()).collect();
    let target_keys: Vec<_> = out.lexical.iter().map(|w| w.key.as_str()).collect();
    if source_keys == target_keys {
        return None;
    }
    // Explicit selection cannot turn a pure insertion/deletion into a replacement.
    let prefix = source_keys
        .iter()
        .zip(&target_keys)
        .take_while(|(a, b)| a == b)
        .count();
    let mut source_end = source_keys.len();
    let mut target_end = target_keys.len();
    while source_end > prefix
        && target_end > prefix
        && source_keys[source_end - 1] == target_keys[target_end - 1]
    {
        source_end -= 1;
        target_end -= 1;
    }
    if source_end == prefix || target_end == prefix {
        return None;
    }

    if !explicit
        && (source_keys.first() == target_keys.first() || source_keys.last() == target_keys.last())
    {
        return None;
    }
    if !explicit
        && source_keys.len() > 2
        && target_keys.len() > 2
        && source_keys[1..source_keys.len() - 1]
            .iter()
            .any(|w| target_keys[1..target_keys.len() - 1].contains(w))
    {
        return None;
    }
    let neighbor = |i: usize| {
        let w = &t.lexical[i];
        (t.text[w.text_token].kind == Kind::Word
            && russian(&before[w.byte_start..w.byte_end])
            && w.key.chars().count() <= 64)
            .then(|| w.key.clone())
    };
    let l = left
        .checked_sub(1)
        .filter(|i| horizontal(&before[t.lexical[*i].byte_end..start]))
        .and_then(neighbor);
    let r = (right < t.lexical.len())
        .then_some(right)
        .filter(|i| horizontal(&before[end..t.lexical[*i].byte_start]))
        .and_then(neighbor);
    let context_pattern = if l.is_none() && r.is_none() {
        None
    } else {
        Some(
            l.into_iter()
                .chain(["{term}".into()])
                .chain(r)
                .collect::<Vec<_>>()
                .join(" "),
        )
    };
    Some(Change {
        byte_start: start,
        byte_end: end,
        source: before[start..end].into(),
        alias: source_keys.join(" "),
        replacement: replacement.into(),
        context_pattern,
    })
}
fn extract(
    before: &str,
    after: &str,
    selection: &Option<Selection>,
    strict: bool,
) -> Result<Option<Change>> {
    let tokens = text::tokenize(before, strict);
    ensure!(
        tokens.lexical.len() <= 128 && text::tokenize(after, strict).lexical.len() <= 128,
        "event exceeds 128 lexical tokens"
    );
    if let Some(s) = selection {
        ensure!(
            s.byte_start < s.byte_end
                && s.byte_end <= before.len()
                && before.is_char_boundary(s.byte_start)
                && before.is_char_boundary(s.byte_end),
            "invalid UTF-8 selection"
        );
        ensure!(
            format!(
                "{}{}{}",
                &before[..s.byte_start],
                s.replacement,
                &before[s.byte_end..]
            ) == after,
            "selection does not reconstruct after"
        );
        return Ok(range(
            before,
            &tokens,
            s.byte_start,
            s.byte_end,
            &s.replacement,
            strict,
            true,
        ));
    }
    if before == after {
        return Ok(None);
    }
    let mut candidates = BTreeMap::new();
    for i in 0..tokens.lexical.len() {
        for j in i..(i + 3).min(tokens.lexical.len()) {
            let start = tokens.lexical[i].byte_start;
            let end = tokens.lexical[j].byte_end;
            let suffix = &before[end..];
            if start + suffix.len() > after.len()
                || !after.starts_with(&before[..start])
                || !after.ends_with(suffix)
            {
                continue;
            }
            let target_end = after.len() - suffix.len();
            if !after.is_char_boundary(start) || !after.is_char_boundary(target_end) {
                continue;
            }
            if let Some(change) = range(
                before,
                &tokens,
                start,
                end,
                &after[start..target_end],
                strict,
                false,
            ) {
                let size = (j - i + 1) + text::tokenize(&change.replacement, strict).lexical.len();
                candidates.insert((size, start, end, change.replacement.clone()), change);
            }
        }
    }
    let Some(min) = candidates.first_key_value().map(|(k, _)| k.0) else {
        return Ok(None);
    };
    let mut best = candidates
        .into_iter()
        .filter(|(k, _)| k.0 == min)
        .map(|(_, v)| v);
    let first = best.next();
    Ok(if best.next().is_some() { None } else { first })
}

#[derive(Clone, Debug, Serialize)]
pub struct Proposal {
    pub id: String,
    pub scope: String,
    pub alias: String,
    pub canonical: String,
    pub context_pattern: Option<String>,
    pub evidence_ids: Vec<String>,
    pub target_class: &'static str,
    pub target_word_membership: Vec<bool>,
    pub conflicting_proposals: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Audit {
    pub event_id: String,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change: Option<Change>,
}
#[derive(Debug, Serialize)]
pub struct Draft {
    pub scope: String,
    pub input_events: usize,
    pub unique_events: usize,
    pub duplicate_events: usize,
    pub active_corrections: usize,
    pub proposals: Vec<Proposal>,
    pub audit: Vec<Audit>,
    pub config: terms::Config,
}

pub fn draft(
    events: &[Event],
    scope: &str,
    user: &terms::Config,
    base: &Baseline,
) -> Result<Draft> {
    ensure!(valid_id(scope), "invalid scope");
    user.validate()?;
    ensure!(events.len() <= 10000, "too many events");
    let mut unique = BTreeMap::<&str, &Event>::new();
    let mut active = BTreeSet::new();
    let mut retracted = BTreeSet::new();
    let mut duplicates = 0;
    for event in events {
        ensure!(
            event.version() == 1 && valid_id(event.id()),
            "invalid event schema/id"
        );
        if let Some(previous) = unique.get(event.id()) {
            ensure!(
                *previous == event,
                "conflicting payload for event id {}",
                event.id()
            );
            duplicates += 1;
            continue;
        }
        match event {
            Event::Correction {
                scope,
                before,
                after,
                selection,
                ..
            } => {
                ensure!(
                    valid_id(scope) && before.len() <= 4096 && after.len() <= 4096,
                    "invalid correction bounds/scope"
                );
                // Explicit ranges and limits are validated even for unconfirmed/foreign/retracted events.
                extract(before, after, selection, base.strict_yo)?;
                active.insert(event.id().to_owned());
            }
            Event::Retract {
                target_event_id, ..
            } => {
                ensure!(
                    matches!(
                        unique.get(target_event_id.as_str()),
                        Some(Event::Correction { .. })
                    ),
                    "retraction must reference an earlier correction"
                );
                active.remove(target_event_id);
                retracted.insert(target_event_id.clone());
            }
        }
        unique.insert(event.id(), event);
    }
    let mut canonicals = BTreeSet::new();
    if user.enabled {
        for set in user.sets.iter().filter(|s| s.enabled) {
            for term in set.terms.iter().filter(|t| t.enabled) {
                canonicals.insert(text::key(&term.canonical, base.strict_yo));
                for f in &term.forms {
                    canonicals.insert(text::key(&f.canonical, base.strict_yo));
                }
            }
        }
    }
    let mut grouped = BTreeMap::<(String, String, Option<String>), Proposal>::new();
    let mut audit = Vec::new();
    for (id, event) in &unique {
        let mut row = Audit {
            event_id: (*id).into(),
            status: "retraction",
            change: None,
        };
        if let Event::Correction {
            scope: event_scope,
            before,
            after,
            confirmed_asr,
            selection,
            ..
        } = event
        {
            row.status = if retracted.contains(*id) {
                "retracted"
            } else if event_scope != scope {
                "other_scope"
            } else if !confirmed_asr {
                "not_confirmed_asr"
            } else {
                "needs_selection_or_review"
            };
            if active.contains(*id)
                && event_scope == scope
                && *confirmed_asr
                && let Some(change) = extract(before, after, selection, base.strict_yo)?
            {
                if change
                    .alias
                    .split(' ')
                    .any(|word| base.ignore.contains(word) || base.domain.contains(word))
                {
                    row.status = "protected_by_profile";
                    audit.push(row);
                    continue;
                }
                let key = (
                    change.alias.clone(),
                    change.replacement.clone(),
                    change.context_pattern.clone(),
                );
                let canonical_key = text::key(&change.replacement, base.strict_yo);
                let membership: Vec<_> = text::tokenize(&change.replacement, base.strict_yo)
                    .lexical
                    .iter()
                    .map(|w| base.dictionary.contains(&w.key))
                    .collect();
                let class = if canonicals.contains(&canonical_key) {
                    "existing_user_term"
                } else if membership.len() > 1 {
                    "phrase"
                } else if membership[0] {
                    "dictionary_word"
                } else {
                    "unknown_word_candidate"
                };
                let fingerprint = sha256_hex(serde_json::to_string(&(scope, &key))?.as_bytes());
                let p = grouped.entry(key).or_insert_with(|| Proposal {
                    id: format!("fb_{}", &fingerprint[..32]),
                    scope: scope.into(),
                    alias: change.alias.clone(),
                    canonical: change.replacement.clone(),
                    context_pattern: change.context_pattern.clone(),
                    evidence_ids: vec![],
                    target_class: class,
                    target_word_membership: membership,
                    conflicting_proposals: vec![],
                });
                p.evidence_ids.push((*id).into());
                row.status = "proposal";
                row.change = Some(change);
            }
        }
        audit.push(row);
    }
    let mut proposals: Vec<_> = grouped.into_values().collect();
    ensure!(proposals.len() <= 512, "too many proposals");
    let mut ids = BTreeSet::new();
    for p in &proposals {
        ensure!(ids.insert(&p.id), "proposal id collision");
    }
    for i in 0..proposals.len() {
        proposals[i].conflicting_proposals = (0..proposals.len())
            .filter(|j| {
                *j != i
                    && proposals[*j].alias == proposals[i].alias
                    && proposals[*j].canonical != proposals[i].canonical
            })
            .map(|j| proposals[j].id.clone())
            .collect();
    }
    let terms:Vec<serde_json::Value>=proposals.iter().map(|p|{
        let mut t=serde_json::json!({"id":p.id,"canonical":p.canonical,"aliases":[p.alias],"enabled":false,"boost":0.0,
            "known_words":if p.context_pattern.is_some(){"always"}else{"never"},"max_known_distance":0});
        if p.canonical.contains(' '){t["kind"]=serde_json::json!("phrase");}
        if let Some(pattern)=&p.context_pattern{t["require_context"]=serde_json::json!(true);t["context_patterns"]=serde_json::json!([pattern]);}
        t
    }).collect();
    let config: terms::Config = serde_json::from_value(
        serde_json::json!({"baseline_config":user.baseline_config,"enabled":false,
        "policy":{"mode":"exact"},"sets":[{"id":scope,"enabled":false,"terms":terms}]}),
    )?;
    config.validate()?;
    Ok(Draft {
        scope: scope.into(),
        input_events: events.len(),
        unique_events: unique.len(),
        duplicate_events: duplicates,
        active_corrections: active.len(),
        proposals,
        audit,
        config,
    })
}
