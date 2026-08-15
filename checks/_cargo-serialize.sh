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
  local lock waited cap owner stale
  lock="${TMPDIR:-/tmp}/loadout-cargo.lock"
  cap="${LOADOUT_CARGO_LOCK_WAIT:-300}"
  waited=0

  while ! mkdir "$lock" 2>/dev/null; do
    # Bramka zabija grupę procesów przy timeoucie, więc trap EXIT bywa nieosiągalny
    # i zamek zostaje po trupie. Pytamy więc o ŻYCIE właściciela, nie o wiek zamka.
    #
    # Wcześniej stał tu wyłącznie próg `find -mmin +15` i miał dwie wady naraz, obie
    # zmierzone 2026-08-15. Po pierwsze był MARTWY dla czekającego: próg 900 s jest trzy
    # razy dłuższy niż sufit czekania 300 s, więc nikt nigdy nie dożył własnego progu.
    # Po drugie, kiedy już strzelał, kasował zamek ŻYWEGO procesu — ci.sh trzymał muteks
    # przez cały bieg, strażnik czekał, próg mijał, i dwa cargo ruszały naraz. Czyli
    # dokładnie to, czemu niezmiennik 26 ma zapobiegać, robione przez jego własną obronę.
    owner="$(cat "$lock/pid" 2>/dev/null || true)"
    if [ -n "$owner" ] && ! kill -0 "$owner" 2>/dev/null; then
      echo "[cargo] lock owner $owner is gone — reclaiming" >&2
      rm -f "$lock/pid" 2>/dev/null || true
      rmdir "$lock" 2>/dev/null || true
      continue
    fi
    # Zamek bez pliku pid pochodzi sprzed tej zmiany albo z wyścigu między mkdir a zapisem.
    # Dla niego zostaje próg czasowy, ale KRÓTSZY niż sufit czekania — inaczej byłby ozdobą.
    if [ -z "$owner" ]; then
      stale="$(find "$lock" -maxdepth 0 -mmin +3 2>/dev/null || true)"
      if [ -n "$stale" ]; then
        rmdir "$lock" 2>/dev/null || true
        continue
      fi
    fi
    sleep 1
    waited=$((waited + 1))
    if [ "$waited" -ge "$cap" ]; then
      echo "another cargo check has held the build lock for ${cap}s" >&2
      echo "detail: lock at $lock, held by pid ${owner:-unknown}" >&2
      echo "detail: remove it by hand if nothing is running" >&2
      return 1
    fi
  done

  # $$ to PID TEJ powłoki, także wewnątrz podpowłoki — czyli właściciela traptu poniżej.
  echo "$$" > "$lock/pid" 2>/dev/null || true

  # shellcheck disable=SC2064 — chcemy rozwinąć $lock TERAZ, nie przy wyjściu.
  trap "rm -f '$lock/pid' 2>/dev/null; rmdir '$lock' 2>/dev/null || true" EXIT
  if [ "$waited" -gt 0 ]; then
    echo "[cargo] waited ${waited}s for the other cargo check (invariant 26)"
  fi
  return 0
}

# Oddaj zamek, zanim skrypt zrobi coś, co cargo nie jest. full-test.sh po suicie Rusta
# uruchamia vitest — trzymanie przez ten czas zamka blokowałoby full-clippy bez powodu.
cargo_release() {
  local lock="${TMPDIR:-/tmp}/loadout-cargo.lock"
  trap - EXIT
  rm -f "$lock/pid" 2>/dev/null || true
  rmdir "$lock" 2>/dev/null || true
}
