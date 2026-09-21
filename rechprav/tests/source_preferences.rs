use rechprav::{
    corrector::Baseline,
    dictionary::Dictionary,
    terms::{Config, KnownWords, TermEngine},
};
use std::collections::BTreeSet;

fn config() -> Config {
    serde_json::from_value(serde_json::json!({
        "baseline_config":"unused.toml", "enabled":true,
        "sets":[{"id":"user","terms":[{"id":"slot","canonical":"слот",
            "aliases":["свод","ввод"],"boost":-2.0,"known_words":"always","max_known_distance":0}]}],
        "source_penalties":[{"id":"global","source":"свод","penalty":2.0}]
    })).unwrap()
}
fn engine(c: Config) -> TermEngine {
    TermEngine::new(
        c,
        Some(Baseline {
            dictionary: Box::new(Dictionary::from_text("свод\nввод\nнет\nзвон", false).unwrap()),
            domain: BTreeSet::new(),
            ignore: BTreeSet::new(),
            policy: Default::default(),
            strict_yo: false,
            frequency: None,
        }),
    )
    .unwrap()
}

#[test]
fn source_penalty_changes_keep_threshold_but_not_competing_target_margin() {
    let mut c = config();
    let selected = engine(c.clone()).analyze("свод");
    assert_eq!(selected.corrected, "слот");
    assert_eq!(selected.decisions[0].candidates[0].score, 3.0);
    assert_eq!(selected.decisions[0].candidates[0].source_penalty, 2.0);
    c.source_penalties.clear();
    assert!(engine(c.clone()).analyze("свод").edits.is_empty());
    let mut c = config();
    let mut other = c.sets[0].clone();
    other.id = "other".into();
    other.terms[0].canonical = "свитч".into();
    c.sets.push(other);
    let result = engine(c.clone()).analyze("свод");
    assert!(result.edits.is_empty());
    assert_eq!(result.decisions[0].runner_up_margin, Some(0.0));
    c.sets[0].boost = 1.0;
    assert_eq!(engine(c).analyze("свод").corrected, "слот");
}

#[test]
fn contextual_maximum_does_not_sum_duplicates_and_respects_boundaries() {
    let mut c = config();
    c.source_penalties[0].penalty = 0.5;
    let mut scoped = c.source_penalties[0].clone();
    scoped.id = "scoped".into();
    scoped.penalty = 2.0;
    scoped.context_patterns = vec!["сняли {term}".into()];
    c.source_penalties.push(scoped.clone());
    scoped.id = "copy".into();
    c.source_penalties.push(scoped);
    for source in ["сняли свод", "Сняли\tсвод!"] {
        let a = engine(c.clone()).analyze(source);
        let d = a.decisions.iter().find(|d| d.selected.is_some()).unwrap();
        assert_eq!(d.candidates[0].source_penalty, 2.0);
        assert_eq!(d.candidates[0].score, 3.0);
        assert_eq!(
            d.candidates[0].source_penalty_hits,
            vec!["copy", "global", "scoped"]
        );
    }
    for source in [
        "свод правил",
        "сняли\nсвод",
        "сняли, свод",
        "сняли СВОД",
        "сняли `свод`",
        "https://host/свод",
    ] {
        assert!(
            engine(c.clone()).analyze(source).edits.is_empty(),
            "{source}"
        );
    }
    c.source_penalties.reverse();
    assert_eq!(engine(c).analyze("сняли свод").corrected, "сняли слот");
}

