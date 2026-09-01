#!/usr/bin/env bash
# Hak Stop: model nie kończy tury na czerwonych checkach.
#
# Zweryfikowane na tej maszynie, że hak Stop odpala się I blokuje także pod `claude -p`:
# `exit 2` oddaje stderr modelowi i tura leci dalej, `exit 0` pozwala skończyć.
#
# I to jest JEDYNY kanał, którym piszący agent widzi wynik checków. Zmierzone 2026-08-28
# dwiema sondami: `./verify.sh`, `cargo` i `npx` są odrzucane w biegu bez człowieka, mimo
# reguł w `permissions.allow` — Claude Code nie honoruje ich dla lokalnych skryptów ani
# interpreterów. Instrukcja „uruchom checki i popraw, co czerwone" jest więc niewykonalna
# jako prompt i wykonalna wyłącznie jako ten hak.
#
# Ograniczony świadomie. Hak blokuje WYJŚCIE, nie umie wymusić poprawki, a model, który się
# nie zgadza, będzie się kłócił zamiast pracować. Po BLOCK_CAP blokadach tura kończy się
# i czyta ją człowiek: bramka, z której nie da się wyjść, jest gorsza niż jej brak.
#
# `set -e` tu NIE MA i to jest decyzja, nie przeoczenie: przy `set -e` przerwanie na losowej
# komendzie kończy skrypt jej kodem, a kod 2 znaczy tutaj „zablokuj model". Blokada z powodu
# literówki w hooku jest nie do odróżnienia od blokady z powodu czerwonego checka.
set -uo pipefail

# Wejście hooka czytamy DOKŁADNIE RAZ — drugi `cat` dostałby już pustkę. Przed zmianą katalogu,
# bo to z niego bierze się katalog, w którym sesja naprawdę pracuje.
INPUT="$(cat 2>/dev/null || true)"

# KATALOG SESJI, NIE GŁÓWNY CHECKOUT. `CLAUDE_PROJECT_DIR` wskazuje główny katalog projektu,
# a sesja może pracować w podpiętym worktree. Hak sądził wtedy CUDZE drzewo: zmierzone
# 2026-08-19, zaświecił się na słowie z pliku innej sesji, w kodzie, którego sądzony agent
# nie tknął. On nie ma jak tego naprawić, więc mielił tury aż do BLOCK_CAP.
HERE="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    print((json.load(sys.stdin) or {}).get("cwd") or "")
except Exception:
    pass
' 2>/dev/null || true)"

cd "${CLAUDE_PROJECT_DIR:-$PWD}" 2>/dev/null || exit 0
if [ -n "$HERE" ] && [ -f "$HERE/harness/h.py" ]; then
  cd "$HERE" || exit 0
fi

BLOCK_CAP=3
[ -f harness/h.py ] || { echo "stop-gate: nie ma tu harnessu — nic do sprawdzenia." >&2; exit 0; }

# NIE ".git/…": w podpiętym worktree `.git` jest PLIKIEM, więc zapis kończy się
# "not a directory". `rev-parse --git-dir` zwraca prawdziwy katalog gita tego worktree,
# więc licznik zostaje prywatny dla gałęzi zamiast być wspólny dla wszystkich naraz.
STATE="$(git rev-parse --git-dir 2>/dev/null || echo .git)/stop-gate-blocks"
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

# `h check` sam wybiera checki po ZMIENIONYCH ścieżkach i kończy zerem, gdy nic nie zmienione.
# Nie ma tu poziomów ani nazw checków — lista mieszka w `harness/checks.json`, w jednym miejscu.
out="$(python3 -B harness/h.py check 2>&1)"; rc=$?

if [ "$rc" -eq 0 ]; then echo 0 > "$STATE"; exit 0; fi

n=$((n + 1)); echo "$n" > "$STATE"
if [ "$n" -gt "$BLOCK_CAP" ]; then
  echo 0 > "$STATE"
  { echo "stop-gate: nadal czerwono po $BLOCK_CAP blokadach — oddaję turę człowiekowi."
    printf '%s\n' "$out" | tail -30; } >&2
  exit 0
fi

{
  echo "stop-gate: check padł (blokada $n z $BLOCK_CAP). Napraw to, zanim skończysz turę."
  printf '%s\n' "$out" | tail -40
  echo
  echo "Nigdy nie tykaj harness/, checks/ ani scripts/ci.sh, żeby check przeszedł,"
  echo "i nigdy nie osłabiaj asercji. Jeśli check jest zły — powiedz to zamiast go zmieniać."
} >&2
exit 2
