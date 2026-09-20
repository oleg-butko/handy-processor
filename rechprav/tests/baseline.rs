use rechprav::{
    corpus::{self, Origin, Status},
    corrector::{self, Baseline, Corrector, Decision, Policy},
    dictionary::Dictionary,
    metrics, review, text,
};
use std::{collections::BTreeSet, path::Path};

fn engine(words: &str) -> Baseline {
    Baseline {
        dictionary: Box::new(Dictionary::from_text(words, false).unwrap()),
        domain: BTreeSet::new(),
        ignore: BTreeSet::new(),
        policy: Policy::default(),
        strict_yo: false,
        frequency: None,
    }
}

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<_> = a.chars().collect();
    let b: Vec<_> = b.chars().collect();
    let mut matrix = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in matrix[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
    }
    matrix[a.len()][b.len()]
}

fn strings(max: usize) -> Vec<String> {
    let mut result = vec![String::new()];
    let mut frontier = vec![String::new()];
    for _ in 0..max {
        frontier = frontier
            .iter()
            .flat_map(|s| ['а', 'б', 'ё'].map(|c| format!("{s}{c}")))
            .collect();
        result.extend(frontier.clone());
    }
    result
}

#[test]
fn trie_matches_exhaustive_reference_including_cyrillic_and_empty_queries() {
    let words: Vec<_> = strings(4).into_iter().filter(|s| !s.is_empty()).collect();
    let dict = Dictionary::from_words(words.clone(), true).unwrap();
    for query in strings(4) {
        for distance in 0..=2 {
            let found = dict.lookup(&query, distance, 1000, 100_000);
            assert!(found.complete);
            let mut expected: Vec<_> = words
                .iter()
                .filter_map(|w| {
                    let d = edit_distance(&query, w);
                    (d <= distance).then_some((d, w.as_str()))
                })
                .collect();
            expected.sort();
            assert_eq!(
                found
                    .candidates
                    .iter()
                    .map(|c| (c.distance, c.key.as_str()))
                    .collect::<Vec<_>>(),
                expected,
                "{query} distance={distance}"
            );
        }
    }
}

#[test]
fn edits_preserve_punctuation_whitespace_case_and_endings() {
    let mut engine = engine("драйвер");
    engine.policy.protect_titlecase = false;
    engine.policy.protect_uppercase = false;
    let result = corrector::analyze("«Драйвир»,\tДРАЙВИР!\r\nдрайве\rдрайверр", &engine);
    assert_eq!(result.corrected, "«Драйвер»,\tДРАЙВЕР!\r\nдрайвер\rдрайвер");
    assert_eq!(result.edits.len(), 4);
    assert!(result.decisions.iter().all(|d| d.confidence.is_none()));
}

#[test]
fn protection_is_prioritized_and_real_words_are_never_replaced() {
    let mut engine = engine("драйвер\nплод\nплот\nвулканы\nмодульки");
    engine.domain.insert("вулкан".into());
    engine.ignore.insert("модулька".into());
    let source = "вулкан модулька плот Драйвир ДРАЙВИР дРайвир CUDA C++ RTX4090 /tmp/драйвир драйвир.txt `драйвир --help` на";
    let a = corrector::analyze(source, &engine);
    assert_eq!(a.corrected, source);
    assert_eq!(a.decisions[0].reason, "domain_dictionary");
    assert_eq!(a.decisions[1].reason, "ignore_dictionary");
    assert_eq!(a.decisions[2].reason, "known_word");
    assert_eq!(a.decisions[3].reason, "titlecase");
}

#[test]
fn dotted_filenames_are_atomic_but_decimals_remain_numbers() {
    let source = "драйвир.txt 12.5 `драйвир --help`";
    let tokens = text::tokenize(source, false);
    assert_eq!(tokens.lexical.len(), 3);
    assert_eq!(
        tokens.text[tokens.lexical[0].text_token].kind,
        text::Kind::Protected
    );
    assert_eq!(
        tokens.text[tokens.lexical[1].text_token].kind,
        text::Kind::Number
    );
    assert_eq!(
        tokens.text[tokens.lexical[2].text_token].kind,
        text::Kind::Protected
    );
}

