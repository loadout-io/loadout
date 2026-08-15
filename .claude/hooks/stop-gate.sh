#!/usr/bin/env bash
# Hak Stop: model nie kończy tury na czerwonym drzewie.
#
# Zweryfikowane na tej maszynie, że hak Stop odpala się I blokuje także pod `claude -p`:
# `exit 2` oddaje stderr modelowi i tura leci dalej, `exit 0` pozwala skończyć. To dlatego
# końcowa bramka to pięćdziesiąt linii basha, a nie maszyna stanów w orkiestratorze.
#
# Ograniczony świadomie. Hak blokuje WYJŚCIE, nie umie wymusić poprawki, a model, który się
# nie zgadza, będzie się kłócił zamiast pracować. Po BLOCK_CAP blokadach bieg kończy się
# czerwono i czyta go człowiek: bramka, z której nie da się wyjść, jest gorsza niż jej brak.
#
# `set -e` tu NIE MA i to jest decyzja, nie przeoczenie: przy `set -e` przerwanie na losowej
# komendzie kończy skrypt jej kodem, a kod 2 znaczy tutaj „zablokuj model". Blokada z powodu
# literówki w hooku jest nie do odróżnienia od blokady z powodu czerwonej bramki.
set -uo pipefail
cd "${CLAUDE_PROJECT_DIR:-$PWD}" || exit 0

BLOCK_CAP=3

# NIE ".git/…": w podpiętym worktree `.git` jest PLIKIEM, więc zapis kończy się
# "not a directory". `rev-parse --git-dir` zwraca prawdziwy katalog gita tego worktree,
# więc licznik zostaje prywatny dla gałęzi zamiast być wspólny dla wszystkich naraz.
STATE="$(git rev-parse --git-dir 2>/dev/null || echo .git)/stop-gate-blocks"

# Wejście hooka czytamy DOKŁADNIE RAZ — drugi `cat` dostałby już pustkę.
INPUT="$(cat 2>/dev/null || true)"

n=0
[ -f "$STATE" ] && n="$(cat "$STATE" 2>/dev/null || echo 0)"
case "$n" in ''|*[!0-9]*) n=0 ;; esac

# `stop_hook_active` jest prawdą, gdy ten Stop wynika z NASZEJ poprzedniej blokady. Bez tego
# hak wchodzi w rekurencję własnej decyzji. jq nie jest gwarantowane na maszynie, a harness
# poza tym zależy tylko od basha i pythona — jedno pole wyjmuje grep.
if printf '%s' "$INPUT" | grep -q '"stop_hook_active"[[:space:]]*:[[:space:]]*true' \
   && [ "$n" -ge "$BLOCK_CAP" ]; then
  echo 0 > "$STATE"; exit 0
fi

[ -f verify.sh ] || { echo "stop-gate: no verify.sh here — nothing to gate." >&2; exit 0; }

out="$(bash verify.sh full 2>&1)"; rc=$?

if [ "$rc" -eq 0 ]; then echo 0 > "$STATE"; exit 0; fi

# 2 to NASZ błąd konfiguracji, nie czerwone drzewo. Model nie może go naprawić (harness ma
# zabroniony do edycji), więc blokowanie go tutaj jest pętlą bez wyjścia.
if [ "$rc" -eq 2 ]; then
  echo 0 > "$STATE"
  { echo "stop-gate: the gate is MISCONFIGURED (exit 2), not red — yielding to a human."
    printf '%s\n' "$out" | tail -20; } >&2
  exit 0
fi

n=$((n + 1)); printf '%s\n' "$n" > "$STATE"
if [ "$n" -ge "$BLOCK_CAP" ]; then
  echo 0 > "$STATE"
  echo "stop-gate: still red after $BLOCK_CAP attempts; stopping for a human." >&2
  exit 0
fi

{ echo "The gate is RED (exit $rc) — you may not finish yet (attempt $n of $BLOCK_CAP)."
  printf '%s\n' "$out" | tail -40
  echo
  echo "Fix it under src/, src-tauri/ or tests/. Never touch TASK.md, verify.sh, harness/,"
  echo "checks/ or tasks/ — and never weaken an assertion to make a check pass."
} >&2
exit 2
