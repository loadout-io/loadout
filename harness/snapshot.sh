#!/usr/bin/env bash
# Punkt przywracania dla niezacommitowanej pracy — brany cicho i często.
#
# Git nie ma haka pre-checkout, więc `git checkout -- .`, `git reset --hard`, `git stash`
# i `git clean` nie dają się przechwycić: kasują drzewo robocze, a reflog notuje wyłącznie
# commity, więc potem nie ma gdzie szukać. W repo źródłowym cztery takie incydenty jednego
# dnia; dwa ciche, wykryte dopiero gdy naprawione sprawdzenie okazało się nienaprawione.
# Zapobieganie było behawioralne i behawioralnie zawiodło — więc zamiast tego robimy stratę
# odwracalną.
#
# Zapisuje bieżące drzewo do refs/snapshots/<epoch> NIE dotykając indeksu, drzewa roboczego
# ani stasha. Woła to verify.sh, które chodzi bez przerwy, więc punkty gromadzą się same.
#
#   git log --oneline refs/snapshots/            # co i kiedy zapisano
#   git show refs/snapshots/<ts>:sciezka/plik    # odczyt jednego pliku
#   git checkout refs/snapshots/<ts> -- sciezka  # przywrócenie jednego pliku
#
# Znane ograniczenie, świadome: `git stash create` nie obejmuje plików nieśledzonych.
# Nowy, jeszcze nie dodany plik NIE jest w snapshocie. Wolimy to zapisać niż udawać,
# że snapshot jest kompletny — `-u` wymaga stasha z prawdziwego zdarzenia i dotyka indeksu.
set -euo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

git rev-parse --git-dir >/dev/null 2>&1 || exit 0

# Nic niezacommitowanego — nie ma czego ratować. Warunek w `if`, bo `set -e` przerwałoby
# skrypt na `git diff --quiet` zwracającym 1, czyli dokładnie wtedy, gdy jest co zapisać.
if git diff --quiet 2>/dev/null && git diff --cached --quiet 2>/dev/null; then
  exit 0
fi

# `stash create` buduje commit i go NIE odkłada, więc drzewo robocze się nie zmienia —
# o to chodzi: snapshot, którego bieg nie może zauważyć, nie może mu przeszkodzić.
sha="$(git stash create "snapshot" 2>/dev/null || true)"
[ -n "$sha" ] || exit 0
git update-ref "refs/snapshots/$(date +%s)" "$sha" 2>/dev/null || exit 0

# Bufor pierścieniowy na 40. Starsze to szum, a obiekty i tak stają się nieosiągalne wraz
# z refem. Sortowanie po nazwie malejąco = po epoce malejąco (stała długość do 2286 roku).
git for-each-ref --format='%(refname)' --sort=-refname refs/snapshots \
  | tail -n +41 \
  | while read -r ref; do git update-ref -d "$ref" 2>/dev/null || true; done

exit 0
