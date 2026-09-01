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
# 2026-08-28: dopisane `.claude/settings.local.json` i `.claude/worktrees/`. To sa pliki
# MASZYNY, nie pracy -- oba sa w .gitignore (linie 44 i 49), ale to sprawdzenie CELOWO omija
# .gitignore (`core.excludesFile=/dev/null`, `--ignored=matching`), zeby bieg nie mogl ukryc
# zapisu poza zakresem, dopisujac sciezke do .gitignore. Wlasna lista jest wiec jedynym
# miejscem, gdzie takie wyjatki maja prawo stac -- i tych dwoch na niej brakowalo.
#
# Co to kosztowalo, zmierzone dzisiaj: `harness/guards.sh` meldowal
# `quick-scope RED WITH THE VIOLATION GONE (exit 1) -- the guard proves nothing`, bo po
# przywroceniu drzewa check nadal swiecil na tych dwoch plikach. Straznik mierzyl wiec stan
# maszyny, nie regule, a `scripts/ci.sh full` nie mial jak byc zielony na zadnym laptopie,
# na ktorym te pliki istnieja. Na CI ich nie ma, wiec objaw byl WYLACZNIE lokalny -- czyli
# najgorszy rodzaj: „u mnie czerwone, na CI zielone".
#
# NIE wykluczamy calego `.claude/`. settings.json, hooks/ i commands/ SA praca i zostaja
# pod `DENIED` nizej -- bieg nie ma prawa ich tknac.
GENERATED='^(target|src-tauri/target|src-tauri/gen|node_modules|dist|\.vite|runs|test-results|playwright-report|\.playwright|\.loadout/scratch|\.loadout/runs|\.git|\.idea|\.vscode|coverage)(/|$)|^\.claude/(settings\.local\.json$|worktrees(/|$))|^\.loadout/loadout\.db|(\.tsbuildinfo|\.DS_Store|\.log|\.jsonl)$|^\.port$'

# ── Co wolno dotknąć, kiedy zadanie nie deklaruje własności (bieg ręczny, bez TASK.md). ────
ALLOWED='^(src/|src-tauri/src/|src-tauri/capabilities/|src-tauri/icons/|src-tauri/tauri\.conf\.json$|docs/|\.loadout/)|^(README\.md|TASK\.md)$'

# ── Czego nie wolno NIGDY, chyba że człowiek wpisał to wprost do bloku OWNS. ───────────────
# AGENTS.md §7: dotknięcie harnessu, sprawdzeń, verify.sh albo zablokowanych decyzji to
# moment na zatrzymanie się i zapytanie człowieka — nie na cichy commit.
DENIED='^(harness/|checks/|scripts/|verify\.sh$|worktree\.sh$|review\.sh$|ship\.sh$|integrate\.sh$|AGENTS\.md$|docs/DECISIONS-LOCKED\.md$|Cargo\.toml$|Cargo\.lock$|package\.json$|package-lock\.json$|tsconfig\.json$|vite\.config\.ts$|rust-toolchain\.toml$|\.gitignore$|\.claude/)'

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
# Zakres jest własnością GAŁĘZI, nie drzewa. Na trunku pytanie „czy to zadanie pisało poza
# swoim pasem" jest już odpowiedziane — odpowiedziała na nie bramka gałęzi, zanim cokolwiek
# zostało zmergowane. Zmierzone przy pierwszym uruchomieniu integrate.sh (2026-08-15): po
# merge'u task-S-1 `kontrakt..HEAD` obejmował także commity harnessu, które wylądowały na main
# PO commicie kontraktowym, więc blok OWNS jednego zadania oskarżał cudzą pracę. Czerwone
# było fałszywe, a integrate.sh słusznie zatrzymał landowanie na fałszywym czerwonym.
TRUNK="${LOADOUT_TRUNK:-main}"
if [ "$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '')" = "$TRUNK" ]; then
  echo "scope: on $TRUNK — scope is a branch property, and the branch gate already enforced it"
  exit 0
fi

