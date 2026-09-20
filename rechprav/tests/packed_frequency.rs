use rechprav::{
    frequency::{FrequencyPolicy, FrequencyScorer},
    packed_frequency::PackedFrequency,
};
use sha2::{Digest, Sha256};

fn reference(text: &str, strict: bool) -> FrequencyScorer {
    FrequencyScorer::from_tsv(text, strict, FrequencyPolicy::default()).unwrap()
}

#[test]
fn values_scores_missing_and_yo_are_exact_after_roundtrip() {
    let long = "ё".repeat(128);
    let text = format!(
        "ёлка\t2.5\nелка\t4.1\nёж\t0.00000000000000001\nноль\t0\nминус\t-0\nкрай\t1000000000\n{long}\t7"
    );
    for strict in [false, true] {
        let original = reference(&text, strict);
        let data = original.pack([17; 32]).unwrap();
        assert_eq!(data, original.pack([17; 32]).unwrap());
        let packed = PackedFrequency::from_bytes(data.clone(), strict).unwrap();
        assert_eq!(packed.source_sha256(), "11".repeat(32));
        assert_eq!(packed.len(), original.len());
        assert!(!packed.is_empty());
        for policy in [
            FrequencyPolicy::default(),
            FrequencyPolicy {
                weight: 0.0,
                ..FrequencyPolicy::default()
            },
            FrequencyPolicy {
                weight: 2.7,
                log_cap: 2.0,
                edit_penalty: 3.2,
                keep_bias: 0.2,
                ..FrequencyPolicy::default()
            },
        ] {
            let a = FrequencyScorer::from_tsv(&text, strict, policy.clone()).unwrap();
            let b = FrequencyScorer::from_packed(data.clone(), strict, policy).unwrap();
            for word in [
                "ёлка",
                "елка",
                "ёж",
                "еж",
                "ноль",
                "минус",
                "край",
                "нет",
                "",
                &long,
            ] {
                let key = rechprav::text::key(word, strict);
                assert_eq!(a.get(&key).map(f64::to_bits), b.get(&key).map(f64::to_bits));
                assert_eq!(a.keep_score(&key).to_bits(), b.keep_score(&key).to_bits());
                for distance in 0..=2 {
                    assert_eq!(
                        a.score(&key, distance).to_bits(),
                        b.score(&key, distance).to_bits()
                    );
                }
            }
        }
        assert!(PackedFrequency::from_bytes(data, !strict).is_err());
    }
}

#[test]
fn binary_search_matches_reference_for_all_small_keys_and_gaps() {
    let words: Vec<_> = ['а', 'б', 'ё', 'я']
        .into_iter()
        .flat_map(|a| {
            ['а', 'б', 'ё', 'я']
                .into_iter()
                .flat_map(move |b| ['а', 'б', 'ё', 'я'].map(|c| format!("{a}{b}{c}")))
        })
        .collect();
    for strict in [false, true] {
        let text = words
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 3 != 0)
            .map(|(i, word)| format!("{word}\t{}", i as f64 / 7.0))
            .collect::<Vec<_>>()
            .join("\n");
        let original = reference(&text, strict);
        let packed = PackedFrequency::from_bytes(original.pack([0; 32]).unwrap(), strict).unwrap();
        for word in words
            .iter()
            .map(String::as_str)
            .chain(["", "а", "я", "яяяя", "ASCII", "🙂"])
        {
            let key = rechprav::text::key(word, strict);
            assert_eq!(
                packed.get(&key).map(f64::to_bits),
                original.get(&key).map(f64::to_bits)
            );
        }
        assert_eq!(packed.entries().count(), original.len());
    }
}

fn rehash(data: &mut [u8]) {
    let mut hash = Sha256::new();
    hash.update(&data[..64]);
    hash.update(&data[96..]);
    data[64..96].copy_from_slice(&hash.finalize());
}

