use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const BUILD_NUMBER_ENV: &str = "HANDY_PROCESSOR_BUILD_NUMBER";

fn main() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let version = display_version(&project_root);
    println!("cargo:rustc-env=HANDY_PROCESSOR_DISPLAY_VERSION={version}");

    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        tauri_build::build();
    }
}

fn display_version(project_root: &Path) -> String {
    let major_path = project_root.join("version-major.txt");
    println!("cargo:rerun-if-changed={}", major_path.display());
    println!("cargo:rerun-if-env-changed={BUILD_NUMBER_ENV}");
    watch_git_head(project_root);

    let major = fs::read_to_string(&major_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", major_path.display()));
    let major = parse_number(major.trim(), "major version");
    let build = env::var(BUILD_NUMBER_ENV)
        .ok()
        .map(|value| parse_number(value.trim(), BUILD_NUMBER_ENV))
        .unwrap_or_else(|| git_commit_count(project_root));

    format!("{major}.{build}")
}

fn parse_number(value: &str, name: &str) -> u64 {
    value
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{name} must be a non-negative integer, got {value:?}"))
}

fn git_commit_count(project_root: &Path) -> u64 {
    let output = Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(["rev-list", "--count", "HEAD"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let value = String::from_utf8(output.stdout)
                .unwrap_or_else(|error| panic!("git commit count is not UTF-8: {error}"));
            parse_number(value.trim(), "git commit count")
        }
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr);
            panic!(
                "failed to determine the build number from Git: {}. Set {BUILD_NUMBER_ENV} to build without Git",
                error.trim()
            );
        }
        Err(error) => panic!(
            "failed to run Git for the build number: {error}. Set {BUILD_NUMBER_ENV} to build without Git"
        ),
    }
}

fn watch_git_head(project_root: &Path) {
    let dot_git = project_root.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else if dot_git.is_file() {
        let pointer = fs::read_to_string(&dot_git).unwrap_or_default();
        let Some(relative) = pointer.trim().strip_prefix("gitdir:") else {
            return;
        };
        let path = PathBuf::from(relative.trim());
        if path.is_absolute() {
            path
        } else {
            project_root.join(path)
        }
    } else {
        return;
    };

    let head_path = git_dir.join("HEAD");
    println!("cargo:rerun-if-changed={}", head_path.display());
    let packed_refs = git_dir.join("packed-refs");
    if packed_refs.is_file() {
        println!("cargo:rerun-if-changed={}", packed_refs.display());
    }

    let head = fs::read_to_string(head_path).unwrap_or_default();
    if let Some(reference) = head.trim().strip_prefix("ref:") {
        println!(
            "cargo:rerun-if-changed={}",
            git_dir.join(reference.trim()).display()
        );
    }
}
