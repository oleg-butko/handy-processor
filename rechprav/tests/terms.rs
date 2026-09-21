use rechprav::{
    corrector::Baseline,
    dictionary::Dictionary,
    pronunciation::{AliasOrigin, Pronunciation},
    terms::{Config, KnownWords, MatchMode, SpanTieBreak, TermEngine},
};
use std::collections::BTreeSet;

fn config() -> Config {
    toml::from_str(
        r#"
baseline_config='base.toml'
enabled=true
[[sets]]
id='hardware'
[[sets.terms]]
id='pnz'
canonical='ПНЗ'
aliases=['пэнзэ','пэ эн зэ','пензе']
known_words='context'
context=['проверь','модуль']
"#,
    )
    .unwrap()
}
fn base() -> Baseline {
    Baseline {
        dictionary: Box::new(Dictionary::from_text("пензе\nввод\nя\nслот", false).unwrap()),
        domain: BTreeSet::new(),
        ignore: BTreeSet::new(),
        policy: Default::default(),
        strict_yo: false,
        frequency: None,
    }
}
fn engine(c: Config) -> TermEngine {
    TermEngine::new(c, Some(base())).unwrap()
}

#[test]
fn letter_generator_has_fixed_names_segmentations_and_bounded_expansion() {
    let g = Pronunciation::RuLetters;
    let actual: BTreeSet<_> = g.generate("ПНЗ").unwrap().into_iter().collect();
    let expected = ["пээнзэ", "пэ энзэ", "пээн зэ", "пэ эн зэ"]
        .map(String::from)
        .into_iter()
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(g.generate("ПЛК").unwrap().len(), 8);
    assert!(g.generate("ПЛК").unwrap().contains(&"пэ эл ка".into()));
    let largest = g.generate("ЛЛЛ").unwrap();
    assert_eq!(largest.len(), 32);
    assert!(largest.iter().all(|a| a.split(' ').count() <= 3));
    for unsupported in ["П", "ПНЗС", "Пнз", "CPU", "П2", "ПЙ", "ЪП", "ПЬ", "", "П Н"]
    {
        assert!(g.generate(unsupported).is_err(), "{unsupported}");
    }
}

#[test]
fn generated_aliases_correct_all_splits_without_loosening_context_or_known_word_gates() {
    let mut c = config();
    c.sets[0].terms[0].pronunciation = Some(Pronunciation::RuLetters);
    c.sets[0].terms[0].aliases.clear();
    c.sets[0].terms[0].require_context = true;
    c.sets[0].terms[0].context_patterns = vec!["проверь {term}".into()];
    c.sets[0].terms[0].context.clear();
    for alias in Pronunciation::RuLetters.generate("ПНЗ").unwrap() {
        let a = engine(c.clone()).analyze(&format!("проверь {alias}!"));
        assert_eq!(a.corrected, "проверь ПНЗ!");
        assert_eq!(a.edits.len(), 1);
        assert_eq!(
            a.decisions
                .iter()
                .find(|d| d.selected.is_some())
                .unwrap()
                .candidates[0]
                .alias_origin,
            Some(AliasOrigin::Generated)
        );
        assert!(engine(c.clone()).analyze(&alias).edits.is_empty());
    }
    assert_eq!(
        engine(c.clone())
            .analyze("Проверь\tпэ  эн\u{00a0}зэ!")
            .corrected,
        "Проверь\tПНЗ!"
    );
    c.sets[0].terms[0].context_exclusions = vec!["{term} на карте".into()];
    for source in [
        "проверь пэ эн зэ на карте",
        "проверь `пэ эн зэ`",
        "проверь ПЭ ЭН ЗЭ",
        "проверь пэ эн\nзэ",
    ] {
        assert_eq!(engine(c.clone()).analyze(source).corrected, source);
    }
    c.sets[0].terms[0].known_words = KnownWords::Never;
    assert!(span_engine(c).analyze("проверь пэ энзэ").edits.is_empty());
}

