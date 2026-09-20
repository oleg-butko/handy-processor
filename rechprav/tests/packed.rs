use rechprav::{
    dictionary::{CandidateGenerator, Dictionary},
    packed::PackedTrie,
};
use sha2::{Digest, Sha256};

fn packed(dictionary: &Dictionary, strict: bool) -> PackedTrie {
    PackedTrie::from_bytes(dictionary.pack([7; 32]).unwrap(), strict).unwrap()
}

fn strings(max: usize) -> Vec<String> {
    let mut all = vec![String::new()];
    let mut layer = all.clone();
    for _ in 0..max {
        layer = layer
            .iter()
            .flat_map(|w| ['а', 'б', 'ё'].map(|c| format!("{w}{c}")))
            .collect();
        all.extend(layer.clone());
    }
    all
}

fn distance(a: &str, b: &str) -> usize {
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

#[test]
fn packed_matches_full_matrix_and_reference_traversal_exhaustively() {
    let words: Vec<_> = strings(4).into_iter().filter(|s| !s.is_empty()).collect();
    let reference = Dictionary::from_words(words.clone(), true).unwrap();
    let compact = packed(&reference, true);
    assert_eq!(
        (compact.len(), compact.node_count()),
        (reference.len(), reference.node_count())
    );
    for query in strings(4)
        .into_iter()
        .chain(["а🙂".into(), "й".into(), "ASCII".into()])
    {
        assert_eq!(compact.contains(&query), reference.contains(&query));
        for d in 0..=2 {
            let found = compact.lookup(&query, d, 1000, 100000);
            let mut expected: Vec<_> = words
                .iter()
                .filter_map(|w| {
                    let actual = distance(&query, w);
                    (actual <= d).then_some((actual, w.as_str()))
                })
                .collect();
            expected.sort();
            assert_eq!(
                found
                    .candidates
                    .iter()
                    .map(|c| (c.distance, c.word.as_str()))
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                serde_json::to_value(&found).unwrap(),
                serde_json::to_value(reference.lookup(&query, d, 1000, 100000)).unwrap()
            );
        }
    }
}

#[test]
fn incomplete_search_returns_identical_prefix_and_diagnostics() {
    let reference = Dictionary::from_text("маска\nмасло\nмаски\nмясо\nёлка\nелки", false).unwrap();
    let compact = packed(&reference, false);
    for query in ["маско", "елко", "", "мяско"] {
        for d in 0..=2 {
            for candidates in [0, 1, 2, 20] {
                for nodes in [0, 1, 2, 10, 100000] {
                    assert_eq!(
                        serde_json::to_value(compact.lookup(query, d, candidates, nodes)).unwrap(),
                        serde_json::to_value(reference.lookup(query, d, candidates, nodes))
                            .unwrap()
                    );
                }
            }
        }
    }
}

#[test]
fn canonical_yo_spelling_long_forms_and_normalization_survive_roundtrip() {
    let long = "ё".repeat(128);
    for strict in [false, true] {
        let reference =
            Dictionary::from_text(&format!("ёлка\nелка\nёж\nежик\n{long}"), strict).unwrap();
        let compact = packed(&reference, strict);
        for word in reference.export().words {
            let query = rechprav::text::key(&word, strict);
            assert!(compact.contains(&query));
            let search = compact.lookup(&query, 0, 20, 100000);
            assert_eq!(search.candidates[0].word, word);
        }
        assert!(PackedTrie::from_bytes(reference.pack([0; 32]).unwrap(), !strict).is_err());
    }
}

fn rehash(data: &mut [u8]) {
    let mut hash = Sha256::new();
    hash.update(&data[..72]);
    hash.update(&data[104..]);
    data[72..104].copy_from_slice(&hash.finalize());
}

#[test]
fn corrupt_truncated_and_trailing_files_are_rejected_without_panics() {
    let data = Dictionary::from_text("аб\nав\nба\nёж", false)
        .unwrap()
        .pack([7; 32])
        .unwrap();
    for size in 0..data.len() {
        assert!(PackedTrie::from_bytes(data[..size].to_vec(), false).is_err());
    }
    let mut trailing = data.clone();
    trailing.push(0);
    assert!(PackedTrie::from_bytes(trailing, false).is_err());
    for index in 0..data.len() {
        let mut corrupt = data.clone();
        corrupt[index] ^= 0x80;
        assert!(PackedTrie::from_bytes(corrupt, false).is_err());
    }
    // A recomputed checksum must not bypass structural validation.
    for (offset, value) in [(104, u32::MAX), (108, 2), (116, 1)] {
        let mut corrupt = data.clone();
        corrupt[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        rehash(&mut corrupt);
        assert!(PackedTrie::from_bytes(corrupt, false).is_err());
    }
    for index in 104..data.len() {
        let mut corrupt = data.clone();
        corrupt[index] ^= 0x80;
        rehash(&mut corrupt);
        assert!(std::panic::catch_unwind(|| PackedTrie::from_bytes(corrupt, false)).is_ok());
    }
}

#[test]
fn binary_format_is_deterministic_and_records_source_identity() {
    let a = Dictionary::from_text("ёж\nелка\nёлка\nмаска", false).unwrap();
    let b = Dictionary::from_text("маска\nёлка\nёж\nелка\nмаска", false).unwrap();
    assert_eq!(a.pack([7; 32]).unwrap(), b.pack([7; 32]).unwrap());
    assert_eq!(packed(&a, false).source_sha256(), "07".repeat(32));
}

#[test]
fn cli_build_and_correction_preserve_output_and_reject_corrupt_resource() {
    use std::{fs, process::Command};
    let binary = env!("CARGO_BIN_EXE_rechprav");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("words.txt");
    let output = dir.path().join("words.rpt");
    fs::write(&input, "драйвер\nёлка").unwrap();
    let run = || {
        Command::new(binary)
            .arg("build-index")
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let bytes = fs::read(&output).unwrap();
    assert!(!run().status.success());
    assert_eq!(fs::read(&output).unwrap(), bytes);
    let config = dir.path().join("config.toml");
    fs::write(&config, "dictionary='words.rpt'").unwrap();
    let (engine, meta) = rechprav::corrector::Baseline::load(&config).unwrap();
    assert_eq!(
        rechprav::corrector::analyze("драйвир", &engine).corrected,
        "драйвер"
    );
    assert_eq!(
        meta["dictionary_storage"]["source_sha256"],
        rechprav::hashing::sha256_hex(&fs::read(&input).unwrap())
    );
    fs::write(&output, b"invalid").unwrap();
    fs::write(dir.path().join("source.txt"), "драйвир").unwrap();
    let review = dir.path().join("review");
    let result = Command::new(binary)
        .arg("review")
        .arg(dir.path().join("source.txt"))
        .arg("--config")
        .arg(config)
        .arg("--output-dir")
        .arg(&review)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!review.exists());
}
