#!/usr/bin/env python3
"""Safely replace a Windows source tree from the newest source ZIP."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import stat
import sys
import tempfile
from datetime import datetime
from pathlib import Path, PurePosixPath
from zipfile import BadZipFile, ZipFile, ZipInfo


PROJECT_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_CONFIG = PROJECT_ROOT / "unpack-sources.config.json"
MAX_ARCHIVE_ENTRIES = 10_000
MAX_UNCOMPRESSED_BYTES = 256 * 1024 * 1024

PRESERVED_DIRECTORY_NAMES = frozenset(
    {
        ".git",
        ".venv",
        "__pycache__",
        "archive",
        "binaries",
        "dist",
        "node_modules",
        "release",
        "sea-build",
        "target",
        "web-dist",
    }
)
PRESERVED_ROOT_FILES = frozenset({".env", "config.json"})
PRESERVED_FILE_SUFFIXES = (
    ".log",
    ".sqlite3",
    ".sqlite3-journal",
    ".sqlite3-shm",
    ".sqlite3-wal",
)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Unpack the newest ./archive source ZIP into path-dist",
    )
    parser.add_argument(
        "--config",
        type=Path,
        default=DEFAULT_CONFIG,
        help=f"config path (default: {DEFAULT_CONFIG})",
    )
    parser.add_argument(
        "--yes",
        action="store_true",
        help="apply without an interactive confirmation",
    )
    return parser.parse_args()


def load_destination(config_path: Path) -> tuple[Path, Path]:
    config_path = config_path.expanduser().resolve()
    try:
        value = json.loads(config_path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise RuntimeError(f"Config does not exist: {config_path}") from error
    except json.JSONDecodeError as error:
        raise RuntimeError(f"Invalid JSON in {config_path}: {error}") from error

    if not isinstance(value, dict):
        raise RuntimeError(f"{config_path} must contain a JSON object")
    configured_path = value.get("path-dist")
    if not isinstance(configured_path, str) or not configured_path.strip():
        raise RuntimeError(f'{config_path}: "path-dist" must be a non-empty string')

    base_directory = config_path.parent
    destination = Path(configured_path).expanduser()
    if not destination.is_absolute():
        destination = base_directory / destination
    destination = destination.resolve()
    if destination.parent == destination:
        raise RuntimeError("Refusing to use a filesystem root as path-dist")
    return base_directory, destination


def newest_archive(archive_directory: Path) -> Path:
    if not archive_directory.is_dir():
        raise RuntimeError(f"Archive directory does not exist: {archive_directory}")
    archives = [path for path in archive_directory.glob("*.zip") if path.is_file()]
    if not archives:
        raise RuntimeError(f"No ZIP archives found in {archive_directory}")
    return max(archives, key=lambda path: (path.stat().st_mtime_ns, path.name))


def safe_archive_name(name: str) -> tuple[PurePosixPath, bool]:
    if "\\" in name:
        raise RuntimeError(f"Archive entry contains a backslash: {name}")
    directory = name.endswith("/")
    normalized_name = name[:-1] if directory else name
    path = PurePosixPath(normalized_name)
    if (
        not normalized_name
        or path.is_absolute()
        or any(part in {"", ".", ".."} for part in path.parts)
        or any(":" in part for part in path.parts)
        or path.as_posix() != normalized_name
    ):
        raise RuntimeError(f"Unsafe archive entry: {name}")
    return path, directory


def validate_archive(archive: ZipFile) -> list[tuple[ZipInfo, PurePosixPath, bool]]:
    entries = archive.infolist()
    if len(entries) > MAX_ARCHIVE_ENTRIES:
        raise RuntimeError(f"Archive contains too many entries: {len(entries)}")

    total_size = sum(entry.file_size for entry in entries)
    if total_size > MAX_UNCOMPRESSED_BYTES:
        raise RuntimeError(
            f"Archive expands to more than {MAX_UNCOMPRESSED_BYTES // (1024 * 1024)} MiB"
        )

    validated: list[tuple[ZipInfo, PurePosixPath, bool]] = []
    names: set[str] = set()
    for entry in entries:
        path, directory = safe_archive_name(entry.filename)
        normalized = path.as_posix()
        if normalized in names:
            raise RuntimeError(f"Duplicate archive entry: {normalized}")
        names.add(normalized)

        unix_mode = entry.external_attr >> 16
        if stat.S_ISLNK(unix_mode):
            raise RuntimeError(f"Archive symlinks are not allowed: {entry.filename}")
        if entry.flag_bits & 0x1:
            raise RuntimeError(f"Encrypted archive entries are not allowed: {entry.filename}")
        validated.append((entry, path, directory or entry.is_dir()))

    bad_file = archive.testzip()
    if bad_file is not None:
        raise RuntimeError(f"Archive integrity check failed for {bad_file}")
    return validated


def extract_to_staging(archive_path: Path, staging: Path) -> None:
    try:
        with ZipFile(archive_path, "r") as archive:
            entries = validate_archive(archive)
            for entry, relative, directory in entries:
                target = staging.joinpath(*relative.parts)
                if directory:
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(entry, "r") as source, target.open("xb") as output:
                    shutil.copyfileobj(source, output)
    except BadZipFile as error:
        raise RuntimeError(f"Invalid ZIP archive: {archive_path}") from error


def is_preserved(relative: Path) -> bool:
    if any(part.casefold() in PRESERVED_DIRECTORY_NAMES for part in relative.parts):
        return True
    if len(relative.parts) == 1 and relative.name.casefold() in PRESERVED_ROOT_FILES:
        return True
    return relative.name.casefold().endswith(PRESERVED_FILE_SUFFIXES)


def unique_backup_path(destination: Path) -> Path:
    preferred = destination.with_name(f"{destination.name}-backup")
    if not preferred.exists():
        return preferred

    timestamp = datetime.now().astimezone().strftime("%Y%m%d-%H%M%S")
    candidate = destination.with_name(f"{destination.name}-backup-{timestamp}")
    suffix = 2
    while candidate.exists():
        candidate = destination.with_name(
            f"{destination.name}-backup-{timestamp}-{suffix}"
        )
        suffix += 1
    return candidate


def move_existing_sources(destination: Path, backup: Path) -> None:
    backup.mkdir(parents=True, exist_ok=False)

    def move_entry(source: Path, relative: Path) -> None:
        if is_preserved(relative):
            return
        target = backup / relative
        if source.is_symlink() or source.is_file():
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.move(str(source), str(target))
            return
        if not source.is_dir():
            raise RuntimeError(f"Unsupported destination entry: {source}")

        children = list(source.iterdir())
        if not children:
            target.mkdir(parents=True, exist_ok=True)
            source.rmdir()
            return
        for child in children:
            move_entry(child, relative / child.name)
        if not any(source.iterdir()):
            target.mkdir(parents=True, exist_ok=True)
            source.rmdir()

    for child in list(destination.iterdir()):
        move_entry(child, Path(child.name))


def install_staged_sources(
    staging: Path,
    destination: Path,
    installed_files: list[Path],
    created_directories: list[Path],
) -> None:
    def ensure_directory(path: Path) -> None:
        missing: list[Path] = []
        current = path
        while not current.exists():
            missing.append(current)
            current = current.parent
        if not current.is_dir():
            raise RuntimeError(f"Cannot create directory below file: {current}")
        for directory in reversed(missing):
            directory.mkdir()
            created_directories.append(directory)

    def install_entry(source: Path, relative: Path) -> None:
        target = destination / relative
        if is_preserved(relative):
            if not (
                len(relative.parts) == 1
                and relative.name.casefold() == "config.json"
                and not target.exists()
            ):
                return
        if source.is_dir():
            if target.exists() and not target.is_dir():
                raise RuntimeError(f"Cannot create directory over file: {target}")
            if not target.exists():
                ensure_directory(target)
            for child in list(source.iterdir()):
                install_entry(child, relative / child.name)
            return

        if target.exists():
            raise RuntimeError(f"Refusing to overwrite preserved path: {target}")
        ensure_directory(target.parent)
        shutil.move(str(source), str(target))
        installed_files.append(target)

    for child in list(staging.iterdir()):
        install_entry(child, Path(child.name))


def restore_backup(backup: Path, destination: Path) -> None:
    def restore_entry(source: Path, relative: Path) -> None:
        target = destination / relative
        if source.is_dir():
            target.mkdir(parents=True, exist_ok=True)
            for child in list(source.iterdir()):
                restore_entry(child, relative / child.name)
            return
        if target.exists():
            raise RuntimeError(f"Cannot restore backup over existing path: {target}")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(source), str(target))

    for child in list(backup.iterdir()):
        restore_entry(child, Path(child.name))


def confirm_update(archive: Path, destination: Path, backup: Path) -> bool:
    print(f"Archive:    {archive}")
    print(f"Destination:{destination}")
    print(f"Backup:     {backup}")
    print("Preserved:  .git, node_modules, config/SQLite data and build binaries")
    if not sys.stdin.isatty():
        print("Interactive confirmation is unavailable; pass --yes to apply.")
        return False
    answer = input("Replace destination sources? [y/N] ").strip().casefold()
    return answer in {"y", "yes"}


def update_sources(config_path: Path, assume_yes: bool) -> tuple[Path, Path]:
    base_directory, destination = load_destination(config_path)
    archive = newest_archive(base_directory / "archive")
    backup = unique_backup_path(destination)

    if not assume_yes and not confirm_update(archive, destination, backup):
        raise RuntimeError("Source update cancelled")

    destination.mkdir(parents=True, exist_ok=True)
    staging = Path(
        tempfile.mkdtemp(
            prefix=f".{destination.name}-unpack-",
            dir=destination.parent,
        )
    )
    installed_files: list[Path] = []
    created_directories: list[Path] = []
    try:
        extract_to_staging(archive, staging)
        move_existing_sources(destination, backup)
        install_staged_sources(
            staging,
            destination,
            installed_files,
            created_directories,
        )
    except BaseException as error:
        rollback_errors: list[str] = []
        backup_restored = False
        for path in reversed(installed_files):
            try:
                path.unlink(missing_ok=True)
            except OSError as rollback_error:
                rollback_errors.append(str(rollback_error))
        for path in reversed(created_directories):
            try:
                path.rmdir()
            except OSError:
                pass
        if backup.exists():
            try:
                restore_backup(backup, destination)
                backup_restored = True
            except BaseException as rollback_error:
                rollback_errors.append(str(rollback_error))
        detail = (
            f" Rollback also reported: {'; '.join(rollback_errors)}. "
            f"Backup remains at {backup}."
            if rollback_errors
            else (
                " Previous sources were restored from backup."
                if backup_restored
                else " Destination was not modified."
            )
        )
        raise RuntimeError(f"Source update failed: {error}.{detail}") from error
    finally:
        shutil.rmtree(staging, ignore_errors=True)

    return archive, backup


def main() -> int:
    arguments = parse_arguments()
    try:
        archive, backup = update_sources(arguments.config, arguments.yes)
    except RuntimeError as error:
        print(f"Error: {error}", file=sys.stderr)
        return 1

    print(f"Sources updated from: {archive}")
    print(f"Previous sources moved to: {backup}")
    print("Preserved dependencies and build binaries were not modified.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