#[test]
fn generated_and_manual_aliases_deduplicate_after_normalization_without_votes() {
    let mut c = config();
    let t = &mut c.sets[0].terms[0];
    t.canonical = "ЁП".into();
    t.pronunciation = Some(Pronunciation::RuLetters);
    t.aliases = vec!["е пэ".into()];
    let first = engine(c.clone()).analyze("проверь е пэ");
    c.sets[0].terms[0]
        .aliases
        .extend(["ё пэ".into(), "Е ПЭ".into(), "е пэ".into()]);
    let duplicate = engine(c.clone()).analyze("проверь е пэ");
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&duplicate).unwrap()
    );
    assert_eq!(
        duplicate.decisions[0].candidates[0].alias_origin,
        Some(AliasOrigin::ManualAndGenerated)
    );
    let mut b = base();
    b.strict_yo = true;
    let e = TermEngine::new(c, Some(b)).unwrap();
    assert_eq!(
        e.analyze("проверь ёпэ").decisions[0].candidates[0].alias_origin,
        Some(AliasOrigin::Generated)
    );
    assert_eq!(
        e.analyze("проверь е пэ").decisions[0].candidates[0].alias_origin,
        Some(AliasOrigin::Manual)
    );
}

#[test]
fn generated_aliases_respect_disabled_scopes_conflicts_and_search_budget() {
    let mut c = config();
    c.sets[0].terms[0].pronunciation = Some(Pronunciation::RuLetters);
    c.sets[0].terms[0].aliases.clear();
    let source = "проверь пэ энзэ";
    for scope in 0..3 {
        let mut disabled = c.clone();
        match scope {
            0 => disabled.enabled = false,
            1 => disabled.sets[0].enabled = false,
            _ => disabled.sets[0].terms[0].enabled = false,
        }
        let a = TermEngine::new(disabled, None).unwrap().analyze(source);
        assert_eq!(a.corrected, source);
        assert_eq!(a.comparisons, 0);
    }
    let mut other = c.sets[0].clone();
    other.id = "lab".into();
    other.terms[0].canonical = "XYZ".into();
    other.terms[0].pronunciation = None;
    other.terms[0].aliases = vec!["пэ энзэ".into()];
    c.sets.push(other);
    assert!(engine(c.clone()).analyze(source).edits.is_empty());
    c.sets.reverse();
    assert!(engine(c.clone()).analyze(source).edits.is_empty());
    c.policy.max_comparisons = 5;
    let a = engine(c).analyze("пэ энзэ проверь пэ энзэ");
    assert!(!a.complete);
    assert!(a.edits.is_empty());
    assert!(a.decisions.iter().all(|d| d.selected.is_none()));
}

#[test]
fn pronunciation_validation_bounds_expanded_not_only_manual_aliases() {
    let mut c = config();
    c.sets[0].terms[0].aliases.clear();
    assert!(c.validate().is_err());
    c.sets[0].terms[0].pronunciation = Some(Pronunciation::RuLetters);
    assert!(c.validate().is_ok());
    c.sets[0].terms[0].canonical = "CPU".into();
    assert!(c.validate().is_err());
    c.sets[0].terms[0].canonical = "ПНЗ".into();
    c.sets[0].terms[0].aliases = (1..=61).map(|n| "а".repeat(n)).collect();
    assert!(c.validate().is_err()); // 61 manual + 4 generated > 64.
    c.sets[0].terms[0].aliases.clear();
    let template = c.sets[0].clone();
    c.sets.clear();
    for group in 0..3 {
        let mut set = template.clone();
        set.id = format!("set{group}");
        set.terms = (0..if group == 2 { 1 } else { 512 })
            .map(|n| {
                let mut t = template.terms[0].clone();
                t.id = format!("t{n}");
                t
            })
            .collect();
        c.sets.push(set);
    }
    assert!(c.validate().is_err()); // 1025 * 4 > 4096.
    let invalid = toml::to_string(&config()).unwrap() + "\npronunciation='guess'\n";
    assert!(toml::from_str::<Config>(&invalid).is_err());
}

#[test]
fn pronunciation_manifest_exposes_active_normalized_aliases_and_provenance() {
    use std::fs;
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("base.toml"), "dictionary='words.txt'").unwrap();
    fs::write(temp.path().join("words.txt"), "пензе\n").unwrap();
    let mut c = config();
    c.sets[0].terms[0].pronunciation = Some(Pronunciation::RuLetters);
    let path = temp.path().join("terms.toml");
    fs::write(&path, toml::to_string(&c).unwrap()).unwrap();
    let (_, metadata) = TermEngine::load(&path, false).unwrap();
    let aliases = &metadata["pronunciation_aliases"][0]["aliases"];
    assert_eq!(aliases["пэ эн зэ"], "manual_and_generated");
    assert_eq!(aliases["пэнзэ"], "manual");
    assert_eq!(aliases["пээнзэ"], "generated");
    let (_, disabled) = TermEngine::load(&path, true).unwrap();
    assert!(disabled.get("pronunciation_aliases").is_none());
    assert!(disabled["baseline_resources"].is_null());
}

