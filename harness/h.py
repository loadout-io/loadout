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
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HDIR = ROOT / "harness"
CFG = json.loads((HDIR / "checks.json").read_text(encoding="utf-8"))
STATE_DIR = ROOT / ".git" / "h"
MAX_FIX_ROUNDS = 2

# H-2 (audyt 2026-09-02): `deny` w .claude/settings.json zamyka tylko Edit/Write, a `allow`
# ma `sed`, `cp`, `python3` -- wiec bieg MOGL przepisac check, ktory go sadzi, i nikt by tego
# nie zobaczyl. Prompt tego zabrania od poczatku (implement.md), ale prompt jest miekki
# (niezmiennik 28). To jest ta sama lista co w `deny`, egzekwowana twardo: raz przed commitem
# biegu, drugi raz przed merge'em w `land`.
ORACLE = ("harness/", "checks/", "scripts/", ".claude/", "AGENTS.md",
          "docs/DECISIONS-LOCKED.md", "worktree.sh", "CLAUDE.md")


def trunk_name():
    return os.environ.get("LOADOUT_TRUNK", "main")


def oracle_hits(paths):
    return [p for p in paths if p.startswith(ORACLE)]

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
    # H-8 (audyt 2026-09-02): `subprocess.run` po timeoucie zabija WYLACZNIE `bash -c`, a nie
    # grupe -- `cargo`, `rustc`, `vitest` i chromium zyly dalej i zjadaly maszyne (pamiec
    # projektu: workery vitest sierociejace na gigabajty). Popen + kill_group daje dowod ESRCH
    # tak samo, jak `call_model` robi to dla modeli.
    proc = subprocess.Popen(["/bin/bash", "-c", cmd], cwd=str(where), stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, env=env,
                            start_new_session=True)
    try:
        out, _ = proc.communicate(timeout=budget)
        code = proc.returncode
        kill_group(proc)
    except subprocess.TimeoutExpired:
        proved = kill_group(proc)
        out = "[TIMEOUT po %ds]%s" % (
            budget, "" if proved else " -- I NIE DA SIE DOWIESC, ze grupa nie zyje")
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
    # H-20 (audyt 2026-09-02): po pierwszym FAIL ciezkie checki (>= 600 s budzetu) nie niosa
    # juz informacji, ktorej runda naprawcza nie dostanie taniej -- a `rust-test` kosztowal
    # do godziny zegara PO tym, jak `rust-clippy` juz powiedzial, co jest zle. Pominiety check
    # jest CZERWONY, nie zielony: nie wiemy, czy przechodzi.
    results, red = [], False
    for cid, cmd, cwd, budget, counts in picked:
        if red and budget >= 600:
            log("  POMIN %s (wczesniejszy check padl, budzet %ds)" % (cid, budget))
            results.append({"id": cid, "ok": False, "cmd": cmd, "seconds": 0,
                            "tail": "POMINIETY: wczesniejszy check w tej rundzie padl"})
            continue
        r = run_check(cid, cmd, cwd, budget, counts, wt)
        results.append(r)
        red = red or not r["ok"]
    return results, paths


# ---------------------------------------------------------------------- modele

