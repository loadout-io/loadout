#!/usr/bin/env bash
# Czy hak `pre-bash` odmawia dokladnie tego, co ma odmawiac.
#
# Tablica przypadkow mieszka OBOK, w `checks/bash-guard-cases.jsonl`, i to jest cala
# tresc tego checka: plik z przypadkami jest sledzony, wiec `harness/guards.sh` dopisuje
# do niego przypadek sprzeczny i sprawdza, ze check czerwienieje. Check, ktorego nie da
# sie obalic, nie dowodzi niczego (niezmiennik 19).
#
# Sadzimy takze cisze poza biegiem: hak bez `LOADOUT_HARNESS=1` ma wyjsc zerem, cokolwiek
# dostanie. Sesja czlowieka w tym repo nie ma go czuc — inaczej zostanie wylaczony.
set -uo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

HOOK=".claude/hooks/pre-bash.sh"
CASES="checks/bash-guard-cases.jsonl"

for f in "$HOOK" ".claude/hooks/pre-bash.py" "$CASES"; do
  if [ ! -f "$f" ]; then
    echo "bash-guard: brak $f, wiec nie ma czym sadzic komend biegu" >&2
    exit 1
  fi
done
# Bit wykonywalnosci. `.claude/settings.json` wola ten hak przez `bash <sciezka>`, wiec
# dzis bit nie jest nosny — i wlasnie dlatego stoi tu asercja: `stop-gate.sh` jest
# wykonywalny, ten ma byc tak samo, a git gubi ten bit przy kazdym zapisie przez plik
# tymczasowy (zlapane przez straznika 2026-09-04, przy pierwszym uruchomieniu).
if [ ! -x "$HOOK" ]; then
  echo "bash-guard: $HOOK nie jest wykonywalny, a drugi hak tego repo jest" >&2
  echo "detail: chmod +x oraz git update-index --chmod=+x, inaczej bit ginie w commicie" >&2
  exit 1
fi

rc=0
LOADOUT_HARNESS=1 bash "$HOOK" --selftest || rc=$?
if [ "$rc" -ne 0 ]; then
  echo "detail: tablica przypadkow to $CASES; kazdy wiersz mowi, czego oczekujemy" >&2
  exit 1
fi

# Cisza poza biegiem. Bez `LOADOUT_HARNESS` hak ma przepuscic nawet to, co w biegu odrzuca.
quiet=0
printf '%s' '{"tool_name":"Bash","tool_input":{"command":"cargo clippy"},"cwd":"'"$PWD"'"}' \
  | env -u LOADOUT_HARNESS bash "$HOOK" >/dev/null 2>&1 || quiet=$?
if [ "$quiet" -ne 0 ]; then
  echo "bash-guard: hak odmowil poza biegiem harnessu (kod $quiet)" >&2
  echo "detail: bez LOADOUT_HARNESS=1 ma wychodzic zerem — inaczej przeszkadza czlowiekowi" >&2
  exit 1
fi

# Kontrola pozytywna na zywym procesie, nie tylko w tablicy: jedna komenda przez stdin.
live=0
printf '%s' '{"tool_name":"Bash","tool_input":{"command":"cargo clippy"},"cwd":"'"$PWD"'"}' \
  | LOADOUT_HARNESS=1 bash "$HOOK" >/dev/null 2>&1 || live=$?
if [ "$live" -ne 2 ]; then
  echo "bash-guard: hak w biegu oddal kod $live na `cargo clippy`, a kontrakt PreToolUse" >&2
  echo "detail: odmawia wylacznie kodem 2 — kazdy inny kod PRZEPUSZCZA komende" >&2
  exit 1
fi

echo "bash-guard: tablica zgodna, hak milczy poza biegiem i odmawia kodem 2 w biegu"
exit 0
