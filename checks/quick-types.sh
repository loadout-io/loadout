#!/usr/bin/env bash
# ~3 s — typy frontendu, sprawdzone konfiguracją, której bieg nie może osłabić.
#
# `-p checks/tsconfig.strict.json`, nie `-p tsconfig.json`. Korzeniowy tsconfig należy do
# zadania; bieg, który nie umie przejść typów, ma tam najtańszą naprawę świata pod ręką.
# Ten plik leży w checks/, poza blokiem OWNS każdego zadania.
#
# Druga rzecz, którą to sprawdzenie pilnuje: WERSJA. `npm install typescript` daje dziś 7,
# bo 7 jest `latest`, a 7 jest jeszcze nielintowalny (raport T8, ryzyko 5) — package.json
# pinuje `~6.0.3` DOKŁADNIE i to jest jedyne miejsce, gdzie ten pin jest egzekwowany.
# Bump majora, który przechodzi bramkę po cichu, to bump, którego nikt nie zauważy.
#
# Wybór wobec pustego drzewa: asercja wersji biegnie ZAWSZE (jest nie-pusta od pierwszego
# dnia), a samo `tsc` jest pomijane tylko wtedy, gdy w src/ nie ma ani jednego .ts/.tsx —
# bo tsc na pustym zbiorze wejściowym zwraca TS18003 i to byłaby czerwień bez powodu.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

TSC=node_modules/.bin/tsc
if [ ! -x "$TSC" ]; then
  # Bez `npx --yes`: pobranie kompilatora z sieci sprawdza INNĄ wersję niż ta, którą
  # zbuduje aplikacja, a to jest dokładnie ten rodzaj cichej podmiany narzędzia,
  # którego to sprawdzenie ma pilnować.
  echo "TypeScript is not installed: run \`npm install\`" >&2
  exit 2
fi

ver="$("$TSC" --version 2>/dev/null | tr -dc '0-9.' || true)"
major="${ver%%.*}"
if [ "${major:-0}" != "6" ]; then
  echo "TypeScript major is ${major:-unknown} (${ver:-none}), and this project needs 6" >&2
  echo "detail: 7 is what \`latest\` resolves to and it is not lintable yet (T8, risk 5)." >&2
  echo "detail: package.json pins ~6.0.3 exactly — reinstall, do not bump." >&2
  exit 1
fi

[ -f checks/tsconfig.strict.json ] \
  || { echo "checks/tsconfig.strict.json is missing" >&2; exit 2; }

ts="$(find src -type f \( -name '*.ts' -o -name '*.tsx' \) 2>/dev/null | head -1 || true)"
if [ -z "$ts" ]; then
  echo "types: TypeScript $ver, no .ts/.tsx under src/ yet, nothing to check"
  exit 0
fi

out=""; rc=0
out="$("$TSC" --noEmit -p checks/tsconfig.strict.json 2>&1)" || rc=$?

# tsc rozróżnia dwie rzeczy i my musimy je przepuścić dalej rozróżnione (N-01):
#   rc 1 = kod się nie typuje        -> czerwone, wina kodu
#   rc 2 = KONFIGURACJA jest zepsuta -> exit 2, wina NASZA
# Wcześniej oba lądowały jako 1. Skutek: błąd w naszym tsconfigu omijał wszystkie trzy
# wyjścia awaryjne wpięte w exit 2 (gate.py, stop-gate.sh, ship-task.sh) i lądował jako
# runda naprawcza, której pisarz nie ma jak wygrać — checks/ jest dla niego zabronione.
if [ "$rc" -eq 2 ] || printf '%s\n' "$out" | grep -qE 'error TS(5[0-9]{3}|18003)'; then
  echo "our TypeScript configuration is broken — this is not your code" >&2
  printf '%s\n' "$out" | head -20 >&2
  echo "detail: checks/tsconfig.strict.json, owned by the harness, not by any task" >&2
  exit 2
fi

if [ "$rc" -ne 0 ]; then
  echo "the frontend does not typecheck under the strict config" >&2
  printf '%s\n' "$out" | head -30 >&2
  echo "detail: config is checks/tsconfig.strict.json, which no task owns" >&2
  exit 1
fi

n="$(find src -type f \( -name '*.ts' -o -name '*.tsx' \) 2>/dev/null | wc -l | tr -d ' ')"
echo "types: TypeScript $ver, $n files strict, no implicit any"
