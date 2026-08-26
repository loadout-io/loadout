#!/usr/bin/env bash
# Wspolna polityka zycia procesow dla review.sh i repair.sh.
#
# 2026-08-26: Ctrl-C zakonczyl repair.sh, ale pisarz Codeksa pracowal we wlasnym PGID,
# zostal adoptowany przez PID 1 i zdazyl zacommitowac po tym, jak harness oddal kod 3.
# Aktywny PGID musi wiec byc znany pulapce sygnalu, a samo zabicie lidera nie jest dowodem.

LOADOUT_AGENT_GROUP_PID=""
LOADOUT_AGENT_GROUP_WATCHER=""
LOADOUT_AGENT_GROUP_PROOF_FAILED=0
LOADOUT_AGENT_GROUP_STARTING=0
LOADOUT_AGENT_GROUP_INTERRUPT_PENDING=0

loadout_agent_group_alive() { # loadout_agent_group_alive <pgid>
  [ -n "${1:-}" ] && kill -0 -"$1" 2>/dev/null
}

loadout_agent_group_dead_by_esrch() { # loadout_agent_group_dead_by_esrch <pgid>
  python3 - "$1" <<'PY'
import os
import sys

try:
    os.killpg(int(sys.argv[1]), 0)
except ProcessLookupError:
    raise SystemExit(0)
except (PermissionError, OSError):
    raise SystemExit(1)
raise SystemExit(1)
PY
}

loadout_agent_group_defer_interrupt() {
  if [ "$LOADOUT_AGENT_GROUP_STARTING" = 1 ]; then
    LOADOUT_AGENT_GROUP_INTERRUPT_PENDING=1
    return 0
  fi
  return 1
}

loadout_agent_group_signal_to_death() { # loadout_agent_group_signal_to_death <pgid>
  local pgid="$1" tick=0 grace="${LOADOUT_AGENT_GROUP_GRACE_TICKS:-20}"
  loadout_agent_group_alive "$pgid" || return 0

  kill -TERM -"$pgid" 2>/dev/null || true
  while loadout_agent_group_alive "$pgid" && [ "$tick" -lt "$grace" ]; do
    sleep 0.1
    tick=$((tick + 1))
  done

  if loadout_agent_group_alive "$pgid"; then
    kill -KILL -"$pgid" 2>/dev/null || true
  fi
  tick=0
  while loadout_agent_group_alive "$pgid" && [ "$tick" -lt 20 ]; do
    sleep 0.1
    tick=$((tick + 1))
  done

  # Tylko ESRCH jest dowodem smierci. EPERM lub inny blad oznacza "nie umiem udowodnic".
  loadout_agent_group_dead_by_esrch "$pgid"
}

loadout_agent_group_start() { # loadout_agent_group_start <funkcja-lub-komenda> [argumenty...]
  if [ -n "$LOADOUT_AGENT_GROUP_PID" ]; then
    echo "agent process group $LOADOUT_AGENT_GROUP_PID is already active" >&2
    return 2
  fi
  # Job control jest wlaczony tylko na czas startu. Dziecko dostaje wlasny PGID rowny PID,
  # ale reszta nieinteraktywnego skryptu nie zaczyna drukowac komunikatow o zadaniach.
  LOADOUT_AGENT_GROUP_STARTING=1
  set -m
  "$@" &
  LOADOUT_AGENT_GROUP_PID=$!
  set +m
  LOADOUT_AGENT_GROUP_STARTING=0
  if [ "$LOADOUT_AGENT_GROUP_INTERRUPT_PENDING" = 1 ]; then
    LOADOUT_AGENT_GROUP_INTERRUPT_PENDING=0
    return 3
  fi
}

loadout_agent_group_stop() {
  local pgid="${LOADOUT_AGENT_GROUP_PID:-}" watcher="${LOADOUT_AGENT_GROUP_WATCHER:-}"
  LOADOUT_AGENT_GROUP_PROOF_FAILED=0
  if [ -n "$watcher" ]; then
    kill -TERM "$watcher" 2>/dev/null || true
    wait "$watcher" 2>/dev/null || true
    LOADOUT_AGENT_GROUP_WATCHER=""
  fi
  [ -n "$pgid" ] || return 0

  loadout_agent_group_signal_to_death "$pgid" || {
    LOADOUT_AGENT_GROUP_PROOF_FAILED=1
    echo "could not prove process group $pgid dead after SIGTERM and SIGKILL" >&2
    return 2
  }
  wait "$pgid" 2>/dev/null || true
  LOADOUT_AGENT_GROUP_PID=""
}

loadout_agent_group_wait() { # loadout_agent_group_wait [budzet-sekund]
  local budget="${1:-}" pgid="$LOADOUT_AGENT_GROUP_PID" rc=0
  [ -n "$pgid" ] || {
    echo "no active agent process group to wait for" >&2
    return 2
  }

  if [ -n "$budget" ]; then
    # Watcher sam wykonuje cala eskalacje. Poprzednio wysylal tylko TERM, po czym rodzic
    # czekal bez konca; linia z KILL lezala za wait, wiec proces ignorujacy TERM jej nie widzial.
    ( sleep "$budget"; loadout_agent_group_signal_to_death "$pgid" ) >/dev/null 2>&1 &
    LOADOUT_AGENT_GROUP_WATCHER=$!
  fi

  wait "$pgid" 2>/dev/null || rc=$?
  loadout_agent_group_stop || return 2
  return "$rc"
}