def kill_group(proc):
    """SIGTERM -> laska -> SIGKILL, i powrot WYMAGA dowodu ESRCH (niezmiennik 6).

    Osierocony `claude` pali limit w tle; to blad finansowy, nie higieniczny. W pythonie
    dowodem jest ProcessLookupError z killpg -- to doslownie ESRCH z jadra.

    H-8 (audyt 2026-09-02): kiedy dziecko jest juz ZEBRANE (po `communicate`), `getpgid`
    daje ESRCH na samym liderze, a jego grupa potrafi dalej zyc -- to wlasnie tam siedza
    `cargo`, `vitest` i chromium checka. Kazdy nasz `Popen` idzie z `start_new_session=True`,
    wiec pgid ZAWSZE rowna sie pid lidera i mozna po nim strzelac takze po zebraniu.
    """
    try:
        pgid = os.getpgid(proc.pid)
    except ProcessLookupError:
        pgid = proc.pid
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
              turns=None, transcript=None, session=None, budget_usd=None):
    exe = shutil.which(vendor)
    if not exe:
        # H-10 (audyt 2026-09-02): D3 mowi wprost "recenzent niedostepny to NIE czerwone".
        # Kod 1 znaczy "sprawdzenie padlo" i wysyla orchestratora szukac defektu kodu,
        # ktorego nie ma; kod 2 znaczy "zatrzymaj sie i zapytaj czlowieka".
        die("nie znaleziono `%s` w PATH" % vendor, 2 if schema else 1)
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
                # Model per ROLA. `schema` podaje wylacznie faza weryfikacji, wiec jest tu
                # czystym dyskryminatorem -- zadnej nowej rurki. Po co to istnieje: decyzja D3
                # mowi, ze przy parze same-vendor weryfikator musi miec INNY MODEL plus role
                # recenzenta, bo ten sam model dwa razy nie jest druga opinia. Domyslnie rowna
                # sie modelowi piszacego, wiec bez tej zmiennej nic sie nie zmienia.
                "--model", os.environ.get(
                    "LOADOUT_CLAUDE_MODEL_VERIFIER" if schema else "LOADOUT_CLAUDE_MODEL",
                    os.environ.get("LOADOUT_CLAUDE_MODEL", "claude-opus-5[1m]"),
                ),
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
        if budget_usd:
            # H-9 (audyt 2026-09-02): bez tego jedna faza potrafila zjesc 25 USD i 123 tury,
            # a jedynym hamulcem byl sufit tur, ktory nie mowi nic o pieniadzach. Sufit jest
            # per FAZA, bo plan i weryfikacja sa tanie, a implementacja jest 3-5x drozsza.
            argv += ["--max-budget-usd", "%.2f" % budget_usd]
        if resume:
            # Poprawka kontynuuje TE SAMA sesje: agent pamieta, co juz probowal, zamiast
            # odtwarzac rozumowanie z samego kodu.
            #
            # H-4 (audyt 2026-09-02): do 2026-09-02 stalo tu `--continue`, ktore bierze
            # NAJNOWSZA sesje w tym katalogu. Przy parze same-vendor (`--verifier claude`)
            # najnowsza jest sesja WERYFIKATORA -- w trybie plan, read-only -- wiec poprawka
            # "pamietala" cudze rozumowanie. To samo, gdy czlowiek otworzyl `claude`
            # w worktree. Jawny identyfikator sesji nie ma jak trafic w cudza.
            argv += ["--resume", session] if session else ["--continue"]
        elif session:
            argv += ["--session-id", session]
        if schema:
            argv += ["--json-schema", json.dumps(schema)]
        # H-10 (2026-09-02), POPRAWKA z tego samego dnia: `--output-format stream-json`
        # WOLNO dolozyc tylko wtedy, gdy nie ma schematu. Razem ze `--json-schema`
        # odpowiedz przestaje byc JSON-em pasujacym do schematu i staje sie strumieniem
        # zdarzen, wiec `parse_json` konczy bieg zdaniem "model nie zwrocil JSON-a"
        # PO calej implementacji. Zmierzone na biegu z28-tests-into-it: weryfikator
        # napisal poprawna diagnoze, ktorej harness nie umial przeczytac.
        # Slad i tak powstaje -- `out` ladu je w pliku transkryptu nizej, bez tych flag.
        if transcript and not schema:
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
        elif transcript:
            # H-1 (audyt 2026-09-02): bez `-o` funkcja oddaje CALY strumien `exec --json`,
            # a `phase_plan` bierze go jako plan: zmierzone 55-317 KB `thread.started`,
            # `item.*` i logow `ERROR rmcp` w KAZDYM promptcie implementacji i weryfikacji,
            # do trzech rund. Ten sam mechanizm, ktorego uzywa juz galaz ze schematem.
            out_file = Path(cwd) / ".h-last.txt"
            argv += ["-o", str(out_file)]
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
    kill_group(proc)
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
                "w worktree; zawez zakres albo podnies sufit tej fazy: LOADOUT_PLAN_TURNS "
                "dla planu, LOADOUT_MAX_TURNS dla implementacji." % (vendor, turns), 3)
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
    # Sufit planu KONFIGUROWALNY, bo 60 tur to za malo na zakres o dwoch mechanizmach
    # w plikach po 10 tys. linii. ZMIERZONE 2026-08-29 na p8-t156-bounded-lifecycle: planista
    # zjadl 60 tur na czytaniu `drivers/claude.rs` i `commands/run.rs`, nie napisawszy planu.
    # Do tego dnia komunikat o sufcie odsylal do LOADOUT_MAX_TURNS, ktory tej fazy NIE dotyczy
    # (`turns=60` stalo tu na sztywno) -- czyli rada byla nieprawdziwa.
    raw = call_model(vendor, p, wt, write=False,
                     turns=int(os.environ.get("LOADOUT_PLAN_TURNS", "60")), budget=2400,
                     budget_usd=float(os.environ.get("LOADOUT_BUDGET_PLAN", "12")),
                     transcript=str(rundir(task_id) / "plan.jsonl"))
    plan = last_text(raw) if vendor == "claude" else raw.strip()
    # H-1 (audyt 2026-09-02): kontrola negatywna do poprawki wyzej. Plan, ktory zaczyna sie
    # od koperty strumienia albo ma rozmiar transkryptu, NIE jest planem -- i lepiej, zeby
    # bieg stanal tutaj, niz zeby smieci pojechaly do trzech kolejnych promptow.
    if plan.lstrip().startswith('{"type":') or len(plan) > 40_000:
        die("planista oddal strumien zamiast planu (%d znakow, zaczyna sie od %r)"
            % (len(plan), plan.lstrip()[:40]), 2)
    (Path(wt) / ".h-plan.md").write_text(plan, encoding="utf-8")
    print("\n\033[1m--- PLAN ---\033[0m\n%s\n" % plan)
    return plan


