#!/usr/bin/env bash
# ~0.05 s — jedyna obrona przed zapisem poza zakresem zadania, niezależnie od tego, jakie
# narzędzie ten zapis zrobiło. Nie hook, nie polityka uprawnień, nie prośba w prompcie:
# stan drzewa, odczytany z gita.
#
# Trzy świadome decyzje, każda z powodem:
#
#   `git status --untracked-files=all`, nigdy `git diff --name-only`. diff nie widzi NOWEGO
#   pliku, a świeży src/setupTests.ts albo vitest.config.ts to najtańszy sposób na
#   wykastrowanie całej suity.
#
#   `--ignored=matching` z WYŁĄCZONYM plikiem ignorowania (`core.excludesFile=/dev/null`),
#   żeby dopisanie jednej linii do .gitignore nie mogło wyprać pliku z pola widzenia.
#   Skutek uboczny: widać też zwykłe wyjście builda — dlatego lista ścieżek generowanych
#   mieszka TUTAJ, w pliku, którego bieg nie może edytować, a nie w .gitignore, który może.
#
#   Blok `<!-- OWNS -->` czytamy z TASK.md, ale samo TASK.md w zmienionych plikach jest
#   naruszeniem. Inaczej pętla się domyka: agent dopisuje ścieżkę do własnego kontraktu
#   i zapis staje się legalny. Kontrakt commituje ship-task.sh PRZED uruchomieniem agenta.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

# ── Ścieżki generowane. Ta lista jest częścią sprawdzenia, nie konfiguracją repo. ──────────
# `(/|$)`, nie samo `/`. Zmierzone: worktree.sh podpina `target` jako SYMLINK do wspólnego
# cache'u builda, a `git status --ignored=matching` melduje go wtedy jako `target` — bez
# ukośnika, bo to nie jest katalog. Wzorzec wymagający `target/` go nie łapał i KAŻDY bieg
# w worktree zaczynał się od „a file was written outside this task's scope: target".
# To samo dotyczy sklonowanego node_modules i każdej innej generowanej ścieżki-liścia.
GENERATED='^(target|src-tauri/target|src-tauri/gen|node_modules|dist|\.vite|runs|test-results|playwright-report|\.playwright|\.loadout/runs|\.git|\.idea|\.vscode|coverage)(/|$)|^\.loadout/loadout\.db|(\.tsbuildinfo|\.DS_Store|\.log|\.jsonl)$|^\.port$'

# ── Co wolno dotknąć, kiedy zadanie nie deklaruje własności (bieg ręczny, bez TASK.md). ────
ALLOWED='^(src/|src-tauri/src/|src-tauri/capabilities/|src-tauri/icons/|src-tauri/tauri\.conf\.json$|docs/|\.loadout/)|^(README\.md|TASK\.md)$'

# ── Czego nie wolno NIGDY, chyba że człowiek wpisał to wprost do bloku OWNS. ───────────────
# AGENTS.md §7: dotknięcie harnessu, sprawdzeń, verify.sh albo zablokowanych decyzji to
# moment na zatrzymanie się i zapytanie człowieka — nie na cichy commit.
DENIED='^(harness/|checks/|tasks/|scripts/|verify\.sh$|worktree\.sh$|review\.sh$|repair\.sh$|integrate\.sh$|ship-task\.sh$|AGENTS\.md$|docs/DECISIONS-LOCKED\.md$|Cargo\.toml$|Cargo\.lock$|package\.json$|package-lock\.json$|tsconfig\.json$|vite\.config\.ts$|rust-toolchain\.toml$|\.gitignore$|\.claude/)'

# Repo bez ani jednego commita: nie ma bazy, wobec której "poza zakresem" cokolwiek znaczy —
# `status` melduje wtedy CAŁE drzewo. Warunek jest mechaniczny i sam się kasuje: ship-task.sh
# commituje TASK.md jako pierwszy commit gałęzi, więc w żadnym prawdziwym biegu ta gałąź
# się nie wykona.
if ! git rev-parse --verify -q HEAD >/dev/null 2>&1; then
  echo "scope: the repo has no commits yet, so there is no baseline to compare against"
  exit 0
fi

