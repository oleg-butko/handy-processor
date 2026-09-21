#[path = "../src/feedback.rs"]
mod feedback;
use feedback::{Event, draft};
use rechprav::{
    corrector::Baseline,
    dictionary::Dictionary,
    terms::{Config, TermEngine},
};
use std::collections::BTreeSet;
fn base() -> Baseline {
    Baseline {
        dictionary: Box::new(
            Dictionary::from_text("свод\nпамяти\nпроколол\nколесо\nнет\nзвон", false).unwrap(),
        ),
        domain: BTreeSet::new(),
        ignore: BTreeSet::new(),
        policy: Default::default(),
        strict_yo: false,
        frequency: None,
    }
}
fn user() -> Config {
    serde_json::from_value(serde_json::json!({"baseline_config":"base.toml","enabled":true,"sets":[{"id":"user","terms":[{"id":"slot","canonical":"слот","aliases":["слот"],"forms":[{"canonical":"слота"}]}]}]})).unwrap()
}
fn event(id: &str, before: &str, after: &str) -> Event {
    serde_json::from_value(serde_json::json!({"schema_version":1,"event_id":id,"kind":"correction","scope":"technical","before":before,"after":after,"confirmed_asr":true})).unwrap()
}
fn undo(id: &str, target: &str) -> Event {
    serde_json::from_value(serde_json::json!({"schema_version":1,"event_id":id,"kind":"retract","target_event_id":target})).unwrap()
}
fn accepted(mut c: Config) -> TermEngine {
    c.enabled = true;
    for set in &mut c.sets {
        set.enabled = true;
        for t in &mut set.terms {
            t.enabled = true;
        }
    }
    TermEngine::new(c, Some(base())).unwrap()
}
#[test]
fn minimal_utf8_change_context_and_disabled_draft_roundtrip() {
    let events = [event(
        "one",
        "мы сняли\tсвод памяти!",
        "мы сняли\tслот памяти!",
    )];
    let d = draft(&events, "technical", &user(), &base()).unwrap();
    assert_eq!(d.proposals.len(), 1);
    let change = d.audit[0].change.as_ref().unwrap();
    assert_eq!(
        &"мы сняли\tсвод памяти!"[change.byte_start..change.byte_end],
        "свод"
    );
    let p = &d.proposals[0];
    assert_eq!(p.context_pattern.as_deref(), Some("сняли {term} памяти"));
    assert_eq!(p.target_class, "existing_user_term");
    assert!(!d.config.enabled);
    assert!(!d.config.sets[0].enabled);
    assert!(!d.config.sets[0].terms[0].enabled);
    let round: Config = toml::from_str(&toml::to_string(&d.config).unwrap()).unwrap();
    round.validate().unwrap();
    assert!(
        TermEngine::new(round, None)
            .unwrap()
            .analyze("сняли свод памяти")
            .edits
            .is_empty()
    );
    let e = accepted(d.config);
    assert_eq!(
        e.analyze("сняли свод памяти").corrected,
        "сняли слот памяти"
    );
    for s in [
        "свод правил",
        "сняли свод арки",
        "сняли\nсвод памяти",
        "сняли, свод памяти",
    ] {
        assert!(e.analyze(s).edits.is_empty(), "{s}");
    }
}
#[test]
fn duplicates_repeated_evidence_retractions_and_readdition_are_recomputed() {
    let a = event("one", "сняли свод памяти", "сняли слот памяти");
    let b = event("two", "сняли свод памяти", "сняли слот памяти");
    let first = draft(std::slice::from_ref(&a), "technical", &user(), &base()).unwrap();
    let repeated = draft(
        &[a.clone(), a.clone(), b.clone()],
        "technical",
        &user(),
        &base(),
    )
    .unwrap();
    assert_eq!(repeated.duplicate_events, 1);
    assert_eq!(repeated.proposals[0].evidence_ids, vec!["one", "two"]);
    assert_eq!(
        toml::to_string(&first.config).unwrap(),
        toml::to_string(&repeated.config).unwrap()
    );
    let once = draft(
        &[a.clone(), b.clone(), undo("u", "one")],
        "technical",
        &user(),
        &base(),
    )
    .unwrap();
    assert_eq!(first.proposals[0].id, once.proposals[0].id);
    assert_eq!(once.proposals[0].evidence_ids, vec!["two"]);
    let mut events = vec![
        a,
        b,
        undo("u", "one"),
        undo("v", "two"),
        undo("again", "two"),
    ];
    assert!(
        draft(&events, "technical", &user(), &base())
            .unwrap()
            .proposals
            .is_empty()
    );
    events.push(event("new", "сняли свод памяти", "сняли слот памяти"));
    assert_eq!(
        draft(&events, "technical", &user(), &base())
            .unwrap()
            .proposals[0]
            .id,
        first.proposals[0].id
    );
}
#[test]
fn scopes_conflicts_and_order_do_not_choose_a_winner() {
    let a = event("slot", "сняли свод памяти", "сняли слот памяти");
    let b = event("switch", "сняли свод памяти", "сняли свитч памяти");
    let d = draft(&[a.clone(), b.clone()], "technical", &user(), &base()).unwrap();
    assert_eq!(d.proposals.len(), 2);
    assert!(
        d.proposals
            .iter()
            .all(|p| p.conflicting_proposals.len() == 1)
    );
    assert!(
        accepted(d.config)
            .analyze("сняли свод памяти")
            .edits
            .is_empty()
    );
    let x = draft(&[b.clone(), a.clone()], "technical", &user(), &base()).unwrap();
    let y = draft(&[a.clone(), b], "technical", &user(), &base()).unwrap();
    assert_eq!(
        toml::to_string(&x.config).unwrap(),
        toml::to_string(&y.config).unwrap()
    );
    let mut other = a.clone();
    if let Event::Correction { scope, .. } = &mut other {
        *scope = "other".into();
    }
    assert!(
        draft(&[other.clone()], "technical", &user(), &base())
            .unwrap()
            .proposals
            .is_empty()
    );
    assert_eq!(
        draft(&[other], "other", &user(), &base())
            .unwrap()
            .proposals
            .len(),
        1
    );
}
#[test]
fn unsupported_or_unconfirmed_edits_are_reviewed_not_learned() {
    for (before, after) in [
        ("свод и ввод", "слот и вход"),
        ("свод  памяти", "слот памяти"),
        ("таб таб", "таб"),
        ("таб", "таб таб"),
        ("слот", "Слот"),
        ("`свод`", "`слот`"),
        ("https://x/свод", "https://x/слот"),
        ("свод", "свод"),
        ("свод", ""),
        ("", "слот"),
    ] {
        let d = draft(
            &[event("edit", before, after)],
            "technical",
            &user(),
            &base(),
        )
        .unwrap();
        assert!(d.proposals.is_empty(), "{before}->{after}");
        assert_eq!(d.audit[0].status, "needs_selection_or_review");
    }
    let mut a = event("style", "свод", "слот");
    if let Event::Correction { confirmed_asr, .. } = &mut a {
        *confirmed_asr = false;
    }
    assert_eq!(
        draft(&[a], "technical", &user(), &base()).unwrap().audit[0].status,
        "not_confirmed_asr"
    );
}
#[test]
fn explicit_selection_requires_exact_utf8_reconstruction() {
    let mut e = event("explicit", "свод и ввод", "слот и вход");
    if let Event::Correction { selection, .. } = &mut e {
        *selection = Some(feedback::Selection {
            byte_start: 0,
            byte_end: "свод и ввод".len(),
            replacement: "слот и вход".into(),
        });
    }
    assert_eq!(
        draft(&[e.clone()], "technical", &user(), &base())
            .unwrap()
            .proposals
            .len(),
        1
    );
    if let Event::Correction { selection, .. } = &mut e {
        selection.as_mut().unwrap().byte_start = 1;
    }
    assert!(draft(&[e], "technical", &user(), &base()).is_err());
    let mut e = event("bad", "свод", "слот");
    if let Event::Correction { selection, .. } = &mut e {
        *selection = Some(feedback::Selection {
            byte_start: 0,
            byte_end: 8,
            replacement: "другое".into(),
        });
    }
    assert!(draft(&[e], "technical", &user(), &base()).is_err());
}
#[test]
fn classification_forms_phrases_and_no_context_do_not_mutate_dictionary() {
    let rows = [
        event("word", "он проковол колесо", "он проколол колесо"),
        event("unknown", "включи катэхэ", "включи КТХ"),
        event("form", "без снятия сваты", "без снятия слота"),
        event(
            "phrase",
            "надо долгождать ответа",
            "надо долго ждать ответа",
        ),
        event("isolated", "нет звон", "недозвон"),
    ];
    let b = base();
    let d = draft(&rows, "technical", &user(), &b).unwrap();
    assert_eq!(d.proposals.len(), 5);
    let class = |canonical: &str| {
        d.proposals
            .iter()
            .find(|p| p.canonical == canonical)
            .unwrap()
            .target_class
    };
    assert_eq!(class("проколол"), "dictionary_word");
    assert_eq!(class("КТХ"), "unknown_word_candidate");
    assert_eq!(class("слота"), "existing_user_term");
    assert_eq!(class("долго ждать"), "phrase");
    assert!(!b.dictionary.contains("ктх"));
    let e = accepted(d.config);
    assert_eq!(
        e.analyze("надо долгождать ответа").corrected,
        "надо долго ждать ответа"
    );
    assert!(e.analyze("нет звон").edits.is_empty());
}
#[test]
fn invalid_journal_ids_forward_retractions_and_limits_fail_closed() {
    let a = event("one", "свод", "слот");
    let b = event("one", "свод", "свитч");
    assert!(draft(&[a.clone(), b], "technical", &user(), &base()).is_err());
    assert!(
        draft(
            &[undo("u", "one"), a.clone()],
            "technical",
            &user(),
            &base()
        )
        .is_err()
    );
    assert!(
        draft(
            &[a.clone(), undo("u", "one"), undo("v", "u")],
            "technical",
            &user(),
            &base()
        )
        .is_err()
    );
    assert!(draft(&[a], "../escape", &user(), &base()).is_err());
    assert!(
        draft(
            &[event("long", &"я".repeat(4097), "слот")],
            "technical",
            &user(),
            &base()
        )
        .is_err()
    );
    let long_word = draft(
        &[event("review", &"я".repeat(129), "слот")],
        "technical",
        &user(),
        &base(),
    )
    .unwrap();
    assert!(long_word.proposals.is_empty());
    assert!(serde_json::from_str::<Event>(r#"{"schema_version":1,"event_id":"x","kind":"retract","target_event_id":"a","extra":true}"#).is_err());
}

#[test]
fn pure_insert_delete_selections_and_profile_protection_stay_review_only() {
    for (before, after) in [("свод", "свод слот"), ("свод слот", "свод")] {
        let mut e = event("selection", before, after);
        if let Event::Correction { selection, .. } = &mut e {
            *selection = Some(feedback::Selection {
                byte_start: 0,
                byte_end: before.len(),
                replacement: after.into(),
            });
        }
        assert!(
            draft(&[e], "technical", &user(), &base())
                .unwrap()
                .proposals
                .is_empty()
        );
    }
    let mut b = base();
    b.ignore.insert("свод".into());
    let d = draft(
        &[event("ignored", "сняли свод памяти", "сняли слот памяти")],
        "technical",
        &user(),
        &b,
    )
    .unwrap();
    assert!(d.proposals.is_empty());
    assert_eq!(d.audit[0].status, "protected_by_profile");
}

#[test]
fn explicit_phrase_selection_cannot_change_runtime_preserved_edges() {
    for (before, after) in [
        ("мой свод", "Мой слот"),
        ("мой  свод", "мой слот"),
        ("свод мой", "слот Мой"),
    ] {
        let mut e = event("edges", before, after);
        if let Event::Correction { selection, .. } = &mut e {
            *selection = Some(feedback::Selection {
                byte_start: 0,
                byte_end: before.len(),
                replacement: after.into(),
            });
        }
        assert!(
            draft(&[e], "technical", &user(), &base())
                .unwrap()
                .proposals
                .is_empty(),
            "{before}->{after}"
        );
    }
    let mut e = event("edges", "мой свод", "мой слот");
    if let Event::Correction { selection, .. } = &mut e {
        *selection = Some(feedback::Selection {
            byte_start: 0,
            byte_end: "мой свод".len(),
            replacement: "мой слот".into(),
        });
    }
    assert_eq!(
        draft(&[e], "technical", &user(), &base())
            .unwrap()
            .proposals
            .len(),
        1
    );
}
