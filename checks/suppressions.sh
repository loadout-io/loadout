#!/usr/bin/env bash
# quick-suppressions — jedna linia w pliku, który zadanie posiada, wyłącza bramkę typów
# albo lintu. Zmierzone (N-09, audyt 2026-08-15), na naprawionym już tsconfigu:
#
#   export const n: number = "not a number";     -> rc 1  TS2322
#   // @ts-nocheck   + ta sama linia             -> rc 0  "1 files strict, no implicit any"
#   // @ts-expect-error + ta sama linia          -> rc 0
#
# `#[allow(clippy::unwrap_used)]` nad funkcją bije workspace'owe `deny` pod `-D warnings`;
# `// prettier-ignore` robi to samo dla formatu. Każde z nich to JEDNA linia, wewnątrz src/
# albo src-tauri/src/, w pełni wewnątrz bloku OWNS — niewidoczna dla quick-scope,
# quick-boundary, quick-tokens i quick-vocabulary. Najtańszy możliwy sposób na zaliczenie
# kryterium, i w diffie wygląda jak zwykły kod.
#
# Referencja: spreadsheet/checks/fast-quality.sh:4-6 — „--max-warnings 0 jest nośne:
# noInlineConfig degraduje komentarz wyłączający do OSTRZEŻENIA".
#
# Kształt dwóch plików (baseline + allowlist) jest ten sam, co w quick-vocabulary, ale
# BEZ zapisywalnego --update-baseline (N-10): baseline wolno tylko opuszczać, i tylko ręcznie.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

ALLOWLIST="checks/suppressions-allowlist.json"

PATTERN='@ts-nocheck|@ts-ignore|@ts-expect-error|eslint-disable|#!?\[allow\(clippy::|#!?\[allow\(unsafe|#!?\[allow\(dead_code|prettier-ignore|// *rustfmt::skip|#!?\[cfg_attr\(.*rustfmt'

dirs=()
[ -d src ] && dirs+=(src)
[ -d src-tauri/src ] && dirs+=(src-tauri/src)
if [ "${#dirs[@]}" -eq 0 ]; then
  echo "suppressions: no src/ or src-tauri/src/ yet, nothing to suppress"
  exit 0
fi

hits="$(grep -rnE "$PATTERN" "${dirs[@]}" 2>/dev/null || true)"

if [ -z "$hits" ]; then
  echo "suppressions: none in ${dirs[*]}"
  exit 0
fi

# Allowlist: `"plik:termin"` z pisemnym powodem. Brak pliku = brak wyjątków, co jest
# poprawnym stanem startowym i nie jest błędem konfiguracji.
allowed=""
if [ -f "$ALLOWLIST" ]; then
  allowed="$(python3 -c "
import json,sys
try: d=json.load(open('$ALLOWLIST'))
except Exception as e: print('BADJSON:%s'%e); sys.exit(0)
for e in d.get('entries',[]):
    if not e.get('reason'): print('NOREASON:%s'%e.get('id','?'))
    else: print(e.get('id',''))
")"
  case "$allowed" in
    BADJSON:*) echo "$ALLOWLIST is not valid JSON: ${allowed#BADJSON:}" >&2; exit 2 ;;
    NOREASON:*) echo "$ALLOWLIST has an entry without a written reason: ${allowed#NOREASON:}" >&2; exit 2 ;;
  esac
fi

live=""
while IFS= read -r h; do
  [ -z "$h" ] && continue
  file="${h%%:*}"
  term="$(printf '%s' "$h" | grep -oE "$PATTERN" | head -1)"
  id="$file::$term"
  if printf '%s\n' "$allowed" | grep -qxF "$id"; then continue; fi
  live+="  $h"$'\n'
done <<< "$hits"

if [ -n "$live" ]; then
  echo "a suppression comment turns off a gate from inside code this task owns" >&2
  printf '%s' "$live" >&2
  echo "" >&2
  echo "These are invisible to every other check: they sit inside the OWNS block and read as" >&2
  echo "ordinary code. If one is genuinely necessary, add it to $ALLOWLIST with a written" >&2
  echo "reason — that is the only way through, and a human reads it." >&2
  exit 1
fi

echo "suppressions: none outside the allowlist"