def phase_implement(task_id, task, plan, wt, vendor, feedback="", rnd=0):
    log("implementacja (%s)%s" % (vendor, " [poprawka]" if feedback else ""))
    p = "%s\n\n## Zadanie\n\n%s\n\n## Plan\n\n%s\n" % (prompt_file("implement"), task, plan)
    if feedback:
        p += ("\n## Weryfikacja odrzucila poprzednia wersje\n\n%s\n\n"
              "Popraw dokladnie to. Nie zaczynaj od zera, nie przepisuj reszty.\n" % feedback)
    # H-4: identyfikator sesji zapisany w stanie, zeby runda naprawcza wznowila TE sesje,
    # a nie te, ktora akurat byla ostatnia w katalogu.
    #
    # POPRAWKA 2026-09-02, po biegu z01-descendants: `--session-id` zaklada sesje NOWA i vendor
    # odmawia ("Session ID ... is already in use"), kiedy ten sam task startuje drugi raz --
    # a to jest normalna droga po kodzie 3 (sufit tur), gdzie praca zostaje w worktree
    # i ma byc kontynuowana. Sesja zapisana w stanie znaczy wiec „wznow", niezaleznie od tego,
    # czy powodem jest runda naprawcza, czy ponowne wywolanie po suficie.
    saved = load_state(task_id).get("session")
    sid = saved or str(uuid.uuid4())
    save_state(task_id, session=sid)
    call_model(vendor, p, wt, write=True, resume=bool(feedback) or bool(saved), budget=5400,
               session=sid,
               budget_usd=float(os.environ.get("LOADOUT_BUDGET_DEV", "40")),
               transcript=str(rundir(task_id) / ("build-%d.jsonl" % rnd)))