fn span_engine(c: Config) -> TermEngine {
    let mut b = base();
    b.dictionary = Box::new(Dictionary::from_text("пэ\nпензе\nя", false).unwrap());
    TermEngine::new(c, Some(b)).unwrap()
}

#[test]
fn span_distance_repairs_whole_pronunciation_but_does_not_infer_semantic_boundaries() {
    let mut c = config();
    c.policy.mode = MatchMode::Edit;
    assert_eq!(
        span_engine(c.clone()).analyze("проверь пэ энзэ").corrected,
        "проверь пэ ПНЗ"
    );
    for policy in [SpanTieBreak::Distance, SpanTieBreak::Longest] {
        c.policy.span_tie_break = policy;
        let e = span_engine(c.clone());
        let a = e.analyze("Проверь\tпэ  энзэ!");
        assert_eq!(a.corrected, "Проверь\tПНЗ!");
        assert_eq!(a.edits.len(), 1);
        assert_eq!(
            (a.edits[0].byte_start, a.edits[0].byte_end),
            ("Проверь\t".len(), "Проверь\tпэ  энзэ".len())
        );
        // Deliberate counterexample: literal letter 'пэ' is consumed as well.
        assert_eq!(
            e.analyze("проверь символ пэ энзэ").corrected,
            "проверь символ ПНЗ"
        );
        assert_eq!(
            e.analyze("проверь пэ энзэ пэнзэ").corrected,
            "проверь ПНЗ ПНЗ"
        );
        // A score difference always takes priority over either new tie-break.
        c.policy.known_penalty = 2.0;
        assert_eq!(
            span_engine(c.clone()).analyze("проверь пэ энзэ").corrected,
            "проверь пэ ПНЗ"
        );
        c.policy.known_penalty = 1.0;
    }
}

#[test]
fn span_distance_keeps_shorter_span_when_both_aliases_are_exact() {
    let mut c = config();
    c.policy.known_penalty = 0.0;
    c.sets[0].terms[0].known_words = KnownWords::Always;
    c.sets[0].terms[0].aliases.push("я пэнзэ".into());
    for policy in [
        SpanTieBreak::Shortest,
        SpanTieBreak::Distance,
        SpanTieBreak::Longest,
    ] {
        c.policy.span_tie_break = policy;
        let expected = if policy == SpanTieBreak::Longest {
            "ПНЗ"
        } else {
            "я ПНЗ"
        };
        assert_eq!(engine(c.clone()).analyze("я пэнзэ").corrected, expected);
        let original = serde_json::to_value(engine(c.clone()).analyze("я пэнзэ")).unwrap();
        c.sets[0].terms[0].aliases.reverse();
        assert_eq!(
            serde_json::to_value(engine(c.clone()).analyze("я пэнзэ")).unwrap(),
            original
        );
    }
}

#[test]
fn span_policies_do_not_bypass_target_conflicts_or_local_ambiguity() {
    for policy in [
        SpanTieBreak::Shortest,
        SpanTieBreak::Distance,
        SpanTieBreak::Longest,
    ] {
        let mut c = config();
        c.policy.span_tie_break = policy;
        c.sets[0].terms[0].aliases = vec!["аб вг".into()];
        let mut other = c.sets[0].terms[0].clone();
        other.id = "other".into();
        other.canonical = "XYZ".into();
        other.aliases = vec!["вг".into()];
        c.sets[0].terms.push(other);
        assert!(engine(c.clone()).analyze("аб вг").edits.is_empty());
        c.sets[0].terms.reverse();
        assert!(engine(c.clone()).analyze("аб вг").edits.is_empty());
        let mut third = c.sets[0].terms[0].clone();
        third.id = "third".into();
        third.canonical = "THIRD".into();
        c.sets[0].terms.push(third);
        assert!(engine(c).analyze("аб вг").edits.is_empty());
    }
}

