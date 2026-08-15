#!/usr/bin/env bash
# Każdy check, udowodniony, że strzela.
#
# To jest odpowiedź na niezmiennik 20. Selftest w spreadsheet asertował
# `"--sandbox workspace-write" in ship-task.sh`, przechodził NA KOMENTARZU, a żywa
# flaga brzmiała `danger-full-access`; 29 z 49 jego asercji tylko czytało pliki.
# Check, który był widziany wyłącznie na zielono, to check, którego nikt nie testował.
#
# Dla każdego checks/<tier>-*.sh: zasadź PRAWDZIWE naruszenie, wymagaj czerwonego,
# przywróć drzewo, wymagaj zielonego.
#
# Dwie rzeczy, których ten plik świadomie NIE robi:
#
# 1. Nie pomija się na brudnym drzewie. Pominięty guard czyta się dokładnie tak
#    samo jak guard, który przeszedł — a to jest ta sama awaria, której cały ten
#    plik ma pilnować. Brudne drzewo to exit 2: to nasz setup jest zły, nie kod.
# 2. Nie pozwala checkowi istnieć bez guarda. Brak funkcji guard_<id> to twarda
#    porażka z nazwą funkcji do dopisania, nie cicha luka.
#
# Jest jedna kategoria pośrednia i jest wypisywana głośno: NOT APPLICABLE, kiedy
# guard ma nazwany warunek wstępny, którego to drzewo jeszcze nie spełnia (nie ma
# crate'a Rusta, nie ma node_modules). Warunek jest drukowany za każdym razem —
# to jest zadeklarowana luka z powodem, nie pominięcie.
set -euo pipefail
shopt -s nullglob

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Sufit na pojedyncze uruchomienie checka. Zimne clippy potrafi iść minuty, a każdy
# check odpalamy tu dwa razy. Bez `timeout` (macOS go nie ma domyślnie) lecimy bez
# sufitu i mówimy o tym w podsumowaniu — zawieszony check ma się dać zauważyć.
CHECK_CEILING="${LOADOUT_GUARD_CEILING:-300}"
TIMEOUT_BIN=""
for cand in timeout gtimeout; do
  if command -v "$cand" >/dev/null 2>&1; then TIMEOUT_BIN="$cand"; break; fi
done

scratch="$(mktemp -d)"

# ── odmowy ───────────────────────────────────────────────────────────────────

if ! git rev-parse --verify -q HEAD >/dev/null; then
  echo "this repo has no commits yet, so nothing can be restored after planting." >&2
  echo "commit once, then run guards again." >&2
  exit 2
fi

if [ -n "$(git status --porcelain -uall)" ]; then
  echo "guards NOT RUN: the tree is dirty, so planting a violation proves nothing" >&2
  echo "  commit or stash first -- a skipped guard is not a passing one" >&2
  git status --porcelain -uall | head -10 | sed 's/^/    /' >&2
  exit 2
fi

CHECKS=( checks/before-*.sh checks/quick-*.sh checks/full-*.sh )
if [ "${#CHECKS[@]}" -eq 0 ]; then
  echo "no checks discovered under checks/ -- this gate can only report on itself" >&2
  exit 2
fi

# ── sadzenie i przywracanie ──────────────────────────────────────────────────
#
# Drzewo na starcie jest czyste (wymuszone wyżej), więc przywrócenie jest dokładne:
# skasuj to, co stworzyliśmy, wycofaj to, co dopisaliśmy. Na końcu i tak pytamy
# gita, czy drzewo jest znowu czyste — przerwany guard nie ma prawa zostawić
# zasadzonego naruszenia w repo.
PLANTED_NEW=()
PLANTED_DIRS=()
PLANTED_MOD=()

plant_new() {   # $1 ścieżka; treść na stdin
  local p="$1" d
  d="$(dirname "$p")"
  if [ ! -d "$d" ]; then
    mkdir -p "$d"
    PLANTED_DIRS=( "$d" ${PLANTED_DIRS[@]+"${PLANTED_DIRS[@]}"} )
  fi
  cat > "$p"
  PLANTED_NEW=( ${PLANTED_NEW[@]+"${PLANTED_NEW[@]}"} "$p" )
}

plant_append() {   # $1 śledzona ścieżka; treść na stdin
  local p="$1"
  cat >> "$p"
  PLANTED_MOD=( ${PLANTED_MOD[@]+"${PLANTED_MOD[@]}"} "$p" )
}

