#!/usr/bin/env bash
# wave.sh — sterownik falowy: laduj zielone, przelicz gotowe, dostaw odblokowane. W kolko.
#
#   ./scripts/wave.sh              # jedz do wyczerpania grafu
#   ./scripts/wave.sh --dry-run    # wypisz, co by zrobil, i wyjdz
#
# Roznica wobec scripts/build-loop.sh: build-loop jedzie SZEREGOWO po sztywnej liscie i staje
# na pierwszym czerwonym. Ten skrypt jedzie SZEROKOSCIA GRAFU: w kazdej chwili biegnie tyle
# zadan, ile ma spelnione zaleznosci, a czerwone odstawia na bok zamiast zatrzymywac reszte.
#
# Powod jest zmierzony (2026-08-16): przy szesciu agentach maszyna ma load 2,6 na szesnastu
# rdzeniach, 2,5 GB z 64 i ZERO czekania na muteksie cargo. Agent czeka na odpowiedz modelu,
# nie na procesor -- wiec limitem nie jest sprzet, tylko to, ile zadan da sie w ogole zaczac.
#
# Trzy rzeczy, ktorych ten skrypt NIE robi, kazda swiadomie:
#   · nie landuje rownolegle. integrate.sh merguje jedna galaz i przepuszcza pelna bramke po
#     kazdej; drugi merge na czerwonym trunku zamienia jeden defekt w dwa nierozroznialne.
#   · nie ponawia czerwonych. Czerwone znaczy "czlowiek albo orchestrator ma to przeczytac";
#     slepy retry pali pieniadze na tym samym bledzie.
#   · nie tyka kontraktow ani harnessu. To nalezy do orchestratora, nie do kierowcy.
set -uo pipefail

if [ -z "${LOADOUT_PINNED_WAVE:-}" ]; then
  LOADOUT_SELF_WAVE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
  LOADOUT_SNAP="$(mktemp -t wave)"
  cat "${BASH_SOURCE[0]}" > "$LOADOUT_SNAP"
  export LOADOUT_PINNED_WAVE=1 LOADOUT_SELF_WAVE
  exec bash "$LOADOUT_SNAP" "$@"
fi
SELF_DIR="${LOADOUT_SELF_WAVE:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)}"
unset LOADOUT_PINNED_WAVE LOADOUT_SELF_WAVE

cd "$SELF_DIR/.."
ROOT="$PWD"
LOG="$ROOT/runs/build-loop.tsv"
WAVE_LOG="$ROOT/runs/wave.log"
BLOCKED="S-3 T-10"                     # czekaja na kredyty Codeksa (2026-08-20)
DRY=0
[ "${1:-}" = "--dry-run" ] && DRY=1

export LOADOUT_CARGO_LOCK_WAIT="${LOADOUT_CARGO_LOCK_WAIT:-2400}"

say() { printf '%s  %s\n' "$(date '+%H:%M:%S')" "$*" | tee -a "$WAVE_LOG"; }

# ── graf zaleznosci: JEDNO zrodlo, tasks/INDEX.md ─────────────────────────────────────────
deps_of() { python3 - "$1" <<'PY'
import re, sys, pathlib
want = sys.argv[1]
for l in pathlib.Path('tasks/INDEX.md').read_text().splitlines():
    m = re.match(r'\|\s*\*\*([ST]-\d+)\*\*\s*\|\s*\d\s*\|[^|]*\|\s*([^|]*)\|', l)
    if m and m.group(1) == want:
        print(' '.join(x.strip() for x in m.group(2).split(',') if re.match(r'^[ST]-\d+$', x.strip())))
        break
PY
}
all_tasks() { python3 - <<'PY'
import re, pathlib
for l in pathlib.Path('tasks/INDEX.md').read_text().splitlines():
    m = re.match(r'\|\s*\*\*([ST]-\d+)\*\*', l)
    if m: print(m.group(1))
PY
}

landed()  { git merge-base --is-ancestor "refs/heads/task-$1" HEAD 2>/dev/null && return 0
            grep -qE "^$1"$'\t'"green" "$LOG" 2>/dev/null; }
running() { pgrep -f "[s]hip-task\.[A-Za-z0-9]{6,} $1 " >/dev/null 2>&1; }
parked()  { grep -qE "^$1"$'\t'"(red|integrate-red)" "$LOG" 2>/dev/null \
            && ! grep -qE "^$1"$'\t'"green" "$LOG" 2>/dev/null; }

