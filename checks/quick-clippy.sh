#!/usr/bin/env bash
# ~10-15 s na ciepłym drzewie — clippy wyłącznie na bibliotece.
#
# NIGDY `--all-targets` tutaj. To nie jest kwestia gustu: --all-targets dokłada profile
# testów, benchów i przykładów, przemiela profil builda i zamienia pętlę wewnętrzną
# w kilkuminutowe czekanie (raport 04 §6.3; AGENTS.md, tabela "zakazane"). Pełna forma
# mieszka w checks/full-clippy.sh i biegnie raz, w bramce.
#
# Polityka lintów NIE jest tutaj. Siedzi w [workspace.lints] w Cargo.toml, żeby IDE widziało
# to samo, co bramka. `-D warnings` tylko podnosi ostrzeżenia do błędów.
#
# Wybór wobec pustego drzewa: brak ani jednego .rs = "nie ma czego lintować" i zielono.
# Warunek jest mechaniczny (istnienie pliku), więc pierwszy plik Rusta włącza clippy z
# powrotem. Alternatywa — czerwone do czasu pierwszego kodu — wywracałaby też tier `before`
# pierwszego zadania, bo sprawdzenia projektowe w `before` nie są odwracane.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

# W `full` to sprawdzenie jest ZBĘDNE i szkodliwe — nagłówek wyżej mówi to od początku
# („pełna forma mieszka w checks/full-clippy.sh i biegnie raz, w bramce"), tylko bramka
# odkrywała oba i uruchamiała je w tej samej fali.
#
# `--all-targets` zawiera `--lib`, więc oba clippy przemielają to samo drzewo — a ponieważ
# obydwa biorą muteks cargo (niezmiennik 26), jeszcze się o niego biją. Zmierzone 2026-08-16
# przy lądowaniu T-27, na PUSTEJ maszynie: drugie clippy czekało 300 s, oddało 2, i trunk
# zaświecił się „MISCONFIGURED" — czyli bramka nie osądziła kodu przez własną kolejkę.
#
# Wyjście zeru z powiedzeniem, DLACZEGO. Ciche pominięcie czyta się identycznie jak zdane
# sprawdzenie i to jest dokładnie ta awaria, przed którą stoi checks/MANIFEST.
if [ "${LOADOUT_TIER:-}" = "full" ]; then
  echo "clippy: superseded by checks/full-clippy.sh at this tier — --all-targets includes --lib"
  exit 0
fi

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

cargo_serialize || exit 2   # 2, nie 1: nic sie nie wykonalo, wiec to nie jest twierdzenie o kodzie (Q-3)

if ! out="$(cargo clippy --lib -- -D warnings 2>&1)"; then
  echo "clippy found something it will not let through" >&2
  # Same diagnostyki, bez ściany "Compiling foo v1.2.3".
  printf '%s\n' "$out" | grep -vE '^\s*(Compiling|Downloading|Updating|Checking|Fresh|Blocking)' \
    | head -50 >&2
  exit 1
fi

warns="$(printf '%s\n' "$out" | grep -cE '^(warning|error)' || true)"
echo "clippy: --lib clean, 0 warnings (${warns:-0} diagnostics printed)"
