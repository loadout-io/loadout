#!/usr/bin/env bash
# Wląduj gałęzie worktree i udowodnij, że całość dalej się trzyma.
#
#   ./integrate.sh S1 S2 S3
#
# Jedna gałąź naraz, pełna bramka po KAŻDEJ. Zmerdżowanie wszystkiego i dopiero
# potem bramka mówi ci, że wynik jest zepsuty; merdżowanie po jednej mówi, KTÓRA
# gałąź go zepsuła. Moduły zielone osobno nie mówią nic o produkcie — to jest to
# samo "zielone z niewłaściwego powodu", tylko piętro wyżej.
#
# Kody wyjścia są przepuszczane z bramki bez zmian: 0 zielono, 1 czerwono,
# 2 nasza konfiguracja jest zepsuta, 3 przerwane / sufit czasu.
set -euo pipefail

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMMON_DIR="$(git -C "$SELF_DIR" rev-parse --path-format=absolute --git-common-dir)"
GIT_DIR="$(git -C "$SELF_DIR" rev-parse --path-format=absolute --git-dir)"
ROOT="$(cd "$(dirname "$COMMON_DIR")" && pwd)"
cd "$ROOT"

[ $# -gt 0 ] || { echo "usage: integrate.sh <branch> [branch...]" >&2; exit 2; }

# W podpiętym worktree --git-dir i --git-common-dir są RÓŻNE (tam .git to plik
# z jedną linią "gitdir: ..."). Lądowanie z wnętrza worktree wmerdżowałoby
# gałęzie w gałąź zadania zamiast w trunk, cicho i nieodwracalnie w połowie.
if [ "$GIT_DIR" != "$COMMON_DIR" ]; then
  echo "run integrate.sh from the main checkout, not from a worktree" >&2
  exit 2
fi

if [ -n "$(git status --porcelain -uall)" ]; then
  echo "the tree is dirty -- commit or stash before landing anything" >&2
  exit 2
fi

if [ ! -x "$ROOT/verify.sh" ]; then
  echo "verify.sh is missing or not executable -- there is no gate to run" >&2
  exit 2
fi

CURRENT="$(git rev-parse --abbrev-ref HEAD)"
for branch in "$@"; do
  if ! git show-ref --verify -q "refs/heads/$branch"; then
    echo "no such branch: $branch" >&2
    exit 2
  fi
  if [ "$branch" = "$CURRENT" ]; then
    echo "$branch is the branch you are on -- nothing to land into" >&2
    exit 2
  fi
done

gate() {
  local rc=0
  "$ROOT/verify.sh" full || rc=$?
  return "$rc"
}

# Bramka RAZ przed pierwszym merdżem.
#
# Cała wartość "bramka po każdej gałęzi" to przypisanie winy. Jeśli trunk był
# czerwony już wcześniej, pierwsza gałąź obrywa za cudzy błąd i człowiek idzie
# czytać niewłaściwy diff. Jeden przebieg z góry to gwarantuje.
echo "── $CURRENT before the first landing ───────────────────────"
rc=0; gate || rc=$?
# Kod 2 na trunku PRZED pierwszym lądowaniem to nie czerwień. Trunk nie nosi TASK.md
# (kopiuje go ship-task.sh na gałąź), więc bramka trafia tam w swojego strażnika pustki:
# „this gate can only report on itself". Traktowanie tego jak czerwonego trunka czyniło
# integrate.sh nieużywalnym w dniu pierwszym — jedyna kopia TASK.md przyjeżdża właśnie
# tą gałęzią, której skrypt odmawiał wmerdżowania. Co egzekwuje trunk, egzekwuje CI
# (`scripts/ci.sh full`), które nie potrzebuje kontraktu zadania.
# ...ale WYLACZNIE wtedy, gdy zadne sprawdzenie nie oddalo 2 samo. Kod 2 ma dwa zrodla i tylko
# jedno z nich jest tu niegrozne: brak kontraktu do osadzenia (poziom bramki) kontra sprawdzenie,
# ktore stwierdzilo, ze NASZA konfiguracja przeczy sobie (np. quick-permissions). Stara wersja
# wybaczala oba, wiec ladowala na drzewie, o ktorym bramka wlasnie powiedziala, ze nie umie go
# osadzic -- a nastepna galaz obrywala za cudza wade. Zmierzone 2026-08-19 po T-53.
if [ "$rc" -eq 2 ]; then
  # KONSERWATYWNIE: wybaczamy tylko przy SWIEZYM paragonie z pusta lista. Brak paragonu,
  # paragon o innym commicie (bramka oddaje 2 takze SCIEZKAMI PRZED zapisem paragonu, wiec
  # zostaje wtedy poprzedni) i nieczytelny JSON znacza to samo -- nie mamy dowodu, ze to ten
  # niegrozny wariant. Domyslna odpowiedzia na brak dowodu jest odmowa, nie zgoda.
  bad="$(python3 - "$ROOT/runs/last.json" "$(git rev-parse HEAD)" <<'PYEOF'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    print("no readable receipt"); raise SystemExit
if d.get("commit") != sys.argv[2]:
    print("receipt describes %s, not this tree" % (d.get("commit") or "no commit")[:8]); raise SystemExit
print(" ".join(d.get("misconfigured") or []))
PYEOF
)"
  if [ -n "$bad" ]; then
    echo >&2
    echo "the gate is MISCONFIGURED on $CURRENT before anything was merged: $bad" >&2
    echo "a check said it cannot judge this tree -- landing on top of that would blame" >&2
    echo "the next branch for it. Fix the configuration first. Nothing was landed." >&2
    exit 2
  fi
  echo
  echo "  (the gate has nothing to judge on $CURRENT yet -- exit 2 is our configuration,"
  echo "   not a red tree. scripts/ci.sh is what gates trunk. Landing anyway.)"
  rc=0
