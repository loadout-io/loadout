#!/usr/bin/env bash
# Pełna forma clippy. TYLKO tutaj, w bramce, raz.
#
# `--all-targets` dokłada testy, benche i przykłady, przez co przemiela profil builda —
# w pętli wewnętrznej to minuty czekania i dlatego AGENTS.md wprost tego zakazuje
# (tabela "zakazane"; raport 04 §6.3). checks/quick-clippy.sh robi wariant `--lib`.
#
# Co ta wersja łapie, a tamta nie: unwrap() i panic!() w kodzie TESTOWYM. Polityka
# [workspace.lints] jest jedna dla całego drzewa, więc test, który ląduje z unwrapem,
# przechodzi pętlę wewnętrzną i zatrzymuje się dopiero tutaj.
#
# Wybór wobec pustego drzewa: identyczny jak w quick-clippy — brak .rs to "nie ma czego
# lintować" i zielono, warunkiem mechanicznym, nie oceną.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"
# shellcheck source=checks/_cargo-serialize.sh
. checks/_cargo-serialize.sh

rs="$(find src-tauri/src -name '*.rs' 2>/dev/null | head -1 || true)"
if [ -z "$rs" ]; then
  echo "clippy: no Rust source yet (src-tauri/src/ holds no .rs), nothing to lint"
  exit 0
fi

command -v cargo >/dev/null 2>&1 || { echo "cargo is not on PATH" >&2; exit 2; }
cargo clippy --version >/dev/null 2>&1 \
  || { echo "clippy is missing: rustup component add clippy" >&2; exit 2; }

# Niezmiennik 26: full-test.sh i to sprawdzenie biegną w tej samej fali bramki.
cargo_serialize || exit 1

if ! out="$(cargo clippy --all-targets -- -D warnings 2>&1)"; then
  echo "clippy --all-targets found something it will not let through" >&2
  printf '%s\n' "$out" | grep -vE '^\s*(Compiling|Downloading|Updating|Checking|Fresh|Blocking)' \
    | head -60 >&2
  echo "detail: this form also judges tests, benches and examples — quick-clippy does not." >&2
  exit 1
fi

n="$(find src-tauri/src -name '*.rs' 2>/dev/null | wc -l | tr -d ' ')"
echo "clippy: --all-targets clean over $n Rust files, 0 warnings"
