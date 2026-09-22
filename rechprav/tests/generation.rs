use rechprav::{
    corrector::Baseline,
    dictionary::Dictionary,
    terms::{Config, TermEngine},
};
use serde_json::json;
use std::collections::BTreeSet;

fn config(weighted: bool) -> Config {
    let mut c = json!({"baseline_config":"unused", "enabled":true,"policy":{"mode":"edit"},
    "sets":[{"id":"test","terms":[
        {"id":"new","canonical":"КТХ","aliases":["катэхэ"],"generation":"exact","known_words":"always","require_context":true,"context_patterns":["включи {term}"]},
        {"id":"manual","canonical":"клипборд","aliases":["калиборд"],"known_words":"always"},
        {"id":"pnz","canonical":"ПНЗ","aliases":["пэ эн зэ"],"generation":"exact","known_words":"always"}
    ]}]});
    if weighted {
        c["policy"]["weighted_distance"] =
            json!({"max_cost":100,"insertion":100,"deletion":100,"substitutions":[]});
    }
    serde_json::from_value(c).unwrap()
}
fn engine(c: Config) -> TermEngine {
    TermEngine::new(
        c,
        Some(Baseline {
            dictionary: Box::new(Dictionary::from_text("пэ\nсвод", false).unwrap()),
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
fn exact_rule_precedes_edit_and_weighted_generation_without_changing_manual_rules() {
    for weighted in [false, true] {
        let e = engine(config(weighted));
        assert_eq!(e.analyze("включи катэхэ").corrected, "включи КТХ");
        for source in ["включи катэхи", "включи ка тэхэ", "слово катэхэ"]
        {
            let a = e.analyze(source);
            assert_eq!(a.corrected, source);
            if source != "слово катэхэ" {
                assert!(
                    a.decisions
                        .iter()
                        .flat_map(|d| &d.candidates)
                        .all(|c| c.term != "new")
                );
            }
        }
        assert_eq!(e.analyze("калиборт").corrected, "клипборд");
        let a = e.analyze("включи катэхэ");
        assert!(
            a.decisions
                .iter()
                .flat_map(|d| &d.candidates)
                .filter(|c| c.term == "new")
                .all(|c| c.weighted_cost.is_none() && c.distance == 0)
        );
    }
}

#[test]
fn exact_preserves_word_boundaries_but_normalizes_horizontal_whitespace_and_case() {
    let e = engine(config(true));
    assert_eq!(e.analyze("Пэ\tэн  зэ").corrected, "ПНЗ");
    for source in ["пээнзэ", "пэ энзэ", "пэ эн\nзэ", "пэ, эн зэ", "`пэ эн зэ`"]
    {
        assert_eq!(e.analyze(source).corrected, source);
    }
}

#[test]
fn exact_does_not_override_weights_switches_known_gates_exclusions_or_budget() {
    let original = config(true);
    for mode in [
        "weight", "term", "set", "global", "exclude", "budget", "known",
    ] {
        let mut c = original.clone();
        match mode {
            "weight" => c.sets[0].terms[0].boost = -10.0,
            "term" => c.sets[0].terms[0].enabled = false,
            "set" => c.sets[0].enabled = false,
            "global" => c.enabled = false,
            "exclude" => c.sets[0].terms[0].context_exclusions = vec!["включи {term}".into()],
            "budget" => c.policy.max_comparisons = 1,
            "known" => c.sets[0].terms[2].known_words = rechprav::terms::KnownWords::Never,
            _ => unreachable!(),
        }
        let source = if mode == "known" {
            "пэ эн зэ"
        } else {
            "включи катэхэ"
        };
        let a = engine(c).analyze(source);
        assert_eq!(a.corrected, source, "{mode}");
        if mode == "budget" {
            assert!(!a.complete);
        }
    }
}

#[test]
fn family_variants_inherit_exact_and_unknown_generation_is_rejected() {
    let mut v = serde_json::to_value(config(false)).unwrap();
    v["sets"][0]["terms"][0]["forms"] = json!([{"canonical":"КТХа","aliases":["катэха"]}]);
    // Family canonical must be a Russian word, unlike this abbreviation.
    v["sets"][0]["terms"][0]["canonical"] = json!("слот");
    v["sets"][0]["terms"][0]["forms"][0]["canonical"] = json!("слота");
    let e = engine(serde_json::from_value(v.clone()).unwrap());
    assert_eq!(e.analyze("включи катэха").corrected, "включи слота");
    assert_eq!(e.analyze("включи катэху").corrected, "включи катэху");
    v["sets"][0]["terms"][0]["generation"] = json!("edit");
    assert!(serde_json::from_value::<Config>(v).is_err());
}