def phase_verify(task_id, task, plan, wt, checks, vendor, rnd=0):
    log("weryfikacja (%s)..." % vendor)
    # H-19 (audyt 2026-09-02): `git diff HEAD` pokazuje wylacznie NIEZACOMMITOWANE zmiany,
    # wiec po ponownym `h run <id>` (praca poprzedniej proby jest juz w commicie biegu)
    # weryfikator dostawal pusty diff i sadzil zadanie po samym planie.
    base = git("merge-base", trunk_name(), "HEAD", cwd=wt, check=False) or "HEAD"
    diff = git("diff", base, cwd=wt, check=False)
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
    # H-10: weryfikacja zostawia slad. Bez niego `die("model nie zwrocil JSON-a")` po 30-50
    # minutach implementacji nie zostawial ani werdyktu, ani niczego do przeczytania.
    return parse_json(call_model(vendor, p, wt, write=False, schema=VERIFY_SCHEMA,
                                 turns=40, budget=1800,
                                 budget_usd=float(os.environ.get("LOADOUT_BUDGET_VERIFY", "6")),
                                 transcript=str(rundir(task_id) / ("verify-%d.jsonl" % rnd))))


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
    `.h-plan.md` nie wchodzi, bo jest w .gitignore -- to plik roboczy harnessu, nie praca.
    Do 2026-08-28 ten komentarz KLAMAL: pliku nie bylo w .gitignore i byl sledzony, wiec
    `git add -A` brał go do commita biegu. Kazde dwa rownolegle biegi mialy go w dwoch
    wersjach i lądowanie drugiego stawało na konflikcie w brudnopisie, przy `ipc.rs`
    zmergowanym automatycznie. Zmierzone przy h-p8-t151-newer-truth.
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

    # H-10 (audyt 2026-09-02): `--no-plan` przy WZNOWIENIU ma uzyc planu, ktory juz stoi
    # w stanie -- inaczej ponowienie po kodzie 3 sadzi zadanie wobec samego zlecenia
    # i gubi kontrakt, za ktory zaplacono w pierwszej probie.
    plan = (load_state(task_id).get("plan") or task) if a.no_plan else phase_plan(
        task_id, task, wt, a.planner)
    save_state(task_id, plan=plan)

    feedback, t0 = "", time.time()
    for rnd in range(MAX_FIX_ROUNDS + 1):
        phase_implement(task_id, task, plan, wt, a.dev, feedback, rnd)
        checks, paths = phase_check(wt)
        if not paths:
            die("agent nic nie zmienil w worktree")
        hit = oracle_hits(paths)
        if hit:
            die("bieg dotknal wyroczni, ktora go sadzi: %s. To jest AGENTS.md §7: check,\n"
                "ktory jest zly, ZGLASZA sie, a nie zmienia. Praca zostaje w worktree."
                % ", ".join(hit[:6]), 2)
        failed = [c for c in checks if not c["ok"]]
        v = phase_verify(task_id, task, plan, wt, checks, a.verifier, rnd)
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
        if verdict == "NIE_WIEM":
            # H-11 (audyt 2026-09-02): AGENTS.md §2 obiecuje trzy wyjscia i STOP przy
            # niepewnosci. Do 2026-09-02 `NIE_WIEM` szlo w `feedback` z PUSTYM opisem, wiec
            # bieg placil do dwoch rund naprawczych za zdanie "nie wiem".
            print("\n\033[33m\033[1m=== NIE_WIEM ===\033[0m")
            print(v.get("co_nie_dziala") or "(weryfikator nie napisal, czego nie wie)")
            print("\nworktree: %s  (nic nie usuniete -- to jest pytanie do czlowieka)" % wt)
            raise SystemExit(2)
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
    trunk = trunk_name()
    if git("rev-parse", "--abbrev-ref", "HEAD") != trunk:
        die("landuj z trunka, nie z galezi")
        return
    if git("status", "--porcelain", "-uall"):
        die("drzewo brudne -- zacommituj albo odloz przed landowaniem")
    branch = "h-%s" % a.task_id
    # H-3 (audyt 2026-09-02): po przepisaniu historii przed publikacja 118 lokalnych galezi
    # nie ma wspolnego przodka z trunkiem. `merge --no-ff` albo odmawia, albo -- gdyby ktos
    # dolozyl --allow-unrelated-histories -- wciaga 1500 commitow sprzed czyszczenia
    # do PUBLICZNEGO repo. Pytamy o to PRZED merge'em, a nie po.
    if not git("merge-base", trunk, branch, check=False):
        die("galaz %s nie ma wspolnego przodka z %s -- nie da sie jej wlac merge'em.\n"
            "Przenies jej commity na swieza galaz od trunka (cherry-pick) i sprobuj ponownie."
            % (branch, trunk), 2)
    hit = oracle_hits(git("diff", "--name-only", "%s...%s" % (trunk, branch),
                          check=False).splitlines())
    if hit:
        die("galaz zmienia wyrocznie, ktora ja sadzi: %s (AGENTS.md §7)" % ", ".join(hit[:6]), 2)
    ahead = git("rev-list", "--count", "%s..%s" % (trunk, branch), check=False)
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
        # H-23 (audyt 2026-09-02): `git reset --hard` jest w `deny` .claude/settings.json,
        # wiec ta rada byla niewykonalna dla biegu, ktory ja czyta. Revert merge'a bierze -m 1.
        print("Cofniecie:  git revert -m 1 HEAD", file=sys.stderr)
        raise SystemExit(1)
    log("wyladowane, CI zielone")
    # H-14 (audyt 2026-09-02): bez tego kazdy zlandowany bieg zostawial worktree z wlasnym
    # `target/` (zmierzone: 20 kopii = 68 GB) i wpis w `h list`, ktory nie mial juz tresci.
    if not getattr(a, "keep", False):
        cmd_clean(a)


def cmd_status(a):
    s = load_state(a.task_id)
    if not s:
        die("nie ma taska %s" % a.task_id)
    print(json.dumps(s, indent=2, ensure_ascii=False))


def cmd_clean(a):
    s = load_state(a.task_id)
    wt = s.get("worktree")
    if wt and Path(wt).exists():
        git("worktree", "remove", *(["--force"] if getattr(a, "force", False) else []),
            wt, check=False)
        # H-14 (audyt 2026-09-02): `check=False` polykal blad, a log i tak mowil "usunieto".
        # Sierota znikala z `h list` i zostawala na dysku -- niewidzialna dla wszystkich.
        if Path(wt).exists():
            die("worktree NIE zszedl: %s (stan zadania zostaje, sprobuj `h clean %s --force`)"
                % (wt, a.task_id))
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
        if name == "land":
            s.add_argument("--keep", action="store_true",
                           help="nie sprzataj worktree po zielonym CI")
        s.set_defaults(fn=fn)

    sub.add_parser("list", help="otwarte taski").set_defaults(fn=cmd_list)
    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