# ── Baza porównania: commit kontraktowy, NIE tylko HEAD ────────────────────────────────────
# N-06 (audyt 2026-08-15, odtworzone end-to-end przez czterech niezależnych audytorów).
# `git status` porównuje z HEAD, a ship-task.sh woła commit_leftovers BEZPOŚREDNIO PRZED
# obiema bramkami — więc to sprawdzenie, jedyna obrona niezależna od narzędzia, którym zrobiono
# zapis, w każdym prawdziwym biegu drukowało "0 changed paths, all inside this task's OWNS block"
# dokładnie w tej chwili, w której miało coś zobaczyć. Odtworzenie: wypatroszona gate.py,
# skasowany checks/quick-permissions.sh, dopisane fałszywe `## AC-7` do TASK.md, unwrap_used
# przestawiony na "allow" — CZERWONO przed commitem, rc 0 po nim.
#
# Baza to commit, który DODAŁ TASK.md, czyli pierwszy commit gałęzi robiony przez ship-task.sh.
# Ta sama definicja, której używa już review.sh. Fail-safe: gdy bazy nie ma (człowiek pracuje
# poza worktree zadania), zostajemy przy samym `status` i mówimy to wprost — cicho zawężony
# zakres jest gorszy niż jawnie węższy.
base=""
if [ -f TASK.md ]; then
  base="$(git log --diff-filter=A --format=%H -- TASK.md 2>/dev/null | tail -1 || true)"
fi
committed=""
basis="uncommitted work only (no contract commit found)"
if [ -n "$base" ] && git rev-parse --verify -q "$base^{commit}" >/dev/null 2>&1; then
  committed="$(git diff --name-only --no-renames "$base"..HEAD 2>/dev/null || true)"
  basis="everything since the contract commit ${base:0:8}"
fi

changed="$( { git -c core.excludesFile=/dev/null status --porcelain=v1 -uall --ignored=matching \
                | cut -c4- | sed 's/.* -> //'
              printf '%s\n' "$committed"
            } | grep -v '^[[:space:]]*$' | sort -u | grep -vE "$GENERATED" || true)"

# ── Blok OWNS. Jedno źródło własności; prozy pod "## What this task owns" nie czytamy. ─────
owns=()
scope="the allowed tree"
if [ -f TASK.md ]; then
  while IFS= read -r line; do
    line="${line#"${line%%[![:space:]]*}"}"   # ltrim
    line="${line%"${line##*[![:space:]]}"}"   # rtrim
    line="${line%/}"
    [ -n "$line" ] && owns+=("$line")
  done < <(sed -n '/<!--[[:space:]]*OWNS/,/-->/p' TASK.md | sed '1d;$d' || true)
fi
[ "${#owns[@]}" -gt 0 ] && scope="this task's OWNS block (${#owns[@]} paths)"

owned() {
  local p="$1" e
  for e in ${owns[@]+"${owns[@]}"}; do
    case "$p" in "$e" | "$e"/*) return 0 ;; esac
  done
  return 1
}

violations=""
n=0
while IFS= read -r p; do
  [ -z "$p" ] && continue
  n=$((n + 1))

  # Kontrakt zadania jest tylko do czytania. Zmiana TASK.md to zmiana warunków zaliczenia.
  if [ "$p" = "TASK.md" ]; then
    violations+="  $p — the task contract was modified; OWNS and the criteria are read-only"$'\n'
    continue
  fi

  if printf '%s' "$p" | grep -qE "$DENIED"; then
    if owned "$p"; then continue; fi
    violations+="  $p — harness, checks or config; stop and ask a human (AGENTS.md §7)"$'\n'
    continue
  fi

  if [ "${#owns[@]}" -gt 0 ]; then
    if owned "$p"; then continue; fi
    violations+="  $p — outside the <!-- OWNS --> block of TASK.md"$'\n'
    continue
  fi

  if ! printf '%s' "$p" | grep -qE "$ALLOWED"; then
    violations+="  $p — outside src/, src-tauri/src/, docs/ and tasks/"$'\n'
  fi
done <<< "$changed"

if [ -n "$violations" ]; then
  echo "a file was written outside this task's scope" >&2
  printf '%s' "$violations" >&2
  echo "detail: compared against $basis" >&2
  echo "detail: scope is $scope; generated paths are excluded by this file, not by .gitignore" >&2
  exit 1
fi

s="s"
if [ "$n" -eq 1 ]; then s=""; fi
echo "scope: $n changed path$s, all inside $scope · basis: $basis"
