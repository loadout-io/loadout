#!/usr/bin/env python3
"""Sedzia jednej komendy Bash w biegu harnessu. Wolany przez `pre-bash.sh`.

Kod 0 przepuszcza, kod 2 odmawia i oddaje modelowi tresc stderr (kontrakt PreToolUse,
ten sam, ktorym stop-gate.sh blokuje koniec tury).

SADZIMY GLOWE SEGMENTU, NIE PODNAPIS. Pierwsza wersja tego pliku szukala napisu
"cargo clippy" w calej komendzie i odmawiala `grep -rn "cargo clippy" runs/` — czyli
dokladnie tego odczytu, ktorym mierzy sie, czy hak dziala. `shlex` zostawia cytowany
napis JEDNYM tokenem, wiec segment zaczynajacy sie od `grep` nigdy nie wyglada jak cargo.
"""
import json
import os
import shlex
import sys

# Wyrocznia: to samo, co krotka ORACLE w harness/h.py, plus dwa pliki dopisane 2026-09-04
# (H-25). Rozjazd miedzy ta lista a tamta jest wada — obie mowia o jednym.
ORACLE = ("harness/", "checks/", "scripts/", ".claude/", "AGENTS.md",
          "docs/DECISIONS-LOCKED.md", "worktree.sh", "CLAUDE.md",
          "docs/ARCHITECTURE.md", "docs/design/DESIGN.md")

# Tokeny, ktore rozdzielaja komende na segmenty. Kazdy segment ma wlasna glowe i jest
# sadzony osobno: `npm ci && cargo clippy` musi paść na drugim czlonie.
SPLIT = {"&&", "||", ";", "|", "&", "\n"}

# Przedrostki, ktore nie sa jeszcze komenda.
SKIP_HEAD = {"sudo", "time", "nice", "nohup", "command", "exec", "builtin", "env",
             "caffeinate", "stdbuf"}

GATE = ("Bramka odpala to po tobie — dwa razy: z haka Stop i z samego harnessu. "
        "Twoj przebieg ma tylko potwierdzic, ze TWOJ test pada przed poprawka i przechodzi po niej.")


def segments(command):
    """Komenda -> lista segmentow, kazdy jako lista tokenow."""
    lex = shlex.shlex(command, posix=True, punctuation_chars=True)
    lex.whitespace_split = True
    try:
        tokens = list(lex)
    except ValueError:
        # Niedomkniety cudzyslow. Nie zgadujemy, co autor mial na mysli — przepuszczamy,
        # bo bash i tak odmowi, a hak, ktory blokuje na wlasnym bledzie parsowania, jest
        # gorszy niz jego brak.
        return []
    out, cur = [], []
    for t in tokens:
        if t in SPLIT or all(c in "&|;" for c in t) and t:
            if cur:
                out.append(cur)
            cur = []
        else:
            cur.append(t)
    if cur:
        out.append(cur)
    return out


def head_of(seg):
    """Glowa segmentu (bez sciezki) i reszta argumentow, po zdjeciu przedrostkow."""
    i = 0
    while i < len(seg):
        t = seg[i]
        if "=" in t and not t.startswith("=") and "/" not in t.split("=", 1)[0]:
            i += 1                      # FOO=bar cargo …
            continue
        if t.rsplit("/", 1)[-1] in SKIP_HEAD:
            i += 1
            continue
        break
    if i >= len(seg):
        return "", []
    return seg[i].rsplit("/", 1)[-1], seg[i + 1:]


def first_word(args):
    for a in args:
        if a.startswith("+"):            # cargo +nightly test
            continue
        if a.startswith("-"):
            continue
        return a
    return ""


def touches_oracle(text):
    for p in ORACLE:
        if p in text:
            return p
    return None


