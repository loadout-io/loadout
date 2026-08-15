#!/usr/bin/env bash
# ~0.2 s — trzy granice z docs/ARCHITECTURE.md §3, egzekwowane zamiast opisane.
#
#   niezmiennik 1  engine/ nie zna słowa "tauri". Bez tego silnik nie da się przetestować
#                  bez okna i osobny daemon nigdy nie powstanie.
#   niezmiennik 3  #[cfg(windows)] / #[cfg(unix)] wyłącznie w engine/supervisor.rs. To jest
#                  jedyny powód, dla którego port na Windows będzie gałęzią cfg, a nie
#                  przepisaniem.
#   niezmiennik 2  do SQLite pisze wyłącznie store/writer.rs. Drugie połączenie zapisujące
#                  to zakleszczenie, nie "czasem wolniej".
#
# TO JEST GREP PO CZYSTYM DRZEWIE i dlatego jest sprawdzeniem PROJEKTOWYM, nigdy kryterium
# akceptacji: przechodzi zanim kod powstanie, więc nie umie zaczerwienić się w tierze
# `before` i niczego by tam nie poświadczyło (00-SYNTHESIS §5).
#
# Wybór wobec pustego drzewa: brak engine/ -> "nie ma czego przekraczać" i zielono, z nazwaną
# ścieżką w komunikacie. Warunek jest mechaniczny — pierwszy plik w src-tauri/src/engine/
# włącza wszystkie trzy reguły naraz.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

ENGINE="src-tauri/src/engine"
SUPERVISOR="$ENGINE/supervisor.rs"
WRITER="src-tauri/src/store/writer.rs"

rs="$(find src-tauri/src -name '*.rs' 2>/dev/null | head -1 || true)"
if [ -z "$rs" ]; then
  echo "boundary: no Rust source yet ($ENGINE does not exist), nothing to keep apart"
  exit 0
fi

problems=""

# Pliki testowe wyłączone ze WSZYSTKICH trzech reguł: test wolno mieć własne połączenie
# i własny cfg, bo nie jest częścią wysyłanego artefaktu. Wyłączenie jest po ŚCIEŻCE,
# nigdy po treści — "czy to jest test" nie może być oceną.
not_test() { case "$1" in */tests/*|*/test/*|*_test.rs|*_tests.rs|*/fake.rs) return 1 ;; esac; return 0; }

# Komentarze liniowe zdejmujemy, żeby zdanie "engine/ nie importuje tauri::*" w nagłówku
# pliku nie wywracało własnej reguły. Blokowych /* */ nie tykamy — w Ruście prawie ich nie ma.
uncommented() { sed 's://.*::' "$1"; }

# ── 1. engine/ vs tauri ────────────────────────────────────────────────────────────────────
if [ -d "$ENGINE" ]; then
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    not_test "$f" || continue
    hit="$(uncommented "$f" | grep -niE 'tauri' | head -3 || true)"
    if [ -n "$hit" ]; then
      problems+="  $f mentions tauri (invariant 1 — engine/ must build and test without a window)"$'\n'
      problems+="$(printf '%s\n' "$hit" | sed 's/^/      /')"$'\n'
    fi
  done < <(find "$ENGINE" -name '*.rs' 2>/dev/null || true)
fi

# ── 2. kod platformowy tylko w supervisor.rs ───────────────────────────────────────────────
while IFS= read -r f; do
  [ -n "$f" ] || continue
  [ "$f" = "$SUPERVISOR" ] && continue
  not_test "$f" || continue
  hit="$(uncommented "$f" | grep -nE '#\[cfg\((any\(|not\()?(windows|unix|target_os|target_family)' | head -3 || true)"
  if [ -n "$hit" ]; then
    problems+="  $f carries platform cfg (invariant 3 — only $SUPERVISOR may)"$'\n'
    problems+="$(printf '%s\n' "$hit" | sed 's/^/      /')"$'\n'
  fi
done < <(find src-tauri/src -name '*.rs' 2>/dev/null || true)

# ── 3. jeden pisarz do SQLite ──────────────────────────────────────────────────────────────
# Dwie reguły, obie mechaniczne, obie z nazwanym ograniczeniem:
#   a) Connection::open* poza writer.rs musi nieść SQLITE_OPEN_READ_ONLY.
#   b) SQL mutujący poza writer.rs/schema.rs w ogóle nie istnieje.
# Czego to NIE widzi: zapytania sklejanego w czasie biegu z fragmentów trzymanych gdzie
# indziej. Ten przypadek zostaje ludzkim osądem i jest tu wypisany, żeby nikt nie wziął
# zieleni za dowód.
while IFS= read -r f; do
  [ -n "$f" ] || continue
  [ "$f" = "$WRITER" ] && continue
  not_test "$f" || continue
  body="$(uncommented "$f")"
  hit="$(printf '%s\n' "$body" | grep -nE 'Connection::open' | grep -vE 'READ_ONLY' | head -3 || true)"
  if [ -n "$hit" ]; then
    problems+="  $f opens a SQLite connection without SQLITE_OPEN_READ_ONLY (invariant 2)"$'\n'
    problems+="$(printf '%s\n' "$hit" | sed 's/^/      /')"$'\n'
  fi
  case "$f" in src-tauri/src/store/schema.rs|src-tauri/src/store/migrate.rs) continue ;; esac
  hit="$(printf '%s\n' "$body" | grep -nEi '(INSERT[[:space:]]+INTO|UPDATE[[:space:]]+[a-z_]+[[:space:]]+SET|DELETE[[:space:]]+FROM|CREATE[[:space:]]+TABLE|ALTER[[:space:]]+TABLE|DROP[[:space:]]+TABLE)' | head -3 || true)"
  if [ -n "$hit" ]; then
    problems+="  $f writes SQL (invariant 2 — only $WRITER may; a second writer is a deadlock)"$'\n'
    problems+="$(printf '%s\n' "$hit" | sed 's/^/      /')"$'\n'
  fi
done < <(find src-tauri/src -name '*.rs' 2>/dev/null || true)

if [ -n "$problems" ]; then
  echo "an architecture boundary was crossed" >&2
  printf '%s' "$problems" >&2
  echo "detail: the three boundaries are docs/ARCHITECTURE.md §3, invariants 1, 2 and 3." >&2
  exit 1
fi

n="$(find src-tauri/src -name '*.rs' 2>/dev/null | wc -l | tr -d ' ')"
echo "boundary: $n Rust files, engine/ free of tauri, platform cfg only in supervisor.rs, one SQLite writer"
