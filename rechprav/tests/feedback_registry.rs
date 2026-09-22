#[path = "../src/feedback.rs"]
mod feedback;
#[path = "../src/feedback_registry.rs"]
mod feedback_registry;
use feedback::Event;
use feedback_registry::{Action, Decision, Registry, rebuild};
use rechprav::{
    corrector::Baseline,
    dictionary::Dictionary,
    terms::{Config, TermEngine},
};
use std::collections::BTreeSet;

fn base() -> Baseline {
    Baseline {
        dictionary: Box::new(Dictionary::from_text("свод\nпамяти", false).unwrap()),
        domain: BTreeSet::new(),
        ignore: BTreeSet::new(),
        policy: Default::default(),
        strict_yo: false,
        frequency: None,
    }
}
fn user() -> Config {
    serde_json::from_value(serde_json::json!({"baseline_config":"base.toml","sets":[]})).unwrap()
}
fn event(id: &str, before: &str, after: &str, scope: &str) -> Event {
    serde_json::from_value(
        serde_json::json!({"schema_version":1,"kind":"correction","event_id":id,
        "scope":scope,"before":before,"after":after,"confirmed_asr":true}),
    )
    .unwrap()
}
fn slot(id: &str) -> Event {
    event(id, "сняли свод памяти", "сняли слот памяти", "technical")
}
fn undo(id: &str, target: &str) -> Event {
    serde_json::from_value(serde_json::json!({"schema_version":1,"kind":"retract","event_id":id,"target_event_id":target})).unwrap()
}
fn build(events: &[Event], decisions: &[Decision], scope: &str) -> Registry {
    rebuild(events, decisions, scope, &user(), &base(), &"a".repeat(64)).unwrap()
}
fn decide(events: &[Event], id: &str, canonical: &str, scope: &str, action: Action) -> Decision {
    let r = build(events, &[], scope);
    let p = r
        .draft
        .proposals
        .iter()
        .find(|p| p.canonical == canonical)
        .unwrap();
    Decision {
        schema_version: 1,
        decision_id: id.into(),
        action,
        review: r
            .review_tokens
            .into_iter()
            .find(|t| t.proposal_id == p.id)
            .unwrap(),
    }
}
fn output(r: Registry, input: &str) -> String {
    let config: Config = toml::from_str(&toml::to_string(&r.config).unwrap()).unwrap();
    TermEngine::new(config, Some(base()))
        .unwrap()
        .analyze(input)
        .corrected
}

#[test]
fn acceptance_is_explicit_and_removed_after_last_approved_evidence() {
    let mut events = vec![slot("first")];
    assert!(!build(&events, &[], "technical").config.enabled);
    let decision = decide(&events, "accept", "слот", "technical", Action::Accept);
    assert_eq!(
        output(
            build(&events, std::slice::from_ref(&decision), "technical"),
            "сняли свод памяти"
        ),
        "сняли слот памяти"
    );
    events.extend([slot("new"), undo("undo", "first")]);
    let r = build(&events, std::slice::from_ref(&decision), "technical");
    assert_eq!(r.states[0].status, "stale_evidence");
    assert!(!r.config.enabled);
    events.push(undo("undo-new", "new"));
    assert_eq!(
        build(&events, std::slice::from_ref(&decision), "technical").states[0].status,
        "no_active_evidence"
    );
    events.push(slot("reborn"));
    assert_eq!(
        build(&events, std::slice::from_ref(&decision), "technical").states[0].status,
        "stale_evidence"
    );
    let next = decide(&events, "accept-again", "слот", "technical", Action::Accept);
    assert_eq!(
        build(&events, &[decision, next], "technical").states[0].surviving_evidence_ids,
        vec!["reborn"]
    );
}

#[test]
fn partial_undo_keeps_other_reviewed_support_without_increasing_weight() {
    let mut events = vec![slot("first"), slot("second")];
    let d = decide(&events, "accept", "слот", "technical", Action::Accept);
    let original = build(&events, std::slice::from_ref(&d), "technical");
    events.push(undo("undo", "first"));
    let reduced = build(&events, std::slice::from_ref(&d), "technical");
    assert_eq!(reduced.states[0].surviving_evidence_ids, vec!["second"]);
    assert_eq!(
        toml::to_string(&original.config).unwrap(),
        toml::to_string(&reduced.config).unwrap()
    );
    events.push(slot("third"));
    assert_eq!(
        toml::to_string(&original.config).unwrap(),
        toml::to_string(&build(&events, &[d], "technical").config).unwrap()
    );
}