# Zielone i czekajace na land: galaz istnieje, ship.log konczy sie GREEN, jeszcze nie w trunku.
awaiting_land() {
  local t
  for t in $(all_tasks); do
    landed "$t" && continue
    running "$t" && continue
    git rev-parse --verify -q "refs/heads/task-$t" >/dev/null 2>&1 || continue
    grep -q "task $t: gate GREEN" "runs/$t/ship.log" 2>/dev/null && printf '%s\n' "$t"
  done
}

ready() {
  local t d ok
  for t in $(all_tasks); do
    case " $BLOCKED " in *" $t "*) continue ;; esac
    landed "$t" && continue
    running "$t" && continue
    parked "$t" && continue
    git rev-parse --verify -q "refs/heads/task-$t" >/dev/null 2>&1 && continue  # ma galaz: albo czeka na land, albo stoi
    ok=1
    for d in $(deps_of "$t"); do landed "$d" || { ok=0; break; }; done
    [ "$ok" = 1 ] && printf '%s\n' "$t"
  done
}

cost_of() { python3 - "$ROOT/runs/$1" <<'PY' 2>/dev/null || echo 0
import json, sys, pathlib
tot=0.0; d=pathlib.Path(sys.argv[1])
for f in (d.glob('*.jsonl') if d.exists() else []):
    for line in f.open():
        line=line.strip()
        if not line.startswith('{'): continue
        try: e=json.loads(line)
        except Exception: continue
        if e.get('type')=='result': tot += e.get('total_cost_usd') or 0.0
print(f'{tot:.2f}')
PY
}

if [ "$DRY" -eq 1 ]; then
  echo "wyladowane : $(for t in $(all_tasks); do landed "$t" && printf '%s ' "$t"; done)"
  echo "biegnie    : $(for t in $(all_tasks); do running "$t" && printf '%s ' "$t"; done)"
  echo "do landu   : $(awaiting_land | tr '\n' ' ')"
  echo "odstawione : $(for t in $(all_tasks); do parked "$t" && printf '%s ' "$t"; done)"
  echo "gotowe     : $(ready | tr '\n' ' ')"
  exit 0
fi

say "wave: start"
idle=0
while :; do
  # ── 1. ladowanie, POJEDYNCZO ────────────────────────────────────────────────────────────
  for t in $(awaiting_land); do
    if [ -n "$(git status --porcelain -uall)" ]; then
      say "wave: drzewo brudne, odkladam land $t na nastepna rundke"; break
    fi
    say "wave: laduje task-$t"
    if ./integrate.sh "task-$t" >> "$WAVE_LOG" 2>&1; then
      printf '%s\tgreen\t0\t%s\t%s\n' "$t" "$(cost_of "$t")" "$(date -Iseconds)" >> "$LOG"
      say "wave: ✓ $t wyladowal (\$$(cost_of "$t"))"
    else
      printf '%s\tintegrate-red\t0\t%s\t%s\n' "$t" "$(cost_of "$t")" "$(date -Iseconds)" >> "$LOG"
      say "wave: ✕ $t przeszedl bramke, ale NIE wszedl do trunka — zostawiam do obejrzenia"
    fi
    break                              # jeden land na rundke: nastepny po przeliczeniu
  done

  # ── 2. dostaw wszystko, co sie wlasnie odblokowalo ──────────────────────────────────────
  for t in $(ready); do
    say "wave: startuje $t"
    nohup ./ship-task.sh "$t" --agent claude --reviewer claude > "runs/fan-$t.log" 2>&1 &
    sleep 15                           # odstep na .git/index.lock przy tworzeniu worktree
  done

  # ── 3. koniec? ──────────────────────────────────────────────────────────────────────────
  live="$(for t in $(all_tasks); do running "$t" && printf '%s ' "$t"; done)"
  wait_land="$(awaiting_land | tr '\n' ' ')"
  rdy="$(ready | tr '\n' ' ')"
  if [ -z "$live$wait_land$rdy" ]; then
    idle=$((idle+1))
    [ "$idle" -ge 2 ] && { say "wave: nie ma nic biegnacego, nic do landu i nic gotowego — koniec"; break; }
  else
    idle=0
  fi
  sleep 60
done

say "wave: koniec. wyladowane $(grep -c green "$LOG") · odstawione: $(for t in $(all_tasks); do parked "$t" && printf '%s ' "$t"; done)"