restore() {
  local p
  for p in ${PLANTED_NEW[@]+"${PLANTED_NEW[@]}"}; do rm -f "$p"; done
  for p in ${PLANTED_DIRS[@]+"${PLANTED_DIRS[@]}"}; do rmdir -p "$p" 2>/dev/null || true; done
  for p in ${PLANTED_MOD[@]+"${PLANTED_MOD[@]}"}; do git checkout -q -- "$p" 2>/dev/null || true; done
  PLANTED_NEW=(); PLANTED_DIRS=(); PLANTED_MOD=()
}

cleanup() { restore; rm -rf "$scratch"; }
trap 'cleanup' EXIT
# `trap - EXIT` przed wyjściem, bo w bashu 3.2 status ostatniej komendy pułapki
# EXIT potrafi nadpisać kod, z którym wychodzimy — a 3 znaczy "przerwane" i ma
# dojechać do wołającego nieprzekłamane.
trap 'cleanup; trap - EXIT; exit 3' INT TERM

run_check() {   # $1 ścieżka do checka -> jego kod wyjścia
  local script="$1" rc=0
  if [ -n "$TIMEOUT_BIN" ]; then
    "$TIMEOUT_BIN" -s TERM "$CHECK_CEILING" bash "$script" >"$scratch/out" 2>&1 || rc=$?
  else
    bash "$script" >"$scratch/out" 2>&1 || rc=$?
  fi
  return "$rc"
}

crate_root() {   # wypisuje korzeń crate'a Rusta albo zwraca 1
  local f
  for f in src-tauri/src/lib.rs src-tauri/src/main.rs; do
    if [ -f "$f" ]; then printf '%s' "$f"; return 0; fi
  done
  return 1
}

# ── rejestr guardów ──────────────────────────────────────────────────────────
#
# Jedna funkcja na check: guard_<id z myślnikami zamienionymi na podkreślenia>.
# Zwraca 0, kiedy naruszenie zostało zasadzone; 1 z ustawionym NA_REASON, kiedy
# to drzewo nie ma jeszcze w co sadzić.
#
# Repo nie ma na razie ANI JEDNEGO pliku Rusta i ani jednego komponentu Reacta.
# Świadomy wybór: sadzimy do plików, które check ma widzieć, i jeśli ich nie ma —
# mówimy "not applicable: <czego brakuje>" zamiast udawać dowód. Guard, który
# zasadził naruszenie w pliku poza zasięgiem checka, jest gorszy niż jego brak:
# raportuje "nie strzelił" o checku, który nie miał czego zobaczyć.

guard_quick_scope() {
  # quick-scope pomija się na trunku (zakres jest własnością gałęzi), a guards biegnie
  # właśnie na trunku. Podstawiamy nazwę, której żadna gałąź nie nosi, żeby sprawdzenie
  # oceniało plant zamiast wychodzić zerem na samej nazwie gałęzi.
  export LOADOUT_TRUNK=__guard_never_a_branch__
  # Dwie drogi zapisu poza dozwolone drzewa. Druga jest ta ciekawsza: plik
  # przemycony przez wpis w .gitignore. Check ma czytać status z
  # `-c core.excludesFile=/dev/null --ignored=matching`, więc ma to zobaczyć.
  plant_new harness/_guard_out_of_scope.txt <<'EOF'
planted by harness/guards.sh
EOF
  plant_append .gitignore <<'EOF'
_guard_laundered.rs
EOF
  plant_new _guard_laundered.rs <<'EOF'
fn _guard() {}
EOF
}

guard_quick_fmt() {
  local planted=1 root
  if [ -d node_modules ]; then
    plant_new src/_guard_fmt.ts <<'EOF'
export const guard   =    {a:1,   b   :2}
EOF
    planted=0
  fi
  if root="$(crate_root)"; then
    plant_append "$root" <<'EOF'

pub fn _guard_fmt(){let x=1;let _=x;}
EOF
    planted=0
  fi
  if [ "$planted" -ne 0 ]; then
    NA_REASON="no node_modules and no Rust crate root to plant into"
    return 1
  fi
}

guard_quick_types() {
  if [ ! -d node_modules ]; then
    NA_REASON="node_modules is missing, tsc cannot run"
    return 1
  fi
  plant_new src/_guard_types.ts <<'EOF'
export const guardCount: number = 'not a number';
EOF
}

guard_quick_clippy() {
  local root
  if ! root="$(crate_root)"; then
    NA_REASON="no Rust crate root yet (src-tauri/src/{lib,main}.rs)"
    return 1
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    NA_REASON="cargo is not installed"
    return 1
  fi
  # `unwrap_used` jest w Cargo.toml zadeklarowany jako deny, więc to jest
  # naruszenie polityki tego repo, a nie dowolny lint z półki.
  plant_append "$root" <<'EOF'

pub fn _guard_clippy(v: Option<i32>) -> i32 {
    let x = v.unwrap();
    return x;
}
EOF
}

