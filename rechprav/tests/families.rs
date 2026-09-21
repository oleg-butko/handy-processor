use rechprav::{
    corrector::Baseline,
    dictionary::Dictionary,
    terms::{Config, TermEngine, TermForm},
};
use std::collections::BTreeSet;

fn config() -> Config {
    toml::from_str(
        r#"
baseline_config='base.toml'
enabled=true
[policy]
mode='edit'
[[sets]]
id='user'
[[sets.terms]]
id='slot'
canonical='слот'
aliases=['слот','свуты']
known_words='always'
max_known_distance=0
boost=1.0
[[sets.terms.forms]]
canonical='слота'
aliases=['слота','сваты']
require_context=true
context_patterns=['без снятия {term}']
[[sets.terms.forms]]
canonical='слоте'
aliases=['слоте','свате']
require_context=true
context_patterns=['в {term}']
[[sets.terms]]
id='tab'
canonical='таб'
aliases=['таб']
known_words='always'
max_known_distance=0
boost=1.0
[[sets.terms.forms]]
canonical='табы'
aliases=['табы','этабы']
"#,
    )
    .unwrap()
}

fn engine(c: Config) -> TermEngine {
    TermEngine::new(
        c,
        Some(Baseline {
            dictionary: Box::new(
                Dictionary::from_text("сваты\nсвате\nв\nбез\nснятия", false).unwrap(),
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
fn families_protect_base_and_forms_and_apply_form_contexts() {
    let e = engine(config());
    for (source, gold) in [
        ("таб и табы", "таб и табы"),
        ("слот слота слоте", "слот слота слоте"),
        ("этабы", "табы"),
        ("свуты", "слот"),
        ("без снятия сваты", "без снятия слота"),
        ("ошибка в свате", "ошибка в слоте"),
        ("сваты приехали", "сваты приехали"),
        ("думаю о свате", "думаю о свате"),
        ("`этабы`", "`этабы`"),
    ] {
        assert_eq!(e.analyze(source).corrected, gold, "{source}");
    }
    let a = e.analyze("без снятия сваты");
    let candidate = &a
        .decisions
        .iter()
        .find(|d| d.selected.is_some())
        .unwrap()
        .candidates[0];
    assert_eq!(candidate.term, "slot");
    assert_eq!(candidate.family_canonical.as_deref(), Some("слот"));
    assert_eq!(candidate.canonical, "слота");
}

#[test]
fn one_weight_and_switch_control_all_forms_without_bypassing_context() {
    let mut c = config();
    let old = engine(c.clone());
    c.sets[0].terms[0].boost += 2.0;
    let raised = engine(c.clone());
    for source in ["свуты", "без снятия сваты", "ошибка в свате"] {
        let score = |e: &TermEngine| {
            e.analyze(source)
                .decisions
                .into_iter()
                .find(|d| d.selected.is_some())
                .unwrap()
                .candidates[0]
                .score
        };
        assert_eq!(score(&raised) - score(&old), 2.0);
    }
    assert!(raised.analyze("сваты приехали").edits.is_empty());
    c.sets[0].terms[0].boost = -4.0;
    for source in ["свуты", "без снятия сваты", "ошибка в свате"] {
        assert!(engine(c.clone()).analyze(source).edits.is_empty());
    }
    c.sets[0].terms[0].boost = 1.0;
    c.sets[0].terms[0].enabled = false;
    assert_eq!(
        engine(c).analyze("этабы и без снятия сваты").corrected,
        "табы и без снятия сваты"
    );
}

#[test]
fn form_removal_changes_protection_and_ambiguous_forms_keep_regardless_of_order() {
    let mut c = config();
    c.sets[0].terms[0].forms.remove(0);
    assert_eq!(engine(c.clone()).analyze("слота").corrected, "слот"); // OOV here, form protection explicitly removed.
    let mut c = config();
    c.sets[0].terms[0].forms[0].require_context = Some(false);
    c.sets[0].terms[0].forms[0].context_patterns = Some(vec![]);
    c.sets[0].terms[0].forms[0].aliases.push("свуты".into());
    assert!(engine(c.clone()).analyze("свуты").edits.is_empty());
    c.sets[0].terms[0].forms.reverse();
    assert!(engine(c).analyze("свуты").edits.is_empty());
}

#[test]
fn families_equal_handwritten_flat_entries_and_share_budget_limits() {
    let grouped = config();
    let flat: Config = toml::from_str(
        r#"
baseline_config='base.toml'
enabled=true
[policy]
mode='edit'
[[sets]]
id='user'
[[sets.terms]]
id='slot'
canonical='слот'
aliases=['слот','свуты']
known_words='always'
max_known_distance=0
boost=1.0
[[sets.terms]]
id='genitive'
canonical='слота'
aliases=['слота','сваты']
known_words='always'
max_known_distance=0
boost=1.0
require_context=true
context_patterns=['без снятия {term}']
[[sets.terms]]
id='locative'
canonical='слоте'
aliases=['слоте','свате']
known_words='always'
max_known_distance=0
boost=1.0
require_context=true
context_patterns=['в {term}']
[[sets.terms]]
id='tab'
canonical='таб'
aliases=['таб']
known_words='always'
max_known_distance=0
boost=1.0
[[sets.terms]]
id='plural'
canonical='табы'
aliases=['табы','этабы']
known_words='always'
max_known_distance=0
boost=1.0
"#,
    )
    .unwrap();
    for source in [
        "таб и этабы",
        "без снятия сваты и ошибка в свате",
        "сваты приехали",
        "слот слота слоте",
    ] {
        let a = engine(grouped.clone()).analyze(source);
        let b = engine(flat.clone()).analyze(source);
        assert_eq!(a.corrected, b.corrected);
        assert_eq!(
            serde_json::to_value(a.edits).unwrap(),
            serde_json::to_value(b.edits).unwrap()
        );
        assert_eq!(a.comparisons, b.comparisons);
    }
    let mut c = grouped;
    c.policy.max_comparisons = 1;
    let a = engine(c).analyze("свуты этабы");
    assert!(!a.complete);
    assert!(a.edits.is_empty());
}

#[test]
fn family_validation_catches_duplicates_inheritance_and_expanded_limits() {
    let form = |canonical: &str| {
        serde_json::from_value::<TermForm>(serde_json::json!({"canonical":canonical})).unwrap()
    };
    let mut c = config();
    c.sets[0].terms[0].forms.push(form("СЛОТА"));
    assert!(c.validate().is_err());
    let mut c = config();
    c.sets[0].terms[0].canonical = "ёж".into();
    c.sets[0].terms[0].forms.push(form("еж"));
    assert!(c.validate().is_err());
    let mut c = config();
    c.sets[0].terms[0].forms = (1..=17).map(|n| form(&"а".repeat(n))).collect();
    assert!(c.validate().is_err());
    c.sets[0].terms[0].forms.pop();
    assert!(c.validate().is_ok());
    c.sets[0].terms[0].aliases = vec!["слот".into(); 64];
    for f in &mut c.sets[0].terms[0].forms {
        f.aliases = vec!["слота".into(); 64];
    }
    let original = c.sets[0].terms[0].clone();
    c.sets[0].terms = (0..4)
        .map(|i| {
            let mut t = original.clone();
            t.id = format!("term{i}");
            t
        })
        .collect();
    assert!(c.validate().is_err());
    let mut c = config();
    c.sets[0].terms[0].pronunciation = Some(rechprav::pronunciation::Pronunciation::RuLetters);
    assert!(c.validate().is_err());
    let mut c = config();
    c.sets[0].terms[0].forms[0].context_patterns = Some(vec![]);
    assert!(c.validate().is_err());
    assert!(
        serde_json::from_value::<TermForm>(serde_json::json!({"canonical":"слота","boost":2}))
            .is_err()
    );
    let mut c = config();
    c.sets[0].terms[0].forms = vec![form("слота")];
    assert!(c.validate().is_ok());
    assert_eq!(engine(c).analyze("слота").corrected, "слота");
}

#[test]
fn load_exposes_resolved_inheritance_and_reads_changed_family_config() {
    use std::fs;
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("base.toml"), "dictionary='words.txt'").unwrap();
    fs::write(temp.path().join("words.txt"), "сваты\nсвате").unwrap();
    let path = temp.path().join("terms.toml");
    let mut c = config();
    fs::write(&path, toml::to_string(&c).unwrap()).unwrap();
    let (old, meta) = TermEngine::load(&path, false).unwrap();
    assert_eq!(meta["active_terms"], 5);
    let family = &meta["term_families"][0];
    assert_eq!(family["canonical"], "слот");
    assert_eq!(family["resolved_forms"][1]["boost"], 1.0);
    assert_eq!(
        family["resolved_forms"][1]["context_patterns"][0],
        "без снятия {term}"
    );
    c.sets[0].terms[0].enabled = false;
    fs::write(&path, toml::to_string(&c).unwrap()).unwrap();
    let (new, updated) = TermEngine::load(&path, false).unwrap();
    assert_ne!(meta["config_sha256"], updated["config_sha256"]);
    assert_eq!(updated["term_families"].as_array().unwrap().len(), 1);
    assert_eq!(old.analyze("свуты").corrected, "слот");
    assert_eq!(new.analyze("свуты").corrected, "свуты");
}
