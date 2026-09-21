use rechprav::{
    corrector::Baseline,
    dictionary::Dictionary,
    terms::{Config, KnownWords, TermEngine, TermKind},
};
use std::collections::BTreeSet;

fn config(canonical: &str) -> Config {
    let mut c: Config = toml::from_str(
        r#"
baseline_config='unused.toml'
enabled=true
[policy]
mode='edit'
[[sets]]
id='phrases'
[[sets.terms]]
id='example'
kind='phrase'
canonical='долго ждать'
aliases=['долго ждать']
known_words='always'
max_known_distance=1
boost=1.0
"#,
    )
    .unwrap();
    c.sets[0].terms[0].canonical = canonical.into();
    c.sets[0].terms[0].aliases = vec![canonical.into()];
    c
}

fn engine(c: Config) -> TermEngine {
    TermEngine::new(
        c,
        Some(Baseline {
            dictionary: Box::new(
                Dictionary::from_text("он\nколесо\nдолго\nждать\nсваты\nслота", false).unwrap(),
            ),
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
fn phrase_output_splits_input_and_trims_unchanged_edge_words_with_original_bytes() {
    let a = engine(config("долго ждать")).analyze("долгождать!");
    assert_eq!(a.corrected, "долго ждать!");
    assert_eq!((a.edits[0].token_start, a.edits[0].token_end), (0, 1));
    let a = engine(config("он проколол колесо")).analyze("он  проковол\tКолесо!");
    assert_eq!(a.corrected, "он  проколол\tКолесо!");
    assert_eq!(a.edits.len(), 1);
    let e = &a.edits[0];
    assert_eq!(
        (e.byte_start, e.byte_end),
        ("он  ".len(), "он  проковол".len())
    );
    assert_eq!((e.token_start, e.token_end), (1, 2));
    assert_eq!(e.replacement, "проколол");
    let d = a.decisions.iter().find(|d| d.selected.is_some()).unwrap();
    assert_eq!(d.original, "он  проковол\tКолесо");
    assert_eq!(
        d.candidates[0]
            .replacement_edit
            .as_ref()
            .unwrap()
            .replacement,
        "проколол"
    );
}

#[test]
fn correct_phrases_and_unsupported_pure_insertions_deletions_are_kept() {
    let mut c = config("долго ждать");
    c.sets[0].terms[0]
        .aliases
        .extend(["долго".into(), "долго долго ждать".into()]);
    let e = engine(c);
    for source in [
        "долго ждать",
        "Долго\tждать",
        "долго",
        "долго долго ждать",
        "`долгождать`",
        "долгождать.txt",
    ] {
        assert_eq!(e.analyze(source).corrected, source, "{source}");
        assert!(e.analyze(source).edits.is_empty());
    }
    let e = engine(config("проколол колесо"));
    for source in ["проковол\nколесо", "проковол, колесо", "ПРОКОВОЛ колесо"]
    {
        assert_eq!(e.analyze(source).corrected, source);
    }
}

#[test]
fn phrase_budget_and_conflicts_do_not_authorize_partial_edits() {
    let mut c = config("он проколол");
    c.policy.known_penalty = 0.0;
    c.sets[0].terms[0].aliases = vec!["он проковол".into()];
    let mut other = c.sets[0].terms[0].clone();
    other.kind = TermKind::Word;
    other.id = "other".into();
    other.canonical = "термин".into();
    other.aliases = vec!["проковол".into()];
    c.sets[0].terms.push(other);
    assert!(engine(c.clone()).analyze("он проковол").edits.is_empty());
    c.sets[0].terms.reverse();
    assert!(engine(c).analyze("он проковол").edits.is_empty());
    let mut c = config("долго ждать");
    c.policy.max_comparisons = 1;
    let a = engine(c).analyze("долгождать долгождать");
    assert!(!a.complete);
    assert!(a.edits.is_empty());
    assert!(a.decisions.iter().all(|d| d.selected.is_none()));
}

#[test]
fn phrase_weights_switches_and_known_caps_remain_independent_controls() {
    let mut c = config("долго ждать");
    c.sets[0].terms[0].boost = -2.0;
    assert!(engine(c.clone()).analyze("долгождать").edits.is_empty());
    c.sets[0].terms[0].boost = 1.0;
    assert_eq!(
        engine(c.clone()).analyze("долгождать").corrected,
        "долго ждать"
    );
    c.sets[0].enabled = false;
    assert!(
        TermEngine::new(c, None)
            .unwrap()
            .analyze("долгождать")
            .edits
            .is_empty()
    );
    let mut c = config("долго ждать");
    let t = &mut c.sets[0].terms[0];
    t.kind = TermKind::Word;
    t.canonical = "слота".into();
    t.aliases = vec!["слота".into()];
    t.max_known_distance = Some(0);
    t.boost = 10.0;
    c.policy.max_distance = 2;
    assert!(engine(c.clone()).analyze("сваты").edits.is_empty());
    c.sets[0].terms[0].aliases.push("сваты".into());
    // An explicit known-word alias needs a context rule if its ordinary use must survive.
    assert_eq!(engine(c.clone()).analyze("сваты").corrected, "слота");
    c.sets[0].terms[0].require_context = true;
    c.sets[0].terms[0].context_patterns = vec!["без снятия {term}".into()];
    assert_eq!(
        engine(c.clone()).analyze("без снятия сваты").corrected,
        "без снятия слота"
    );
    assert_eq!(
        engine(c).analyze("сваты приехали").corrected,
        "сваты приехали"
    );
}

#[test]
fn phrase_validation_requires_explicit_kind_bounded_words_and_no_generator() {
    let c = config("долго ждать");
    assert!(c.validate().is_ok());
    for text in [
        "долго",
        "долго  ждать",
        "долго\nждать",
        "долго ждать ещё немного",
        "долго, ждать",
        "долго C++",
    ] {
        let c = config(text);
        assert!(c.validate().is_err(), "{text}");
    }
    let mut c = config("долго ждать");
    c.sets[0].terms[0].kind = TermKind::Word;
    assert!(c.validate().is_err());
    c.sets[0].terms[0].kind = TermKind::Phrase;
    c.sets[0].terms[0].pronunciation = Some(rechprav::pronunciation::Pronunciation::RuLetters);
    assert!(c.validate().is_err());
}

#[test]
fn preserve_known_words_allows_oov_edit_but_not_a_known_word_inside_the_actual_edit() {
    let mut c = config("проколол колесо");
    c.sets[0].terms[0].known_words = KnownWords::Preserve;
    assert_eq!(
        engine(c).analyze("проковол колесо").corrected,
        "проколол колесо"
    );
    let mut c = config("долги ждать");
    assert_eq!(
        engine(c.clone()).analyze("долго ждать").corrected,
        "долги ждать"
    );
    c.sets[0].terms[0].known_words = KnownWords::Preserve;
    c.sets[0].terms[0].boost = 10.0;
    let a = engine(c).analyze("долго ждать");
    assert!(a.edits.is_empty());
    assert!(
        a.decisions
            .iter()
            .any(|d| d.candidates[0].gate == "known_changed_word_blocked")
    );
}

#[test]
fn phrase_metrics_score_minimal_spans_and_one_to_many_without_harming_preserved_words() {
    let mut c = config("проколол колесо");
    let mut split = c.sets[0].terms[0].clone();
    split.id = "split".into();
    split.canonical = "долго ждать".into();
    split.aliases = vec![split.canonical.clone()];
    c.sets[0].terms.push(split);
    let records: Vec<rechprav::corpus::Record> = [("проковол колесо", "проколол колесо", "проковол", "проколол"),
        ("долгождать", "долго ждать", "долгождать", "долго ждать")].into_iter().enumerate().map(|(i,(source,gold,old,new))| {
        serde_json::from_value(serde_json::json!({"schema_version":1,"id":format!("p{i}"),"group_id":format!("p{i}"),"split":"dev",
            "source":source,"gold":gold,"edits":[{"byte_start":0,"byte_end":old.len(),"token_start":0,"token_end":1,"replacement":new}],
            "tags":[],"provenance":{"kind":"synthetic","origin":"unit test","asr_model":null},"annotation_status":"reviewed"})).unwrap()
    }).collect();
    let run = rechprav::term_metrics::benchmark(&records, &engine(c)).unwrap();
    assert_eq!(
        run.report["span_counts"],
        serde_json::json!({"tp":2,"fp":0,"fn_":0,"correct_input":1,"harmed_correct_input":0})
    );
    assert_eq!(run.report["wer_after"], 0.0);
    assert_eq!(run.report["exact_records"], 2);
}