guard_full_clippy() { guard_quick_clippy; }

guard_quick_vocabulary() {
  # Żargon w tekście widocznym dla użytkownika (niezmiennik 14, tabela z
  # 00-SYNTHESIS §2.2). Sadzimy i w treści JSX, i w etykiecie w stringu, bo
  # check może skanować tylko jedno z dwóch.
  plant_new src/_guard_vocabulary.tsx <<'EOF'
export const guardLabel = 'Open the ledger';

export function GuardPanel() {
  return <button title="policy kernel">Claim this work item</button>;
}
EOF
}

guard_quick_boundary() {
  # checks/quick-boundary.sh egzekwuje TRZY granice naraz (niezmienniki 1, 2 i 3),
  # więc guard sadzi po jednym naruszeniu każdej. Jedno by wystarczyło do czerwonego
  # i dokładnie dlatego nie wystarcza: check, który stracił dwie reguły z trzech,
  # dalej byłby czerwony i dalej czytałby się jak zdrowy.
  #
  # Katalog engine/ jeszcze nie istnieje — tworzymy go razem z naruszeniem, żeby
  # dowieść, że reguły zapalają się w tej sekundzie, w której pierwszy plik tam trafi.
  #
  # UWAGA: quick-boundary pomija się, dopóki w src-tauri/src/ nie ma ANI JEDNEGO .rs,
  # więc sam plant już un-skipuje check. Nazwy plików nie mogą wpaść w wyjątek dla
  # testów (`*_test.rs`, `*/tests/*`, `fake.rs`), bo check je świadomie ignoruje.
  plant_new src-tauri/src/engine/_guard_tauri.rs <<'EOF'
use tauri::AppHandle;

pub fn _guard(_h: &AppHandle) {}
EOF
  plant_new src-tauri/src/engine/_guard_platform.rs <<'EOF'
#[cfg(windows)]
pub fn _guard() {}
EOF
  plant_new src-tauri/src/store/_guard_writer.rs <<'EOF'
pub fn _guard(c: &rusqlite::Connection) {
    c.execute("INSERT INTO steps (id) VALUES (1)", []).ok();
}
EOF
}

guard_quick_suppressions() {
  # Jedna linia wewnątrz OWNS wyłącza bramkę typów. Plant musi być w src/, bo tam check patrzy,
  # i musi być w PLIKU NOWYM — modyfikacja istniejącego .ts nie istnieje, bo src/ ma dziś
  # wyłącznie theme.css.
  plant_new src/_guard_suppression.ts <<'EOF'
// @ts-nocheck
export const guardValue: number = "not a number";
EOF
}

guard_quick_permissions() {
  # Nie da się tego zasadzić przez plant_append: doklejenie czegokolwiek do JSON-a robi
  # z niego śmieć, a wtedy check woła exit 2 („nasza konfiguracja"), nie exit 1 („czerwone").
  # Guard musi wywołać CZERWONE, więc podmieniamy treść na poprawny JSON ze złą regułą —
  # dokładnie tą, która 2026-08-15 kosztowała 55 tur i $6,98.
  python3 - <<'PY'
import json
p = '.claude/settings.json'
cfg = json.load(open(p))
cfg['permissions']['deny'].insert(0, 'Write(~/**)')
json.dump(cfg, open(p, 'w'), indent=2)
PY
  PLANTED_MOD=( ${PLANTED_MOD[@]+"${PLANTED_MOD[@]}"} .claude/settings.json )
}

guard_quick_tokens() {
  # Obie połowy checks/quick-tokens.sh. Połowa 2 (literał w komponencie) jest tą,
  # która dziś jeszcze nic nie ogląda — src/ nie ma ani jednego .tsx — więc plant
  # jest jedynym dowodem, że w ogóle strzela.
  plant_new src/_guard_tokens.tsx <<'EOF'
export const guardStyle = { color: '#5c7a8a', fontSize: 13 };
EOF
}

guard_full_test() {
  local planted=1 root
  if [ -d node_modules ]; then
    plant_new src/_guard.test.ts <<'EOF'
import { expect, test } from 'vitest';

test('planted by harness/guards.sh', () => {
  expect(1).toBe(2);
});
EOF
    planted=0
  fi
  if root="$(crate_root)" && command -v cargo >/dev/null 2>&1; then
    plant_append "$root" <<'EOF'

#[cfg(test)]
mod _guard_tests {
    #[test]
    fn planted_by_guards_sh() {
        assert_eq!(1, 2);
    }
}
EOF
    planted=0
  fi
  if [ "$planted" -ne 0 ]; then
    NA_REASON="neither vitest nor cargo has anything to run here yet"
    return 1
  fi
}

