use rechprav::{
    corpus::{self, Origin, Split, Status},
    metrics::{self, Counts, WordErrors},
    review, text,
};
use serde::Deserialize;
use std::path::Path;

fn corpus() -> Vec<corpus::Record> {
    corpus::read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("data/fixtures/corpus.jsonl")).unwrap()
}

#[derive(Deserialize)]
struct Fixture {
    name: String,
    source: String,
    lexical: Vec<String>,
}

#[test]
fn curated_tokenization_is_lossless_and_mapped() {
    let fixtures: Vec<Fixture> =
        serde_json::from_str(include_str!("../data/fixtures/tokenization.json")).unwrap();
    assert!(fixtures.len() >= 50);
    for fixture in fixtures {
        let tokens = text::tokenize(&fixture.source, false);
        let actual: Vec<_> = tokens
            .lexical
            .iter()
            .map(|t| &fixture.source[t.byte_start..t.byte_end])
            .collect();
        assert_eq!(actual, fixture.lexical, "{}", fixture.name);
        let reconstructed: String = tokens
            .text
            .iter()
            .map(|t| &fixture.source[t.byte_start..t.byte_end])
            .collect();
        assert_eq!(reconstructed, fixture.source, "{}", fixture.name);
        let mut previous_end = 0;
        for t in &tokens.text {
            assert_eq!(t.byte_start, previous_end);
            assert!(t.byte_end > t.byte_start);
            previous_end = t.byte_end;
        }
        assert_eq!(previous_end, fixture.source.len());
        for (i, lexical) in tokens.lexical.iter().enumerate() {
            assert_eq!(tokens.text[lexical.text_token].lexical_index, Some(i));
            assert_eq!(
                tokens.text[lexical.text_token].byte_start,
                lexical.byte_start
            );
        }
    }
}

#[test]
fn normalization_never_changes_source_ranges() {
    let source = "Е\u{308}лка е\u{308}лка ЁЛКА елка";
    let normalized = text::tokenize(source, false);
    assert!(normalized.lexical.iter().all(|t| t.key == "елка"));
    let strict = text::tokenize(source, true);
    assert_eq!(
        strict
            .lexical
            .iter()
            .map(|t| t.key.as_str())
            .collect::<Vec<_>>(),
        ["ёлка", "ёлка", "ёлка", "елка"]
    );
    assert_eq!(
        &source[normalized.lexical[0].byte_start..normalized.lexical[0].byte_end],
        "Е\u{308}лка"
    );
}

#[test]
fn sensitive_atoms_and_mixed_case_are_protected() {
    let source = "CUDA C++ RTX4090 12.5 ~/x --help дРайвер ДРАЙВЕР я Драйвер";
    let tokens = text::tokenize(source, false);
    assert!(tokens.lexical[..9].iter().all(|t| t.protection.is_some()));
    assert_eq!(tokens.lexical[9].protection, None);
}

#[test]
fn line_reader_round_trips_all_endings_without_phantom_lines() {
    for source in [
        "",
        "\n",
        "\r\n",
        "а\r\n\rб\nв",
        "а\n",
        "\u{feff}текст\r",
        "\n\n",
    ] {
        let lines = text::lines(source);
        let rebuilt: String = lines
            .iter()
            .map(|l| format!("{}{}", l.content, l.ending))
            .collect();
        assert_eq!(rebuilt, source);
        for l in &lines {
            assert_eq!(
                &source[l.byte_start..l.byte_start + l.content.len()],
                l.content
            );
        }
    }
    assert_eq!(text::lines("а\n").len(), 1);
    assert!(text::lines("").is_empty());
}

#[test]
fn review_golden_preserves_source_and_one_marker_per_lexical_token() {
    let line = text::lines("«Драйвир», C++ и 12.5!\r\n").remove(0);
    let (review, debug) = review::render(&line, 130, false).unwrap();
    assert_eq!(review, "[000001.1]\n«Драйвир», C++ и 12.5!\n- - - -\n\n");
    assert_eq!(debug.ending, "\r\n");
    assert_eq!(debug.decisions.len(), 4);
    assert!(
        debug
            .decisions
            .iter()
            .all(|d| d.confidence.is_none() && d.candidate == d.original)
    );
}