#[test]
fn span_policies_preserve_gates_protection_and_all_or_nothing_budget() {
    for policy in [
        SpanTieBreak::Shortest,
        SpanTieBreak::Distance,
        SpanTieBreak::Longest,
    ] {
        let mut c = config();
        c.policy.span_tie_break = policy;
        c.policy.mode = MatchMode::Edit;
        c.sets[0].terms[0].context_exclusions = vec!["{term} на карте".into()];
        let e = span_engine(c.clone());
        for source in [
            "проверь пэ энзэ на карте",
            "проверь `пэ энзэ`",
            "проверь ПЭ ЭНЗЭ",
            "проверь пэ\nэнзэ на карте",
        ] {
            assert_eq!(e.analyze(source).corrected, source, "{policy:?}: {source}");
        }
        c.sets[0].terms[0].require_context = true;
        assert!(span_engine(c.clone()).analyze("пэ энзэ").edits.is_empty());
        c.policy.max_comparisons = 5;
        let a = span_engine(c.clone()).analyze("пэнзэ проверь пэ энзэ");
        assert!(!a.complete);
        assert!(a.edits.is_empty());
        assert!(a.decisions.iter().all(|d| d.selected.is_none()));
        c.enabled = false;
        let a = TermEngine::new(c, None).unwrap().analyze("проверь пэ энзэ");
        assert!(a.edits.is_empty());
        assert_eq!(a.comparisons, 0);
    }
}

#[test]
fn span_policy_serialization_preserves_legacy_metadata_and_rejects_typos() {
    let mut c = config();
    assert!(
        serde_json::to_value(&c).unwrap()["policy"]
            .get("span_tie_break")
            .is_none()
    );
    for policy in [
        SpanTieBreak::Shortest,
        SpanTieBreak::Distance,
        SpanTieBreak::Longest,
    ] {
        c.policy.span_tie_break = policy;
        let parsed: Config = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert_eq!(parsed.policy.span_tie_break, policy);
    }
    let invalid = toml::to_string(&c).unwrap().replace("longest", "longer");
    assert!(toml::from_str::<Config>(&invalid).is_err());
}

#[test]
fn known_distance_cap_blocks_fuzzy_real_words_but_keeps_oov_and_split_zero_distance() {
    let mut c = config();
    c.policy.mode = MatchMode::Edit;
    let make = |c| {
        let mut b = base();
        b.dictionary = Box::new(Dictionary::from_text("пение\nпензе\nэн\nпэ", false).unwrap());
        TermEngine::new(c, Some(b)).unwrap()
    };
    assert_eq!(
        make(c.clone()).analyze("проверь пение").corrected,
        "проверь ПНЗ"
    );
    c.sets[0].terms[0].max_known_distance = Some(0);
    let a = make(c.clone()).analyze("проверь пение");
    assert!(a.edits.is_empty());
    assert_eq!(
        a.decisions.last().unwrap().candidates[0].gate,
        "known_distance_limit"
    );
    assert_eq!(make(c.clone()).analyze("бэнзэ").corrected, "ПНЗ");
    assert_eq!(
        make(c.clone()).analyze("проверь пензе").corrected,
        "проверь ПНЗ"
    );
    assert!(make(c.clone()).analyze("проверь пэ эн зе").edits.is_empty());
    // Isolate zero-distance segmentation: a fuzzy OOV suffix can otherwise tie
    // the known-word whole span under the existing scorer/overlap policy.
    let mut zero_only = c.clone();
    zero_only.policy.max_distance = 0;
    assert_eq!(
        make(zero_only).analyze("проверь пэ энзэ").corrected,
        "проверь ПНЗ"
    );
    c.policy.boost = 10.0;
    assert!(make(c).analyze("проверь пение").edits.is_empty());
}

#[test]
fn anchored_context_requires_both_adjacent_sides_without_crossing_boundaries() {
    let mut c = config();
    let t = &mut c.sets[0].terms[0];
    t.context.clear();
    t.require_context = true;
    t.context_patterns = vec!["проверь {term} модуля".into()];
    let e = engine(c);
    let a = e.analyze("Проверь\tпэ  эн зэ\u{00a0}модуля!");
    assert_eq!(a.corrected, "Проверь\tПНЗ\u{00a0}модуля!");
    assert_eq!(
        a.decisions
            .iter()
            .find(|d| d.selected.is_some())
            .unwrap()
            .candidates[0]
            .context_pattern_hits,
        vec!["проверь {term} модуля"]
    );
    for source in [
        "проверь новый пэнзэ модуля",
        "проверь пэнзэ нового модуля",
        "пэнзэ модуля",
        "проверь пэнзэ",
        "проверь. пэнзэ модуля",
        "проверь пэнзэ, модуля",
        "проверь\nпэнзэ модуля",
        "проверь пэнзэ\rмодуля",
        "`проверь` пэнзэ модуля",
    ] {
        assert_eq!(e.analyze(source).corrected, source, "{source}");
    }
}

