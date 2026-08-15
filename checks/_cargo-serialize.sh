#!/usr/bin/env bash
# Sourced, never discovered. Nazwa nie pasuje do checks/<before|quick|full>-*.sh, więc bramka
# jej nie odkryje — a to jedyna rzecz, która czyni z pliku w checks/ sprawdzenie.
#
# Niezmiennik 26: dwa ciężkie `cargo`/`rustc` naraz przypinają kompresor pamięci macOS
# i zamrażają maszynę przy swapie równym zeru. Bramka odpala sprawdzenia projektowe
# RÓWNOLEGLE w obrębie fali, więc w pełnej bramce full-clippy i full-test spotkałyby się
# w jednym oknie czasowym. Polityka mieszka tutaj, w jednym miejscu (niezmiennik 23),
# a nie przepisana w trzech skryptach.
#
# Muteks na katalogu, bo `flock(1)` nie istnieje na macOS. mkdir jest atomowy na APFS.

# shellcheck shell=bash

cargo_serialize() {
  local lock waited cap stale
  lock="${TMPDIR:-/tmp}/loadout-cargo.lock"
  cap="${LOADOUT_CARGO_LOCK_WAIT:-300}"
  waited=0

  while ! mkdir "$lock" 2>/dev/null; do
    # Bramka zabija grupę procesów przy timeoucie, więc trap EXIT bywa nieosiągalny
    # i zamek zostaje po trupie. Po 15 minutach uznajemy go za martwy.
    stale="$(find "$lock" -maxdepth 0 -mmin +15 2>/dev/null || true)"
    if [ -n "$stale" ]; then
      rmdir "$lock" 2>/dev/null || true
      continue
    fi
    sleep 1
    waited=$((waited + 1))
    if [ "$waited" -ge "$cap" ]; then
      echo "another cargo check has held the build lock for ${cap}s" >&2
      echo "detail: lock at $lock — remove it by hand if nothing is running" >&2
      return 1
    fi
  done

  # shellcheck disable=SC2064 — chcemy rozwinąć $lock TERAZ, nie przy wyjściu.
  trap "rmdir '$lock' 2>/dev/null || true" EXIT
  if [ "$waited" -gt 0 ]; then
    echo "[cargo] waited ${waited}s for the other cargo check (invariant 26)"
  fi
  return 0
}

# Oddaj zamek, zanim skrypt zrobi coś, co cargo nie jest. full-test.sh po suicie Rusta
# uruchamia vitest — trzymanie przez ten czas zamka blokowałoby full-clippy bez powodu.
cargo_release() {
  trap - EXIT
  rmdir "${TMPDIR:-/tmp}/loadout-cargo.lock" 2>/dev/null || true
}