#[test]
fn ambiguity_and_incomplete_search_never_authorize_a_replacement() {
    let mut engine = engine("маска\nмасло\nмаски");
    let result = corrector::analyze("маско", &engine);
    assert_eq!(result.decisions[0].reason, "ambiguous_candidates");
    assert_eq!(result.corrected, "маско");
    engine.policy.max_candidates = 2;
    let result = corrector::analyze("маско", &engine);
    assert_eq!(result.decisions[0].reason, "candidate_limit");
    assert!(!result.decisions[0].search_complete);
    assert_eq!(result.corrected, "маско");
    let mut engine = super_engine();
    engine.policy.max_search_nodes = 1;
    let result = corrector::analyze("драйвир", &engine);
    assert_eq!(result.decisions[0].reason, "search_node_limit");
    assert_eq!(result.corrected, "драйвир");
}

fn super_engine() -> Baseline {
    engine("драйвер")
}

#[test]
fn spelling_order_and_yo_collisions_are_deterministic() {
    let a = Dictionary::from_text("ёж\nеж\nёлка\nель", false).unwrap();
    let b = Dictionary::from_text("ель\nёлка\nеж\nёж", false).unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(
        serde_json::to_string(&a.export()).unwrap(),
        serde_json::to_string(&b.export()).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&a.lookup("елко", 1, 20, 1000)).unwrap(),
        serde_json::to_string(&b.lookup("елко", 1, 20, 1000)).unwrap()
    );
    let roundtrip =
        Dictionary::from_json(&serde_json::to_string(&a.export()).unwrap(), false).unwrap();
    assert!(roundtrip.contains("елка"));
    assert!(Dictionary::from_json(&serde_json::to_string(&a.export()).unwrap(), true).is_err());
    assert!(Dictionary::from_text("CUDA", false).is_err());
    assert!(Dictionary::from_text("слово другое", false).is_err());
}

#[test]
fn transpositions_are_two_levenshtein_edits_not_one() {
    let engine = engine("драйвер");
    assert_eq!(edit_distance("драйвре", "драйвер"), 2);
    assert_eq!(corrector::analyze("драйвре", &engine).corrected, "драйвре");
}

#[test]
fn config_paths_are_relative_to_config_and_unknown_keys_fail() {
    let (engine, meta) = Baseline::load(&root().join("configs/baseline.toml")).unwrap();
    assert_eq!(engine.dictionary.len(), 57);
    assert!(meta["resource_sha256"]["dictionary"].is_string());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "dictionary='missing'\n[policy]\nmax_distnce=1").unwrap();
    assert!(Baseline::load(&path).is_err());
    let bad = Policy {
        max_distance: 2,
        ..Policy::default()
    };
    assert!(bad.validate().is_err());
    let bad = Policy {
        max_candidates: 1,
        ..Policy::default()
    };
    assert!(bad.validate().is_err());
}

fn record(source: &str, gold: &str) -> corpus::Record {
    let mut r = corpus::prepare(source, "test", Origin::Synthetic)
        .unwrap()
        .remove(0);
    r.gold = Some(gold.into());
    r.annotation_status = Status::Reviewed;
    if source != gold {
        r.edits.push(corpus::Edit {
            byte_start: 0,
            byte_end: source.len(),
            token_start: 0,
            token_end: text::tokenize(source, false).lexical.len(),
            replacement: gold.into(),
        });
    }
    r
}

#[test]
fn evaluator_counts_wrong_candidate_as_both_fp_and_fn_and_harms_separately() {
    let e = engine("завет\nкатер");
    let r = metrics::benchmark(&[record("савет", "совет")], &e)
        .unwrap()
        .report;
    assert_eq!(
        (r.counts_1_to_1.tp, r.counts_1_to_1.fp, r.counts_1_to_1.fn_),
        (0, 1, 1)
    );
    assert_eq!(r.counts_1_to_1.harmed_correct_input, 0);
    assert_eq!(r.overall_precision, Some(0.0));
    let r = metrics::benchmark(&[record("капер", "капер")], &e)
        .unwrap()
        .report;
    assert_eq!(
        (
            r.counts_1_to_1.fp,
            r.counts_1_to_1.fn_,
            r.counts_1_to_1.harmed_correct_input
        ),
        (1, 0, 1)
    );
    assert_eq!(r.rates_1_to_1.false_correction_rate, Some(1.0));
    assert_eq!(r.wer_before, Some(0.0));
    assert_eq!(r.wer_after, Some(1.0));
}