#[test]
fn source_penalties_never_bypass_gates_protection_or_candidate_generation() {
    let mut c = config();
    c.source_penalties[0].penalty = 10.0;
    for policy in [KnownWords::Never, KnownWords::Preserve] {
        c.sets[0].terms[0].known_words = policy;
        assert!(engine(c.clone()).analyze("свод").edits.is_empty());
    }
    c.sets[0].terms[0].known_words = KnownWords::Always;
    c.sets[0].terms[0].require_context = true;
    c.sets[0].terms[0].context_patterns = vec!["сняли {term}".into()];
    assert!(engine(c.clone()).analyze("свод правил").edits.is_empty());
    c.sets[0].terms[0].context_exclusions = vec!["{term} арки".into()];
    assert!(
        engine(c.clone())
            .analyze("сняли свод арки")
            .edits
            .is_empty()
    );
    let mut c = config();
    c.policy.mode = rechprav::terms::MatchMode::Edit;
    c.sets[0].terms[0].aliases = vec!["своды".into()];
    assert!(engine(c).analyze("свод").edits.is_empty()); // known distance cap zero
    let mut c = config();
    c.source_penalties[0].source = "постороннее".into();
    assert!(engine(c).analyze("постороннее").edits.is_empty());
    let mut c = config();
    c.sets[0].terms[0].canonical = "свод".into();
    assert!(engine(c).analyze("свод").edits.is_empty());
}

#[test]
fn multiword_source_matches_token_boundaries_without_deleting_neighbors() {
    let mut c = config();
    c.sets[0].terms[0].canonical = "недозвон".into();
    c.sets[0].terms[0].aliases = vec!["нет звон".into()];
    c.source_penalties[0].source = "нет звон".into();
    let a = engine(c.clone()).analyze("это нет\tзвон клиента");
    assert_eq!(a.corrected, "это недозвон клиента");
    assert_eq!(a.edits.len(), 1);
    assert_eq!(a.edits[0].token_end - a.edits[0].token_start, 2);
    for source in ["нет, звон", "нет\nзвон", "нетзвон"] {
        assert!(engine(c.clone()).analyze(source).edits.is_empty());
    }
}

#[test]
fn switches_and_budget_drop_partial_edits_and_empty_defaults_preserve_debug() {
    let mut c = config();
    c.source_penalties[0].enabled = false;
    assert!(engine(c.clone()).analyze("свод").edits.is_empty());
    c.source_penalties.clear();
    let encoded = serde_json::to_value(&c).unwrap();
    assert!(encoded.get("source_penalties").is_none());
    let candidate =
        serde_json::to_value(&engine(c).analyze("свод").decisions[0].candidates[0]).unwrap();
    assert!(candidate.get("source_penalty").is_none());
    assert!(candidate.get("source_penalty_hits").is_none());
    let mut c = config();
    c.sets[0].enabled = false;
    assert!(engine(c).analyze("свод").edits.is_empty());
    let mut c = config();
    c.enabled = false;
    assert!(engine(c).analyze("свод").edits.is_empty());
    let mut c = config();
    c.policy.max_comparisons = 3;
    let a = engine(c).analyze("свод ввод");
    assert!(!a.complete);
    assert!(a.edits.is_empty());
    assert_eq!(a.corrected, "свод ввод");
}

#[test]
fn malformed_or_unbounded_source_preferences_are_rejected() {
    for value in [-1.0, 11.0, f64::NAN, f64::INFINITY] {
        let mut c = config();
        c.source_penalties[0].penalty = value;
        assert!(c.validate().is_err());
    }
    for source in ["", "свод  правил", "one", "свод, правил", "а б в г"] {
        let mut c = config();
        c.source_penalties[0].source = source.into();
        assert!(c.validate().is_err());
    }
    let mut c = config();
    c.source_penalties.push(c.source_penalties[0].clone());
    assert!(c.validate().is_err());
    let mut c = config();
    c.source_penalties[0].context_patterns = vec!["{term}".into()];
    assert!(c.validate().is_err());
    let mut c = config();
    c.source_penalties[0].context_patterns = vec!["сняли {term}".into(); 33];
    assert!(c.validate().is_err());
    let mut c = config();
    let rule = c.source_penalties[0].clone();
    c.source_penalties = (0..129)
        .map(|i| {
            let mut r = rule.clone();
            r.id = format!("r{i}");
            r
        })
        .collect();
    assert!(c.validate().is_err());
}
