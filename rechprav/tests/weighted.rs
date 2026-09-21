use rechprav::{
    corrector::Baseline,
    dictionary::Dictionary,
    terms::{Config, KnownWords, TermEngine},
};
use std::collections::BTreeSet;
fn config() -> Config {
    serde_json::from_value(serde_json::json!({"baseline_config":"unused","enabled":true,
        "policy":{"mode":"edit","weighted_distance":{"max_cost":100,"insertion":100,"deletion":100,
            "substitutions":[{"from":"п","to":"б","cost":50},{"from":"т","to":"д","cost":50}]}},
        "sets":[{"id":"user","terms":[{"id":"clipboard","canonical":"клипборд","aliases":["клипборд"],
            "boost":1,"known_words":"always","max_known_distance":0}]}]})).unwrap()
}
fn engine(c: Config, words: &str) -> TermEngine {
    TermEngine::new(
        c,
        Some(Baseline {
            dictionary: Box::new(Dictionary::from_text(words, false).unwrap()),
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
fn weighted_finds_new_double_error_but_keeps_raw_known_cap() {
    let c = config();
    let a = engine(c.clone(), "обычное").analyze("клиппорт");
    assert_eq!(a.corrected, "клипборд");
    let x = &a.decisions[0].candidates[0];
    assert_eq!(x.distance, 2);
    assert_eq!(x.weighted_cost, Some(1.0));
    assert_eq!(
        engine(c.clone(), "клиппорт").analyze("клиппорт").decisions[0].candidates[0].gate,
        "known_distance_limit"
    );
    let mut c = c;
    c.policy.weighted_distance = None;
    assert!(engine(c, "обычное").analyze("клиппорт").edits.is_empty());
}
#[test]
fn raw_cap_prefers_feasible_alias_even_when_its_weighted_cost_is_higher() {
    let mut c = config();
    c.policy.weighted_distance.as_mut().unwrap().max_cost = 150;
    c.policy.weighted_distance.as_mut().unwrap().substitutions[0].cost = 50;
    c.sets[0].terms[0].aliases = vec!["бб".into(), "пв".into()];
    c.sets[0].terms[0].max_known_distance = Some(1);
    c.policy
        .weighted_distance
        .as_mut()
        .unwrap()
        .substitutions
        .push(serde_json::from_value(serde_json::json!({"from":"п","to":"в","cost":150})).unwrap());
    let a = engine(c, "пп").analyze("пп");
    let x = &a.decisions[0].candidates[0];
    assert_eq!(x.alias, "пв");
    assert_eq!(x.distance, 1);
    assert_eq!(x.weighted_cost, Some(1.5));
}
#[test]
fn protections_context_conflicts_and_all_or_nothing_budget_remain() {
    let c = config();
    for s in [
        "`клиппорт`",
        "https://host/клиппорт",
        "КЛИППОРТ",
        "клипборд",
    ] {
        assert!(engine(c.clone(), "обычное").analyze(s).edits.is_empty());
    }
    let mut c = config();
    c.sets[0].terms[0].known_words = KnownWords::Never;
    assert!(engine(c, "клиппорт").analyze("клиппорт").edits.is_empty());
    let mut c = config();
    c.sets[0].terms[0].require_context = true;
    c.sets[0].terms[0].context_patterns = vec!["открой {term}".into()];
    assert!(
        engine(c.clone(), "обычное")
            .analyze("клиппорт")
            .edits
            .is_empty()
    );
    assert_eq!(
        engine(c, "обычное").analyze("открой клиппорт").corrected,
        "открой клипборд"
    );
    let mut c = config();
    let mut rival = c.sets[0].terms[0].clone();
    rival.id = "other".into();
    rival.canonical = "другое".into();
    c.sets[0].terms.push(rival);
    assert!(engine(c, "обычное").analyze("клиппорт").edits.is_empty());
    let mut c = config();
    c.policy.max_comparisons = 1;
    let a = engine(c, "обычное").analyze("клиппорт клиппорт");
    assert!(!a.complete);
    assert!(a.edits.is_empty());
}
#[test]
fn weighted_config_is_optional_and_rejects_unbounded_or_ambiguous_costs() {
    let c = config();
    assert!(c.validate().is_ok());
    for cost in [0, 49, 201, usize::MAX] {
        let mut c = config();
        c.policy.weighted_distance.as_mut().unwrap().insertion = cost;
        assert!(c.validate().is_err());
    }
    let mut c = config();
    c.policy.weighted_distance.as_mut().unwrap().max_cost = 201;
    assert!(c.validate().is_err());
    let mut c = config();
    let w = c.policy.weighted_distance.as_mut().unwrap();
    w.substitutions.push(w.substitutions[0].clone());
    assert!(c.validate().is_err());
    let mut c = config();
    c.policy.mode = rechprav::terms::MatchMode::Exact;
    assert!(c.validate().is_err());
    for from in ['ё', 'Б', 'a', ' '] {
        let mut c = config();
        c.policy.weighted_distance.as_mut().unwrap().substitutions[0].from = from;
        assert!(c.validate().is_err());
    }
    let mut c = config();
    c.policy.weighted_distance = None;
    assert!(
        serde_json::to_value(&c).unwrap()["policy"]
            .get("weighted_distance")
            .is_none()
    );
    let a = engine(c, "обычное").analyze("клиппорд");
    assert!(
        serde_json::to_value(&a.decisions[0].candidates[0])
            .unwrap()
            .get("weighted_cost")
            .is_none()
    );
}