#[test]
fn wrapping_long_tokens_controls_and_combining_clusters_obeys_both_limits() {
    let cases = [
        "слово ".repeat(200),
        format!("https://example.org/{} конец", "я".repeat(500)),
        "🙂".repeat(200),
        format!("е{}", "\u{301}".repeat(300)),
        "\t\u{1b}[31mтекст\u{202e}\0".into(),
        " ".repeat(500),
        "".into(),
    ];
    for source in cases {
        let line = text::Line {
            number: 1,
            byte_start: 0,
            content: &source,
            ending: "",
        };
        for width in [32, 65, 130] {
            let (rendered, debug) = review::render(&line, width, false).unwrap();
            assert!(
                rendered.lines().all(|s| review::fits(s, width)),
                "width {width}"
            );
            assert!(!rendered.contains('\u{1b}') && !rendered.contains('\u{202e}'));
            let markers = debug
                .segments
                .iter()
                .map(|s| s.changes.split_whitespace().filter(|&c| c == "-").count())
                .sum::<usize>();
            assert_eq!(markers, debug.tokens.lexical.len());
            for s in debug.segments {
                assert!(s.token_start <= s.token_end && s.token_end <= debug.tokens.lexical.len());
            }
        }
    }
}

#[test]
fn wrapping_ranges_cover_each_lexical_position_once_except_continuations() {
    let source = "первое второе третье четвёртое пятое шестое седьмое восьмое";
    let line = text::lines(source).remove(0);
    let (_, debug) = review::render(&line, 32, false).unwrap();
    let indices: Vec<_> = debug
        .segments
        .iter()
        .filter(|s| s.continuation_part == 0)
        .flat_map(|s| s.token_start..s.token_end)
        .collect();
    assert_eq!(indices, (0..8).collect::<Vec<_>>());
    assert_eq!(
        debug
            .segments
            .iter()
            .map(|s| s.source.as_str())
            .collect::<String>(),
        source
    );
}

#[test]
fn corpus_fixture_gold_and_counts_are_consistent() {
    let records = corpus();
    let summary = corpus::validate(&records).unwrap();
    assert_eq!(summary.reviewed, 12);
    assert_eq!(summary.synthetic, 12);
    let report = metrics::benchmark_keep(&records, false).unwrap();
    assert_eq!(report.engine, "keep");
    assert_eq!(report.rates_1_to_1.precision, None);
    assert_eq!(report.counts_1_to_1.fp, 0);
    assert_eq!(report.counts_1_to_1.harmed_correct_input, 0);
    assert_eq!(report.rates_1_to_1.recall, Some(0.0));
    assert_eq!(report.unsupported_gold_spans, 2);
    assert_eq!(report.wer_before, report.wer_after);
    assert!(report.wer_before.unwrap() > 0.0);
}

#[test]
fn validator_rejects_bad_utf8_offsets_inconsistent_gold_and_overlaps() {
    let base = corpus().into_iter().find(|r| !r.edits.is_empty()).unwrap();
    for mutation in 0..5 {
        let mut r = base.clone();
        match mutation {
            0 => r.edits[0].byte_start += 1,
            1 => r.gold = Some("не тот эталон".into()),
            2 => r.edits.push(r.edits[0].clone()),
            3 => r.edits[0].token_start = usize::MAX,
            _ => r.edits[0].replacement = "".into(),
        }
        assert!(corpus::validate(&[r]).is_err(), "mutation {mutation}");
    }
}

#[test]
fn validation_checks_ids_groups_and_cross_source_gold_leakage() {
    let base = corpus()[0].clone();
    assert!(corpus::validate(&[base.clone(), base.clone()]).is_err());
    let mut other = base.clone();
    other.id = "other".into();
    other.split = Split::Test;
    assert!(corpus::validate(&[base.clone(), other.clone()]).is_err());
    other.group_id = "other-group".into();
    other.source = base.gold.clone().unwrap().to_uppercase();
    other.gold = Some(other.source.clone());
    other.edits.clear();
    assert!(corpus::validate(&[base, other]).is_err());
}

