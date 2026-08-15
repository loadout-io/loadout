#!/usr/bin/env bash
# build-loop.sh — przejedź zadania po kolei zależności, zatrzymaj się na pierwszym czerwonym.
#
#   ./scripts/build-loop.sh                  # od początku
#   ./scripts/build-loop.sh --from T-05      # wznów od zadania
#   ./scripts/build-loop.sh --only T-01      # jedno zadanie i koniec
#   ./scripts/build-loop.sh --dry-run        # wypisz plan, nic nie odpalaj
#
# Trzy rzeczy, które ten skrypt robi i które trzeba rozumieć:
#
# 1. LĄDUJE PO KAŻDYM ZIELONYM. To nie jest wygoda — T-02 potrzebuje `pub mod engine;`
#    w lib.rs, który tworzy T-01. Bez lądowania każde kolejne zadanie startuje z trunka
#    sprzed wszystkich poprzednich i łańcuch zależności się rozpada.
#
# 2. ZATRZYMUJE SIĘ NA PIERWSZYM CZERWONYM i zostawia worktree. Bieg agentów jest drogi;
#    ciągnięcie dalej po tym, jak podstawa jest zepsuta, mnoży koszt przez liczbę zadań,
#    które i tak trzeba będzie powtórzyć.
#
# 3. Jest IDEMPOTENTNY. Zadanie, którego gałąź już jest w trunku, jest pomijane, więc
#    ponowne uruchomienie po naprawie kontynuuje zamiast zaczynać od zera.
set -euo pipefail

# Bash czyta ten plik PRZYROSTOWO, po offsetach bajtowych. Edycja w trakcie biegu przesuwa
# wszystko za kursorem i proces wykonuje smieci -- skladniowo poprawne, semantycznie losowe.
# Zdarzylo sie trzy razy 2026-08-15, za kazdym razem po moim wlasnym ostrzezeniu, i za
# kazdym razem kosztowalo diagnostyke "czy ten bieg jeszcze jest wazny".
# Kopia jest niezmienna, wiec orchestrator moze naprawiac harness, kiedy petla chodzi.
# Katalog skryptu liczony PRZED exec: w kopii $0 wskazuje na mktemp, a nie na repo.
# Sentinel WŁASNY dla tego skryptu — patrz ten sam komentarz w ship-task.sh. Wspólna nazwa
# wyciekała do dziecka i wyłączała mu przypięcie.
if [ -z "${LOADOUT_PINNED_BUILD_LOOP:-}" ]; then
  LOADOUT_SELF_BUILD_LOOP="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
  LOADOUT_SNAP="$(mktemp -t build-loop)"
  cat "${BASH_SOURCE[0]}" > "$LOADOUT_SNAP"
  export LOADOUT_PINNED_BUILD_LOOP=1 LOADOUT_SELF_BUILD_LOOP
  exec bash "$LOADOUT_SNAP" "$@"
fi

SELF_DIR="${LOADOUT_SELF_BUILD_LOOP:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)}"
unset LOADOUT_PINNED_BUILD_LOOP LOADOUT_SELF_BUILD_LOOP

cd "$SELF_DIR/.."
ROOT="$PWD"
LOG="$ROOT/runs/build-loop.tsv"

# Kolejność zależności z docs/PLAN.md §2-§6. T-24 po T-21, bo od niego zależy.
# S-3 i T-10 są POMINIĘTE: potrzebują Codeksa, a konto jest bez kredytów do 2026-08-20
# (T1 ryzyko 8). Wracają osobnym przebiegiem `--only`.
TASKS=(
  S-1 S-2
  T-01 T-02 T-03 T-04 T-05 T-06 T-07 T-08 T-09
  T-11 T-12 T-13 T-14 T-15
  T-16 T-17 T-18 T-19
  T-20 T-21 T-24 T-22 T-23
)
BLOCKED="S-3 T-10"

AGENT="${LOADOUT_AGENT:-claude}"
REVIEWER="${LOADOUT_REVIEWER:-codex}"
FROM=""; ONLY=""; DRY=0

while [ $# -gt 0 ]; do
  case "$1" in
    --from) FROM="$2"; shift 2 ;;
    --only) ONLY="$2"; shift 2 ;;
    --agent) AGENT="$2"; shift 2 ;;
    --reviewer) REVIEWER="$2"; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
done

[ -f ship-task.sh ] || { echo "run me from the repo root" >&2; exit 2; }
[ -n "$(git status --porcelain)" ] && { echo "the tree is dirty — commit first, the loop lands branches into it" >&2; exit 2; }

