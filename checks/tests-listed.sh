#!/usr/bin/env bash
# ~0,05 s, bez kompilacji. Plik testowy, którego nikt nie zadeklarował, nie uruchamia niczego.
#
# INCYDENT, PO KTÓRYM TO POWSTAŁO — i jest to incydent tego samego dnia, co scalenie.
# 2026-08-17 `src-tauri/tests/` miało 122 pliki, a Rust robi z każdego pliku w `tests/` OSOBNE
# binarium linkujące całą bibliotekę z 527 skrzyniami. Same testy trwały 6,0 s, składanie tych
# programów — godziny. Pliki są teraz modułami JEDNEGO celu (`tests/it/main.rs`).
#
# Ta zmiana wprowadziła nową, cichą awarię i nagłówek `main.rs` obiecał strażnika, którego
# w tej samej godzinie nie napisano: plik leżący w `tests/it/` bez linii `mod` w `main.rs`
# **nie jest częścią żadnego celu**. Nie kompiluje się, nie uruchamia ani jednego testu i nie
# mówi o tym ani słowa. Na ekranie bramki wygląda identycznie jak zestaw, który przeszedł —
# a przed 2026-08-17 ten sam plik byłby własnym celem i biegł sam z siebie.
#
# To jest dokładnie kształt z niezmienników 19 i 20: „czysty przebieg, który nic nie zmierzył".
# Dlatego strażnik jest MECHANICZNY (porównanie dwóch list) i nie wymaga kompilacji — działa
# także wtedy, gdy drzewo nie da się zbudować, czyli wtedy, kiedy jest najbardziej potrzebny.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

DIR="src-tauri/tests/it"
MAIN="$DIR/main.rs"

# Brak katalogu to nie jest awaria: repo bez testów integracyjnych jest możliwe (i było takie
# przed T-01). Cisza jest tu poprawną odpowiedzią, a nie pominięciem sprawdzenia.
if [ ! -d "$DIR" ]; then
  echo "tests-listed: no $DIR yet, nothing to declare"
  exit 0
fi

# Katalog Z plikami, ale BEZ `main.rs`, to inna sprawa: wtedy każdy plik w środku jest martwy,
# bo podkatalog `tests/` nie jest celem sam z siebie. To awaria naszego układu, nie kodu.
if [ ! -f "$MAIN" ]; then
  echo "$DIR exists but has no main.rs, so nothing in it is a test target at all" >&2
  echo "detail: a subdirectory of tests/ is only compiled through its own main.rs" >&2
  exit 2
fi

# Pliki, które MAJĄ być modułami. `main.rs` jest celem, nie modułem.
on_disk="$(find "$DIR" -maxdepth 1 -name '*.rs' ! -name 'main.rs' -exec basename {} .rs \; | sort)"

# Deklaracje `mod x;` z `main.rs`. `[[:space:]]*` na początku, bo moduł wolno wciąć; średnik
# na końcu, żeby `mod x { … }` w środku pliku nie udawał deklaracji pliku.
declared="$(grep -oE '^[[:space:]]*mod[[:space:]]+[a-z_][a-z0-9_]*[[:space:]]*;' "$MAIN" \
            | sed -E 's/^[[:space:]]*mod[[:space:]]+//; s/[[:space:]]*;$//' | sort)"

orphans="$(comm -23 <(printf '%s\n' "$on_disk") <(printf '%s\n' "$declared") || true)"
ghosts="$(comm -13 <(printf '%s\n' "$on_disk") <(printf '%s\n' "$declared") || true)"

bad=0
if [ -n "${orphans//[$'\n' ]/}" ]; then
  echo "these test files are not declared in $MAIN, so they run NOTHING:" >&2
  printf '%s\n' "$orphans" | sed '/^$/d;s|^|  src-tauri/tests/it/|;s|$|.rs|' >&2
  echo >&2
  echo "A file under tests/it/ is a MODULE of one target, not a target of its own. Without a" >&2
  echo "\`mod <name>;\` line in main.rs it is never compiled and never run — and an absent test" >&2
  echo "reads exactly like a passing one. Add the line in the same commit as the file." >&2
  bad=1
