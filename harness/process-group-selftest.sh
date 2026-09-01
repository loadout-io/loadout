#!/usr/bin/env bash
# Odtwarza incydent T-129 bez prawdziwego vendora: agent ignoruje TERM i pisze dalej,
# a zewnetrzny Ctrl-C trafia tylko do grupy wolajacego. Zielone wymaga ESRCH i ciszy po exit.
#
# Dwa adaptery tej samej polityki, oba sprawdzane naprawde:
#   * PISARZ z ship.sh -- `write_with` + `_loadout_spawn_writer`, wyciete z zywego pliku
#     i uruchomione w piaskownicy. Do 2026-08-28 pisarz NIE biegl pod ta polityka (stary
#     ship-task.sh wolal go zwyklym podshellem), czyli najdluzszy i najdrozszy proces
#     w biegu byl jedynym bez dowodu smierci.
#   * RECENZENT z review.sh -- przerwanie i sufit czasu.
#
# `repair.sh` byl tu trzecim przypadkiem do 2026-08-28; odszedl razem ze starym harnessem,
# a jego role -- runde naprawcza -- przejal pisarz, ktory jest teraz sprawdzany wyzej.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
SANDBOX="$(mktemp -d)"
cleanup() { rm -rf "$SANDBOX"; }
trap cleanup EXIT

mkdir -p "$SANDBOX/harness" "$SANDBOX/runs" "$SANDBOX/bin"
cp "$ROOT/review.sh" "$SANDBOX/"
cp "$ROOT/harness/process-group.sh" "$ROOT/harness/review-schema.json" "$SANDBOX/harness/"

# Pisarz jest WYCIETY Z ZYWEGO ship.sh, nie przepisany tutaj. Kopia promptu w selftescie
# to dokladnie ten rodzaj testu, ktory przechodzi na wlasnej kopii i nie widzi regresji
# w kodzie produkcyjnym (niezmiennik 20).
python3 - "$ROOT/ship.sh" "$SANDBOX/writer.sh" <<'EXTRACT'
import io, sys

lines = io.open(sys.argv[1], encoding="utf-8").read().split("\n")

def body(name):
    head = [k for k, l in enumerate(lines) if l.startswith(name + "() {")]
    if len(head) != 1:
        sys.exit("%s() wystepuje %d razy w ship.sh" % (name, len(head)))
    i = head[0]
    j = next(k for k in range(i + 1, len(lines)) if lines[k] == "}")
    return "\n".join(lines[i:j + 1])

io.open(sys.argv[2], "w", encoding="utf-8").write("""#!/usr/bin/env bash
# GENEROWANY przez harness/process-group-selftest.sh z zywego ship.sh. Nie edytuj.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd -P)"
cd "$ROOT"
. "$ROOT/harness/process-group.sh"
AGENT=codex
WT="$ROOT"
GIT_COMMON="$ROOT"
LOADOUT_CODEX_MODEL=fake-model
LOADOUT_CODEX_EFFORT=low
LOADOUT_CLAUDE_MODEL=fake-model
LOADOUT_CLAUDE_EFFORT=low
%s

%s

%s

trap ship_interrupted INT TERM
rc=0
write_with "$ROOT/writer.jsonl" 10 <<'PROMPT' || rc=$?
implement the planted behaviour
PROMPT
exit "$rc"
""" % (body("ship_interrupted"), body("_loadout_spawn_writer"), body("write_with")))
EXTRACT
chmod +x "$SANDBOX/writer.sh"

cat > "$SANDBOX/TASK.md" <<'TASK'
# process group selftest

## AC-1 przerwanie zabija agenta
check: bash probe.sh
expect: (\d+) passed
TASK
cat > "$SANDBOX/runs/last.json" <<'JSON'
{"failed":["AC-1"]}
JSON
cat > "$SANDBOX/verify.sh" <<'VERIFY'
#!/usr/bin/env bash
if [ "${1:-}" = "--report" ]; then
  echo "AC-1 failed on the planted behavior"
else
  echo "1 passed"
fi
VERIFY
chmod +x "$SANDBOX/verify.sh"

# Planista (tryb read-only bez schematu) konczy od razu. Pisarz i recenzent ignoruja TERM
# i stale pisza marker, wiec tylko poprawna eskalacja calego PGID moze zatrzymac selftest.
cat > "$SANDBOX/bin/codex" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
mode=""; out=""; review=0
while [ $# -gt 0 ]; do
  case "$1" in
    -s) mode="${2:-}"; shift 2 ;;
    -o) out="${2:-}"; shift 2 ;;
    --output-schema) review=1; shift 2 ;;
    *) shift ;;
  esac
done
if [ "$mode" = read-only ] && [ "$review" = 0 ]; then
  printf 'repair the planted behavior\n' > "$out"
  exit 0
