#[path = "../src/feedback.rs"]
mod feedback;
#[path = "../src/feedback_composition.rs"]
mod feedback_composition;
#[path = "../src/feedback_registry.rs"]
mod feedback_registry;
use feedback::Event;
use feedback_composition::{Binding, Composition, Owner, Settings, compose};
use feedback_registry::{Action, Decision, Registry, rebuild};
use rechprav::{
    corrector::Baseline,
    dictionary::Dictionary,
    terms::{Config, TermEngine},
};
use serde_json::json;
use std::collections::BTreeSet;

fn base() -> Baseline {
    Baseline {
        dictionary: Box::new(Dictionary::from_text("свод\nсваты\nпамяти", false).unwrap()),
        domain: BTreeSet::new(),
        ignore: BTreeSet::new(),
        policy: Default::default(),
        strict_yo: false,
        frequency: None,
    }
}
fn manual() -> Config {
    serde_json::from_value(json!({"baseline_config":"base.toml","enabled":true,"sets":[{"id":"hardware","terms":[
        {"id":"slot","canonical":"слот","aliases":["свуты"],"known_words":"always","forms":[
            {"canonical":"слота","require_context":true,"context_patterns":["без снятия {term}"]}]}]}]})).unwrap()
}
fn event(id: &str, before: &str, after: &str) -> Event {
    serde_json::from_value(json!({"schema_version":1,"kind":"correction","event_id":id,"scope":"technical","before":before,"after":after,"confirmed_asr":true})).unwrap()
}
fn slot() -> Event {
    event("slot", "сняли свод памяти", "сняли слот памяти")
}
fn registry(user: &Config, events: &[Event]) -> Registry {
    let r = rebuild(events, &[], "technical", user, &base(), &"a".repeat(64)).unwrap();
    let decisions: Vec<_> = r
        .review_tokens
        .into_iter()
        .enumerate()
        .map(|(i, review)| Decision {
            schema_version: 1,
            decision_id: format!("accept-{i}"),
            action: Action::Accept,
            review,
        })
        .collect();
    rebuild(
        events,
        &decisions,
        "technical",
        user,
        &base(),
        &"a".repeat(64),
    )
    .unwrap()
}
fn settings(r: &Registry) -> Settings {
    Settings {
        enabled: true,
        bindings: r
            .draft
            .proposals
            .iter()
            .map(|p| Binding {
                proposal_id: p.id.clone(),
                enabled: true,
                owner: Some(Owner {
                    set_id: "hardware".into(),
                    term_id: "slot".into(),
                }),
                boost: 0.0,
            })
            .collect(),
    }
}
fn built(user: &Config, events: &[Event]) -> Composition {
    let r = registry(user, events);
    let s = settings(&r);
    compose(user, r, &s, false).unwrap()
}
fn corrected(c: &Config, source: &str) -> String {
    let round: Config = toml::from_str(&toml::to_string(c).unwrap()).unwrap();
    TermEngine::new(round, Some(base()))
        .unwrap()
        .analyze(source)
        .corrected
}

#[test]
fn manual_definitions_are_preserved_and_feedback_is_routed_separately() {
    let user = manual();
    let c = built(&user, &[slot()]);
    assert_eq!(
        serde_json::to_value(&user.sets[0].terms[0]).unwrap(),
        serde_json::to_value(&c.config.sets[0].terms[0]).unwrap()
    );
    assert_eq!(c.audit[0].status, "added");
    assert_eq!(c.config.sets[0].terms.len(), 2);
    assert_eq!(corrected(&c.config, "свуты"), "слот");
    assert_eq!(
        corrected(&c.config, "сняли свод памяти"),
        "сняли слот памяти"
    );
    assert_eq!(corrected(&c.config, "свод правил"), "свод правил");
    let r = registry(&user, &[slot()]);
    let c = compose(
        &user,
        r,
        &Settings {
            enabled: true,
            bindings: vec![],
        },
        false,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&c.config).unwrap(),
        serde_json::to_value(&user).unwrap()
    );
}

#[test]
fn global_set_term_and_binding_switches_do_not_resurrect_terms() {
    for mode in ["global", "set", "term", "feedback", "binding"] {
        let mut user = manual();
        match mode {
            "global" => user.enabled = false,
            "set" => user.sets[0].enabled = false,
            "term" => user.sets[0].terms[0].enabled = false,
            _ => {}
        }
        let r = registry(&user, &[slot()]);
        let mut s = settings(&r);
        if mode == "feedback" {
            s.enabled = false;
        }
        if mode == "binding" {
            s.bindings[0].enabled = false;
        }
        let c = compose(&user, r, &s, false).unwrap();
        assert_ne!(c.audit[0].status, "added");
        assert_eq!(
            serde_json::to_value(&c.config).unwrap(),
            serde_json::to_value(&user).unwrap()
        );
    }
}