# Baza to PUNKT ODGAŁĘZIENIA od trunka, nie commit kontraktowy. Ta sama klasa, którą opisuje
# akapit wyżej, ma bowiem drugie wejście, otwarte 2026-08-15 przez podciąganie trunka przed
# rundą naprawczą: kiedy main jest wmergowany W GAŁĄŹ, `kontrakt..HEAD` znowu obejmuje cudze
# commity — tym razem nie dlatego, że stoimy na trunku, tylko dlatego, że trunk stoi w nas.
# Zmierzone na T-04: siedemnaście plików harnessu i zadań oskarżonych o „zapis poza zakresem",
# z których żadnego nie tknął ani pisarz, ani naprawiacz.
#
# `merge-base` odpowiada na właściwe pytanie — **co ta gałąź dopisała** — i odpowiada tak samo
# przed merge'em (baza = punkt cięcia) i po nim (baza = wierzchołek trunka, bo trunk jest już
# przodkiem). Commit kontraktowy zostaje jako zapasowa baza dla gałęzi bez trunka; samo TASK.md
# ma niżej własną regułę, więc pojawienie się go w diffie niczego nie psuje.
TRUNK_REF=""
for cand in "$TRUNK" "refs/heads/$TRUNK" "origin/$TRUNK"; do
  git rev-parse --verify -q "$cand^{commit}" >/dev/null 2>&1 && { TRUNK_REF="$cand"; break; }
done

base=""
[ -n "$TRUNK_REF" ] && base="$(git merge-base HEAD "$TRUNK_REF" 2>/dev/null || true)"
basis_kind="the branch point off $TRUNK"
if [ -z "$base" ] && [ -f TASK.md ]; then
  base="$(git log --diff-filter=A --format=%H -- TASK.md 2>/dev/null | head -1 || true)"
  basis_kind="the contract commit (no $TRUNK to branch from)"
fi
committed=""
basis="uncommitted work only (no baseline found)"
if [ -n "$base" ] && git rev-parse --verify -q "$base^{commit}" >/dev/null 2>&1; then
  committed="$(git diff --name-only --no-renames "$base"..HEAD 2>/dev/null || true)"
  basis="everything this branch added since $basis_kind ${base:0:8}"
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
  # `sed '$d'` kasowalo CALA ostatnia linie -- a gdy terminator jest sklejony ze sciezka
  # (`...cancel.rs-->`, tak konczy 42 z 60 plikow zadan), ginela razem z nim ostatnia
  # pozycja OWNS. Zadanie traci prawo do pliku, ktorego wymaga jego wlasne `check:`,
  # i nie ma ruchu, ktory to zamyka. Zmierzone 2026-08-19 na T-10 AC-6.
  # Teraz terminator jest OBCINANY, nie kasowany z wierszem; linia zlozona z samego `-->`
  # robi sie pusta i odpada na warunku [ -n "$line" ] wyzej.
  done < <(sed -n '/<!--[[:space:]]*OWNS/,/-->/p' TASK.md | sed '1d' | sed 's/-->.*$//' || true)
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

  # Kontrakt jest tylko do czytania — a od 2026-08-28 "zmieniony" znaczy "rozni sie od
  # wersji z commita, ktory TASK.md DODAL", czyli od commita etapu planu.
  #
  # Do tego dnia baza byla `tasks/<ID>.md`: kontrakt pisal czlowiek przed biegiem, a
  # ship-task.sh tylko go kopiowal. Ten katalog zniknal razem ze starym harnessem, wiec
  # porownanie nie mialo z czym porownywac i KAZDY bieg zapalal sie tutaj na TASK.md,
  # ktory sam wlasnie wygenerowal.
  #
  # Nowa baza jest scislejsza od starej, i to jest zamierzone. Stara wybaczala zmiane
  # kontraktu w trakcie biegu, jesli tylko orchestrator zmienil tez plik zadania. Teraz
  # nie wybacza nic: kryterium dopisane albo rozluznione PO etapie planu jest czerwone,
  # bo bieg nie moze zmieniac warunkow wlasnego zaliczenia. Rozjazd lapie tez gate.py
  # (ta sama regula, kod 2, bo to defekt kontraktu, nie kodu).
  if [ "$p" = "TASK.md" ]; then
    add="$(git log --diff-filter=A --format=%H -- TASK.md 2>/dev/null | head -1 || true)"
    if [ -n "$add" ] && git show "$add:TASK.md" 2>/dev/null | cmp -s - TASK.md; then
      continue
    fi
    violations+="  $p — the contract changed after the plan commit; OWNS and the criteria are read-only"$'\n'
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
    violations+="  $p — outside src/, src-tauri/src/, docs/ and .loadout/"$'\n'
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