def under(cwd, path):
    """Czy `path` zostaje w drzewie sesji albo w katalogu tymczasowym."""
    if path.startswith("-"):
        return True
    full = os.path.realpath(os.path.join(cwd, os.path.expanduser(path)))
    allowed = [os.path.realpath(cwd)]
    tmp = os.environ.get("TMPDIR")
    if tmp:
        allowed.append(os.path.realpath(tmp))
    allowed.append("/private/tmp")
    allowed.append("/tmp")
    return any(full == a or full.startswith(a + os.sep) for a in allowed)


def judge(command, cwd):
    """Zwraca powod odmowy albo None."""
    for seg in segments(command):
        head, args = head_of(seg)
        if not head:
            continue
        whole = " ".join(seg)

        # ── zapis w wyrocznie ────────────────────────────────────────────────
        # Przekierowanie: `>` i `>>` sa u shlexa osobnymi tokenami, wiec celem jest
        # token NASTEPNY. `cat harness/h.py` i `grep -n x checks/*.sh` przechodza,
        # bo czytaja — sadzimy kierunek, nie wzmianke o sciezce.
        for i, t in enumerate(seg):
            if t in (">", ">>") and i + 1 < len(seg):
                hit = touches_oracle(seg[i + 1])
                if hit:
                    return ("ta komenda pisze w %s, czyli w wyrocznie, ktora cie sadzi "
                            "(AGENTS.md §7). Jesli kryterium da sie spelnic tylko przez jej "
                            "zmiane — powiedz to i nic nie zmieniaj." % hit)
        if head in ("python3", "python", "node", "ruby", "perl", "tee", "dd", "truncate") \
                or (head == "sed" and any(a.startswith("-i") for a in args)):
            hit = touches_oracle(whole)
            if hit:
                return ("interpretery i `sed -i` moga pisac, a ta komenda nazywa %s — "
                        "wyrocznie, ktora cie sadzi. Do CZYTANIA uzyj Read albo `cat`/`grep`; "
                        "do zmiany — powiedz czlowiekowi, ze kryterium jej wymaga." % hit)

        # ── cargo ─────────────────────────────────────────────────────────────
        if head == "cargo":
            sub = first_word(args)
            if sub in ("clippy", "build", "bench", "doc", "clean"):
                return ("`cargo %s` nalezy do bramki, nie do ciebie. %s" % (sub, GATE))
            # `cargo check` ZOSTAJE legalne, i to jest decyzja przeciwko pierwszej wersji tej
            # listy. Zmierzone: 44 wywolania w fali Z, wszystkie tanie (bez linkowania), a
            # pamiec projektu mowi wprost, ze bieg, ktory nie sprawdzil, czy drzewo sie
            # kompiluje, produkuje CZERWIEN NIE DO ODROZNIENIA od prawdziwej. Odmowa
            # oszczedzilaby 40 sekund i kupowala stracona runde.
            if sub == "test" and "--test" not in args:
                return ("`cargo test` bez `--test` buduje i odpala WSZYSTKIE cele. Zawez do "
                        "jednego binarium: `cargo test --test it <modul>::<nazwa>` albo "
                        "`cargo test --test it -- <slowa filtru>`. %s" % GATE)

        # ── vitest ────────────────────────────────────────────────────────────
        vitest = head == "vitest" or (head in ("npx", "npm", "pnpm", "yarn", "bun")
                                      and "vitest" in args)
        if vitest:
            # Celem jest KAZDY nie-flagowy token poza nazwa narzedzia — takze katalog.
            # `npx --no-install vitest run e2e` jest zawezone i ma przejsc (bieg z34).
            paths = [a for a in args if not a.startswith("-")
                     and a not in ("vitest", "run", "exec", "watch", "--")]
            if not paths:
                return ("goly `vitest run` odpala cala suite webowa. Podaj SCIEZKE pliku, "
                        "ktory piszesz: `npx vitest run src/…/twoj.test.tsx`. %s" % GATE)
        if head in ("npm", "pnpm", "yarn", "bun") and first_word(args) in ("test", "run"):
            script = [a for a in args if not a.startswith("-")][1:2]
            if first_word(args) == "test" or (script and script[0] in ("test", "test:e2e")):
                return ("`npm test` odpala cala suite. Podaj sciezke pliku przez "
                        "`npx vitest run <plik>`. %s" % GATE)

        # ── sama bramka ───────────────────────────────────────────────────────
        if head in ("h", "h.py") or (head in ("bash", "sh", "zsh", "python3", "python")
                                     and any(a.endswith(("scripts/h", "harness/h.py"))
                                             for a in args)):
            if "check" in args:
                return ("`h check` biegnie po tobie dwa razy — z haka Stop i z harnessu. %s"
                        % GATE)
        # WYWOLANIE, nie wzmianka. Pierwsza wersja odmawiala `grep -n clippy scripts/ci.sh`
        # — czyli odczytu, ktorym sprawdza sie, co bramka robi (zlapane sonda nad korpusem
        # fali Z, 2026-09-04).
        runs_ci = head.endswith("ci.sh") or (head in ("bash", "sh", "zsh")
                                             and any(a.endswith("ci.sh") for a in args))
        if runs_ci:
            return "`scripts/ci.sh` to pelna bramka; odpala ja harness po tobie. %s" % GATE

        # ── kasowanie poza drzewem ────────────────────────────────────────────
        if head == "rm" and any(a.startswith("-") and "r" in a for a in args):
            for a in args:
                if a.startswith("-"):
                    continue
                if not under(cwd, a):
                    return ("`rm -r %s` wychodzi poza drzewo tej sesji. Kasuj wylacznie to, "
                            "co sam stworzyles, i wylacznie tutaj." % a)

        # ── gałęzie ───────────────────────────────────────────────────────────
        # Tylko galezie, w ktorych MIESZKA PRACA. Bieg z07 zalozyl repozytorium probne
        # w /tmp i skasowal w nim galaz `r`; blokowanie takich sond nic nie chroni.
        if head == "git" and first_word(args) == "branch" \
                and any(a in ("-D", "-d", "--delete") for a in args):
            targets = [a for a in args if not a.startswith("-") and a != "branch"]
            if any(t == "main" or t.startswith(("h-", "loadout/", "backup/")) for t in targets):
                return ("kasowanie galezi %s nalezy do `h land`/`h clean`, nie do biegu. "
                        "Praca, ktorej nikt nie wlal, znika razem z galezia."
                        % ", ".join(targets))
    return None


