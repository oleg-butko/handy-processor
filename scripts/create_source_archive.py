#!/usr/bin/env python3
"""Create a ZIP containing only the Handy Processor source allowlist."""

from __future__ import annotations

import os
import subprocess
from datetime import datetime
from pathlib import Path, PurePosixPath
from zipfile import ZIP_DEFLATED, ZipFile


PROJECT_ROOT = Path(__file__).resolve().parent.parent
ARCHIVE_DIRECTORY = PROJECT_ROOT / "archive"
BUILD_NUMBER_ENV = "HANDY_PROCESSOR_BUILD_NUMBER"
BUILD_NUMBER_FILE = "version-build.txt"

SOURCE_DIRECTORIES = (
    "docs",
    "launcher/assets",
    "launcher/fallback",
    "launcher/src-tauri/capabilities",
    "launcher/src-tauri/gen",
    "launcher/src-tauri/icons",
    "launcher/src-tauri/src",
    "scripts",
    "src",
    "ui",
)

SOURCE_FILES = (
    "launcher/src-tauri/build.rs",
    "launcher/src-tauri/Cargo.lock",
    "launcher/src-tauri/Cargo.toml",
    "launcher/src-tauri/tauri.conf.json",
    "launcher/package-lock.json",
    "launcher/package.json",
    "launcher/README.md",
    ".gitignore",
    "AGENTS.md",
    "config.json",
    "package-lock.json",
    "package.json",
    "README.md",
    "requirements-generator.txt",
    "sea-config.node26.json",
    "tsconfig.json",
    "unpack-sources.config.json",
    "version-major.txt",
    "vite.config.ts",
)


def archive_name(path: Path, *, directory: bool = False) -> str:
    relative = path.relative_to(PROJECT_ROOT).as_posix()
    return f"{relative}/" if directory else relative


def collect_sources() -> list[tuple[Path, str]]:
    entries: list[tuple[Path, str]] = []

    for relative_directory in SOURCE_DIRECTORIES:
        directory = PROJECT_ROOT / relative_directory
        if not directory.is_dir():
            raise FileNotFoundError(f"Required source directory is missing: {directory}")
        if directory.is_symlink():
            raise RuntimeError(f"Source directory must not be a symlink: {directory}")

        entries.append((directory, archive_name(directory, directory=True)))
        for path in sorted(directory.rglob("*")):
            if path.is_symlink():
                raise RuntimeError(f"Source path must not be a symlink: {path}")
            if path.is_dir():
                entries.append((path, archive_name(path, directory=True)))
            elif path.is_file():
                entries.append((path, archive_name(path)))
            else:
                raise RuntimeError(f"Unsupported source path: {path}")

    for relative_file in SOURCE_FILES:
        path = PROJECT_ROOT / relative_file
        if not path.is_file():
            raise FileNotFoundError(f"Required source file is missing: {path}")
        if path.is_symlink():
            raise RuntimeError(f"Source file must not be a symlink: {path}")
        entries.append((path, archive_name(path)))

    entries.sort(key=lambda entry: entry[1])
    names = [name for _, name in entries]
    if len(names) != len(set(names)):
        raise RuntimeError("Source allowlist produced duplicate archive entries")
    return entries


def choose_destination() -> Path:
    timestamp = datetime.now().astimezone().strftime("%Y%m%d-%H%M%S")
    base_name = f"handy-processor-sources-{timestamp}"
    destination = ARCHIVE_DIRECTORY / f"{base_name}.zip"
    suffix = 2
    while destination.exists():
        destination = ARCHIVE_DIRECTORY / f"{base_name}-{suffix}.zip"
        suffix += 1
    return destination


def parse_build_number(value: str, source: str) -> str:
    normalized = value.strip()
    if not normalized.isascii() or not normalized.isdecimal():
        raise RuntimeError(
            f"Build number from {source} must be a non-negative integer"
        )
    return str(int(normalized))


def resolve_build_number() -> str:
    configured = os.environ.get(BUILD_NUMBER_ENV)
    if configured is not None:
        return parse_build_number(configured, BUILD_NUMBER_ENV)

    try:
        result = subprocess.run(
            ["git", "-C", str(PROJECT_ROOT), "rev-list", "--count", "HEAD"],
            check=False,
            capture_output=True,
            text=True,
        )
    except OSError:
        result = None

    if result is not None and result.returncode == 0:
        return parse_build_number(result.stdout, "Git")

    archived_path = PROJECT_ROOT / BUILD_NUMBER_FILE
    if archived_path.is_file():
        return parse_build_number(
            archived_path.read_text(encoding="utf-8"), BUILD_NUMBER_FILE
        )

    git_error = "Git is unavailable"
    if result is not None and result.stderr.strip():
        git_error = result.stderr.strip()
    raise RuntimeError(
        f"Cannot determine build number: {git_error}; "
        f"{BUILD_NUMBER_FILE} is missing; set {BUILD_NUMBER_ENV} explicitly"
    )


def validate_archive(
    path: Path, expected_names: set[str], generated_files: dict[str, bytes]
) -> None:
    with ZipFile(path, "r") as archive:
        actual_names = archive.namelist()
        if len(actual_names) != len(set(actual_names)):
            raise RuntimeError("Created archive contains duplicate entries")
        if set(actual_names) != expected_names:
            missing = sorted(expected_names.difference(actual_names))
            unexpected = sorted(set(actual_names).difference(expected_names))
            raise RuntimeError(
                f"Created archive has an unexpected manifest; "
                f"missing={missing}, unexpected={unexpected}"
            )
        for name in actual_names:
            path_name = PurePosixPath(name)
            if path_name.is_absolute() or ".." in path_name.parts:
                raise RuntimeError(f"Unsafe archive entry: {name}")
        bad_file = archive.testzip()
        if bad_file is not None:
            raise RuntimeError(f"Archive integrity check failed for {bad_file}")
        for name, expected_content in generated_files.items():
            if archive.read(name) != expected_content:
                raise RuntimeError(f"Generated archive entry is invalid: {name}")


def create_source_archive() -> Path:
    entries = collect_sources()
    generated_files = {
        BUILD_NUMBER_FILE: f"{resolve_build_number()}\n".encode("ascii"),
    }
    physical_names = {name for _, name in entries}
    duplicate_names = physical_names.intersection(generated_files)
    if duplicate_names:
        raise RuntimeError(
            f"Generated archive entries duplicate source paths: {sorted(duplicate_names)}"
        )
    ARCHIVE_DIRECTORY.mkdir(parents=True, exist_ok=True)
    destination = choose_destination()
    temporary = destination.with_name(f".{destination.name}.tmp-{os.getpid()}")

    try:
        with ZipFile(
            temporary,
            mode="x",
            compression=ZIP_DEFLATED,
            compresslevel=9,
        ) as archive:
            for source, name in entries:
                archive.write(source, arcname=name)
            for name, content in sorted(generated_files.items()):
                archive.writestr(name, content)

        validate_archive(
            temporary,
            physical_names.union(generated_files),
            generated_files,
        )
        temporary.replace(destination)
    except BaseException:
        temporary.unlink(missing_ok=True)
        raise

    return destination


if __name__ == "__main__":
    result = create_source_archive()
    size_mib = result.stat().st_size / (1024 * 1024)
    print(f"Source archive created: {result}")
    print(f"Size: {size_mib:.2f} MiB")
