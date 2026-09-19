use std::{fs, process::Command};
use tempfile::tempdir;

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rechprav"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn baseline_cli_corrects_with_config_and_exports_reproducible_resources() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("input.txt");
    let out = temp.path().join("out");
    let source = "«драйвир»,\tдрайве\r\nфайл драйвир.txt\n";
    fs::write(&input, source).unwrap();
    let config = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("configs/baseline.toml");
    let result = cli(&[
        "correct",
        input.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read_to_string(out.join("corrected.txt")).unwrap(),
        "«драйвер»,\tдрайвер\r\nфайл драйвир.txt\n"
    );
    assert_eq!(fs::read_to_string(input).unwrap(), source);
    let info: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(info["engine"], "levenshtein_unique");
    assert!(info["resources"]["resource_sha256"]["dictionary"].is_string());
}

#[test]
fn build_dict_is_sorted_reusable_and_does_not_overwrite() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("words.txt");
    let output = temp.path().join("dictionary.json");
    fs::write(&input, "ёж\nдрайвер\nёж\n").unwrap();
    let args = [
        "build-dict",
        input.to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
    ];
    assert!(cli(&args).status.success());
    assert!(!cli(&args).status.success());
    let config = temp.path().join("config.toml");
    fs::write(&config, "dictionary='dictionary.json'\n").unwrap();
    let result = cli(&["inspect", "драйвир", "--config", config.to_str().unwrap()]);
    assert!(result.status.success());
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["corrected"], "драйвер");
}

#[test]
fn invalid_baseline_config_does_not_create_output_directory() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("input");
    let out = temp.path().join("out");
    let config = temp.path().join("config.toml");
    fs::write(&input, "драйвир").unwrap();
    fs::write(&config, "dictionary='missing.txt'\n").unwrap();
    let result = cli(&[
        "correct",
        input.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(!out.exists());
}

#[test]
fn cli_review_preserves_bytes_and_refuses_existing_output() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("input.txt");
    let out = temp.path().join("review");
    let original = "\u{feff}Драйвир\tCUDA\r\n\r\nконец\r";
    fs::write(&input, original).unwrap();
    let args = [
        "review",
        input.to_str().unwrap(),
        "--output-dir",
        out.to_str().unwrap(),
    ];
    let result = cli(&args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(out.join("corrected.txt")).unwrap(),
        original.as_bytes()
    );
    assert_eq!(fs::read(&input).unwrap(), original.as_bytes());
    assert!(!cli(&args).status.success());
    assert_eq!(
        fs::read(out.join("corrected.txt")).unwrap(),
        original.as_bytes()
    );
    assert_eq!(
        fs::read_to_string(out.join("debug.jsonl"))
            .unwrap()
            .lines()
            .count(),
        3
    );
}

#[test]
fn prepare_refuses_overwrite_and_benchmark_refuses_pending() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("source.txt");
    let dataset = temp.path().join("annotations.jsonl");
    fs::write(&input, "драйвир\nнормальный текст\n").unwrap();
    let args = [
        "prepare",
        input.to_str().unwrap(),
        "--output",
        dataset.to_str().unwrap(),
        "--group",
        "pilot",
        "--kind",
        "synthetic",
    ];
    assert!(cli(&args).status.success());
    assert!(!cli(&args).status.success());
    let same = [
        "prepare",
        input.to_str().unwrap(),
        "--output",
        input.to_str().unwrap(),
        "--group",
        "pilot",
        "--kind",
        "synthetic",
    ];
    assert!(!cli(&same).status.success());
    assert_eq!(
        fs::read_to_string(&input).unwrap(),
        "драйвир\nнормальный текст\n"
    );
    assert!(
        cli(&["validate", dataset.to_str().unwrap()])
            .status
            .success()
    );
    let out = temp.path().join("benchmark");
    assert!(
        !cli(&[
            "benchmark",
            dataset.to_str().unwrap(),
            "--output-dir",
            out.to_str().unwrap()
        ])
        .status
        .success()
    );
    assert!(!out.exists());
}

#[test]
fn invalid_utf8_and_width_do_not_create_outputs() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("invalid");
    let out = temp.path().join("out");
    fs::write(&input, [0xff]).unwrap();
    assert!(
        !cli(&[
            "review",
            input.to_str().unwrap(),
            "--output-dir",
            out.to_str().unwrap()
        ])
        .status
        .success()
    );
    assert!(!out.exists());
    fs::write(&input, "ok").unwrap();
    assert!(
        !cli(&[
            "review",
            input.to_str().unwrap(),
            "--output-dir",
            out.to_str().unwrap(),
            "--width",
            "131"
        ])
        .status
        .success()
    );
    assert!(!out.exists());
}

#[cfg(unix)]
#[test]
fn symlinks_and_hardlinks_cannot_overwrite_sources() {
    let temp = tempdir().unwrap();
    let input = temp.path().join("input");
    let hard = temp.path().join("hard");
    let sym = temp.path().join("sym");
    fs::write(&input, "данные").unwrap();
    fs::hard_link(&input, &hard).unwrap();
    std::os::unix::fs::symlink(&input, &sym).unwrap();
    for output in [&hard, &sym] {
        assert!(
            !cli(&[
                "prepare",
                input.to_str().unwrap(),
                "--output",
                output.to_str().unwrap(),
                "--group",
                "test",
                "--kind",
                "synthetic"
            ])
            .status
            .success()
        );
    }
    let directory = temp.path().join("directory");
    fs::create_dir(&directory).unwrap();
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&directory, &alias).unwrap();
    assert!(
        !cli(&[
            "review",
            input.to_str().unwrap(),
            "--output-dir",
            alias.to_str().unwrap()
        ])
        .status
        .success()
    );
    assert_eq!(fs::read_to_string(&input).unwrap(), "данные");
    assert_eq!(fs::read_dir(directory).unwrap().count(), 0);
}
