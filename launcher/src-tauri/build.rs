use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const BUILD_NUMBER_ENV: &str = "HANDY_PROCESSOR_BUILD_NUMBER";
const BUILD_NUMBER_FILE: &str = "version-build.txt";

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
    let build_path = project_root.join(BUILD_NUMBER_FILE);
    println!("cargo:rerun-if-changed={}", major_path.display());
    println!("cargo:rerun-if-changed={}", build_path.display());
    println!("cargo:rerun-if-env-changed={BUILD_NUMBER_ENV}");
    watch_git_head(project_root);

    let major = fs::read_to_string(&major_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", major_path.display()));
    let major = parse_number(major.trim(), "major version");
    let build = env::var(BUILD_NUMBER_ENV)
        .ok()
        .map(|value| parse_number(value.trim(), BUILD_NUMBER_ENV))
        .unwrap_or_else(|| match git_commit_count(project_root) {
            Ok(value) => value,
            Err(git_error) => archived_build_number(&build_path).unwrap_or_else(|file_error| {
                panic!(
                    "failed to determine the build number from Git ({git_error}) and {BUILD_NUMBER_FILE} ({file_error}). Set {BUILD_NUMBER_ENV} explicitly"
                )
            }),
        });

    format!("{major}.{build}")
}

fn parse_number(value: &str, name: &str) -> u64 {
    value
        .parse::<u64>()
        .unwrap_or_else(|_| panic!("{name} must be a non-negative integer, got {value:?}"))
}

fn git_commit_count(project_root: &Path) -> Result<u64, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project_root)
        .args(["rev-list", "--count", "HEAD"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let value = String::from_utf8(output.stdout)
                .map_err(|error| format!("git commit count is not UTF-8: {error}"))?;
            Ok(parse_number(value.trim(), "git commit count"))
        }
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(error.trim().to_owned())
        }
        Err(error) => Err(format!("failed to run Git: {error}")),
    }
}

fn archived_build_number(path: &Path) -> Result<u64, String> {
    let value = fs::read_to_string(path).map_err(|error| error.to_string())?;
    Ok(parse_number(value.trim(), BUILD_NUMBER_FILE))
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