#[test]
fn exclusion_vetoes_known_oov_and_positive_cues_even_at_high_boost() {
    let mut c = config();
    c.policy.boost = 10.0;
    let t = &mut c.sets[0].terms[0];
    t.context_exclusions = vec!["{term} на карте".into()];
    t.context_patterns = vec!["проверь {term}".into()];
    let e = engine(c);
    for source in [
        "проверь пензе на карте",
        "проверь пэнзэ на карте",
        "проверь пэ эн зэ на карте",
    ] {
        let a = e.analyze(source);
        assert_eq!(a.corrected, source);
        assert!(
            a.decisions
                .iter()
                .any(|d| d.candidates[0].gate == "context_excluded")
        );
    }
    // Exclusions are local; they deliberately do not search across punctuation or gaps.
    assert_eq!(
        e.analyze("проверь пэнзэ. на карте").corrected,
        "проверь ПНЗ. на карте"
    );
    assert_eq!(
        e.analyze("проверь пэнзэ на новой карте").corrected,
        "проверь ПНЗ на новой карте"
    );
}

#[test]
fn context_is_optional_for_oov_unless_required_and_can_route_conflicting_sets() {
    let mut c = config();
    let t = &mut c.sets[0].terms[0];
    t.context.clear();
    t.context_patterns = vec!["модуль {term}".into()];
    assert_eq!(engine(c.clone()).analyze("пэнзэ").corrected, "ПНЗ");
    c.sets[0].terms[0].require_context = true;
    let mut lab = c.sets[0].clone();
    lab.id = "lab".into();
    lab.terms[0].canonical = "БНЗ".into();
    lab.terms[0].context_patterns = vec!["{term} на стенде".into()];
    c.sets.push(lab);
    let e = engine(c.clone());
    assert_eq!(e.analyze("модуль пэнзэ").corrected, "модуль ПНЗ");
    assert_eq!(e.analyze("пэнзэ на стенде").corrected, "БНЗ на стенде");
    assert_eq!(
        e.analyze("модуль пэнзэ на стенде").corrected,
        "модуль пэнзэ на стенде"
    );
    assert_eq!(e.analyze("пэнзэ").corrected, "пэнзэ");
    c.sets.reverse();
    assert_eq!(
        engine(c).analyze("пэнзэ на стенде").corrected,
        "БНЗ на стенде"
    );
}

#[test]
fn pattern_work_consumes_budget_and_discards_previous_selected_spans() {
    let mut c = config();
    c.sets[0].terms[0].context_patterns = vec!["проверь {term}".into()];
    c.policy.max_tokens = 1;
    c.policy.max_comparisons = 4;
    // Three aliases + one pattern for the first span exhaust the budget.
    let a = engine(c).analyze("пэнзэ пэнзэ");
    assert!(!a.complete);
    assert_eq!(a.comparisons, 4);
    assert_eq!(a.corrected, "пэнзэ пэнзэ");
    assert!(a.edits.is_empty());
    assert!(!a.decisions.is_empty());
    assert!(a.decisions.iter().all(|d| d.selected.is_none()));
}

#[test]
fn context_patterns_normalize_case_and_yo_using_baseline_mode() {
    let mut c = config();
    let t = &mut c.sets[0].terms[0];
    t.context.clear();
    t.require_context = true;
    t.context_patterns = vec!["Ёлки {term}".into()];
    assert_eq!(
        engine(c.clone()).analyze("елки пэнзэ").corrected,
        "елки ПНЗ"
    );
    let mut b = base();
    b.strict_yo = true;
    let e = TermEngine::new(c, Some(b)).unwrap();
    assert!(e.analyze("елки пэнзэ").edits.is_empty());
    assert_eq!(e.analyze("ёлки пэнзэ").corrected, "ёлки ПНЗ");
}

