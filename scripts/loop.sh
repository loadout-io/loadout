#!/usr/bin/env bash
# loop.sh — jedyne miejsce, które wie, JAK znaleźć i zatrzymać bieg.
#
#   ./scripts/loop.sh status     # co biegnie i gdzie stoi
#   ./scripts/loop.sh stop       # zatrzymaj czysto, razem z agentem
#   ./scripts/loop.sh wait       # blokuj, dopóki pętla biegnie (do skryptów)
#
# Powód istnienia: od czasu przypięcia (`exec bash "$snap"`) skrypty harnessu biegną pod
# nazwą `/var/folders/…/ship-task.F1KPavKuWS`, a nie `./ship-task.sh`. Każde `pgrep -f
# 'ship-task.sh'` i `pkill -f 'scripts/build-loop.sh'` napisane wcześniej CICHO nie trafia.
# Zmierzone 2026-08-15: obserwator zameldował „build-loop wyszedł", kiedy pętla spokojnie
# pisała kontrakt T-04. Fałszywe „skończone" jest gorsze niż brak monitoringu.
#
# Druga rzecz, którą to miejsce pamięta za wszystkich: zabicie samego basha ZOSTAWIA AGENTA.
# `pkill -f ship-task.sh` ubija rodzica, a `claude -p …` biegnie dalej jako sierota i pisze
# do worktree, którego nikt nie odbierze. Zmierzone tego samego wieczoru na T-04.
set -uo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

# Wzorce dopasowują OBIE formy: przypiętą kopię w $TMPDIR i wywołanie wprost z repo.
# `[b]uild-loop` nie łapie samego pgrepa.
PAT_LOOP='[b]uild-loop'
PAT_SHIP='[s]hip-task'
# Agent harnessu, NIE interaktywne sesje użytkownika. Rozróżnia je `-p`: sesja człowieka
# biegnie bez niego. Zawężenie do stream-json jest drugim zabezpieczeniem.
PAT_AGENT='[c]laude -p --output-format stream-json'

pids() { pgrep -f "$1" 2>/dev/null; }

status() {
  local l s a
  l="$(pids "$PAT_LOOP")"; s="$(pids "$PAT_SHIP")"; a="$(pids "$PAT_AGENT")"
  printf 'build-loop : %s\n' "${l:-—}"
  printf 'ship-task  : %s\n' "${s:-—}"
  printf 'agent      : %s\n' "${a:-—}"
  if [ -f runs/build-loop.tsv ]; then
    printf '\nostatnie wiersze:\n'
    tail -5 runs/build-loop.tsv | sed 's/^/  /'
  fi
  # Który task i w jakiej fazie — z ship.log najświeżej dotkniętego katalogu biegu.
  local newest
  newest="$(ls -td runs/*/ 2>/dev/null | head -1 || true)"
  if [ -n "$newest" ] && [ -f "$newest/ship.log" ]; then
    printf '\n%s:\n' "${newest%/}"
    grep -E '^== ' "$newest/ship.log" | tail -3 | sed 's/^/  /'
  fi
  [ -n "$l$s$a" ]     # kod wyjścia: 0 = coś biegnie, 1 = cisza
}

stop() {
  local had=0
  # Kolejność ma znaczenie: najpierw rodzic, żeby nie wystartował następnego zadania,
  # potem agent, żeby nie został sierotą.
  for pat in "$PAT_LOOP" "$PAT_SHIP" "$PAT_AGENT"; do
    if [ -n "$(pids "$pat")" ]; then had=1; pkill -f "$pat" 2>/dev/null; fi
    sleep 1
  done
  sleep 2
  # Kto nie ustąpił po TERM, dostaje KILL. Bez tego „zatrzymane" bywa nieprawdą.
  for pat in "$PAT_LOOP" "$PAT_SHIP" "$PAT_AGENT"; do
    [ -n "$(pids "$pat")" ] && pkill -9 -f "$pat" 2>/dev/null
  done
  sleep 1
  if [ -n "$(pids "$PAT_LOOP")$(pids "$PAT_SHIP")$(pids "$PAT_AGENT")" ]; then
    echo "nie wszystko ustąpiło:" >&2; status >&2; return 1
  fi
  [ "$had" = 1 ] && echo "zatrzymane" || echo "nic nie biegło"
  return 0
}

wait_out() {
  while [ -n "$(pids "$PAT_LOOP")" ]; do sleep 20; done
  echo "build-loop wyszedł $(date +%H:%M:%S)"
}

case "${1:-status}" in
  status) status ;;
  stop)   stop ;;
  wait)   wait_out ;;
  *) echo "usage: loop.sh [status|stop|wait]" >&2; exit 2 ;;
esac
