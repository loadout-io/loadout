#!/usr/bin/env python3
"""Atomowo oznacza workspace jako zaufany u obu vendorow."""

from __future__ import annotations

import fcntl
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
from typing import Callable


def locked(lock_path: Path, change: Callable[[], None]) -> None:
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with lock_path.open("a+", encoding="utf-8") as lock_file:
        fcntl.flock(lock_file.fileno(), fcntl.LOCK_EX)
        try:
            change()
        finally:
            fcntl.flock(lock_file.fileno(), fcntl.LOCK_UN)


def replace_atomically(path: Path, body: str) -> None:
    mode = stat.S_IMODE(path.stat().st_mode)
    descriptor, temporary_name = tempfile.mkstemp(
        dir=path.parent,
        prefix=f".{path.name}.loadout.",
    )
    temporary = Path(temporary_name)
    try:
        os.fchmod(descriptor, mode)
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            output.write(body)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.close(descriptor)
        except OSError:
            pass
        temporary.unlink(missing_ok=True)
        raise


def trust_for_claude(home: Path, destination: str) -> None:
    config = home / ".claude.json"
    if not config.is_file():
        return

    def update() -> None:
        data = json.loads(config.read_text(encoding="utf-8"))
        projects = data.setdefault("projects", {})
        for path in {destination, os.path.realpath(destination)}:
            projects.setdefault(path, {})["hasTrustDialogAccepted"] = True
        replace_atomically(config, json.dumps(data, indent=2) + "\n")

    # 2026-08-24: dwa rownolegle worktree uzyly wspolnego .tmp; jeden os.replace
    # skasowal plik drugiemu. Staly plik blokady przezywa podmiane inode konfiguracji.
    locked(config.with_name(config.name + ".loadout.lock"), update)


def trust_for_codex(home: Path, destination: str) -> None:
    codex_home = Path(os.environ.get("CODEX_HOME", str(home / ".codex")))
    config = codex_home / "config.toml"
    if not config.is_file():
        return

    def update() -> None:
        body = config.read_text(encoding="utf-8")
        quoted = json.dumps(destination, ensure_ascii=False)
        header = f"[projects.{quoted}]"
        if header in body.splitlines():
            return
        separator = "" if body.endswith("\n") else "\n"
        replace_atomically(
            config,
            f'{body}{separator}\n{header}\ntrust_level = "trusted"\n',
        )

    # 2026-08-24: check-then-append z dwoch procesow spletl dwa naglowki i dwa
    # trust_level, przez co Codex nie umial nawet sparsowac swojej konfiguracji.
    locked(config.with_name(config.name + ".loadout.lock"), update)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: trust-workspace.py <workspace>", file=sys.stderr)
        return 2
    home_value = os.environ.get("HOME")
    if not home_value:
        print("HOME is required to mark a workspace trusted", file=sys.stderr)
        return 2
    destination = os.path.abspath(sys.argv[1])
    home = Path(home_value)
    trust_for_claude(home, destination)
    trust_for_codex(home, destination)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