# ── pętla ────────────────────────────────────────────────────────────────────

fired=0; missed=0; no_guard=0; na=0
NA_LINES=()
say_fail() { printf '  %-44s %s\n' "$1" "$2" >&2; }

for script in "${CHECKS[@]}"; do
  id="$(basename "$script" .sh)"
  fn="guard_$(printf '%s' "$id" | tr '-' '_')"

  # Check, który sam odpala ten plik, nie da się nim sprawdzić — zapętliłby się.
  #
  # Szukamy WYWOŁANIA, nie wzmianki. Pierwsza wersja szukała samego "guards.sh"
  # i wybaczyła checkowi zakresu, który ma tę ścieżkę na liście dozwolonych —
  # czyli dokładnie ten cichy skip, którego ten plik zabrania. Komentarze
  # obcinamy z tego samego powodu.
  if sed 's/#.*//' "$script" | grep -Eq '(^|[[:space:];&|(])((exec[[:space:]]+)?(bash|sh|zsh)[[:space:]]+[^[:space:];&|)]*guards\.sh|(exec[[:space:]]+)?"?(\.|\$\{?[A-Za-z_][A-Za-z_0-9]*\}?)/[^[:space:];&|)"]*guards\.sh)'; then
    na=$((na + 1))
    NA_LINES=( ${NA_LINES[@]+"${NA_LINES[@]}"} "$id: it runs guards.sh itself (guarding it would recurse)" )
    printf '  %-44s NOT APPLICABLE (runs guards.sh)\n' "$id"
    continue
  fi

  if ! declare -F "$fn" >/dev/null; then
    no_guard=$((no_guard + 1))
    say_fail "$id" "NO GUARD -- add $fn() to harness/guards.sh"
    continue
  fi

  NA_REASON=""
  if ! "$fn"; then
    restore
    na=$((na + 1))
    NA_LINES=( ${NA_LINES[@]+"${NA_LINES[@]}"} "$id: ${NA_REASON:-unstated precondition}" )
    printf '  %-44s NOT APPLICABLE (%s)\n' "$id" "${NA_REASON:-unstated precondition}"
    continue
  fi

  rc_planted=0; run_check "$script" || rc_planted=$?
  cp "$scratch/out" "$scratch/out.planted"

  restore

  # Zasadzone naruszenie, które przetrwało przywracanie, jest gorsze niż czerwony
  # check: zostaje w repo i przewraca cudzy bieg pół godziny później. Sprawdzamy
  # to PRZED przebiegiem na czysto, bo inaczej ten przebieg mierzy brudne drzewo.
  if [ -n "$(git status --porcelain -uall)" ]; then
    echo "guards left the tree dirty after $id -- restore failed, stopping here:" >&2
    git status --porcelain -uall | sed 's/^/    /' >&2
    exit 2
  fi

  rc_clean=0; run_check "$script" || rc_clean=$?

  # Klasyfikacja JEDEN raz. Check czerwony także bez naruszenia liczył się kiedyś
  # naraz jako "wystrzelił" i jako "spudłował" — dwa wiersze o jednym zdarzeniu,
  # z których jeden był nieprawdą.
  if [ "$rc_clean" -ne 0 ]; then
    missed=$((missed + 1))
    say_fail "$id" "RED WITH THE VIOLATION GONE (exit $rc_clean) -- the guard proves nothing"
    head -5 "$scratch/out" | sed 's/^/      /' >&2
  elif [ "$rc_planted" -eq 0 ]; then
    missed=$((missed + 1))
    say_fail "$id" "DID NOT FIRE -- a real violation was planted and it exited 0"
    head -5 "$scratch/out.planted" | sed 's/^/      /' >&2
  else
    fired=$((fired + 1))
  fi
done

[ -n "$TIMEOUT_BIN" ] || echo "note: no timeout(1) here, checks ran without a ceiling" >&2

if [ "$na" -gt 0 ]; then
  echo "not applicable, and why:" >&2
  for line in ${NA_LINES[@]+"${NA_LINES[@]}"}; do echo "  $line" >&2; done
fi

printf 'guards: %d fired as expected, %d misfired, %d without a guard, %d not applicable\n' \
  "$fired" "$missed" "$no_guard" "$na"

if [ "$missed" -gt 0 ] || [ "$no_guard" -gt 0 ]; then
  exit 1
fi
exit 0