fi
if [ "$rc" -ne 0 ]; then
  echo >&2
  echo "the gate is red on $CURRENT BEFORE anything was merged -- nothing was landed." >&2
  echo "fix trunk first, otherwise the first branch gets blamed for it." >&2
  exit "$rc"
fi

for branch in "$@"; do
  echo "── landing $branch ─────────────────────────────────────────"

  merge_rc=0
  git merge --no-ff -m "chore(main): land $branch" "$branch" || merge_rc=$?
  if [ "$merge_rc" -ne 0 ]; then
    unmerged="$(git diff --name-only --diff-filter=U)"

    if [ -z "$unmerged" ]; then
      # Merdż padł, ale nie na konflikcie treści (np. git odmówił startu).
      # Nie ma czego rozwiązywać, więc sprzątamy i oddajemy sterowanie.
      echo "merge of $branch failed without a content conflict -- see the message above" >&2
      git merge --abort 2>/dev/null || true
      exit 1
    fi

    # TASK.md to nie praca, tylko historia, którą gałąź dostała na wejściu.
    # ship-task.sh kopiuje tasks/<id>.md na TASK.md na KAŻDEJ gałęzi, więc każde
    # drugie lądowanie konfliktuje na ścieżce, której żadne zadanie nie dotyka
    # i której żaden czytelnik nie potrzebuje. Kopia z trunka wygrywa,
    # deterministycznie — a konflikt, który zostanie, jest już prawdziwy.
    if [ "$unmerged" = "TASK.md" ]; then
      git checkout --ours TASK.md
      git add TASK.md
      git commit -q --no-edit
      echo "  (TASK.md resolved to trunk's copy -- the branch's story stays in tasks/)"
    else
      echo "MERGE CONFLICT landing $branch -- two tasks claimed the same lines:" >&2
      printf '%s\n' "$unmerged" | sed 's/^/  /' >&2
      echo "resolve it, commit, then run ./integrate.sh again for the rest" >&2
      echo "to back out instead: git merge --abort" >&2
      exit 1
    fi
  fi

  # TASK.md jest artefaktem GAŁĘZI i nie ma prawa przeżyć lądowania. Zmierzone przy pierwszym
  # udanym landowaniu (2026-08-15): S-1 commitował TASK.md jako pierwszy commit gałęzi, merge
  # wciągnął go na main, i od tej chwili KAŻDY nowy worktree rodził się z cudzym kontraktem
  # w środku — a ship-task.sh słusznie odmawia startu, kiedy widzi TASK.md w świeżej
  # przestrzeni ("a second run there cannot prove the criteria red"). Pętla stanęła na
  # następnym zadaniu, dwie sekundy po starcie, z kodem 2.
  #
  # Trwałym źródłem kontraktu jest tasks/<ID>.md, które i tak leży w repo. Kasujemy więc kopię
  # i doszywamy to do commita lądowania, żeby trunk nigdy nie miał tego pliku ani przez chwilę
  # — także dla bramki, która biegnie zaraz niżej.
  if git ls-files --error-unmatch TASK.md >/dev/null 2>&1; then
    git rm -q TASK.md
    git commit -q --amend --no-edit
    echo "  (TASK.md removed from trunk -- it is a branch artifact; tasks/ holds the contract)"
  fi

  rc=0; gate || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo >&2
    echo "$branch merged cleanly and the gate came back non-zero (exit $rc)." >&2
    echo "That is an integration failure, not a merge one -- the branch was green alone." >&2
    echo "The merge is LEFT IN PLACE so you can read it. To back it out:" >&2
    echo >&2
    echo "    git reset --hard HEAD~1" >&2
    echo >&2
    echo "Nothing after $branch was landed." >&2
    exit "$rc"
  fi

  echo "$branch landed, gate green"
done

echo "all landed"
