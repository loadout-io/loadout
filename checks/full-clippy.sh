#!/usr/bin/env bash
# Pełna forma clippy. TYLKO tutaj, w bramce, raz.
#
# `--all-targets` dokłada testy, benche i przykłady, przez co przemiela profil builda —
# w pętli wewnętrznej to minuty czekania i dlatego AGENTS.md wprost tego zakazuje
# (tabela "zakazane"; raport 04 §6.3). checks/quick-clippy.sh robi wariant `--lib`.
#
# Co ta wersja łapie, a tamta nie: unwrap() i panic!() w kodzie TESTOWYM. Polityka
# [workspace.lints] jest jedna dla całego drzewa, więc test, który ląduje z unwrapem,
# przechodzi pętlę wewnętrzną i zatrzymuje się dopiero tutaj.
#
# Wybór wobec pustego drzewa: identyczny jak w quick-clippy — brak .rs to "nie ma czego
# lintować" i zielono, warunkiem mechanicznym, nie oceną.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"
# shellcheck source=checks/_cargo-serialize.sh
. checks/_cargo-serialize.sh

rs="$(find src-tauri/src -name '*.rs' 2>/dev/null | head -1 || true)"
if [ -z "$rs" ]; then
  echo "clippy: no Rust source yet (src-tauri/src/ holds no .rs), nothing to lint"
  exit 0
fi

# ── polityka lintów jest PODŁĄCZONA ────────────────────────────────────────────────────────
#
# Niezmiennik 23: polityka mieszka w jednym rdzeniu, czyli w `[workspace.lints]` w korzeniowym
# Cargo.toml. Członek podłącza się do niej jedną linią `lints.workspace = true`. Kiedy ta linia
# zniknie — albo, co gorsza, zamieni się w KOMENTARZ — clippy dalej biegnie, dalej kończy zerem
# i dalej nie widzi ani jednego `unwrap()`. Bramka świeci zielono z powodu, który nie ma nic
# wspólnego z jakością kodu. Zmierzone w piaskownicy: ten sam plik z `.unwrap()` w celu
# testowym daje exit 1 z podłączoną polityką i czysty przebieg z polityką w komentarzu.
#
# Czego to sprawdzenie NIE robi i nie ma prawa zrobić: dopisać `-D clippy::unwrap_used` do
# wywołania niżej. To jest przepisanie polityki w adapterze — dokładnie tak umarło po cichu
# skanowanie sekretów na PR #535 [05 §4] — i zamaskowałoby rozłączoną politykę, bo drzewo
# z komentarzem dostałoby wtedy exit 1 ("twój kod jest zły") zamiast 2 ("nasz plik jest zły").
#
# Parsowanie jest SEKCJAMI TOML-a, nie gerpem po ciągu. `grep 'workspace = true'` przechodzi
# na komentarzu i jest literalnie incydentem `--sandbox workspace-write` z raportu 06 §2:
# selftest asertował obecność flagi, przechodził na komentarzu, a żywa flaga brzmiała inaczej.

# Zdejmuje komentarz TOML-a, respektując oba rodzaje cudzysłowów: `libc = "0.2" # powód`
# ma stracić powód, a `name = "a#b"` ma zostać w całości.
TOML_STRIP='
function strip(line,   i, c, out, q, sq, dq) {
  sq = sprintf("%c", 39); dq = sprintf("%c", 34)
  q = ""; out = ""
  for (i = 1; i <= length(line); i++) {
    c = substr(line, i, 1)
    if (q != "") { if (c == q) q = ""; out = out c; continue }
    if (c == sq || c == dq) { q = c; out = out c; continue }
    if (c == "#") break
    out = out c
  }
  return out
}
function header(line,   h) {
  h = line
  sub(/^[[:space:]]*\[/, "", h); sub(/\][[:space:]]*$/, "", h)
  return h
}
'