#[test]
fn malformed_or_unbounded_context_rules_fail_validation() {
    for pattern in [
        "{term}",
        "проверь",
        "{term} {term}",
        "проверь  {term}",
        "проверь\n{term}",
        "проверь, {term}",
        "* {term}",
        "а б в г д {term}",
    ] {
        let mut c = config();
        c.sets[0].terms[0].context_patterns = vec![pattern.into()];
        assert!(c.validate().is_err(), "{pattern}");
    }
    let mut c = config();
    c.sets[0].terms[0].max_known_distance = Some(3);
    assert!(c.validate().is_err());
    let mut c = config();
    c.policy.context_window = 0;
    c.sets[0].terms[0].context_patterns = vec!["проверь {term}".into()];
    assert!(c.validate().is_err());
    let mut c = config();
    let t = &mut c.sets[0].terms[0];
    t.context.clear();
    t.known_words = KnownWords::Always;
    t.require_context = true;
    t.context_exclusions = vec!["{term} данных".into()];
    assert!(c.validate().is_err());
    let mut c = config();
    c.sets[0].terms[0].context_patterns = vec!["проверь {term}".into(); 33];
    assert!(c.validate().is_err());
}

#[test]
fn disabled_feature_set_and_term_are_identity_without_loading_resources() {
    for level in 0..3 {
        let mut c = config();
        match level {
            0 => c.enabled = false,
            1 => c.sets[0].enabled = false,
            _ => c.sets[0].terms[0].enabled = false,
        };
        let e = TermEngine::new(c, None).unwrap();
        let source = "\u{feff}пэнзэ\tпензе\r\nпэ эн зэ";
        let a = e.analyze(source);
        assert_eq!(a.corrected, source);
        assert!(a.edits.is_empty());
        assert_eq!(a.comparisons, 0);
    }
    assert!(TermEngine::new(config(), None).is_err());
}

#[test]
fn aliases_and_multiword_spans_preserve_surrounding_bytes_and_canonical_case() {
    let e = engine(config());
    let source = "\u{feff}«пэнзэ»,\tпроверь пэ\tэн  зэ!\r\n";
    let a = e.analyze(source);
    assert_eq!(a.corrected, "\u{feff}«ПНЗ»,\tпроверь ПНЗ!\r\n");
    assert_eq!(a.edits.len(), 2);
    assert_eq!(a.edits[1].token_end - a.edits[1].token_start, 3);
    assert_eq!(
        &source[a.edits[1].byte_start..a.edits[1].byte_end],
        "пэ\tэн  зэ"
    );
    assert_eq!(e.analyze("ПНЗ").corrected, "ПНЗ");
    assert!(e.analyze("пэ, эн зэ").edits.is_empty());
    assert!(e.analyze("пэ эн\nзэ").edits.is_empty());
    assert!(e.analyze("пэ эн\rзэ").edits.is_empty());
}

#[test]
fn edit_mode_uses_pronunciation_distance_and_controls_thresholds() {
    let mut c = config();
    assert!(engine(c.clone()).analyze("бэнзэ").edits.is_empty());
    c.policy.mode = MatchMode::Edit;
    assert_eq!(engine(c.clone()).analyze("бэнзэ").corrected, "ПНЗ");
    assert_eq!(engine(c.clone()).analyze("пэ энзэ").corrected, "ПНЗ");
    c.sets[0].terms[0].boost = -0.1;
    assert!(engine(c.clone()).analyze("бэнзэ").edits.is_empty());
    c.sets[0].boost = 0.1;
    assert_eq!(engine(c.clone()).analyze("бэнзэ").corrected, "ПНЗ");
    c.policy.boost = -5.0;
    assert!(engine(c).analyze("пэнзэ").edits.is_empty());
}

#[test]
fn known_word_policy_is_a_gate_that_boost_cannot_bypass_and_context_has_boundaries() {
    let mut c = config();
    assert!(engine(c.clone()).analyze("я живу в пензе").edits.is_empty());
    assert_eq!(
        engine(c.clone()).analyze("проверь пензе").corrected,
        "проверь ПНЗ"
    );
    assert!(engine(c.clone()).analyze("проверь. пензе").edits.is_empty());
    assert!(engine(c.clone()).analyze("проверь\nпензе").edits.is_empty());
    assert!(
        engine(c.clone())
            .analyze("пензе а б в г проверь")
            .edits
            .is_empty()
    );
    c.sets[0].terms[0].known_words = KnownWords::Never;
    c.sets[0].terms[0].boost = 10.0;
    assert!(engine(c.clone()).analyze("проверь пензе").edits.is_empty());
    c.sets[0].terms[0].known_words = KnownWords::Always;
    assert_eq!(
        engine(c).analyze("я живу в пензе").corrected,
        "я живу в ПНЗ"
    );
}

