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

# Wejście hooka czytamy DOKŁADNIE RAZ — drugi `cat` dostałby już pustkę. Czytamy je PRZED zmianą
# katalogu, bo to z niego bierze się katalog, w którym sesja naprawdę pracuje.
INPUT="$(cat 2>/dev/null || true)"

# KATALOG SESJI, NIE GŁÓWNY CHECKOUT — i to jest naprawa, nie wygoda. `CLAUDE_PROJECT_DIR` wskazuje
# główny katalog projektu, a sesja może pracować w podpiętym worktree. Hak sądził wtedy CUDZE
# drzewo: zmierzone 2026-08-19, bramka poszła na czerwono na słowie z pliku innej sesji, w kodzie,
# którego sądzony agent nawet nie dotknął. On nie ma jak tego naprawić — nie wolno mu pisać po
# cudzej robocie — więc miele tury aż do BLOCK_CAP i z zewnątrz wygląda to na zawieszenie.
#
# `cwd` z wejścia hooka jest jedynym miejscem, z którego da się to wyczytać. Bierzemy je tylko
# wtedy, gdy naprawdę wygląda na checkout (`verify.sh` na miejscu); inaczej zostaje stara droga,
# bo hak, który nie znajdzie bramki, ma milczeć, a nie zgadywać.
HERE="$(printf '%s' "$INPUT" | python3 -c '
import json, sys
try:
    print((json.load(sys.stdin) or {}).get("cwd") or "")
except Exception:
    pass
' 2>/dev/null || true)"

cd "${CLAUDE_PROJECT_DIR:-$PWD}" 2>/dev/null || exit 0
if [ -n "$HERE" ] && [ -f "$HERE/verify.sh" ]; then
  cd "$HERE" || exit 0
fi

BLOCK_CAP=3

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

[ -f verify.sh ] || { echo "stop-gate: no verify.sh here — nothing to gate." >&2; exit 0; }

# `quick`, nie `full`. Zmierzone na pierwszym przejeździe T-01 (2026-08-15): hak odpalał pełną
# bramkę na KAŻDYM końcu tury, także w fazie kontraktu — a ta z definicji produkuje padające
# testy, więc pełna bramka nie miała prawa przejść. Hak blokował, model „naprawiał", hak
# blokował znowu. Trzy razy, aż do BLOCK_CAP, przy suficie 600 s na każdy przebieg.
# To był harness blokujący sam siebie.
#
# `quick` to higiena granicy tury i ma sens na każdym etapie: format, zakres plików, typy,
# słownictwo, tokeny, granice modułów. ~20 s. Pełna bramka należy do granic ETAPÓW
# w ship-task.sh, gdzie już jest wołana i gdzie jej czerwień coś znaczy.
out="$(bash verify.sh quick 2>&1)"; rc=$?

if [ "$rc" -eq 0 ]; then echo 0 > "$STATE"; exit 0; fi

# 2 to NASZ błąd konfiguracji, nie czerwone drzewo. Model nie może go naprawić (harness ma
# zabroniony do edycji), więc blokowanie go tutaj jest pętlą bez wyjścia.
if [ "$rc" -eq 2 ]; then
  echo 0 > "$STATE"
  { echo "stop-gate: the gate is MISCONFIGURED (exit 2), not red — yielding to a human."
    printf '%s\n' "$out" | tail -20; } >&2
  exit 0
fi

# BEZ TASK.md NIE MA CZEGO EGZEKWOWAC, wiec hak MELDUJE, a nie blokuje. To trzecie wystapienie
# tej samej zasady w tym pliku: nie blokuj na czerwieni, ktorej sadzony agent nie moze naprawic
# (patrz katalog sesji wyzej i `rc -eq 2` powyzej).
#
# Dwa powody, oba zmierzone. PIERWSZY: bez TASK.md nie ma bloku OWNS, wiec instrukcja "napraw
# u siebie" degraduje sie do statycznej listy -- dokladnie tej, ktora S-1 zacytowal jako
# sprzecznosc i ktora kosztowala 37 tur. DRUGI: `quick-permissions` i `quick-scope` sadza CALA
# galaz, a nie ture. 2026-08-19: commit czlowieka `c7fe838` wjechal w trakcie sesji, w ktorej
# model nie napisal ani bajtu, i zjadl dwie tury na spor o plik, ktorego nie wolno mu tknac --
# poprawke i tak zrobil czlowiek dwoma wlasnymi commitami.
#
# `verify.sh` sam nazywa ten tryb higiena ("this tier reports hygiene, never that a task is
# done"). Hak to teraz honoruje, zamiast robic z higieny blokade.
#
# CENA, swiadoma: w sesji orkiestratora czerwien od WLASNEJ edycji tez nie zablokuje. Zostaje
# widoczna w raporcie ponizej, formatowanie i tak lapie hak `PostToolUse`, a pelna bramka stoi
# na granicach etapow w ship-task.sh, gdzie jej czerwien cos znaczy.
if [ ! -f TASK.md ]; then
  echo 0 > "$STATE"
  { echo "stop-gate: RED (exit $rc), but there is no TASK.md here — reporting, not blocking."
    printf '%s\n' "$out" | tail -40; } >&2
  exit 0
fi

n=$((n + 1)); printf '%s\n' "$n" > "$STATE"
if [ "$n" -ge "$BLOCK_CAP" ]; then
  echo 0 > "$STATE"
  echo "stop-gate: still red after $BLOCK_CAP attempts; stopping for a human." >&2
  exit 0
fi

# Gdzie wolno naprawiać — z bloku OWNS zadania, nie ze statycznej listy (F2, zmierzone na S-1).
# Ten tekst mówił „Fix it under src/, src-tauri/ or tests/" każdemu zadaniu. S-1 posiada trzy
# pliki pod docs/research/topics/ i agent sam to zacytował jako sprzeczność: hak wysyłał go
# do naprawiania czegoś w drzewie, którego nie posiada. Instrukcja niewykonalna kosztuje tury
# tak samo jak zakaz — 37 tur i $2,02 na jednym biegu.
where="src/, src-tauri/ or tests/"
if [ -f TASK.md ]; then
  owns="$(sed -n '/<!--[[:space:]]*OWNS/,/-->/p' TASK.md | sed '1d;$d' \
          | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | grep -v '^$' | paste -sd', ' -)"
  [ -n "$owns" ] && where="$owns"
fi

{ echo "The gate is RED (exit $rc) — you may not finish yet (attempt $n of $BLOCK_CAP)."
  printf '%s\n' "$out" | tail -40
  echo
  echo "Fix it inside this task's own paths: $where"
  echo "Never touch TASK.md, verify.sh, harness/, checks/ or tasks/ — and never weaken an"
  echo "assertion to make a check pass. If the criterion itself is wrong, say so and stop:"
  echo "that is a finding for a human (AGENTS.md §7), not a file to edit."
} >&2
exit 2
