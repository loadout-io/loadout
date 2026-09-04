#!/usr/bin/env bash
# Hak PreToolUse na Bash: ciezkie komendy odpala BRAMKA, nie agent.
#
# DLACZEGO SKRYPT, A NIE ZDANIE W PROMPCIE (niezmiennik 28). `harness/prompts/implement.md`
# prosil o to od 2026-09-02 trzema akapitami z liczbami. Zmierzone 2026-09-04 parserem nad
# 1 475 komendami Bash z `runs/z*/build-*.jsonl` (faza implementacji calej fali Z): `cargo
# clippy` 89 razy, pelny albo `--lib` `cargo test` 13, `cargo build` 3. Prosba nie dziala.
# Kazde z tych 105 wywolan to 10-200 s zegara na maszynie, ktora rownolegle prowadzi hak
# Stop i bramke — a zajeta maszyna udaje czerwony test.
#
# AKTYWNY WYLACZNIE W BIEGU HARNESSU. `LOADOUT_HARNESS=1` ustawia `harness/h.py` w srodowisku
# dziecka (`child_env`), wiec sesja czlowieka w tym repo nie czuje tego haka wcale — a to jest
# warunek, zeby go w ogole wpiac: bramka, ktora przeszkadza wlascicielowi, zostanie wylaczona.
#
# `set -e` tu NIE MA, tak samo jak w stop-gate.sh i z tego samego powodu: kod 2 znaczy
# „zablokuj to wywolanie", wiec przerwanie na literowce byloby nie do odroznienia od odmowy.
set -uo pipefail

[ "${LOADOUT_HARNESS:-}" = "1" ] || exit 0

case "${1:-}" in
  --selftest) exec python3 "$(dirname "${BASH_SOURCE[0]}")/pre-bash.py" --selftest ;;
esac

exec python3 "$(dirname "${BASH_SOURCE[0]}")/pre-bash.py"