#[test]
fn conflicting_sets_keep_ties_and_order_does_not_choose_winner() {
    let mut c = config();
    let mut other = c.sets[0].clone();
    other.id = "lab".into();
    other.terms[0].canonical = "БНЗ".into();
    c.sets.push(other);
    for reverse in [false, true] {
        let mut cc = c.clone();
        if reverse {
            cc.sets.reverse();
        }
        let a = engine(cc).analyze("пэнзэ");
        assert!(a.edits.is_empty());
        assert_eq!(a.decisions[0].reason, "term_conflict");
    }
    c.sets[0].boost = 0.99;
    assert!(engine(c.clone()).analyze("пэнзэ").edits.is_empty());
    c.sets[0].boost = 1.0;
    assert_eq!(engine(c.clone()).analyze("пэнзэ").corrected, "ПНЗ");
    c.sets[0].enabled = false;
    assert_eq!(engine(c.clone()).analyze("пэнзэ").corrected, "БНЗ");
    // Identical output from two sets must not create a false tie or additive boost.
    c.sets[0].enabled = true;
    c.sets[0].boost = 0.0;
    c.sets[1].terms[0].canonical = "ПНЗ".into();
    let a = engine(c).analyze("пэнзэ");
    assert_eq!(a.corrected, "ПНЗ");
    assert_eq!(a.decisions[0].candidates.len(), 1);
    assert_eq!(a.decisions[0].candidates[0].score, 4.0);
}

#[test]
fn technical_atoms_ignore_domain_and_canonical_terms_remain_protected() {
    let c = config();
    let mut b = base();
    b.ignore.insert("пэнзэ".into());
    b.domain.insert("пензе".into());
    assert!(
        TermEngine::new(c.clone(), Some(b))
            .unwrap()
            .analyze("проверь пэнзэ пензе")
            .edits
            .is_empty()
    );
    let e = engine(c);
    for s in [
        "`пэнзэ`",
        "https://host/пэнзэ",
        "./пэнзэ",
        "пэнзэ.txt",
        "ПЭНЗЭ",
        "пЭнзэ",
        "ПНЗ",
        "проверь 123",
    ] {
        assert_eq!(e.analyze(s).corrected, s, "{s}");
    }
}

#[test]
fn overlapping_ranges_never_double_apply_or_bypass_a_conflict() {
    let mut c = config();
    c.sets[0].terms[0].aliases = vec!["аб вг".into()];
    let mut other = c.sets[0].terms[0].clone();
    other.id = "other".into();
    other.canonical = "XYZ".into();
    other.aliases = vec!["вг де".into()];
    c.sets[0].terms.push(other);
    let a = engine(c.clone()).analyze("аб вг де");
    assert!(a.edits.is_empty());
    assert!(a.decisions.iter().all(|d| d.reason == "overlap_conflict"));
    c.sets[0].terms[0].boost = 1.0;
    assert_eq!(engine(c.clone()).analyze("аб вг де").corrected, "ПНЗ де");
    c.sets[0].terms[0].boost = 0.0;
    let mut third = c.sets[0].terms[0].clone();
    third.id = "third".into();
    third.canonical = "THIRD".into();
    c.sets[0].terms.push(third);
    assert!(engine(c).analyze("аб вг де").edits.is_empty());
    // Same-target overlap favors fewer consumed words when scores are identical.
    let mut c = config();
    c.policy.known_penalty = 0.0;
    c.sets[0].terms[0].known_words = KnownWords::Always;
    c.sets[0].terms[0].aliases.push("я пэнзэ".into());
    assert_eq!(engine(c).analyze("я пэнзэ").corrected, "я ПНЗ");
}

#[test]
fn exhausted_search_discards_all_partial_edits() {
    let mut c = config();
    c.policy.max_comparisons = 3;
    let a = engine(c).analyze("пэнзэ пэнзэ");
    assert!(!a.complete);
    assert_eq!(a.comparisons, 3);
    assert!(a.edits.is_empty());
    assert_eq!(a.corrected, "пэнзэ пэнзэ");
    assert!(a.decisions.iter().all(|d| d.selected.is_none()));
}