def selftest():
    """Tablica przypadkow z `checks/bash-guard-cases.jsonl`, sadzona bez procesu haka."""
    here = os.path.dirname(os.path.abspath(__file__))
    cases = os.path.join(here, "..", "..", "checks", "bash-guard-cases.jsonl")
    bad = 0
    seen = 0
    with open(os.path.realpath(cases), encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line or line.startswith("//"):
                continue
            case = json.loads(line)
            seen += 1
            reason = judge(case["cmd"], case.get("cwd", os.getcwd()))
            got = "deny" if reason else "allow"
            if got != case["expect"]:
                bad += 1
                sys.stderr.write("bash-guard: %r -> %s, oczekiwano %s%s\n"
                                 % (case["cmd"], got, case["expect"],
                                    " (%s)" % reason if reason else ""))
    sys.stdout.write("bash-guard: %d przypadkow, %d niezgodnych\n" % (seen, bad))
    return 1 if bad else 0


def main():
    if "--selftest" in sys.argv:
        raise SystemExit(selftest())
    try:
        payload = json.load(sys.stdin)
    except Exception:
        raise SystemExit(0)
    if (payload or {}).get("tool_name") != "Bash":
        raise SystemExit(0)
    command = ((payload.get("tool_input") or {}).get("command") or "")
    cwd = payload.get("cwd") or os.getcwd()
    reason = judge(command, cwd)
    if not reason:
        raise SystemExit(0)
    sys.stderr.write("pre-bash: %s\n" % reason)
    raise SystemExit(2)


if __name__ == "__main__":
    main()
