#!/usr/bin/env bash
# ~2 s — formatowanie obu połówek repo. Rust przez rustfmt, frontend przez prettier.
#
# Wybór wobec pustego drzewa: KAŻDA połówka zgłasza uczciwie, że nie ma czego formatować,
# i przechodzi. To nie może ukryć późniejszej awarii, bo warunek pominięcia jest
# "nie istnieje ani jeden plik tego typu" — pierwszy plik .rs albo .tsx włącza sprawdzenie
# z powrotem, bez niczyjej decyzji.
#
# Czego NIE robimy: nie udajemy zieleni, kiedy nie ma narzędzia. Brak rustfmt albo prettiera
# to nasza niesprawna konfiguracja, czyli kod 2 — nigdy 1. Zielona bramka na maszynie bez
# formattera jest gorsza niż czerwona.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

did=""

# ── Rust ───────────────────────────────────────────────────────────────────────────────────
rs="$(find src-tauri -name '*.rs' -not -path '*/target/*' 2>/dev/null | head -1 || true)"
if [ -n "$rs" ]; then
  command -v cargo >/dev/null 2>&1 || { echo "cargo is not on PATH" >&2; exit 2; }
  cargo fmt --version >/dev/null 2>&1 \
    || { echo "rustfmt is missing: rustup component add rustfmt" >&2; exit 2; }
  # `cargo fmt` nie kompiluje niczego, więc nie bierze zamka z niezmiennika 26.
  if ! out="$(cargo fmt --all --check 2>&1)"; then
    echo "Rust code is not formatted" >&2
    printf '%s\n' "$out" | head -40 >&2
    echo "detail: run \`cargo fmt --all\`" >&2
    exit 1
  fi
  did="rust"
fi

# ── Frontend ───────────────────────────────────────────────────────────────────────────────
web="$(find src -type f \( -name '*.ts' -o -name '*.tsx' -o -name '*.css' -o -name '*.json' \) 2>/dev/null | head -1 || true)"
if [ -n "$web" ]; then
  if [ -x node_modules/.bin/prettier ]; then
    PRETTIER=(node_modules/.bin/prettier)
  else
    # --no-install: `npx prettier` na maszynie bez node_modules ściąga pakiet z sieci
    # i formatuje INNĄ wersją niż CI. Cicha zmiana narzędzia jest gorsza niż brak narzędzia.
    echo "prettier is not installed: run \`npm install\`" >&2
    exit 2
  fi
  if ! out="$("${PRETTIER[@]}" --check "src/**/*.{ts,tsx,css,json}" 2>&1)"; then
    echo "frontend code is not formatted" >&2
    printf '%s\n' "$out" | head -40 >&2
    echo "detail: run \`npm run fmt\`" >&2
    exit 1
  fi
  did="${did:+$did + }web"
fi

if [ -z "$did" ]; then
  echo "fmt: nothing to format yet (no .rs under src-tauri/, no .ts/.tsx/.css under src/)"
  exit 0
fi
echo "fmt: $did formatted correctly"