fi

if [ -n "${ghosts//[$'\n' ]/}" ]; then
  echo "these modules are declared in $MAIN but the file is gone:" >&2
  printf '%s\n' "$ghosts" | sed '/^$/d;s/^/  mod /;s/$/;/' >&2
  echo "The target will not compile. Remove the declaration together with the file." >&2
  bad=1
fi

# ── DRUGA POŁOWA: co wolno stać WPROST w `src-tauri/tests/` ──────────────────
#
# 2026-09-02 (R-5/H-5). Do tego dnia ten strażnik pytał wyłącznie „czy każdy plik w `tests/it/`
# ma swój `mod`" i nie miał pojęcia o plikach na najwyższym poziomie. A to tam mieszkał koszt:
# 60 plików = 60 osobnych binariów, każde linkujące całą bibliotekę z 527 skrzyniami Tauri
# (~60 s za sztukę), przy 6 s wykonania wszystkich testów razem. Zadanie Z-28 zeszło z 60 do 8;
# bez tej listy 61. plik wróciłby po cichu, bo AGENTS.md §2a jest regułą w prozie, a proza
# nie świeci na czerwono.
#
# Ośmiu wyjątków nie da się scalić i każdy ma inny powód:
#   * pięć `flow_*` i `t149_phase7_paid_live_oracle` uruchamiają PRAWDZIWYCH agentów — są
#     `#[ignore]`, płatne, i wołane wprost, nie przez suitę;
#   * `shell_logging` liczy deskryptory przez `/dev/fd` i instaluje globalny hak paniki,
#     a `supervisor_env_hygiene` woła `env::set_var` — oba mierzą stan CAŁEGO PROCESU, więc
#     w scalonym binarium mierzyłyby cudze testy (`shell_logging` dostał 96 zamiast swojej
#     liczby przy pierwszym lądowaniu po scaleniu 2026-08-17).
ALLOWED_TOP="flow_fan_in flow_lead_agent_chat flow_say_to_agent flow_skill flow_todo_app \
t149_phase7_paid_live_oracle shell_logging supervisor_env_hygiene"

top_on_disk="$(find src-tauri/tests -maxdepth 1 -name '*.rs' -exec basename {} .rs \; | sort)"
allowed_sorted="$(printf '%s\n' $ALLOWED_TOP | sort)"
extra="$(comm -23 <(printf '%s\n' "$top_on_disk") <(printf '%s\n' "$allowed_sorted") || true)"
gone="$(comm -13 <(printf '%s\n' "$top_on_disk") <(printf '%s\n' "$allowed_sorted") || true)"

if [ -n "${extra//[$'\n' ]/}" ]; then
  echo "these files sit directly in src-tauri/tests/, so each is its OWN test binary:" >&2
  printf '%s\n' "$extra" | sed '/^$/d;s|^|  src-tauri/tests/|;s|$|.rs|' >&2
  echo >&2
  echo "Every file there links the whole library with 527 Tauri crates -- about 60 s each," >&2
  echo "against 6 s to run all the tests together (AGENTS.md 2a). Make it a module of the one" >&2
  echo "integration target instead: src-tauri/tests/it/<name>.rs plus \`mod <name>;\` in main.rs." >&2
  echo "If it truly cannot be merged -- it measures whole-process state, or it costs money --" >&2
  echo "say so and add it to ALLOWED_TOP in this file, with the reason." >&2
  bad=1
fi

if [ -n "${gone//[$'\n' ]/}" ]; then
  echo "ALLOWED_TOP names files that are not there any more:" >&2
  printf '%s\n' "$gone" | sed '/^$/d;s|^|  src-tauri/tests/|;s|$|.rs|' >&2
  echo "An exception without a file is an exception nobody can read. Remove it." >&2
  bad=1
fi

[ "$bad" = 0 ] || exit 1

echo "tests-listed: $(printf '%s\n' "$on_disk" | grep -c . || true) modules declared in main.rs, $(printf '%s\n' "$top_on_disk" | grep -c . || true) standalone targets (all on the allowlist)"
