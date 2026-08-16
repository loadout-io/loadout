#!/usr/bin/env bash
# Obie suity. Tylko w pełnej bramce — `cargo test` linkuje, a linkowanie w pętli
# wewnętrznej to minuty, nie sekundy.
#
# `--tests`, NIGDY `--lib`. Do 2026-08-16 stało tu `cargo test --lib`, a `--lib` nie kompiluje
# `src-tauri/tests/*.rs` ANI RAZU. Każde kryterium akceptacji w tym repo ma postać
# `cargo test --test <cel>` i mieszka właśnie tam: pełna bramka nie odpalała celów, na których
# stoi cała wyrocznia projektu. Test integracyjny mógł być czerwony od tygodnia, a
# `./verify.sh full` mówił "zielono".
#
# Dlaczego `--tests`, a nie gołe `cargo test`: gołe dokłada doctesty, które przy zerze
# przykładów drukują własne `test result: ok. 0 passed` — czyli linię nie do odróżnienia od
# celu, który nic nie uruchomił. `--tests` bierze lib, biny i KAŻDY cel z tests/, bez doctestów.
#
# NIEZMIENNIK 19: kod wyjścia nie jest dowodem. Kod testowany biegnie w tym samym procesie,
# którego kod wyjścia czytasz — `os._exit(0)` na poziomie modułu zazielenia całą suitę,
# a filtr, który nie dopasował niczego, kończy się zerem. Dlatego to sprawdzenie:
#   - wypisuje LICZBĘ przejść w linii podsumowania (regułę dowodu czyta bramka),
#   - samo się przewraca, kiedy testy ISTNIEJĄ, a runner melduje zero przejść.
#
# Uwaga na vitest: "Test Files 1 passed (1)" potrafi stać NAD "Tests 4 skipped (4)".
# Zliczamy z linii `Tests`, nigdy z linii `Test Files` — pomyłka między nimi raz zaraportowała
# przejście dla biegu, w którym nie wykonało się nic (raport 06 §3).
#
# Wybór wobec pustego drzewa: brak jakiegokolwiek testu -> mówimy to jednym zdaniem i
# przechodzimy, BEZ zmyślonego licznika. Zieleń nie jest tu pusta w praktyce, bo bramka i tak
# kończy się kodem 2, kiedy TASK.md nie deklaruje kryteriów ("this gate can only report on
# itself"). Pierwszy #[test] albo pierwszy *.test.tsx włącza egzekucję z powrotem.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"
# shellcheck source=checks/_cargo-serialize.sh
. checks/_cargo-serialize.sh

summary=""

# ── Rust ───────────────────────────────────────────────────────────────────────────────────
rs="$(find src-tauri/src -name '*.rs' 2>/dev/null | head -1 || true)"
if [ -n "$rs" ]; then
  command -v cargo >/dev/null 2>&1 || { echo "cargo is not on PATH" >&2; exit 2; }
  has_tests="$(grep -rlE '^\s*#\[(tokio::)?test\]' src-tauri/src 2>/dev/null | head -1 || true)"
  cargo_serialize || exit 2   # 2, nie 1: nic sie nie wykonalo, wiec to nie jest twierdzenie o kodzie (Q-3)
  rc=0
  out="$(cargo test --tests 2>&1)" || rc=$?
  cargo_release
  # `cargo test` drukuje jedną linię "test result:" na cel testowy. Sumujemy wszystkie.
  passed="$(printf '%s\n' "$out" | grep -oE 'test result: ok\. [0-9]+ passed' \
            | grep -oE '[0-9]+' | awk '{s+=$1} END {print s+0}')"
  if [ "$rc" -ne 0 ]; then
    echo "the Rust suite failed" >&2
    printf '%s\n' "$out" | grep -vE '^\s*(Compiling|Downloading|Updating|Fresh|Blocking)' \
      | tail -40 >&2
    exit 1
  fi
  # NIEZMIENNIK 19 od strony POJEDYNCZEGO CELU. Suma kłamie przez uśrednienie: cel, który
  # zameldował zero przejść, znika pod jedynką z lib-a i całość czyta się jak sukces.
  # Cel PARUJEMY z jego linią `Running <ścieżka>` — to cargo mówi, co uruchomił, więc reguła
  # nie zna żadnego układu katalogów poza tym, który cargo sam wypisał. Bin bez testów
  # jednostkowych (nasz main.rs ma cztery linie) jest normalny i celowo tu nie wchodzi:
  # sądzimy wyłącznie cele z tests/, bo plik położony tam istnieje po to, żeby coś dowieść.
  empty="$(printf '%s\n' "$out" | awk '
      /^[[:space:]]+Running / { target = ($2 == "unittests") ? $3 : $2; next }
      /^test result:/ {
        if (target ~ /^tests\// && index($0, "ok. 0 passed") > 0) print target
        target = ""
      }' | head -5)"
  if [ -n "$empty" ]; then
    echo "an integration test target ran and reported no passing tests" >&2
    printf '%s\n' "$empty" | sed 's/^/  /' >&2
    echo "detail: the target compiled and exited 0 with zero #[test] executed — a filter," >&2
    echo "detail: a cfg, or a file that declares nothing. Exit 0 is not evidence (inv. 19)." >&2
    exit 1
  fi
  if [ -n "$has_tests" ] && [ "${passed:-0}" -eq 0 ]; then
    echo "cargo test exited 0 and reports no passing tests" >&2
    echo "detail: #[test] exists in src-tauri/src but nothing ran — a filter, a cfg or a" >&2
    echo "detail: module that is not declared. Exit 0 is not evidence (invariant 19)." >&2
    printf '%s\n' "$out" | tail -20 >&2
    exit 1
  fi
  summary="${passed:-0} rust"
fi

# ── Frontend ───────────────────────────────────────────────────────────────────────────────
spec="$(find src -type f \( -name '*.test.ts' -o -name '*.test.tsx' \
        -o -name '*.spec.ts' -o -name '*.spec.tsx' \) 2>/dev/null | head -1 || true)"
if [ -n "$spec" ]; then
  if [ -x node_modules/.bin/vitest ]; then
    VITEST=(node_modules/.bin/vitest)
  else
    echo "vitest is not installed: run \`npm install\`" >&2
    exit 2
  fi
  rc=0
  out="$("${VITEST[@]}" run --reporter=default 2>&1)" || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "the frontend suite failed" >&2
    printf '%s\n' "$out" | tail -40 >&2
    exit 1
  fi
  # WYŁĄCZNIE linia `Tests`. Linia `Test Files` mówi o plikach i potrafi meldować przejście
  # nad suitą, w której każdy test został pominięty.
  line="$(printf '%s\n' "$out" | grep -E '^\s*Tests\s' | tail -1 || true)"
  web="$(printf '%s' "$line" | grep -oE '[0-9]+ passed' | grep -oE '[0-9]+' | head -1 || true)"
  if [ "${web:-0}" -eq 0 ]; then
    echo "vitest exited 0 and reports no passing tests" >&2
    echo "detail: test files exist under src/ but the Tests line says none passed." >&2
    echo "detail: ${line:-<no Tests line at all>}" >&2
    exit 1
  fi
  summary="${summary:+$summary + }$web web"
fi

if [ -z "$summary" ]; then
  echo "test: nothing to test yet (no #[test] in src-tauri/src, no *.test.* under src/)"
  exit 0
fi
echo "test: $summary tests passed"
