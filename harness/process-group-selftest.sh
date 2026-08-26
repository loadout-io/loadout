#!/usr/bin/env bash
# Odtwarza incydent T-129 bez prawdziwego vendora: agent ignoruje TERM i pisze dalej,
# a zewnetrzny Ctrl-C trafia tylko do grupy review/repair. Zielone wymaga ESRCH i ciszy po exit.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
SANDBOX="$(mktemp -d)"
cleanup() { rm -rf "$SANDBOX"; }
trap cleanup EXIT

mkdir -p "$SANDBOX/harness" "$SANDBOX/runs" "$SANDBOX/bin"
cp "$ROOT/repair.sh" "$ROOT/review.sh" "$SANDBOX/"
cp "$ROOT/harness/process-group.sh" "$ROOT/harness/review-schema.json" "$SANDBOX/harness/"

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

# Planista repair konczy od razu. Pisarz repair i recenzent review ignorują TERM i stale
# pisza marker, wiec tylko poprawna eskalacja calego PGID moze zatrzymac selftest.
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


run_case("repair", ["bash", "repair.sh", "--agent", "codex", "--reviewer", "codex"])
run_case("review", ["bash", "review.sh", "--agent", "codex", "--reviewer", "codex"])
run_case(
    "timeout",
    ["bash", "repair.sh", "--agent", "codex", "--reviewer", "codex"],
    send_interrupt=False,
    expected_rc=0,
    extra_env={"LOADOUT_EXEC_BUDGET": "0.1"},
)
print("harness process groups: SIGINT, timeout, ESRCH and no post-exit writes (3 passed)")
PY
