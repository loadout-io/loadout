#!/usr/bin/env bash
# Wytnij izolowaną kopię repo dla jednego agenta i zrób ją używalną.
#
#   ./worktree.sh S3            -> ../loadout-S3 na gałęzi S3
#   FROM=main ./worktree.sh S3  -> to samo, ale odbite od main zamiast HEAD
#
# Na stdout leci JEDNA linia: ścieżka. To jest cały interfejs — ship-task.sh
# i integrate.sh składają się na tym echu, nic więcej z tego skryptu nie czytają.
# Wszystko pozostałe (port, notatki, ostrzeżenia) idzie na stderr.
set -euo pipefail

NAME="${1:-}"
PORT="${2:-}"

if [ -z "$NAME" ]; then
  echo "usage: worktree.sh <name> [port]" >&2
  exit 2
fi

# Nazwa jest jednocześnie nazwą gałęzi i składnikiem ścieżki obok repo.
# "feat/x" dałoby katalog ../loadout-feat/x, czyli cichy bałagan piętro wyżej.
case "$NAME" in
  *[!A-Za-z0-9._-]*|-*|.*)
    echo "bad worktree name '$NAME': use [A-Za-z0-9._-], not starting with - or ." >&2
    exit 2
    ;;
esac

SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# W podpiętym worktree `.git` jest PLIKIEM ("gitdir: ..."), nie katalogiem.
# --git-common-dir zawsze wskazuje katalog .git głównego worktree, więc
# ./worktree.sh odpalone z wnętrza worktree nadal wycina obok GŁÓWNEGO repo,
# a nie obok kopii. Bez tego drzewo kopii rosłoby w bok w nieskończoność.
COMMON_DIR="$(git -C "$SELF_DIR" rev-parse --path-format=absolute --git-common-dir)"
ROOT="$(cd "$(dirname "$COMMON_DIR")" && pwd)"

# Nazwa katalogu: małą literą, bo repo nazywa się Loadout, a ../Loadout-S3
# obok ../loadout-S3 to dwie ścieżki, które w Finderze wyglądają tak samo.
PREFIX="$(basename "$ROOT" | tr '[:upper:]' '[:lower:]')"
DEST="$(dirname "$ROOT")/${PREFIX}-${NAME}"

if ! git -C "$ROOT" rev-parse --verify -q HEAD >/dev/null; then
  # Nie da się odbić gałęzi od niczego. To nasza konfiguracja, nie awaria testu.
  echo "this repo has no commits yet -- commit once before cutting a worktree" >&2
  exit 2
fi

# Idempotencja: powtórzone wywołanie oddaje tę samą ścieżkę zamiast się wywalać.
# ship-task.sh bywa wznawiany po przerwanym biegu i nie ma czego naprawiać.
if git -C "$ROOT" worktree list --porcelain | grep -qxF "worktree $DEST"; then
  echo "worktree $NAME already exists, reusing it" >&2
  echo "$DEST"
  exit 0
fi

if [ -e "$DEST" ]; then
  echo "$DEST exists but git does not know it as a worktree -- move it away first" >&2
  exit 2
fi

if git -C "$ROOT" show-ref --verify -q "refs/heads/$NAME"; then
  echo "branch $NAME already exists, attaching the worktree to it" >&2
  git -C "$ROOT" worktree add -q "$DEST" "$NAME"
else
  git -C "$ROOT" worktree add -q -b "$NAME" "$DEST" "${FROM:-HEAD}"
fi

# ── port ─────────────────────────────────────────────────────────────────────
#
# Wyprowadzony z NAZWY, nigdy z liczby żywych worktree. Liczenie było błędne
# dwa razy naraz (raport 06 §5): dwa biegi wystartowane razem czytają ten sam
# licznik i dostają ten sam port — a cały powód istnienia portu per worktree
# jest taki, że kilka biegów działa jednocześnie.
#
# Z nazwy jest deterministycznie (ponowne wycięcie S3 odzyskuje port S3)
# i bez wyścigu (nie ma wspólnego stanu do odczytu). Potem i tak sondujemy,
# bo deterministyczny to nie to samo co wolny.
BASE_PORT="${LOADOUT_PORT_BASE:-5300}"   # 5273 trzyma główny dev server (vite.config.ts)
SPAN=80

if [ -z "$PORT" ]; then
  OFFSET=$(( $(printf '%s' "$NAME" | cksum | cut -d' ' -f1) % SPAN ))
  for try in $(seq 0 $((SPAN - 1))); do
    CAND=$(( BASE_PORT + (OFFSET + try) % SPAN ))
    if command -v nc >/dev/null 2>&1; then
      if ! nc -z 127.0.0.1 "$CAND" 2>/dev/null; then PORT="$CAND"; break; fi
    else
      PORT="$CAND"; break   # bez nc bierzemy deterministyczny strzał i mówimy o tym niżej
    fi
  done
  if [ -z "$PORT" ]; then
    echo "no free port in ${BASE_PORT}..$((BASE_PORT + SPAN - 1))" >&2
    exit 2
  fi
  command -v nc >/dev/null 2>&1 || echo "note: nc missing, port $PORT was not probed" >&2