fi
pgid="$(ps -o pgid= -p "$$" | tr -d ' ')"
printf '%s %s\n' "$$" "$pgid" > "$LOADOUT_SELFTEST_PID"
trap '' INT TERM
while :; do
  printf 'still alive\n' >> "$LOADOUT_SELFTEST_ACTIVITY"
  sleep 0.05
done
FAKE
chmod +x "$SANDBOX/bin/codex"

git -C "$SANDBOX" init -q -b main
git -C "$SANDBOX" -c user.email=ci@loadout -c user.name=ci add -A
git -C "$SANDBOX" -c user.email=ci@loadout -c user.name=ci commit -q -m selftest

python3 - "$SANDBOX" <<'PY'
import os
import pathlib
import signal
import subprocess
import sys
import time

root = pathlib.Path(sys.argv[1])


def wait_for(path, process, seconds=10):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if path.exists() and path.stat().st_size:
            return
        if process.poll() is not None:
            raise RuntimeError("harness exited before the fake agent started")
        time.sleep(0.02)
    raise RuntimeError("fake agent did not start before the watchdog")


def run_case(name, argv, send_interrupt=True, expected_rc=3, extra_env=None):
    pid_file = root / (name + ".pid")
    activity = root / (name + ".activity")
    log = root / (name + ".log")
    env = dict(os.environ)
    env["PATH"] = str(root / "bin") + os.pathsep + env.get("PATH", "")
    env["LOADOUT_SELFTEST_PID"] = str(pid_file)
    env["LOADOUT_SELFTEST_ACTIVITY"] = str(activity)
    env["LOADOUT_AGENT_GROUP_GRACE_TICKS"] = "5"
    env.update(extra_env or {})
    process = None
    agent_pgid = None
    try:
        with log.open("wb") as stream:
            process = subprocess.Popen(
                argv,
                cwd=root,
                env=env,
                stdout=stream,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            wait_for(pid_file, process)
            agent_pid, agent_pgid = (int(value) for value in pid_file.read_text().split())
            if agent_pid != agent_pgid:
                raise RuntimeError(
                    "%s fake agent PID %s is not its PGID %s" % (name, agent_pid, agent_pgid)
                )
            if agent_pgid == process.pid:
                raise RuntimeError("%s fake agent shares the outer harness PGID" % name)
            wait_for(activity, process)
            if send_interrupt:
                os.killpg(process.pid, signal.SIGINT)
                time.sleep(0.05)
                # Drugi Ctrl-C podczas laski TERM nie moze zabic powloki przed KILL i proof.
                try:
                    os.killpg(process.pid, signal.SIGINT)
                except ProcessLookupError:
                    pass
            rc = process.wait(timeout=10)
        if rc != expected_rc:
            raise RuntimeError("%s returned %s, expected %s" % (name, rc, expected_rc))
        try:
            os.killpg(agent_pgid, 0)
        except ProcessLookupError:
            pass
        else:
            raise RuntimeError("%s left agent PGID %s alive" % (name, agent_pgid))
        before = activity.stat().st_size
        time.sleep(0.3)
        after = activity.stat().st_size
        if after != before:
            raise RuntimeError("%s agent wrote after the harness returned" % name)
    except Exception as error:
        if process is not None and process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        if agent_pgid is not None:
            try:
                os.killpg(agent_pgid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        detail = log.read_text(errors="replace") if log.exists() else "(no log)"
        raise SystemExit("%s\n--- %s log ---\n%s" % (error, name, detail))


run_case("writer", ["bash", "writer.sh"])
run_case("review", ["bash", "review.sh", "--agent", "codex", "--reviewer", "codex"])
# Sufit czasu sprawdzamy na recenzencie, bo tam OCZEKIWANY kod wyjscia jest zdefiniowany:
# review.sh jest zawsze-zerem poza wlasna dwojka (D3). Pisarz zabity na sufit oddaje kod
# zabitego procesu, ktory zalezy od systemu -- asercja na nim mierzylaby platforme, nie polityke.
run_case(
    "timeout",
    ["bash", "review.sh", "--agent", "codex", "--reviewer", "codex"],
    send_interrupt=False,
    expected_rc=0,
    # review.sh ma WLASNA nazwe budzetu (LOADOUT_REVIEW_BUDGET, domyslnie 900 s);
    # LOADOUT_EXEC_BUDGET nalezal do repair.sh i odszedl razem z nim. Podanie nie tej nazwy
    # daje przypadek, ktory czeka 900 s i przewraca sie na watchdogu selftestu -- czyli
    # zielone bez pomiaru byloby tu niemozliwe, ale czerwone tez nic nie mowi.
    extra_env={"LOADOUT_REVIEW_BUDGET": "0.1"},
)
print("harness process groups: SIGINT, timeout, ESRCH and no post-exit writes (3 passed)")
PY