#[test]
fn rejects_truncation_corruption_and_malformed_rows_even_with_valid_checksum() {
    let data = reference("а\t1\nб\t2", false).pack([0; 32]).unwrap();
    for size in 0..data.len() {
        assert!(PackedFrequency::from_bytes(data[..size].to_vec(), false).is_err());
    }
    let mut trailing = data.clone();
    trailing.push(0);
    assert!(PackedFrequency::from_bytes(trailing, false).is_err());
    for index in 0..data.len() {
        let mut corrupt = data.clone();
        corrupt[index] ^= 0x80;
        assert!(PackedFrequency::from_bytes(corrupt, false).is_err());
    }
    for (offset, bytes) in [
        (16, u64::MAX.to_le_bytes().to_vec()),
        (24, u64::MAX.to_le_bytes().to_vec()),
        (96, 1u32.to_le_bytes().to_vec()),
        (100, 0u16.to_le_bytes().to_vec()),
        (100, 258u16.to_le_bytes().to_vec()),
        (102, vec![1]),
        (104, f64::NAN.to_le_bytes().to_vec()),
        (104, f64::INFINITY.to_le_bytes().to_vec()),
        (104, (-1.0f64).to_le_bytes().to_vec()),
        (104, (1e9 + 1.0f64).to_le_bytes().to_vec()),
        (112, 0u32.to_le_bytes().to_vec()),
        (128, "я".as_bytes().to_vec()), // keys out of order
        (130, "а".as_bytes().to_vec()), // duplicate
        (128, "ё".as_bytes().to_vec()), // unnormalized key
        (128, vec![0xff, 0xff]),
    ] {
        let mut corrupt = data.clone();
        corrupt[offset..offset + bytes.len()].copy_from_slice(&bytes);
        rehash(&mut corrupt);
        assert!(
            PackedFrequency::from_bytes(corrupt, false).is_err(),
            "offset {offset}"
        );
    }
    for index in 96..data.len() {
        let mut corrupt = data.clone();
        corrupt[index] ^= 0x80;
        rehash(&mut corrupt);
        assert!(std::panic::catch_unwind(|| PackedFrequency::from_bytes(corrupt, false)).is_ok());
    }
}

#[test]
fn cli_build_preserves_decisions_refuses_overwrite_and_rejects_bad_resource() {
    use std::{fs, process::Command};
    let binary = env!("CARGO_BIN_EXE_rechprav");
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("frequency.tsv");
    let output = dir.path().join("frequency.rpf");
    fs::write(&input, "драйвер\t10000\nёлка\t17").unwrap();
    let build = || {
        Command::new(binary)
            .arg("build-frequency")
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap()
    };
    let result = build();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let stats: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(stats["verified_bitwise_values"], 2);
    let bytes = fs::read(&output).unwrap();
    assert!(!build().status.success());
    assert_eq!(fs::read(&output).unwrap(), bytes);
    fs::write(dir.path().join("dictionary.txt"), "драйвер\nёлка").unwrap();
    let config = dir.path().join("config.toml");
    let mut analyses = Vec::new();
    for resource in ["frequency.tsv", "frequency.rpf"] {
        fs::write(
            &config,
            format!("dictionary='dictionary.txt'\n[frequency]\npath='{resource}'\n"),
        )
        .unwrap();
        let (engine, _) = rechprav::corrector::Baseline::load(&config).unwrap();
        let analysis = rechprav::corrector::analyze("драйвир и ёлка", &engine);
        analyses.push(serde_json::json!({"corrected":analysis.corrected, "decisions":analysis.decisions, "edits":analysis.edits}));
    }
    assert_eq!(analyses[0], analyses[1]);
    assert_eq!(analyses[1]["corrected"], "драйвер и ёлка");
    fs::write(&output, b"corrupt").unwrap();
    fs::write(dir.path().join("input.txt"), "драйвир").unwrap();
    let review = dir.path().join("review");
    let result = Command::new(binary)
        .arg("review")
        .arg(dir.path().join("input.txt"))
        .arg("--config")
        .arg(&config)
        .arg("--output-dir")
        .arg(&review)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!review.exists());
    fs::write(&input, "драйвер\tNaN").unwrap();
    let invalid_output = dir.path().join("invalid.rpf");
    assert!(
        !Command::new(binary)
            .arg("build-frequency")
            .arg(&input)
            .arg("--output")
            .arg(&invalid_output)
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(!invalid_output.exists());
}