#[test]
fn last_unique_decision_wins_duplicate_accept_does_not_reenable() {
    let events = [slot("first")];
    let a = decide(&events, "a", "слот", "technical", Action::Accept);
    let d = decide(&events, "d", "слот", "technical", Action::Disable);
    let r = build(&events, &[a.clone(), d.clone(), a.clone()], "technical");
    assert_eq!(r.states[0].status, "disabled");
    assert_eq!(r.duplicate_decisions, 1);
    assert!(!r.config.enabled);
    let no = decide(&events, "no", "слот", "technical", Action::Reject);
    assert_eq!(
        build(&events, &[a.clone(), no], "technical").states[0].status,
        "rejected"
    );
    let yes = decide(&events, "yes", "слот", "technical", Action::Accept);
    assert!(
        build(&events, &[a.clone(), d, yes], "technical")
            .config
            .enabled
    );
    let mut changed = a.clone();
    changed.action = Action::Disable;
    assert!(
        rebuild(
            &events,
            &[a, changed],
            "technical",
            &user(),
            &base(),
            &"a".repeat(64)
        )
        .is_err()
    );
}

#[test]
fn old_prefix_cannot_be_rewritten_or_truncated_and_bad_proposals_fail() {
    let events = [slot("first")];
    let d = decide(&events, "a", "слот", "technical", Action::Accept);
    let check =
        |e: &[Event], d: &[Decision]| rebuild(e, d, "other", &user(), &base(), &"a".repeat(64));
    assert!(check(&[], std::slice::from_ref(&d)).is_err());
    let changed = [event(
        "first",
        "сняли свод памяти",
        "сняли свитч памяти",
        "technical",
    )];
    assert!(check(&changed, std::slice::from_ref(&d)).is_err());
    let mut bogus = d.clone();
    bogus.review.proposal_id = format!("fb_{}", "0".repeat(32));
    assert!(check(&events, &[bogus]).is_err());
    let mut future = d.clone();
    future.review.checkpoint.event_count = usize::MAX;
    assert!(check(&events, &[future]).is_err());
    // Formatting in JSONL is not part of the normalized checkpoint.
    let round: Vec<Event> =
        serde_json::from_str(&serde_json::to_string_pretty(&events).unwrap()).unwrap();
    assert!(check(&round, &[d]).is_ok());
}

#[test]
fn environment_change_requires_review_even_if_the_same_proposal_survives() {
    let events = [slot("first")];
    let d = decide(&events, "a", "слот", "technical", Action::Accept);
    let r = rebuild(
        &events,
        &[d],
        "technical",
        &user(),
        &base(),
        &"b".repeat(64),
    )
    .unwrap();
    assert_eq!(r.states[0].status, "stale_environment");
    assert!(!r.config.enabled);
    let fresh = Decision {
        schema_version: 1,
        decision_id: "new".into(),
        action: Action::Accept,
        review: r.review_tokens[0].clone(),
    };
    assert!(
        rebuild(
            &events,
            &[fresh],
            "technical",
            &user(),
            &base(),
            &"b".repeat(64)
        )
        .unwrap()
        .config
        .enabled
    );
}

#[test]
fn unreviewed_conflicts_do_not_apply_but_two_accepted_targets_keep() {
    let first = [slot("first")];
    let a = decide(&first, "a", "слот", "technical", Action::Accept);
    let events = [
        first[0].clone(),
        event(
            "rival",
            "сняли свод памяти",
            "сняли свитч памяти",
            "technical",
        ),
    ];
    assert_eq!(
        output(
            build(&events, std::slice::from_ref(&a), "technical"),
            "сняли свод памяти"
        ),
        "сняли слот памяти"
    );
    let b = decide(&events, "b", "свитч", "technical", Action::Accept);
    assert_eq!(
        output(build(&events, &[a, b], "technical"), "сняли свод памяти"),
        "сняли свод памяти"
    );
}

#[test]
fn scopes_and_new_context_do_not_inherit_acceptance() {
    let first = [slot("first")];
    let a = decide(&first, "a", "слот", "technical", Action::Accept);
    let events = [
        first[0].clone(),
        undo("undo", "first"),
        event(
            "context",
            "освободили свод памяти",
            "освободили слот памяти",
            "technical",
        ),
        event("other", "сняли свод памяти", "сняли слот памяти", "other"),
    ];
    let r = build(&events, std::slice::from_ref(&a), "technical");
    assert!(!r.config.enabled);
    assert!(r.states.iter().any(|s| s.status == "unreviewed"));
    assert!(r.states.iter().any(|s| s.status == "no_active_evidence"));
    let r = build(&events, &[a], "other");
    assert_eq!(r.states.len(), 1);
    assert_eq!(r.states[0].status, "unreviewed");
}

#[test]
fn strict_schema_and_decision_budget_reject_invalid_input() {
    let events = [slot("first")];
    let d = decide(&events, "a", "слот", "technical", Action::Accept);
    let mut value = serde_json::to_value(&d).unwrap();
    value["review"]["checkpoint"]["extra"] = true.into();
    assert!(serde_json::from_value::<Decision>(value).is_err());
    assert!(
        rebuild(
            &events,
            &vec![d.clone(); 1025],
            "technical",
            &user(),
            &base(),
            &"a".repeat(64)
        )
        .is_err()
    );
    let mut bad = d;
    bad.review.checkpoint.environment_sha256 = "not-a-hash".into();
    assert!(
        rebuild(
            &events,
            &[bad],
            "technical",
            &user(),
            &base(),
            &"a".repeat(64)
        )
        .is_err()
    );
}
