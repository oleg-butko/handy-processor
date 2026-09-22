//! Offline acceptance state, rebuilt from immutable journal prefixes and decisions.
use crate::feedback::{self, Draft, Event};
use anyhow::{Result, ensure};
use rechprav::{corrector::Baseline, hashing::sha256_hex, terms};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub event_count: usize,
    pub events_sha256: String,
    pub environment_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReviewToken {
    pub scope: String,
    pub proposal_id: String,
    pub checkpoint: Checkpoint,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Accept,
    Reject,
    Disable,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub schema_version: u32,
    pub decision_id: String,
    pub action: Action,
    pub review: ReviewToken,
}
#[derive(Debug, Serialize)]
pub struct State {
    pub proposal_id: String,
    pub decision_id: Option<String>,
    pub status: &'static str,
    pub approved_evidence_ids: Vec<String>,
    pub current_evidence_ids: Vec<String>,
    pub surviving_evidence_ids: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Registry {
    pub draft: Draft,
    pub review_tokens: Vec<ReviewToken>,
    pub states: Vec<State>,
    pub input_decisions: usize,
    pub unique_decisions: usize,
    pub duplicate_decisions: usize,
    pub config: terms::Config,
}
fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn checkpoint(events: &[Event], environment: &str) -> Result<Checkpoint> {
    ensure!(valid_hash(environment), "invalid environment hash");
    Ok(Checkpoint {
        event_count: events.len(),
        events_sha256: sha256_hex(&serde_json::to_vec(events)?),
        environment_sha256: environment.into(),
    })
}

pub fn rebuild(
    events: &[Event],
    decisions: &[Decision],
    scope: &str,
    user: &terms::Config,
    base: &Baseline,
    environment: &str,
) -> Result<Registry> {
    // Validate the entire journal even with no decisions or no matching scope.
    let draft = feedback::draft(events, scope, user, base)?;
    let current_checkpoint = checkpoint(events, environment)?;
    ensure!(decisions.len() <= 1024, "too many decisions");
    let mut unique = BTreeMap::<&str, &Decision>::new();
    let mut latest = BTreeMap::new();
    let mut prefix_hashes = BTreeMap::<usize, String>::new();
    let mut historical = BTreeMap::<(usize, &str), Draft>::new();
    let mut history_keys = BTreeSet::new();
    let mut duplicates = 0;
    for decision in decisions {
        let review = &decision.review;
        let cp = &review.checkpoint;
        ensure!(
            decision.schema_version == 1
                && valid_id(&decision.decision_id)
                && valid_id(&review.scope)
                && review.proposal_id.starts_with("fb_")
                && review.proposal_id.len() == 35
                && review.proposal_id[3..]
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                && valid_hash(&cp.events_sha256)
                && valid_hash(&cp.environment_sha256),
            "invalid decision schema/id/checkpoint"
        );
        if let Some(previous) = unique.get(decision.decision_id.as_str()) {
            ensure!(
                *previous == decision,
                "conflicting decision id {}",
                decision.decision_id
            );
            duplicates += 1;
            // A duplicate of an earlier accept must not override a later disable.
            continue;
        }
        ensure!(
            cp.event_count <= events.len(),
            "checkpoint is beyond current journal"
        );
        if let std::collections::btree_map::Entry::Vacant(entry) =
            prefix_hashes.entry(cp.event_count)
        {
            entry.insert(checkpoint(&events[..cp.event_count], environment)?.events_sha256);
        }
        ensure!(prefix_hashes.len() <= 32, "too many historical checkpoints");
        ensure!(
            prefix_hashes[&cp.event_count] == cp.events_sha256,
            "journal prefix changed"
        );
        let key = (cp.event_count, review.scope.as_str());
        history_keys.insert(key);
        ensure!(history_keys.len() <= 64, "too many checkpoint/scope pairs");
        if cp.environment_sha256 == environment {
            if let std::collections::btree_map::Entry::Vacant(entry) = historical.entry(key) {
                entry.insert(feedback::draft(
                    &events[..cp.event_count],
                    &review.scope,
                    user,
                    base,
                )?);
            }
            ensure!(
                historical[&key]
                    .proposals
                    .iter()
                    .any(|p| p.id == review.proposal_id),
                "proposal was not present at reviewed checkpoint"
            );
        }
        // Unknown old environments cannot be replayed; they never activate a rule.
        unique.insert(&decision.decision_id, decision);
        latest.insert(
            (review.scope.as_str(), review.proposal_id.as_str()),
            decision,
        );
    }
    let current: BTreeMap<_, _> = draft.proposals.iter().map(|p| (p.id.as_str(), p)).collect();
    let ids: BTreeSet<_> = current
        .keys()
        .copied()
        .chain(
            latest
                .keys()
                .filter(|(s, _)| *s == scope)
                .map(|(_, id)| *id),
        )
        .collect();
    let mut states = Vec::new();
    let mut enabled = BTreeSet::new();
    for id in ids {
        let current_evidence_ids = current
            .get(id)
            .map(|p| p.evidence_ids.clone())
            .unwrap_or_default();
        let mut row = State {
            proposal_id: id.into(),
            decision_id: None,
            status: "unreviewed",
            approved_evidence_ids: vec![],
            current_evidence_ids,
            surviving_evidence_ids: vec![],
        };
        if let Some(decision) = latest.get(&(scope, id)) {
            row.decision_id = Some(decision.decision_id.clone());
            let cp = &decision.review.checkpoint;
            if cp.environment_sha256 != environment {
                row.status = "stale_environment";
            } else {
                let p = historical[&(cp.event_count, scope)]
                    .proposals
                    .iter()
                    .find(|p| p.id == id)
                    .unwrap();
                row.status = match decision.action {
                    Action::Reject => "rejected",
                    Action::Disable => "disabled",
                    Action::Accept => {
                        row.approved_evidence_ids = p.evidence_ids.clone();
                        row.surviving_evidence_ids = p
                            .evidence_ids
                            .iter()
                            .filter(|id| row.current_evidence_ids.contains(id))
                            .cloned()
                            .collect();
                        if row.current_evidence_ids.is_empty() {
                            "no_active_evidence"
                        } else if row.surviving_evidence_ids.is_empty() {
                            "stale_evidence"
                        } else {
                            enabled.insert(id);
                            "accepted"
                        }
                    }
                };
            }
        }
        states.push(row);
    }
    let mut config = draft.config.clone();
    config.enabled = !enabled.is_empty();
    for set in &mut config.sets {
        set.terms.retain(|t| enabled.contains(t.id.as_str()));
        set.enabled = !set.terms.is_empty();
        for term in &mut set.terms {
            term.enabled = true;
        }
    }
    config.validate()?;
    let review_tokens = draft
        .proposals
        .iter()
        .map(|p| ReviewToken {
            scope: scope.into(),
            proposal_id: p.id.clone(),
            checkpoint: current_checkpoint.clone(),
        })
        .collect();
    Ok(Registry {
        draft,
        review_tokens,
        states,
        input_decisions: decisions.len(),
        unique_decisions: unique.len(),
        duplicate_decisions: duplicates,
        config,
    })
}