#[test]
fn pending_and_ambiguous_cannot_silently_become_gold() {
    let pending = corpus::prepare("пример\nещё пример", "pilot", Origin::Synthetic).unwrap();
    assert!(
        pending
            .iter()
            .all(|r| r.gold.is_none() && r.edits.is_empty())
    );
    assert!(metrics::benchmark_keep(&pending, false).is_err());
    let mut r = pending[0].clone();
    r.gold = Some(r.source.clone());
    assert!(corpus::validate(&[r]).is_err());
    let mut r = pending[0].clone();
    r.annotation_status = Status::Ambiguous;
    assert!(metrics::benchmark_keep(&[r], false).is_err());
    assert!(
        corpus::prepare(
            "исходник\n\n--- РЕЗУЛЬТАТ ---\n\nрезультат",
            "pilot",
            Origin::RealAsr
        )
        .is_err()
    );
}

#[test]
fn benchmarks_do_not_mix_synthetic_real_or_splits() {
    let mut records = corpus();
    records[1].provenance.kind = Origin::RealAsr;
    assert!(metrics::benchmark_keep(&records, false).is_err());
    let mut records = corpus();
    records[1].split = Split::Test;
    assert!(metrics::benchmark_keep(&records, false).is_err());
}

#[test]
fn correction_metrics_use_hand_computed_confusion_counts() {
    let mut c = Counts::default();
    c.observe("драйвир", "драйвер", "драйвер"); // TP
    c.observe("кат", "кот", "кит"); // FP and FN
    c.observe("плот", "плод", "плот"); // FN only
    c.observe("куда", "куда", "CUDA"); // FP and harm
    c.observe("мир", "мир", "мир"); // correct KEEP
    assert_eq!(
        c,
        Counts {
            tp: 1,
            fp: 2,
            fn_: 2,
            correct_input: 2,
            harmed_correct_input: 1
        }
    );
    let r = c.rates();
    assert_eq!(r.precision, Some(1.0 / 3.0));
    assert_eq!(r.recall, Some(1.0 / 3.0));
    assert_eq!(r.f05, Some(1.0 / 3.0));
    assert_eq!(r.false_correction_rate, Some(0.5));
    assert!(Counts::default().rates().precision.is_none());
    assert!(Counts::default().rates().f1.is_none());
    let unequal = Counts {
        tp: 2,
        fp: 1,
        fn_: 4,
        correct_input: 5,
        harmed_correct_input: 1,
    }
    .rates();
    assert_eq!(unequal.f05, Some(5.0 / 9.0));
    assert_eq!(unequal.f1, Some(4.0 / 9.0));
}

#[test]
fn lexical_annotations_cannot_rewrite_punctuation() {
    let mut r = corpus()[0].clone();
    r.source = "кот, дом".into();
    r.gold = Some("кот дом".into());
    r.edits = vec![corpus::Edit {
        byte_start: 0,
        byte_end: r.source.len(),
        token_start: 0,
        token_end: 2,
        replacement: "кот дом".into(),
    }];
    assert!(corpus::validate_record(&r).is_err());
    r.source = "кот дом".into();
    r.gold = Some("кот, дом".into());
    r.edits[0].byte_end = r.source.len();
    r.edits[0].replacement = "кот, дом".into();
    assert!(corpus::validate_record(&r).is_err());
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn wer_counts_insertions_deletions_and_compounds_not_zip() {
    assert_eq!(
        metrics::word_errors(&words("а б в"), &words("а г")),
        WordErrors {
            substitutions: 1,
            deletions: 1,
            insertions: 0
        }
    );
    assert_eq!(
        metrics::word_errors(&words(""), &words("а б")),
        WordErrors {
            substitutions: 0,
            deletions: 0,
            insertions: 2
        }
    );
    assert_eq!(
        metrics::word_errors(&words("а б"), &words("")),
        WordErrors {
            substitutions: 0,
            deletions: 2,
            insertions: 0
        }
    );
    assert_eq!(
        metrics::word_errors(&words("например"), &words("на пример")).total(),
        2
    );
    assert_eq!(
        metrics::word_errors(&words("а б"), &words("а б")).total(),
        0
    );
}
