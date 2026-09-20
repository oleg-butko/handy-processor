use rechprav::{
    corrector::{Baseline, Policy, analyze},
    dictionary::Dictionary,
    frequency::{FrequencyPolicy, FrequencyScorer},
};
use std::collections::BTreeSet;

fn engine(words: &str, frequencies: &str) -> Baseline {
    Baseline {
        dictionary: Box::new(Dictionary::from_text(words, false).unwrap()),
        domain: BTreeSet::new(),
        ignore: BTreeSet::new(),
        policy: Policy::default(),
        strict_yo: false,
        frequency: Some(
            FrequencyScorer::from_tsv(frequencies, false, FrequencyPolicy::default()).unwrap(),
        ),
    }
}

#[test]
fn bounded_log_prior_has_known_values_and_zero_weight_ablation() {
    let mut p = FrequencyPolicy::default();
    assert_eq!(p.prior(None), 0.0);
    assert_eq!(p.prior(Some(0.0)), 0.0);
    assert!((p.prior(Some(999.0)) - 3.0).abs() < 1e-12);
    assert_eq!(p.prior(Some(1e9)), 6.0);
    p.weight = 0.0;
    p.validate().unwrap();
    assert_eq!(p.prior(Some(1e9)), 0.0);
}

#[test]
fn frequency_can_resolve_multiple_candidates_and_scores_are_explained() {
    let e = engine("маска\nмасло", "маска\t9999\nмасло\t9\nмаско\t0");
    let a = analyze("«маско»!", &e);
    assert_eq!(a.corrected, "«маска»!");
    let d = &a.decisions[0];
    assert_eq!(d.reason, "frequency_margins_passed");
    assert!((d.candidate_score.unwrap() - 3.0).abs() < 1e-12);
    assert_eq!(d.keep_score, Some(1.0));
    assert!((d.keep_margin.unwrap() - 2.0).abs() < 1e-12);
    assert!((d.runner_up_margin.unwrap() - 3.0).abs() < 1e-12);
    assert_eq!(d.alternatives[0].frequency_per_billion, Some(9999.0));
    assert!(d.confidence.is_none());
}

#[test]
fn frequent_original_can_keep_even_with_one_candidate() {
    let e = engine("катер", "катер\t9999\nкапер\t999");
    let a = analyze("капер", &e);
    assert_eq!(a.corrected, "капер");
    assert_eq!(a.decisions[0].reason, "keep_score_margin");
    // Presence in the frequency table must not add a valid dictionary entry.
    assert!(!e.dictionary.contains("капер"));
    assert!((a.decisions[0].keep_margin.unwrap() + 1.0).abs() < 1e-12);
}

#[test]
fn frequent_wrong_candidate_is_still_possible_without_context() {
    // Explicit counterexample: frequency preference is not semantic evidence.
    let e = engine("маска\nмасло", "маска\t9\nмасло\t9999");
    assert_eq!(analyze("маско", &e).corrected, "масло");
}

#[test]
fn tie_and_near_tie_keep_with_deterministic_alternatives() {
    let e = engine("масло\nмаска", "масло\t9999\nмаска\t9999");
    let a = analyze("маско", &e);
    assert_eq!(a.corrected, "маско");
    assert_eq!(a.decisions[0].reason, "runner_up_score_margin");
    assert_eq!(a.decisions[0].alternatives[0].word, "маска");
    let e = engine("масло\nмаска", "масло\t9999\nмаска\t9000");
    assert_eq!(analyze("маско", &e).corrected, "маско");
}

#[test]
fn unknown_and_explicit_zero_frequency_do_not_authorize_a_change() {
    for frequencies in ["масло\t9999", "маска\t0"] {
        let a = analyze("маско", &engine("маска", frequencies));
        assert_eq!(a.corrected, "маско");
        assert_eq!(a.decisions[0].reason, "candidate_frequency_unknown");
    }
}

#[test]
fn incomplete_search_and_protection_override_frequency() {
    let mut e = engine("маска\nмасло\nмаски", "маска\t1e9\nмасло\t1\nмаски\t1");
    e.policy.max_candidates = 2;
    let a = analyze("маско", &e);
    assert_eq!(a.corrected, "маско");
    assert_eq!(a.decisions[0].reason, "candidate_limit");
    e.policy.max_candidates = 20;
    e.policy.max_search_nodes = 1;
    assert_eq!(
        analyze("маско", &e).decisions[0].reason,
        "search_node_limit"
    );
    e.policy.max_search_nodes = 100_000;
    e.ignore.insert("маско".into());
    assert_eq!(
        analyze("маско", &e).decisions[0].reason,
        "ignore_dictionary"
    );
    e.ignore.clear();
    e.domain.insert("маско".into());
    assert_eq!(
        analyze("маско", &e).decisions[0].reason,
        "domain_dictionary"
    );
    e.domain.clear();
    assert_eq!(
        analyze("Маско МАСКО CUDA маска", &e).corrected,
        "Маско МАСКО CUDA маска"
    );
}

#[test]
fn malformed_frequencies_and_nonfinite_or_unsafe_policies_are_rejected() {
    for data in [
        "",
        "слово NaN",
        "слово\tNaN",
        "слово\tinf",
        "слово\t-1",
        "слово\t1000000001",
        "слово\t1\t2",
        "два слова\t1",
        "CUDA\t1",
        "Слово\t1",
    ] {
        assert!(
            FrequencyScorer::from_tsv(data, false, FrequencyPolicy::default()).is_err(),
            "{data}"
        );
    }
    for p in [
        FrequencyPolicy {
            weight: f64::NAN,
            ..Default::default()
        },
        FrequencyPolicy {
            keep_bias: f64::INFINITY,
            ..Default::default()
        },
        FrequencyPolicy {
            min_keep_margin: 0.0,
            ..Default::default()
        },
        FrequencyPolicy {
            min_runner_up_margin: 0.0,
            ..Default::default()
        },
        FrequencyPolicy {
            log_cap: -1.0,
            ..Default::default()
        },
    ] {
        assert!(p.validate().is_err());
    }
}

#[test]
fn yo_collisions_use_max_deterministically_not_double_counting() {
    let a = FrequencyScorer::from_tsv("ёлка\t10\nелка\t20\nёлка\t5", false, Default::default())
        .unwrap();
    let b = FrequencyScorer::from_tsv("ёлка\t5\nелка\t20\nёлка\t10", false, Default::default())
        .unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a.get("елка"), Some(20.0));
    assert_eq!(a.get("елка"), b.get("елка"));
    let strict = FrequencyScorer::from_tsv("ёлка\t10\nелка\t20", true, Default::default()).unwrap();
    assert_eq!(strict.len(), 2);
}

#[test]
fn config_loads_relative_frequency_path_and_hashes_exact_bytes() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let data = "маска\t9999\nмасло\t9";
    std::fs::write(dir.path().join("words.txt"), "маска\nмасло").unwrap();
    std::fs::write(dir.path().join("frequency.tsv"), data).unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "dictionary='words.txt'\n[frequency]\npath='frequency.tsv'\n[frequency.scoring]\nweight=1.0").unwrap();
    let (e, meta) = Baseline::load(&config).unwrap();
    assert_eq!(
        meta["resource_sha256"]["frequency"],
        format!("{:x}", Sha256::digest(data.as_bytes()))
    );
    assert_eq!(analyze("маско", &e).corrected, "маска");
    std::fs::write(&config, "dictionary='words.txt'\n[frequency]\npath='frequency.tsv'\n[frequency.scoring]\nweigth=1.0").unwrap();
    assert!(Baseline::load(&config).is_err());
}