fi

# Port ląduje w PRYWATNYM katalogu git tego worktree, nie w pliku .port w drzewie.
#
# Źródłowe repo trzymało .port w drzewie roboczym i zapłaciło za to dwa razy:
# skomitowany .port wjeżdżał z gałęzią na trunk (trunk dostawał port worktree,
# który wciąż na nim słuchał), a nieskomitowany czytał się checkowi zakresu jako
# zapis poza dozwolonymi ścieżkami. Katalog git jest per-worktree, `git status`
# nigdy do niego nie zagląda, i umiera razem z worktree.
#
# W podpiętym worktree to NIE jest "$DEST/.git" — tam leży plik z jedną linią.
GITDIR="$(git -C "$DEST" rev-parse --absolute-git-dir)"
printf '%s\n' "$PORT" > "$GITDIR/loadout-port"

# ── node_modules ─────────────────────────────────────────────────────────────
#
# Klon copy-on-write (APFS), nie symlink: symlink dzieliłby jedno drzewo między
# worktree, więc `npm install` w jednym przepisałby zależności drugiego w trakcie
# jego biegu. Na APFS `cp -Rc` jest praktycznie darmowe; fallback na zwykłe
# kopiowanie jest wolny, ale poprawny.
if [ -d "$ROOT/node_modules" ] && [ ! -e "$DEST/node_modules" ]; then
  cp -Rc "$ROOT/node_modules" "$DEST/node_modules" 2>/dev/null \
    || { echo "note: APFS clone unavailable, copying node_modules (slow)" >&2
         cp -R "$ROOT/node_modules" "$DEST/node_modules"; }
fi

# ── target/ ──────────────────────────────────────────────────────────────────
#
# Wspólny katalog build, przez symlink. Zimny `cargo clippy --lib` w świeżym
# worktree to minuty, a sufit tieru quick wynosi 45 s — osobny target znaczyłby,
# że bramka mierzy stan cache'u, nie kod.
#
# Skutek uboczny jest tu POŻĄDANY: cargo bierze lock na katalog target, więc dwa
# ciężkie buildy szeregują się zamiast biec naraz (niezmiennik 26 — kilka
# równoległych linków przypina kompresor pamięci macOS i zamraża maszynę).
# Drogie zależności są dzielone, nasze crate'y przebudowują się przy przełączeniu.
# Wyłączenie: LOADOUT_SHARE_TARGET=0.
if [ "${LOADOUT_SHARE_TARGET:-1}" = "1" ] && [ ! -e "$DEST/target" ]; then
  mkdir -p "$ROOT/target"
  ln -s "$ROOT/target" "$DEST/target"
fi

# ── zaufanie do workspace'u ──────────────────────────────────────────────────
#
# Nieufany workspace CICHO wyrzuca każdy wpis permissions.allow: notatka idzie na
# stderr, bieg leci dalej, a headless agent przepala cały budżet na odmowach
# zapisu, których nikt nie zatwierdzi. Wygląda to jak błąd rozumowania modelu,
# a jest błędem setupu. Piszemy atomowo, bo to konfiguracja użytkownika.
if [ -f "$HOME/.claude.json" ]; then
  python3 - "$DEST" <<'PY' || echo "note: could not mark the workspace trusted for claude" >&2
import json, os, sys
p = os.path.expanduser("~/.claude.json")
with open(p) as fh:
    d = json.load(fh)
for path in {sys.argv[1], os.path.realpath(sys.argv[1])}:
    d.setdefault("projects", {}).setdefault(path, {})["hasTrustDialogAccepted"] = True
tmp = p + ".tmp"
with open(tmp, "w") as fh:
    json.dump(d, fh, indent=2)
os.replace(tmp, p)
PY
fi

# Codex trzyma to samo w ~/.codex/config.toml jako [projects."<ścieżka>"].
# Dopisujemy tylko wtedy, gdy ścieżki tam jeszcze nie ma — powtórzona tabela
# to błąd parsowania TOML, czyli zepsuta konfiguracja użytkownika.
CODEX_CFG="${CODEX_HOME:-$HOME/.codex}/config.toml"
if [ -f "$CODEX_CFG" ] && ! grep -qF "[projects.\"$DEST\"]" "$CODEX_CFG"; then
  printf '\n[projects."%s"]\ntrust_level = "trusted"\n' "$DEST" >> "$CODEX_CFG"
fi

echo "port $PORT (read it from $GITDIR/loadout-port)" >&2
echo "$DEST"