# Czy korzeń w ogóle deklaruje politykę? Bez `[workspace.lints…]` nie ma do czego podłączać
# i nie ma czego wymagać — mówimy to zdaniem zamiast wymyślać wymaganie.
declares_policy() {
  awk "$TOML_STRIP"'
    { line = strip($0)
      if (line ~ /^[[:space:]]*\[/) {
        h = header(line)
        if (h == "workspace.lints" || h ~ /^workspace\.lints\./) found = 1
      } }
    END { exit found ? 0 : 1 }' "$1"
}

# Ścieżki członków workspace'u, po jednej na linię. Tablica bywa wielolinijkowa.
workspace_members() {
  awk "$TOML_STRIP"'
    function emit(s) {
      while (match(s, /"[^"]*"/)) {
        print substr(s, RSTART + 1, RLENGTH - 2)
        s = substr(s, RSTART + RLENGTH)
      }
    }
    { line = strip($0)
      if (collecting) { buf = buf " " line
        if (index(line, "]") > 0) { collecting = 0; emit(buf) }
        next }
      if (line ~ /^[[:space:]]*\[/) { section = header(line); next }
      if (section == "workspace" && line ~ /^[[:space:]]*members[[:space:]]*=/) {
        rest = line; sub(/^[^=]*=/, "", rest)
        if (index(rest, "]") > 0) emit(rest); else { collecting = 1; buf = rest }
      } }' "$1"
}

# Czy TEN manifest ma AKTYWNY `lints.workspace = true` — jako `[lints]` + `workspace = true`
# albo jako klucz z kropką w tabeli korzeniowej. Obie formy są legalnym TOML-em.
lints_connected() {
  awk "$TOML_STRIP"'
    { line = strip($0)
      if (line ~ /^[[:space:]]*\[/) { section = header(line); next }
      if (line ~ /=/) {
        key = line; sub(/=.*$/, "", key); gsub(/[[:space:]]/, "", key)
        val = line; sub(/^[^=]*=/, "", val); gsub(/[[:space:]]/, "", val)
        full = (section == "") ? key : section "." key
        if (full == "lints.workspace" && val == "true") connected = 1
      } }
    END { exit connected ? 0 : 1 }' "$1"
}

if [ ! -f Cargo.toml ]; then
  echo "our configuration is broken: $ROOT/Cargo.toml is absent, so no lint policy is" >&2
  echo "detail: declared anywhere and clippy would judge this tree against nothing." >&2
  exit 2
fi

if declares_policy Cargo.toml; then
  members="$(workspace_members Cargo.toml)"
  if [ -z "$members" ]; then
    echo "our configuration is broken: Cargo.toml declares [workspace.lints] but its" >&2
    echo "detail: [workspace] members list could not be read, so no member can be checked" >&2
    echo "detail: for the one line that connects it to the policy." >&2
    exit 2
  fi
  while IFS= read -r member; do
    [ -n "$member" ] || continue
    manifest="$member/Cargo.toml"
    if [ ! -f "$manifest" ]; then
      echo "our configuration is broken: [workspace] members names $member, which has no" >&2
      echo "detail: Cargo.toml — the lint policy cannot reach a member that does not exist." >&2
      exit 2
    fi
    if ! lints_connected "$manifest"; then
      echo "our configuration is broken, not the code: the lint policy is not connected" >&2
      echo "  $manifest has no active \`lints.workspace = true\`" >&2
      echo "detail: Cargo.toml declares [workspace.lints], so the policy exists — this member" >&2
      echo "detail: simply does not inherit it, and clippy then judges it against the stock" >&2
      echo "detail: default. A commented-out line reads exactly like a connected one to grep;" >&2
      echo "detail: this check parses TOML sections, so it does not (invariant 23)." >&2
      exit 2
    fi
  done < <(printf '%s\n' "$members")
fi

command -v cargo >/dev/null 2>&1 || { echo "cargo is not on PATH" >&2; exit 2; }
cargo clippy --version >/dev/null 2>&1 \
  || { echo "clippy is missing: rustup component add clippy" >&2; exit 2; }

# Niezmiennik 26: full-test.sh i to sprawdzenie biegną w tej samej fali bramki.
cargo_serialize || exit 2   # 2, nie 1: nic sie nie wykonalo, wiec to nie jest twierdzenie o kodzie (Q-3)

# `--keep-going` NIE jest kosmetyką. Bez niego cargo przestaje kompilować kolejne cele po
# kilku porażkach, więc raport wygląda na kompletny — każdy błąd ma plik i linię — a jest
# PREFIKSEM listy, nie listą. Zmierzone 2026-08-17 na T-30: zmiana trzeciego argumentu
# `run_workflow_inner` łamała sześć plików `tests/runcmd_*.rs`, raport pokazał trzy, więc
# rozszerzenie OWNS objęło trzy — i następna pełna fala padła na czwartym. Ten sam błąd
# dwa razy pod rząd, każdy za jeden pełny bieg.
#
# Naprawa jest tutaj, a nie w cudzej głowie: jeżeli bramka podaje niepełną listę, to każdy,
# kto ją czyta, wyciągnie za wąski wniosek — i będzie miał rację co do tego, co zobaczył.
if ! out="$(cargo clippy --all-targets --keep-going -- -D warnings 2>&1)"; then
  echo "clippy --all-targets found something it will not let through" >&2
  printf '%s\n' "$out" | grep -vE '^\s*(Compiling|Downloading|Updating|Checking|Fresh|Blocking)' \
    | head -60 >&2
  echo "detail: this form also judges tests, benches and examples — quick-clippy does not." >&2
  echo "detail: --keep-going is on, so the files listed above are ALL of them, not the first few." >&2
  exit 1
fi

n="$(find src-tauri/src -name '*.rs' 2>/dev/null | wc -l | tr -d ' ')"
echo "clippy: --all-targets clean over $n Rust files, 0 warnings, [workspace.lints] connected"