struct CompoundDamage;
impl Corrector for CompoundDamage {
    fn name(&self) -> &'static str {
        "test"
    }
    fn strict_yo(&self) -> bool {
        false
    }
    fn suggest(&self, source: &str, i: usize, t: &text::LexicalToken) -> Decision {
        let mut d = Decision::keep(source, i, t, "test");
        if t.key == "пример" {
            d.candidate = "примеры".into();
        }
        d
    }
}

#[test]
fn changes_inside_unsupported_gold_spans_are_not_hidden_from_precision() {
    let r = metrics::benchmark(&[record("на пример", "например")], &CompoundDamage)
        .unwrap()
        .report;
    assert_eq!(r.unsupported_gold_spans, 1);
    assert_eq!(r.proposals_in_unsupported_spans, 1);
    assert_eq!(r.overall_precision, Some(0.0));
    assert_eq!(r.all_gold_span_recall, Some(0.0));
    assert_eq!(r.rates_1_to_1.precision, None);
}

#[test]
fn review_wraps_on_both_sides_when_replacements_grow() {
    let mut engine = engine("ааааа");
    engine.policy.min_word_length = 4;
    let source = "аааа аааа аааа аааа аааа аааа аааа";
    let line = text::lines(source).remove(0);
    let (rendered, debug) = review::render_engine(&line, 32, &engine).unwrap();
    assert!(rendered.lines().all(|l| review::fits(l, 32)));
    assert_eq!(
        debug
            .segments
            .iter()
            .map(|s| s.changes.split_whitespace().count())
            .sum::<usize>(),
        7
    );
    assert!(
        debug
            .segments
            .iter()
            .all(|s| s.token_end - s.token_start == s.changes.split_whitespace().count())
    );
    assert_eq!(debug.corrected, ["ааааа"; 7].join(" "));
    assert!(debug.segments.len() > 1);
}

#[test]
fn long_replacement_and_source_share_one_token_across_visual_fragments() {
    let spelling = "а".repeat(34);
    let input = "а".repeat(33);
    let e = engine(&spelling);
    let line = text::lines(&input).remove(0);
    let (rendered, debug) = review::render_engine(&line, 32, &e).unwrap();
    assert!(rendered.lines().all(|l| review::fits(l, 32)));
    assert_eq!(debug.segments.len(), 2);
    assert!(
        debug
            .segments
            .iter()
            .all(|s| (s.token_start, s.token_end) == (0, 1))
    );
    assert_eq!(
        debug
            .segments
            .iter()
            .map(|s| s.changes.as_str())
            .collect::<String>(),
        spelling
    );
    assert_eq!(debug.corrected, spelling);
}

#[test]
fn resource_ablation_changes_only_the_intended_protection() {
    let (normal, _) = Baseline::load(&root().join("configs/baseline.toml")).unwrap();
    let (no_domain, _) = Baseline::load(&root().join("configs/baseline-no-domain.toml")).unwrap();
    let (no_ignore, _) = Baseline::load(&root().join("configs/baseline-no-ignore.toml")).unwrap();
    let (title, _) = Baseline::load(&root().join("configs/baseline-allow-titlecase.toml")).unwrap();
    assert_eq!(
        corrector::analyze("вулкан модулька", &normal).corrected,
        "вулкан модулька"
    );
    assert_eq!(
        corrector::analyze("вулкан модулька", &no_domain).corrected,
        "вулканы модулька"
    );
    assert_eq!(
        corrector::analyze("вулкан модулька", &no_ignore).corrected,
        "вулкан модульки"
    );
    assert_eq!(
        corrector::analyze("Драйвир Капер", &normal).corrected,
        "Драйвир Капер"
    );
    assert_eq!(
        corrector::analyze("Драйвир Капер", &title).corrected,
        "Драйвер Катер"
    );
}
