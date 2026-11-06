#!/usr/bin/env python3
"""Locate a pinned hmi-compiler, or download a verified Windows release on request."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile


TOOL_VERSION = "0.1.0"
REPOSITORY = "kanami09/hmi-compiler"


def verify_version(executable):
    result = subprocess.run(
        [str(executable), "--version"],
        capture_output=True, text=True, encoding="utf-8", timeout=15, check=False,
    )
    expected = "hmi-compiler " + TOOL_VERSION
    if result.returncode != 0 or result.stdout.strip() != expected:
        raise ValueError(f"Expected {expected}, got {result.stdout.strip()!r}: {executable}")


def default_cache_dir():
    if sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA") or Path.home() / "AppData/Local")
    elif sys.platform == "darwin":
        base = Path.home() / "Library/Caches"
    else:
        base = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache")
    return base / "hmi-compiler"


def release_arch():
    machine = platform.machine().lower()
    if sys.platform != "win32" or machine not in {"amd64", "x86_64", "x86", "i386", "i686"}:
        raise ValueError("Automatic download supports Windows x64/x86 only; use a local tool or build from source")
    return "x64" if machine in {"amd64", "x86_64"} else "x86"


def download(url, destination):
    request = urllib.request.Request(url, headers={"User-Agent": "hmi-project-editor"})
    with urllib.request.urlopen(request, timeout=45) as response:
        with destination.open("wb") as output:
            shutil.copyfileobj(response, output)


def verify_archive(archive, checksum):
    fields = checksum.read_text(encoding="utf-8-sig").strip().split()
    if (len(fields) != 2 or not re.fullmatch(r"[0-9a-fA-F]{64}", fields[0])
            or fields[1] != archive.name):
        raise ValueError("Invalid SHA-256 file or unexpected archive name")
    digest = hashlib.sha256()
    with archive.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != fields[0].lower():
        raise ValueError("Release archive SHA-256 mismatch")


def install_release(cache_dir, arch):
    target_dir = cache_dir / ("v" + TOOL_VERSION) / ("windows-" + arch)
    executable = target_dir / "hmi-compiler.exe"
    if executable.is_file():
        verify_version(executable)
        return executable, "cache"

    target_dir.mkdir(parents=True, exist_ok=True)
    package = f"hmi-compiler-v{TOOL_VERSION}-windows-{arch}.zip"
    base_url = f"https://github.com/{REPOSITORY}/releases/download/v{TOOL_VERSION}/"
    with tempfile.TemporaryDirectory(prefix=".download-", dir=target_dir) as staging:
        staging_dir = Path(staging)
        archive = staging_dir / package
        checksum = staging_dir / (package + ".sha256")
        download(base_url + package + ".sha256", checksum)
        download(base_url + package, archive)
        verify_archive(archive, checksum)
        # Read only the release's root executable; never extract arbitrary ZIP paths.
        candidate = staging_dir / "hmi-compiler.exe"
        with zipfile.ZipFile(archive) as bundle:
            matches = [item for item in bundle.infolist() if item.filename == candidate.name]
            if len(matches) != 1 or matches[0].is_dir():
                raise ValueError("Release must contain exactly one root hmi-compiler.exe")
            with bundle.open(matches[0]) as source, candidate.open("wb") as output:
                shutil.copyfileobj(source, output)
        verify_version(candidate)
        os.replace(candidate, executable)
    return executable, "download"


def resolve_tool(executable, cache_dir, allow_download):
    explicit = executable or os.environ.get("HMI_COMPILER_BIN")
    if explicit:
        path = Path(explicit).expanduser().resolve()
        if not path.is_file():
            raise ValueError(f"Explicit executable does not exist: {path}")
        verify_version(path)
        return path, "explicit"

    found = shutil.which("hmi-compiler")
    if found:
        path = Path(found).resolve()
        verify_version(path)
        return path, "path"

    arch = release_arch()
    cached = cache_dir / ("v" + TOOL_VERSION) / ("windows-" + arch) / "hmi-compiler.exe"
    if cached.is_file():
        verify_version(cached)
        return cached, "cache"
    if not allow_download:
        raise ValueError("hmi-compiler not found; provide --executable or retry with --download")
    return install_release(cache_dir, arch)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", help="Explicit executable path (or set HMI_COMPILER_BIN)")
    parser.add_argument("--cache-dir", type=Path, default=default_cache_dir())
    parser.add_argument("--download", action="store_true", help="Download the pinned release if missing")
    args = parser.parse_args()
    try:
        path, source = resolve_tool(
            args.executable, args.cache_dir.expanduser().resolve(), args.download,
        )
    except (OSError, ValueError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        print(json.dumps({"ok": False, "error": str(error)}, ensure_ascii=True))
        return 1
    print(json.dumps({
        "ok": True, "path": str(path), "version": TOOL_VERSION, "source": source,
    }, ensure_ascii=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
