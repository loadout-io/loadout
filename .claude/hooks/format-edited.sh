#!/usr/bin/env bash
# PostToolUse(Edit|Write) — sformatuj plik, który właśnie został zapisany. Po cichu.
#
# Dlaczego hak, a nie zdanie w promcie: „pamiętaj uruchomić formatter" to instrukcja miękka.
# Bieg ją czasem wykonuje, a kiedy nie wykona, checks/quick-fmt.sh świeci na czerwono i
# kosztuje CAŁĄ rundę naprawczą za przecinek. Zmierzone na T-02: osobny commit
# „style(engine): cargo fmt" musiał zrobić operator. Hak usuwa tę klasę czerwieni w całości,
# bo formatowanie przestaje być decyzją kogokolwiek.
#
# Trzy reguły, które ten hak trzyma:
#
# 1. FORMATUJE TYLKO EDYTOWANY PLIK. Nigdy `cargo fmt --all` ani `prettier "src/**"`.
#    Formatowanie całego drzewa dotknęłoby plików spoza bloku OWNS zadania i zrobiłoby
#    z czerwieni formatowania czerwień ZAKRESU — gorszą, bo wygląda na złamanie granicy.
#
# 2. NIGDY NIE BLOKUJE. Zawsze wychodzi zerem. Brakujący formatter to nie jest problem
#    biegu; checks/quick-fmt.sh nadal go zgłosi jako exit 2 (nasza zła konfiguracja),
#    czyli sygnał trafia tam, gdzie ma trafić, a nie w środek pracy pisarza.
#
# 3. NIE DOTYKA PLIKÓW SPOZA src/ I src-tauri/. Reszta repo należy do harnessu.
set -uo pipefail

payload="$(cat 2>/dev/null || true)"
[ -n "$payload" ] || exit 0

f="$(printf '%s' "$payload" | python3 -c '
import json,sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(0)
p = (d.get("tool_input") or {}).get("file_path") or ""
print(p)
' 2>/dev/null || true)"

[ -n "$f" ] && [ -f "$f" ] || exit 0

ROOT="${CLAUDE_PROJECT_DIR:-$PWD}"
case "$f" in
  "$ROOT"/*) rel="${f#"$ROOT"/}" ;;
  *)         exit 0 ;;             # plik spoza projektu — nie nasza sprawa
esac

case "$rel" in
  src-tauri/*.rs|src-tauri/**/*.rs)
    command -v rustfmt >/dev/null 2>&1 || exit 0
    # --edition musi być podany jawnie: rustfmt wołany bezpośrednio nie czyta Cargo.toml
    # i domyśla się 2015, a wtedy `gen`/`async` w edycji 2024 to błąd składni i plik
    # zostaje NIESFORMATOWANY po cichu.
    ed="$(sed -n 's/^edition *= *"\([0-9]*\)".*/\1/p' "$ROOT/Cargo.toml" 2>/dev/null | head -1)"
    rustfmt --edition "${ed:-2024}" "$f" >/dev/null 2>&1 || true
    ;;
  src/*.ts|src/*.tsx|src/*.css|src/*.json|src/**/*.ts|src/**/*.tsx|src/**/*.css|src/**/*.json)
    # Bez `npx`: npx bez node_modules ściąga prettiera z sieci i formatuje INNĄ wersją
    # niż ta, którą sprawdzi bramka. Cicha podmiana narzędzia jest gorsza niż brak haka.
    [ -x "$ROOT/node_modules/.bin/prettier" ] || exit 0
    "$ROOT/node_modules/.bin/prettier" --write "$f" >/dev/null 2>&1 || true
    ;;
esac

exit 0
