#!/usr/bin/env bash
# loop.sh — jedyne miejsce, które wie, JAK znaleźć i zatrzymać bieg.
#
#   ./scripts/loop.sh status     # co biegnie, przy którym zadaniu, w jakiej fazie
#   ./scripts/loop.sh stop       # zatrzymaj czysto, całe drzewo razem z agentem
#   ./scripts/loop.sh wait       # blokuj, dopóki pętla biegnie (do skryptów)
#
# ── dlaczego PID, a nie nazwa procesu ─────────────────────────────────────────────────────
#
# Bo rozpoznawanie procesów po treści linii poleceń jest zgadywaniem, i przegrało trzy razy
# jednego wieczoru (2026-08-15):
#
#   1. `pkill -f 'ship-task.sh'` ubił rodzica i ZOSTAWIŁ agenta. `claude -p` biegł dalej jako
#      sierota i pisał do worktree, którego nikt już nie odbierze.
#   2. Po przypięciu (`exec bash "$snap"`) skrypty biegną jako `/var/folders/…/ship-task.F1KP…`,
#      więc `pgrep -f 'scripts/build-loop.sh'` przestał trafiać. Obserwator zameldował
#      „build-loop wyszedł", kiedy pętla spokojnie pisała kontrakt.
#   3. Poprawiony wzorzec `[b]uild-loop` dopasował SAM SIEBIE — powłoka obserwatora miała
#      w linii poleceń `runs/build-loop.log`. `wait` nie wyszedłby nigdy.
#
# Wszystkie trzy to ten sam błąd w trzech przebraniach, więc naprawa też jest jedna:
# **pętla zapisuje swój PID, a resztę drzewa znajdujemy przez `pgrep -P`.** Nie ma tekstu
# do dopasowania, więc nie ma czego pomylić.
set -uo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

PIDFILE="runs/.build-loop.pid"

# Wzorce zostają WYŁĄCZNIE jako awaryjne: dla biegu wystartowanego przed tą zmianą i dla
# ship-task.sh odpalonego ręcznie, bez pętli. Nawias w pierwszej literze nie łapie pgrepa.
FALLBACK=('[b]uild-loop\.(sh|[A-Za-z0-9]{6,})' '[s]hip-task\.(sh|[A-Za-z0-9]{6,})')

loop_pid() {                    # wypisuje PID żyjącej pętli albo nic
  local p
  [ -f "$PIDFILE" ] || return 1
  p="$(cat "$PIDFILE" 2>/dev/null || true)"
  [ -n "$p" ] || return 1
  kill -0 "$p" 2>/dev/null || { rm -f "$PIDFILE"; return 1; }   # plik po trupie
  printf '%s' "$p"
}

descendants() {                 # $1 = pid → wszystkie potomki, najgłębsze najpierw
  local pid="$1" kid
  for kid in $(pgrep -P "$pid" 2>/dev/null); do
    descendants "$kid"
    printf '%s\n' "$kid"
  done
}

# Cały bieg jako lista PID-ów: pętla + jej drzewo. Kiedy pliku PID nie ma, spadamy na wzorce.
run_pids() {
  local root
  if root="$(loop_pid)"; then
    printf '%s\n' "$root"
    descendants "$root"
    return
  fi
  # Tryb awaryjny tez musi zejsc po drzewie. Bez tego widzi wylacznie skrypty i NIE widzi
  # agenta -- czyli `stop` zostawialby sierote, ten sam blad, przeciw ktoremu ten plik powstal.
  # Zlapane 2026-08-15, kiedy plik PID zniknal w trakcie biegu i status przelaczyl sie na wzorce:
  # agent 26814 po prostu wypadl z listy, cicho.
  local pat p
  {
    for pat in "${FALLBACK[@]}"; do
      for p in $(pgrep -f "$pat" 2>/dev/null); do
        printf '%s\n' "$p"
        descendants "$p"
      done
    done
  } | sort -u
}

show() { ps -p "$1" -o command= 2>/dev/null | cut -c1-100; }

status() {
  local all p root
  all="$(run_pids)"
  if [ -z "$all" ]; then
    echo "nic nie biegnie"
  else
    root="$(loop_pid || true)"
    [ -n "$root" ] && echo "pętla: $root (z $PIDFILE)" || echo "pętla: rozpoznana po wzorcu (bieg sprzed pliku PID)"
    for p in $all; do printf '  %-7s %s\n' "$p" "$(show "$p")"; done
  fi
  if [ -f runs/build-loop.tsv ]; then
    printf '\nostatnie wiersze:\n'; tail -5 runs/build-loop.tsv | sed 's/^/  /'
  fi
  local newest
  newest="$(ls -td runs/*/ 2>/dev/null | head -1 || true)"
  if [ -n "$newest" ] && [ -f "$newest/ship.log" ]; then
    printf '\n%s:\n' "${newest%/}"
    grep -E '^== ' "$newest/ship.log" | tail -3 | sed 's/^/  /'
  fi
  [ -n "$all" ]                 # 0 = coś biegnie, 1 = cisza
}

stop() {
  local all p left
  # Zbieramy drzewo PRZED zabiciem czegokolwiek. Po zabiciu rodzica potomkowie tracą
  # rodzicielstwo i `pgrep -P` już ich nie znajdzie — to jest dokładnie ta droga, którą
  # agent zostawał sierotą.
  all="$(run_pids)"
  [ -n "$all" ] || { echo "nic nie biegło"; return 0; }

  # Rodzic pierwszy, żeby nie zdążył wystartować następnego zadania.
  for p in $all; do kill -TERM "$p" 2>/dev/null; done
  sleep 3
  for p in $all; do kill -0 "$p" 2>/dev/null && kill -KILL "$p" 2>/dev/null; done
  sleep 1
  rm -f "$PIDFILE"

  left=""
  for p in $all; do kill -0 "$p" 2>/dev/null && left="$left $p"; done
  if [ -n "$left" ]; then echo "nie ustąpiły:$left" >&2; return 1; fi
  echo "zatrzymane ($(printf '%s' "$all" | wc -w | tr -d ' ') procesów)"
}

wait_out() {
  while [ -n "$(run_pids)" ]; do sleep 20; done
  echo "pętla wyszła $(date +%H:%M:%S)"
}

case "${1:-status}" in
  status) status ;;
  stop)   stop ;;
  wait)   wait_out ;;
  *) echo "usage: loop.sh [status|stop|wait]" >&2; exit 2 ;;
esac
