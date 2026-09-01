#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

mkdir -p "$scratch/home/.codex"
printf '{"projects": {}}\n' > "$scratch/home/.claude.json"
printf 'model = "test"\n' > "$scratch/home/.codex/config.toml"

pids=()
for number in 1 2 3 4 5 6 7 8 9 10 11 12; do
  HOME="$scratch/home" CODEX_HOME="$scratch/home/.codex" \
    python3 "$ROOT/.loadout/h/trust-workspace.py" "$scratch/work-$number" &
  pids+=("$!")
done
for pid in "${pids[@]}"; do
  wait "$pid"
done

python3 - "$scratch" <<'PY'
import json
import os
from pathlib import Path
import sys

root = Path(sys.argv[1])
expected = {str(root / f"work-{number}") for number in range(1, 13)}
expected_claude = expected | {os.path.realpath(path) for path in expected}

claude = json.loads((root / "home/.claude.json").read_text())
actual_claude = {
    path
    for path, settings in claude.get("projects", {}).items()
    if settings.get("hasTrustDialogAccepted") is True
}
if actual_claude != expected_claude:
    raise SystemExit(
        f"parallel Claude trust updates were lost: expected {len(expected_claude)}, got {len(actual_claude)}"
    )

current = None
trusted = {}
for raw in (root / "home/.codex/config.toml").read_text().splitlines():
    line = raw.strip()
    if line.startswith("[projects.") and line.endswith("]"):
        current = json.loads(line[len("[projects.") : -1])
    elif line.startswith("trust_level"):
        if current is None or current in trusted:
            raise SystemExit("Codex trust entries are interleaved or duplicated")
        trusted[current] = line.split("=", 1)[1].strip().strip('"')

actual_codex = {path for path, level in trusted.items() if level == "trusted"}
if actual_codex != expected:
    raise SystemExit(
        f"parallel Codex trust updates were lost: expected {len(expected)}, got {len(actual_codex)}"
    )

print("1 passed")
PY