# Wyladowane = galaz jest przodkiem HEAD ALBO log mowi, ze przeszlo. Sama ancestralnosc
# nie wystarcza: integrate.sh landuje, a operator kasuje galaz — i przy nastepnym restarcie
# petla planowala zadanie, ktore juz stoi w trunku. Zmierzone przy wznowieniu po S-2.
landed() {
  git merge-base --is-ancestor "refs/heads/task-$1" HEAD 2>/dev/null && return 0
  [ -f "$LOG" ] && grep -qE "^$1"$'\t'"green" "$LOG" && return 0
  return 1
}

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }

[ -f "$LOG" ] || printf 'task\tstatus\tseconds\tcost_usd\tat\n' > "$LOG"

started_all=$(date +%s)
planned=(); skipped=()
for t in "${TASKS[@]}"; do
  [ -n "$ONLY" ] && [ "$t" != "$ONLY" ] && continue
  if [ -n "$FROM" ] && [ -z "${reached:-}" ]; then
    [ "$t" = "$FROM" ] && reached=1 || { skipped+=("$t"); continue; }
  fi
  if landed "$t"; then skipped+=("$t (już w trunku)"); continue; fi
  planned+=("$t")
done

say "build-loop — ${#planned[@]} zadań do zrobienia, ${#skipped[@]} pominiętych"
echo "   pisze: $AGENT · recenzuje: $REVIEWER"
echo "   zablokowane brakiem Codeksa: $BLOCKED"
[ ${#skipped[@]} -gt 0 ] && printf '   pomijam: %s\n' "${skipped[*]}"
printf '   plan:    %s\n' "${planned[*]}"

if [ "$DRY" -eq 1 ]; then echo; echo "--dry-run: nic nie odpalam."; exit 0; fi

for t in "${planned[@]}"; do
  say "▶ $t   ($(date '+%H:%M:%S'))"
  t0=$(date +%s)
  rc=0
  ./ship-task.sh "$t" --agent "$AGENT" --reviewer "$REVIEWER" || rc=$?
  secs=$(( $(date +%s) - t0 ))

  # Koszt: suma `total_cost_usd` ze wszystkich transkryptów tego zadania.
  cost=$(python3 - "$ROOT/runs/$t" <<'PY' 2>/dev/null || echo 0
import json, sys, pathlib
tot = 0.0
d = pathlib.Path(sys.argv[1])
for f in d.glob('*.jsonl') if d.exists() else []:
    for line in f.open():
        line = line.strip()
        if not line.startswith('{'):
            continue
        try:
            e = json.loads(line)
        except Exception:
            continue
        if e.get('type') == 'result':
            tot += e.get('total_cost_usd') or 0.0
print(f'{tot:.2f}')
PY
)

  if [ "$rc" -ne 0 ]; then
    printf '%s\tred(%s)\t%s\t%s\t%s\n' "$t" "$rc" "$secs" "$cost" "$(date -Iseconds)" >> "$LOG"
    say "✕ $t zatrzymał pętlę — kod $rc, ${secs}s, \$$cost"
    echo "   worktree został do obejrzenia: ../loadout-task-$t"
    echo "   transkrypty: runs/$t/"
    case "$rc" in
      1) echo "   1 = sprawdzenie padło. To defekt zadania albo implementacji." ;;
      2) echo "   2 = MY jesteśmy źle skonfigurowani. To defekt harnessu — napraw i wznów." ;;
      3) echo "   3 = przerwane albo sufit czasu." ;;
    esac
    echo
    echo "   po naprawie:  ./scripts/build-loop.sh --from $t"
    exit "$rc"
  fi

  # Zielone -> ląduj, bo następne zadania potrzebują tego kodu w trunku.
  say "  ląduję task-$t"
  if ! ./integrate.sh "task-$t"; then
    printf '%s\tintegrate-red\t%s\t%s\t%s\n' "$t" "$secs" "$cost" "$(date -Iseconds)" >> "$LOG"
    say "✕ $t przeszedł bramkę, ale nie wszedł do trunka"
    echo "   merge został na miejscu; obejrzyj go i cofnij ręcznie, jeśli trzeba."
    exit 1
  fi

  printf '%s\tgreen\t%s\t%s\t%s\n' "$t" "$secs" "$cost" "$(date -Iseconds)" >> "$LOG"
  say "✓ $t — ${secs}s, \$$cost"
done

total=$(( $(date +%s) - started_all ))
say "pętla skończona: ${#planned[@]} zadań w $(( total / 60 )) min"
echo "   podsumowanie: $LOG"
echo "   zostało do zrobienia po odblokowaniu Codeksa: $BLOCKED"