#[test]
fn invalid_configs_reject_unsafe_targets_ranges_and_nonfinite_preferences() {
    let mut cases = Vec::new();
    let mut c = config();
    c.policy.min_margin = 0.0;
    cases.push(c);
    let mut c = config();
    c.policy.boost = f64::NAN;
    cases.push(c);
    let mut c = config();
    c.policy.max_tokens = 4;
    cases.push(c);
    let mut c = config();
    c.sets[0].terms[0].aliases = vec!["пэ, эн зэ".into()];
    cases.push(c);
    let mut c = config();
    c.sets[0].terms[0].canonical = "ПНЗ\nдругое".into();
    cases.push(c);
    let mut c = config();
    c.sets[0].terms[0].context.clear();
    cases.push(c);
    let mut c = config();
    c.sets.push(c.sets[0].clone());
    cases.push(c);
    for c in cases {
        assert!(c.validate().is_err());
    }
}

#[test]
fn span_metrics_count_multiword_true_positive_and_harmed_correct_input_without_zip() {
    let mut c = config();
    let mut slot = c.sets[0].terms[0].clone();
    slot.id = "slot".into();
    slot.canonical = "слот".into();
    slot.aliases = vec!["ввод".into()];
    slot.known_words = KnownWords::Always;
    c.sets[0].terms.push(slot);
    let source = "пэ эн зэ ввод";
    let mut records =
        rechprav::corpus::prepare(source, "metrics", rechprav::corpus::Origin::Synthetic).unwrap();
    records[0].annotation_status = rechprav::corpus::Status::Reviewed;
    records[0].gold = Some("ПНЗ ввод".into());
    records[0].edits = vec![rechprav::corpus::Edit {
        byte_start: 0,
        byte_end: "пэ эн зэ".len(),
        token_start: 0,
        token_end: 3,
        replacement: "ПНЗ".into(),
    }];
    let run = rechprav::term_metrics::benchmark(&records, &engine(c)).unwrap();
    assert_eq!(
        run.report["span_counts"],
        serde_json::json!({"tp":1,"fp":1,"fn_":0,"correct_input":1,"harmed_correct_input":1})
    );
    assert_eq!(run.report["wer_after"], 0.5);
    assert_eq!(run.report["exact_records"], 0);
    records[0].annotation_status = rechprav::corpus::Status::Pending;
    records[0].gold = None;
    records[0].edits.clear();
    assert!(rechprav::term_metrics::benchmark(&records, &engine(config())).is_err());
}

#[test]
fn cli_preserves_file_endings_provenance_and_refuses_existing_or_invalid_outputs() {
    use std::{fs, process::Command};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("base.toml"), "dictionary='words.txt'").unwrap();
    fs::write(root.join("words.txt"), "пензе\nслот").unwrap();
    fs::write(root.join("terms.toml"), toml::to_string(&config()).unwrap()).unwrap();
    fs::write(root.join("input.txt"), "пэнзэ\r\n\r\nпроверь пэ эн зэ\r").unwrap();
    let run = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rechprav"))
            .current_dir(root)
            .args([
                "terms-review",
                "input.txt",
                "--config",
                "terms.toml",
                "--output-dir",
                "out",
            ])
            .args(extra)
            .output()
            .unwrap()
    };
    assert!(run(&[]).status.success());
    let corrected = fs::read(root.join("out/corrected.txt")).unwrap();
    assert_eq!(corrected, "ПНЗ\r\n\r\nпроверь ПНЗ\r".as_bytes());
    assert!(!run(&[]).status.success());
    assert_eq!(fs::read(root.join("out/corrected.txt")).unwrap(), corrected);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("out/manifest.json")).unwrap()).unwrap();
    assert!(
        manifest["resources"]["baseline_resources"]["resource_sha256"]["dictionary"].is_string()
    );
    assert!(manifest["compiled_source_sha256"]["src/terms.rs"].is_string());
    // Disable does not require the missing base dictionary.
    let mut c = config();
    c.baseline_config = "missing.toml".into();
    fs::write(root.join("terms.toml"), toml::to_string(&c).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_rechprav"))
        .current_dir(root)
        .args([
            "terms-inspect",
            "пэнзэ",
            "--config",
            "terms.toml",
            "--disable-terms",
        ])
        .output()
        .unwrap();
    assert!(result.status.success());
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["analysis"]["corrected"], "пэнзэ");
    let result = Command::new(env!("CARGO_BIN_EXE_rechprav"))
        .current_dir(root)
        .args([
            "terms-review",
            "input.txt",
            "--config",
            "terms.toml",
            "--output-dir",
            "bad",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!root.join("bad").exists());
}
