#!/usr/bin/env python3
"""h — maly harness Loadouta: prompt -> plan -> kod -> checki + weryfikacja -> max 2 poprawki.

Bez poziomow bramki, bez plikow zadan, bez blokow OWNS, bez odwracania kryteriow,
bez paragonow, bez NOT_A_REAL_RED, bez recenzenta ze schematem findingow. Poprzednik
mial 9323 linie w czternastu plikach i to jest dokladny powod, dla ktorego go nie ma.

ZAMIERZONA GRANICA: ten plik ma zostac maly. Jesli rosnie powyzej ~500 linii, cos tu
nie pasuje. Zanim cokolwiek dopiszesz, sprawdz w `runs/`, czy to kiedykolwiek zlapalo
realny blad.

Czego tu swiadomie NIE MA, i co kazde z tego kosztowalo, zmierzone na 121 biegach
starego harnessu (2026-08):
  * DWA przebiegi `verify.sh full` na bieg = 640 s na przebudowanie rzeczy, ktorych bieg
    nie tknal. `full` to 319 s, z czego 280 s (88%) suita CALEGO repo;
  * obowiazkowa recenzja: 97 uwag na 105 recenzji, wiec runda naprawcza odpalala sie
    w 81% biegow i regularnie trwala dluzej niz implementacja;
  * `tasks/*.md`: 26 617 linii kontraktow pisanych RECZNIE przed biegiem.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HDIR = ROOT / ".loadout" / "h"
CFG = json.loads((HDIR / "checks.json").read_text(encoding="utf-8"))
STATE_DIR = ROOT / ".git" / "h"
MAX_FIX_ROUNDS = 2

# `target/` NIE jest dzielony miedzy worktree, i to jest decyzja o POPRAWNOSCI, nie
# o wydajnosci. Odtworzone w ../meetnotes przy ZEROWEJ rownoleglosci: dwa checkouty
# o tej samej nazwie pakietu, wersji i ukladzie WZGLEDNYM, budowane przez jeden
# CARGO_TARGET_DIR, daja jeden odcisk metadanych. Sekwencja `build A; build B; build A`
# melduje A jako `Fresh`, podczas gdy rlib na dysku zbudowano ze zrodel B -- czyli check
# potrafi osadzic CUDZY kod i zameldowac zielen. Do tego zmierzone tutaj 2026-08-17:
# 24 worktree na jeden `target/` = 66 GB i 886 645 plikow, a rozjazd odciskow przebudowywal
# drzewo przy KAZDYM przelaczeniu.
#
# Wydajnosc bierzemy wiec z drugiego lewara, tego bezpiecznego: checki lecą TYLKO wtedy,
# gdy ich sciezki sie zmienily, i sa zawezane do tego, co zmienione (`scoped`).
# Odwrocenie tej decyzji: LOADOUT_SHARE_TARGET=1 w worktree.sh, wylacznie do odtworzenia pomiaru.

VERIFY_SCHEMA = {
    "type": "object",
    "properties": {
        "werdykt": {"type": "string", "enum": ["DZIALA", "NIE_DZIALA", "NIE_WIEM"]},
        "co_nie_dziala": {"type": "string"},
        "jak_naprawic": {"type": "string"},
    },
    "required": ["werdykt", "co_nie_dziala", "jak_naprawic"],
    "additionalProperties": False,
}

# Licznik przejsc (niezmiennik 19: kod wyjscia to nie dowod). Kod testowany biegnie w tym
# samym procesie, ktorego kod wyjscia czytasz, wiec `os._exit(0)` na poziomie modulu
# zazielenia cala suite, a filtr, ktory nic nie dopasowal, konczy sie zerem. To 15 linii
# i jedyna rzecz, ktora z calej starej maszynerii dowodowej tu zostala.
PASS_COUNT = re.compile(r"(?:test result: ok\. (\d+) passed|Tests\s+(?:\S+\s+)?(\d+) passed)")


def log(msg):
    print("\033[36m[h]\033[0m %s" % msg, flush=True)


def die(msg, code=1):
    print("\033[31m[h] %s\033[0m" % msg, file=sys.stderr, flush=True)
    raise SystemExit(code)


def glob_re(pattern):
    """Glob -> regex. `**` przechodzi przez `/`, `*` nie."""
    out, i = [], 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out.append("(?:.*/)?"); i += 3
        elif pattern.startswith("**", i):
            out.append(".*"); i += 2
        elif pattern[i] == "*":
            out.append("[^/]*"); i += 1
        elif pattern[i] == "?":
            out.append("[^/]"); i += 1
        else:
            out.append(re.escape(pattern[i])); i += 1
    return re.compile("^" + "".join(out) + "$")


def matches(path, patterns):
    return any(glob_re(p).match(path) for p in patterns)


def git(*args, cwd=None, check=True, strip=True):
    """strip=False dla --porcelain: wiodaca spacja pierwszej linii NIESIE ZNACZENIE."""
    r = subprocess.run(["git", *args], cwd=str(cwd or ROOT), capture_output=True, text=True)
    if check and r.returncode != 0:
        die("git %s -> %s" % (" ".join(args), r.stderr.strip()))
    return r.stdout.strip() if strip else r.stdout


def state_path(task_id):
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    return STATE_DIR / ("%s.json" % task_id)


def load_state(task_id):
    p = state_path(task_id)
    return json.loads(p.read_text(encoding="utf-8")) if p.exists() else {}


def save_state(task_id, **kw):
    s = load_state(task_id)
    s.update(kw)
    state_path(task_id).write_text(json.dumps(s, indent=2, ensure_ascii=False), encoding="utf-8")
    return s


# ---------------------------------------------------------------------- checki

def changed_paths(wt):
    out = git("status", "--porcelain=v1", "--untracked-files=all", cwd=wt, strip=False)
    paths = []
    for line in out.splitlines():
        if len(line) < 4:
            continue
        p = line[3:].strip()
        if " -> " in p:
            p = p.split(" -> ", 1)[1]
        p = p.strip('"')
        if p and not Path(p).name.startswith(".h-"):
            paths.append(p)
    return paths


def rust_modules(paths):
    """Nazwy modulow celu `it` z dotknietych plikow testowych."""
    mods = []
    for p in paths:
        if p.startswith("src-tauri/tests/it/") and p.endswith(".rs"):
            stem = Path(p).stem
            if stem != "main" and stem not in mods:
                mods.append(stem)
    return mods


def vitest_specs(paths):
    return [p for p in paths
            if re.search(r"\.(test|spec)\.[jt]sx?$", p) and (p.startswith("src/") or p.startswith("e2e/"))]


def scope_cmd(kind, cmd, paths):
    """Zawez check do tego, co realnie zmienione. To jest CALA oszczednosc czasu."""
    limit = CFG.get("scope_limit", 6)
    if kind == "cargo":
        mods = rust_modules(paths)
        if mods and len(mods) <= limit:
            # Jeden cel `it`, filtr po sciezce modulu. Filtr, ktory nic nie dopasuje, da
            # `0 passed` i polegnie na regule licznika przejsc nizej -- wiec zawezenie
            # nie moze po cichu zazielenic checka.
            # Filtry ida PO `--`, razem z --test-threads=1: pozycyjne argumenty za dwoma
            # myslnikami to filtry runnera. Jednowatkowo takze przy zawezeniu -- flake
            # w trzech testach procesowych nie zaleza od tego, ile testow leci obok.
            return ("cargo test --test it -- --test-threads=1 "
                    + " ".join("%s::" % m for m in mods))
    if kind == "vitest":
        specs = vitest_specs(paths)
        if specs and len(specs) <= limit:
            return cmd + " " + " ".join(specs)
    return cmd


def derive_checks(paths):
    picked = []
    for cid, spec in CFG["checks"].items():
        if cid.startswith("_") or not any(matches(p, spec["when"]) for p in paths):
            continue
        cmd = spec["cmd"]
        if spec.get("scoped"):
            cmd = scope_cmd(spec["scoped"], cmd, paths)
        picked.append((cid, cmd, spec.get("cwd"), spec.get("budget_s", 900),
                       bool(spec.get("counts_tests"))))
    return picked


def run_check(cid, cmd, cwd, budget, counts, wt):
    where = Path(wt) / cwd if cwd else Path(wt)
    log("check %s: %s" % (cid, cmd))
    t0 = time.time()
    env = dict(os.environ, CI="1", NO_COLOR="1", FORCE_COLOR="0", CARGO_TERM_COLOR="never")
    try:
        r = subprocess.run(["/bin/bash", "-c", cmd], cwd=str(where), capture_output=True,
                           text=True, timeout=budget, env=env, start_new_session=True)
        out, code = (r.stdout + r.stderr), r.returncode
    except subprocess.TimeoutExpired as e:
        out = ((e.stdout or b"").decode("utf-8", "replace") if isinstance(e.stdout, bytes)
               else (e.stdout or "")) + "\n[TIMEOUT po %ds]" % budget
        code = 124
    dt = time.time() - t0
    reason = ""
    if code == 0 and counts:
        counted = [int(g) for m in PASS_COUNT.finditer(out) for g in m.groups() if g]
        if not counted or max(counted) == 0:
            code, reason = 1, ("exit 0, ale runner nie zameldowal ani jednego przejscia -- "
                               "kod wyjscia to nie dowod (niezmiennik 19)")
    ok = code == 0
    log("  %s %s (%ds)%s" % ("OK  " if ok else "FAIL", cid, dt, "  " + reason if reason else ""))
    if not ok:
        # Pokaz POWOD od razu. Bez tego patrzysz na "FAIL rust-clippy (35s)" i czekasz
        # na weryfikatora, zeby sie dowiedziec, co sie stalo.
        for line in (reason or out).strip().splitlines()[-25:]:
            print("      %s" % line)
    return {"id": cid, "ok": ok, "cmd": cmd, "seconds": round(dt),
            "tail": (reason + "\n" + out)[-4000:] if not ok else ""}


def phase_check(wt):
    paths = changed_paths(wt)
    if not paths:
        return [], paths
    picked = derive_checks(paths)
    if not picked:
        log("zaden check nie pasuje do zmienionych sciezek: %s" % ", ".join(paths[:5]))
        return [], paths
    return [run_check(*c, wt) for c in picked], paths


# ---------------------------------------------------------------------- modele

def kill_group(proc):
    """SIGTERM -> laska -> SIGKILL, i powrot WYMAGA dowodu ESRCH (niezmiennik 6).

    Osierocony `claude` pali limit w tle; to blad finansowy, nie higieniczny. W pythonie
    dowodem jest ProcessLookupError z killpg -- to doslownie ESRCH z jadra.
    """
    try:
        pgid = os.getpgid(proc.pid)
    except ProcessLookupError:
        return True
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(pgid, sig)
        except ProcessLookupError:
            return True
        for _ in range(20):
            time.sleep(0.1)
            try:
                os.killpg(pgid, 0)
            except ProcessLookupError:
                return True
    return False


def call_model(vendor, prompt, cwd, *, write, schema=None, budget=None, resume=False,
              turns=None, transcript=None):
    exe = shutil.which(vendor)
    if not exe:
        die("nie znaleziono `%s` w PATH" % vendor)
    if turns is None:
        turns = int(os.environ.get("LOADOUT_MAX_TURNS", "250"))
    out_file = None
    if vendor == "claude":
        # --setting-sources project, NIE "": flaga "" tnie koszt kontekstu ~6x, ale wycina
        # tez .claude/settings.json, czyli NASZ hak Stop i NASZA liste permissions. Bieg bez
        # naglowka sesji nie ma kto zatwierdzic, wiec "nie zabronione" znaczy w praktyce
        # "zablokowane na zawsze": w repo zrodlowym 28 tur i 4,65 USD na zbudowanie niczego.
        argv = [exe, "-p", "--setting-sources", "project", "--strict-mcp-config",
                "--disable-slash-commands",
                "--permission-mode", "acceptEdits" if write else "plan",
                "--model", os.environ.get("LOADOUT_CLAUDE_MODEL", "claude-opus-5[1m]"),
                # Wysilek per FAZA, nie na caly bieg. `write` juz rozdziela plan (False)
                # od implementacji (True), wiec nie ma tu nowej rurki -- tylko drugi domyslny.
                # ZMIERZONE 2026-08-28 z mtime'ow transkryptow w runs/: plan 10 min i 12 min,
                # implementacja 30 min i 49 min, checki 25 s, weryfikacja Codeksem 4,5 min.
                # Czyli implementacja to 3-5x plan, a plan jest ta faza, ktora w OBU biegach
                # poprawila przeslanke zlecenia (p8-t158: odmowa na sciezce pollu to `Api`,
                # nie `ConnectionRefused`; p8-t201: dziura jest w sciezce UDANEJ, nie w Stopie).
                # Tanszy plan kupilby wiec kilka minut i zaplacil za nie zlym kontraktem.
                "--effort", os.environ.get(
                    "LOADOUT_CLAUDE_EFFORT_DEV" if write else "LOADOUT_CLAUDE_EFFORT",
                    os.environ.get("LOADOUT_CLAUDE_EFFORT", "max"),
                ),
                "--max-turns", str(turns)]
        if resume:
            # Poprawka kontynuuje TE SAMA sesje: agent pamieta, co juz probowal, zamiast
            # odtwarzac rozumowanie z samego kodu.
            argv.append("--continue")
        if schema:
            argv += ["--json-schema", json.dumps(schema)]
        if transcript:
            argv += ["--output-format", "stream-json", "--verbose"]
    elif vendor == "codex":
        argv = [exe, "exec", "--json", "--skip-git-repo-check", "-C", str(cwd),
                "-s", "workspace-write" if write else "read-only",
                "-m", os.environ.get("LOADOUT_CODEX_MODEL", "gpt-5.6-sol"),
                "-c", "model_reasoning_effort=%s" % os.environ.get("LOADOUT_CODEX_EFFORT", "xhigh")]
        if schema:
            sf = Path(cwd) / ".h-schema.json"
            sf.write_text(json.dumps(schema), encoding="utf-8")
            out_file = Path(cwd) / ".h-out.json"
            argv += ["--output-schema", str(sf), "-o", str(out_file)]
        argv.append("-")
    else:
        die("nieznany vendor: %s (claude albo codex)" % vendor)

    # Prompt STDIN-em, nigdy w argv (niezmiennik 9): argv widzi kazdy `ps`.
    # Wlasna grupa procesow, zeby dalo sie ubic CALE drzewo z dowodem (patrz kill_group).
    proc = subprocess.Popen(argv, cwd=str(cwd), stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            text=True, start_new_session=True)
    try:
        out, _ = proc.communicate(input=prompt, timeout=budget)
    except subprocess.TimeoutExpired:
        proved = kill_group(proc)
        die("%s przekroczyl %ss%s" % (vendor, budget,
            "" if proved else " -- I NIE DA SIE DOWIESC, ze grupa nie zyje"), 3)
    except KeyboardInterrupt:
        proved = kill_group(proc)
        die("przerwane%s" % ("" if proved else " -- grupa NIE dowiedziona jako martwa"), 3)
    if transcript:
        Path(transcript).write_text(out, encoding="utf-8")
    if proc.returncode != 0:
        # Sufit tur to NIE kod 1. ZMIERZONE 2026-08-28 na biegu p8-t151-newer-truth: agent
        # zjadl 250 tur na 145 edycjach mechanicznego wachlarza (`tsc` wymusza jedna linie
        # w kazdej atrapie IPC), skonczyl z `tsc rc=0` i 67 zmienionymi plikami -- czyli praca
        # BYLA prawie gotowa. Harness zameldowal to jako kod 1, ktory wedlug README znaczy
        # "sprawdzenie padlo", wiec orchestrator poszedl szukac defektu kodu, ktorego nie bylo.
        # Sufit nalezy do kodu 3 ("przerwane albo sufit czasu") i worktree zostaje do wznowienia.
        if '"subtype":"error_max_turns"' in out or '"terminal_reason":"max_turns"' in out:
            die("%s wyczerpal sufit %d tur -- to NIE porazka sprawdzenia. Praca zostaje "
                "w worktree; podnies LOADOUT_MAX_TURNS albo zawez zakres." % (vendor, turns), 3)
        die("%s zakonczyl sie kodem %d:\n%s" % (vendor, proc.returncode, out[-1500:]))
    if out_file and out_file.exists():
        text = out_file.read_text(encoding="utf-8")
        out_file.unlink(missing_ok=True)
        (Path(cwd) / ".h-schema.json").unlink(missing_ok=True)
        return text
    return out


def parse_json(text):
    text = (text or "").strip()
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        m = re.search(r"\{.*\}", text, re.S)
        if m:
            try:
                return json.loads(m.group(0))
            except json.JSONDecodeError:
                pass
    die("model nie zwrocil JSON-a:\n%s" % text[:800])


def prompt_file(name):
    return (HDIR / "prompts" / ("%s.md" % name)).read_text(encoding="utf-8")


def last_text(transcript_out):
    """Ostatnia proza z strumienia stream-json. Plan jest tekstem, nie schematem."""
    text = []
    for line in (transcript_out or "").splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            d = json.loads(line)
        except json.JSONDecodeError:
            continue
        if d.get("type") == "assistant":
            for c in d.get("message", {}).get("content", []):
                if c.get("type") == "text" and c.get("text", "").strip():
                    text.append(c["text"])
        elif d.get("type") == "result" and d.get("result"):
            text.append(str(d["result"]))
    return text[-1].strip() if text else (transcript_out or "").strip()


# ------------------------------------------------------------------------ fazy

def rundir(task_id):
    d = ROOT / "runs" / task_id
    d.mkdir(parents=True, exist_ok=True)
    return d


def phase_plan(task_id, task, wt, vendor):
    log("plan (%s)..." % vendor)
    p = "%s\n\n## Zadanie\n\n%s\n" % (prompt_file("plan"), task)
    raw = call_model(vendor, p, wt, write=False, turns=60, budget=2400,
                     transcript=str(rundir(task_id) / "plan.jsonl"))
    plan = last_text(raw) if vendor == "claude" else raw.strip()
    (Path(wt) / ".h-plan.md").write_text(plan, encoding="utf-8")
    print("\n\033[1m--- PLAN ---\033[0m\n%s\n" % plan)
    return plan


def phase_implement(task_id, task, plan, wt, vendor, feedback="", rnd=0):
    log("implementacja (%s)%s" % (vendor, " [poprawka]" if feedback else ""))
    p = "%s\n\n## Zadanie\n\n%s\n\n## Plan\n\n%s\n" % (prompt_file("implement"), task, plan)
    if feedback:
        p += ("\n## Weryfikacja odrzucila poprzednia wersje\n\n%s\n\n"
              "Popraw dokladnie to. Nie zaczynaj od zera, nie przepisuj reszty.\n" % feedback)
    call_model(vendor, p, wt, write=True, resume=bool(feedback), budget=5400,
               transcript=str(rundir(task_id) / ("build-%d.jsonl" % rnd)))


def phase_verify(task, plan, wt, checks, vendor):
    log("weryfikacja (%s)..." % vendor)
    diff = git("diff", "HEAD", cwd=wt, check=False)
    for p in changed_paths(wt):
        f = Path(wt) / p
        try:
            if f.is_file() and f.stat().st_size < 200_000 and p not in diff:
                diff += "\n--- NOWY PLIK: %s ---\n%s\n" % (p, f.read_text(errors="replace"))
        except (OSError, UnicodeDecodeError):
            pass
    if len(diff) > 400_000:
        diff = diff[:400_000] + "\n[... diff obciety ...]"
    csum = "\n".join(
        "- %s: %s (%ds)%s" % (c["id"], "OK" if c["ok"] else "FAIL", c["seconds"],
                              "\n```\n%s\n```" % c["tail"][-2500:] if not c["ok"] else "")
        for c in checks) or "(brak checkow dla tych sciezek)"
    p = ("%s\n\n## Zadanie\n\n%s\n\n## Plan i akceptacja\n\n%s\n\n## Wynik checkow\n\n%s\n\n"
         "## Diff\n\n```diff\n%s\n```\n" % (prompt_file("verify"), task, plan, csum, diff))
    return parse_json(call_model(vendor, p, wt, write=False, schema=VERIFY_SCHEMA,
                                 turns=40, budget=1800))


# -------------------------------------------------------------------- komendy

def cut_worktree(task_id):
    """worktree.sh decyduje o nazwie katalogu i wypisuje ja JEDNA linia. To caly interfejs.

    Nie powtarzamy tu jego logiki (port z nazwy, klon APFS node_modules, zaufanie dla obu
    vendorow, wlasny target) -- druga kopia tych regul rozjezdzala sie z pierwsza przy
    kazdej zmianie nazewnictwa.
    """
    r = subprocess.run(["bash", "worktree.sh", "h-%s" % task_id], cwd=str(ROOT),
                       capture_output=True, text=True)
    if r.returncode != 0:
        die("worktree.sh: %s" % (r.stderr.strip() or r.stdout.strip()))
    wt = r.stdout.strip().splitlines()[-1].strip()
    if not wt or not Path(wt).is_dir():
        die("worktree.sh wypisal %r, co nie jest katalogiem" % wt)
    return wt


def commit_work(wt, task_id):
    """Domknij prace jednym commitem, zeby galaz naprawde ja niosla.

    Prompt implementacji zabrania agentowi ruszac gita -- i to jest sluszne, bo commit w polowie
    pracy miesza dwie odpowiedzialnosci. Ale bez tego commita `h land` merguje PUSTA galaz
    i melduje sukces: zmierzone przy pierwszym prawdziwym biegu, gdzie worktree mial trzy
    zmienione pliki, a `git diff main..h-<id>` byl pusty.
    `.h-plan.md` nie wchodzi -- jest w .gitignore, bo to plik roboczy harnessu, nie praca.
    """
    if not git("status", "--porcelain", cwd=wt):
        log("nic do zacommitowania -- galaz juz niesie prace")
        return
    git("add", "-A", cwd=wt)
    git("-c", "user.email=h@loadout", "-c", "user.name=h",
        "commit", "-q", "-m", "feat(%s): %s" % (task_id, "praca tego biegu"), cwd=wt)
    log("praca zacommitowana na galezi h-%s" % task_id)


def cmd_run(a):
    task_id, task = a.task_id, a.prompt
    wt = load_state(task_id).get("worktree")
    if wt and Path(wt).is_dir():
        log("worktree istnieje: %s" % wt)
    else:
        wt = cut_worktree(task_id)
        log("worktree %s" % wt)
    save_state(task_id, task=task, worktree=wt, started=time.time())
    (rundir(task_id) / "request.txt").write_text(task, encoding="utf-8")

    plan = task if a.no_plan else phase_plan(task_id, task, wt, a.planner)
    save_state(task_id, plan=plan)

    feedback, t0 = "", time.time()
    for rnd in range(MAX_FIX_ROUNDS + 1):
        phase_implement(task_id, task, plan, wt, a.dev, feedback, rnd)
        checks, paths = phase_check(wt)
        if not paths:
            die("agent nic nie zmienil w worktree")
        failed = [c for c in checks if not c["ok"]]
        v = phase_verify(task, plan, wt, checks, a.verifier)
        save_state(task_id, rounds=rnd + 1, last_verdict=v, checks=checks)

        verdict = v.get("werdykt")
        if verdict == "DZIALA" and not failed:
            commit_work(wt, task_id)
            print("\n\033[32m\033[1m=== DZIALA ===\033[0m  (%ds, rund: %d)" % (time.time() - t0, rnd + 1))
            print("worktree: %s\nbranch:   h-%s" % (wt, task_id))
            print("zmienione: %d plikow | checki: %s" % (
                len(paths), ", ".join("%s %ds" % (c["id"], c["seconds"]) for c in checks)))
            print("\nDiff:   git -C %s diff HEAD" % wt)
            print("Laduj:  scripts/h land %s" % task_id)
            print("Koniec: scripts/h clean %s" % task_id)
            return
        why = v.get("co_nie_dziala") or ""
        how = v.get("jak_naprawic") or ""
        if failed and verdict == "DZIALA":
            why = ("Weryfikator uznal zadanie za zrobione, ale check padl: "
                   + ", ".join(c["id"] for c in failed))
            how = "Napraw padajacy check, nie zmieniajac zachowania, ktore przeszlo weryfikacje."
        print("\n\033[33m--- %s (runda %d/%d) ---\033[0m\n%s\n" % (verdict, rnd + 1, MAX_FIX_ROUNDS + 1, why))
        if rnd == MAX_FIX_ROUNDS:
            print("\033[31m\033[1m=== STOP po %d rundach ===\033[0m" % (MAX_FIX_ROUNDS + 1))
            print("Ostatni werdykt: %s\n%s\n\nSugestia weryfikatora:\n%s" % (verdict, why, how))
            print("\nworktree: %s  (nic nie usuniete, popraw recznie albo zmien zadanie)" % wt)
            raise SystemExit(2)
        feedback = "Werdykt: %s\n\nCo nie dziala:\n%s\n\nJak naprawic:\n%s" % (verdict, why, how)


def cmd_check(a):
    wt = load_state(a.task_id).get("worktree", str(ROOT)) if a.task_id else str(ROOT)
    manual = {k: v for k, v in CFG["manual_only"].items() if not k.startswith("_")}
    if a.check_id:
        if a.check_id in manual:
            spec = manual[a.check_id]
            r = run_check(a.check_id, spec["cmd"], spec.get("cwd"), 1800, False, wt)
            raise SystemExit(0 if r["ok"] else 1)
        if a.check_id in CFG["checks"]:
            spec = CFG["checks"][a.check_id]
            r = run_check(a.check_id, spec["cmd"], spec.get("cwd"),
                          spec.get("budget_s", 900), bool(spec.get("counts_tests")), wt)
            raise SystemExit(0 if r["ok"] else 1)
        die("nie ma checka %r. Automatyczne: %s. Manualne: %s"
            % (a.check_id, ", ".join(CFG["checks"]), ", ".join(manual)))
    checks, paths = phase_check(wt)
    if not paths:
        print("nic nie zmienione -- nie ma czego sprawdzac")
        raise SystemExit(0)
    print("\n" + ", ".join("%s %s" % (c["id"], "OK" if c["ok"] else "FAIL") for c in checks))
    raise SystemExit(0 if all(c["ok"] for c in checks) else 1)


def cmd_land(a):
    """Merge jednej galezi i PELNE CI na trunku. Tu, raz -- nie w petli zadania."""
    if git("rev-parse", "--abbrev-ref", "HEAD") != os.environ.get("LOADOUT_TRUNK", "main"):
        die("landuj z trunka, nie z galezi")
        return
    if git("status", "--porcelain", "-uall"):
        die("drzewo brudne -- zacommituj albo odloz przed landowaniem")
    branch = "h-%s" % a.task_id
    if not git("show-ref", "--verify", "-q", "refs/heads/%s" % branch, check=False) == "":
        pass
    ahead = git("rev-list", "--count", "%s..%s" % (os.environ.get("LOADOUT_TRUNK", "main"), branch),
                check=False)
    if ahead in ("", "0"):
        die("galaz %s nie ma ani jednego commita ponad trunkiem -- nie ma czego landowac" % branch)
    log("merge --no-ff %s (%s commit(ow))" % (branch, ahead))
    if subprocess.run(["git", "merge", "--no-ff", "-m", "chore(main): land %s" % branch, branch],
                      cwd=str(ROOT)).returncode != 0:
        die("merge sie nie udal -- rozwiaz konflikt, zacommituj i uruchom land ponownie")
    log("pelne CI na trunku (tutaj mieszka suita calego repo)")
    if subprocess.run(["bash", "scripts/ci.sh", "full"], cwd=str(ROOT)).returncode != 0:
        print("\033[31mCI czerwone PO merge'u. Merge zostaje na miejscu, zebys go przeczytal.\033[0m",
              file=sys.stderr)
        print("Cofniecie:  git reset --hard HEAD~1", file=sys.stderr)
        raise SystemExit(1)
    log("wyladowane, CI zielone")


def cmd_status(a):
    s = load_state(a.task_id)
    if not s:
        die("nie ma taska %s" % a.task_id)
    print(json.dumps(s, indent=2, ensure_ascii=False))


def cmd_clean(a):
    s = load_state(a.task_id)
    wt = s.get("worktree")
    if wt and Path(wt).exists():
        git("worktree", "remove", *(["--force"] if a.force else []), wt, check=False)
        log("usunieto worktree %s" % wt)
    git("branch", "-D", "h-%s" % a.task_id, check=False)
    state_path(a.task_id).unlink(missing_ok=True)
    log("task %s zamkniety" % a.task_id)


def cmd_list(a):
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    rows = sorted(STATE_DIR.glob("*.json"))
    if not rows:
        print("brak otwartych taskow")
        return
    for p in rows:
        s = json.loads(p.read_text(encoding="utf-8"))
        v = (s.get("last_verdict") or {}).get("werdykt", "-")
        print("%-32s rundy=%s werdykt=%s" % (p.stem, s.get("rounds", "-"), v))


def main():
    ap = argparse.ArgumentParser(prog="h", description="maly harness Loadouta")
    sub = ap.add_subparsers(dest="cmd", required=True)

    r = sub.add_parser("run", help="prompt -> plan -> kod -> checki + weryfikacja -> koniec")
    r.add_argument("task_id")
    r.add_argument("--prompt", required=True)
    r.add_argument("--planner", default=os.environ.get("H_PLANNER", "claude"))
    r.add_argument("--dev", default=os.environ.get("H_DEV", "claude"))
    r.add_argument("--verifier", default=os.environ.get("H_VERIFIER", "codex"))
    r.add_argument("--no-plan", action="store_true", help="pomin planiste, zadanie idzie wprost")
    r.set_defaults(fn=cmd_run)

    c = sub.add_parser("check", help="odpal checki (albo jeden po nazwie)")
    c.add_argument("check_id", nargs="?", default="")
    c.add_argument("--task-id", default="")
    c.set_defaults(fn=cmd_check)

    for name, fn, hlp in (("status", cmd_status, "stan taska"),
                          ("land", cmd_land, "merge galezi + pelne CI"),
                          ("clean", cmd_clean, "zamknij task")):
        s = sub.add_parser(name, help=hlp)
        s.add_argument("task_id")
        if name == "clean":
            s.add_argument("--force", action="store_true")
        s.set_defaults(fn=fn)

    sub.add_parser("list", help="otwarte taski").set_defaults(fn=cmd_list)
    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