#[test]
fn family_patterns_are_intersected_not_ored_and_exclusions_survive() {
    let mut user = manual();
    let c = built(
        &user,
        &[event("form", "без снятия сваты", "без снятия слота")],
    );
    assert_eq!(
        c.config.sets[0].terms[1].context_patterns,
        vec!["без снятия {term}"]
    );
    assert_eq!(corrected(&c.config, "без снятия сваты"), "без снятия слота");
    assert_eq!(
        corrected(&c.config, "после снятия сваты"),
        "после снятия сваты"
    );
    user.sets[0].terms[0].context_patterns = vec![
        "вчера сняли {term} памяти".into(),
        "сняли {term} телефона".into(),
    ];
    let c = built(&user, &[slot()]);
    assert_eq!(
        c.config.sets[0].terms[1].context_patterns,
        vec!["вчера сняли {term} памяти"]
    );
    assert_eq!(
        corrected(&c.config, "сняли свод памяти"),
        "сняли свод памяти"
    );
    assert_eq!(
        corrected(&c.config, "вчера сняли свод памяти"),
        "вчера сняли слот памяти"
    );
    user.sets[0].terms[0].context_exclusions = vec!["вчера сняли {term}".into()];
    assert_eq!(
        corrected(&built(&user, &[slot()]).config, "вчера сняли свод памяти"),
        "вчера сняли свод памяти"
    );
}

#[test]
fn owner_weight_and_known_gate_apply_to_learned_alias() {
    let mut user = manual();
    user.sets[0].boost = 1.0;
    user.sets[0].terms[0].boost = -5.0;
    let c = built(&user, &[slot()]);
    assert_eq!(c.config.sets[0].terms[1].boost, -5.0);
    assert_eq!(
        corrected(&c.config, "сняли свод памяти"),
        "сняли свод памяти"
    );
    user.sets[0].terms[0].boost = 3.0;
    assert_eq!(
        corrected(&built(&user, &[slot()]).config, "сняли свод памяти"),
        "сняли слот памяти"
    );
    user.sets[0].terms[0].known_words = rechprav::terms::KnownWords::Never;
    assert_eq!(
        corrected(&built(&user, &[slot()]).config, "сняли свод памяти"),
        "сняли свод памяти"
    );
}

#[test]
fn explicit_new_terms_cannot_evade_owners_and_deleted_owner_remains_missing() {
    let mut user = manual();
    let r = registry(&user, &[slot()]);
    let mut s = settings(&r);
    s.bindings[0].owner = None;
    let c = compose(&user, r, &s, false).unwrap();
    assert_eq!(c.audit[0].status, "existing_target_requires_owner");
    let r = registry(&user, &[slot()]);
    let s = settings(&r);
    user.sets[0].terms.clear();
    assert_eq!(
        compose(&user, r, &s, false).unwrap().audit[0].status,
        "owner_missing"
    );
    let r = registry(&user, &[event("new", "включи катэхэ", "включи КТХ")]);
    let mut s = settings(&r);
    s.bindings[0].owner = None;
    let c = compose(&user, r, &s, false).unwrap();
    assert_eq!(c.audit[0].status, "added");
    assert_eq!(corrected(&c.config, "включи катэхэ"), "включи КТХ");
}

#[test]
fn unsupported_modes_cues_and_disjoint_patterns_preserve_manual_profile() {
    for mode in ["edit", "cues", "disjoint"] {
        let mut user = manual();
        match mode {
            "edit" => user.policy.mode = rechprav::terms::MatchMode::Edit,
            "cues" => user.sets[0].terms[0].context = vec!["памяти".into()],
            _ => user.sets[0].terms[0].context_patterns = vec!["вставили {term}".into()],
        }
        let c = built(&user, &[slot()]);
        assert_ne!(c.audit[0].status, "added");
        assert_eq!(
            serde_json::to_value(&c.config).unwrap(),
            serde_json::to_value(&user).unwrap()
        );
    }
}

#[test]
fn retraction_removes_feedback_without_removing_manual_alias() {
    let user = manual();
    let r = registry(&user, &[slot()]);
    let s = settings(&r);
    let decision = Decision {
        schema_version: 1,
        decision_id: "a".into(),
        action: Action::Accept,
        review: r.review_tokens[0].clone(),
    };
    let undo: Event = serde_json::from_value(
        json!({"schema_version":1,"event_id":"undo","kind":"retract","target_event_id":"slot"}),
    )
    .unwrap();
    let r = rebuild(
        &[slot(), undo],
        &[decision],
        "technical",
        &user,
        &base(),
        &"a".repeat(64),
    )
    .unwrap();
    let c = compose(&user, r, &s, false).unwrap();
    assert_eq!(c.audit[0].status, "not_accepted");
    assert_eq!(corrected(&c.config, "свуты"), "слот");
    assert_eq!(
        corrected(&c.config, "сняли свод памяти"),
        "сняли свод памяти"
    );
}

#[test]
fn invalid_bindings_and_term_id_collisions_are_errors() {
    assert!(
        serde_json::from_value::<Binding>(
            json!({"proposal_id":"example","enabled":true,"boost":0.0})
        )
        .is_err()
    );
    let mut user = manual();
    let r = registry(&user, &[slot()]);
    let mut s = settings(&r);
    s.bindings.push(s.bindings[0].clone());
    assert!(compose(&user, r, &s, false).is_err());
    let r = registry(&user, &[slot()]);
    let mut s = settings(&r);
    s.bindings[0].boost = 1.0;
    assert!(compose(&user, r, &s, false).is_err());
    let r = registry(&user, &[slot()]);
    let s = settings(&r);
    let mut collision = user.sets[0].terms[0].clone();
    collision.id = s.bindings[0].proposal_id.clone();
    user.sets[0].terms.push(collision);
    assert!(compose(&user, r, &s, false).is_err());
    assert!(
        serde_json::from_value::<Settings>(json!({"enabled":true,"bindings":[],"unexpected":1}))
            .is_err()
    );
}
