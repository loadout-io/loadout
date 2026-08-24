#!/usr/bin/env python3
"""Bramka Loadout. `verify.sh` to trzy linijki wokół tego pliku.

Trzy poziomy — nazwy są zwykłymi słowami, bo trafiają do UI i do promptów:
    before   każde kryterium MUSI być czerwone, i to z właściwego powodu. Raz, przed kodem.
    quick    sprawdzenia projektowe rangi quick + kryteria. Pętla robocza, ~20 s.
    full     wszystko + testy. Raz, przed oddaniem, i to samo woła CI.

Sprawdzenia pochodzą z DOKŁADNIE dwóch miejsc i znikąd indziej:
    checks/<poziom>-<id>.sh   jeden plik = jedno sprawdzenie projektowe
    TASK.md                   jedno sprawdzenie na `## AC-n`, z jego linii `check:`

Kody wyjścia, identyczne w całym harnessie:
    0 poziom przeszedł · 1 sprawdzenie padło (uczciwa porażka) · 3 przerwane albo sufit czasu
    2 bramka jest źle skonfigurowana: brak sprawdzeń, brak kryteriów w TASK.md, niespójny
      kontrakt zadania, brak wymaganego narzędzia. NIGDY mylone z 1.
"""
from __future__ import annotations

import argparse
import concurrent.futures
import glob
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TASK_PATH = os.path.join(ROOT, "TASK.md")

# Sprawdzenie projektowe biegnie, gdy jego ranga <= rangi żądanego poziomu, więc `quick`
# jest podzbiorem `full`, a `before` nie ciągnie fmt/clippy po drzewie, którego nie ma.
# Coś, co MA biec także w `before` (postawienie serwera, którego kryteria potrzebują),
# nazywa się checks/before-<id>.sh.
TIER_RANK = {"before": 0, "quick": 1, "full": 2}

# Sufit poziomu -> exit 3. Wolna bramka czyni pętlę agenta nie do zniesienia, więc poziom
# przewraca się pod własnym nazwiskiem zamiast rozpłynąć się w zewnętrznym timeoucie.
CEILING = {"before": 60.0, "quick": 45.0, "full": 600.0}

# Ten sam katalog, co muteks cargo -- zeby oba zamki dalo sie znalezc jednym ls.
TMPDIR = os.environ.get("TMPDIR") or "/tmp"

# NA SPRAWDZENIE, nie na poziom: bez tego jedno sprawdzenie dziedziczy cały budżet poziomu
# (w repo źródłowym siedem czekających lokatorów zrobiło z 2 s trzy i pół minuty).
CHECK_TIMEOUT = {"before": 20.0, "quick": 20.0, "full": 90.0}

# Nadpisania per sprawdzenie, po id. Świadomie tutaj, w oracle'u, a nie w TASK.md: bieg nie
# może podnieść sobie limitu. Wpis wolno dodać tylko ze ZMIERZONYM uzasadnieniem obok.
#
# Trzy sprawdzenia cargo. Powód jest jeden i mierzalny: ZIMNY build. `cargo clippy --lib`
# na zależnościach Tauri idzie minuty, nie sekundy, a checks/_cargo-serialize.sh dokłada do
# tego czekanie na muteks (niezmiennik 26, cap LOADOUT_CARGO_LOCK_WAIT=300 s). Bez tych
# wpisów bramka ubija pierwszy build w 20 s, ponawia go raz i melduje "wisi" o czymś, co
# po prostu się kompilowało.
CHECK_TIMEOUT_OVERRIDE = {
    "quick-clippy": 420.0,   # 300 s zamka + zimne clippy --lib
    "full-clippy": 600.0,    # to samo z --all-targets (testy, benche, przykłady)
    # 1800 s NIE jest zapasem "na wszelki wypadek", tylko liczbą. ZMIERZONE 2026-08-17:
    # `cargo test --tests` to 119 binariów i 1121 s na maszynie, na której obok chodzi fala
    # zadania — przy 444 s na maszynie bezczynnej. Budżet 600 s stał tu od początku BEZ
    # pomiaru, więc margines wynosił 15% na pustej maszynie i znikał przy pierwszym biegu obok.
    #
    # Co to naprawdę kosztowało: bramka zabiła suitę, powtórzyła ją, zabiła znowu i zameldowała
    # „waiting for something that is not going to arrive" — czyli postawiła DIAGNOZĘ, na którą
    # nie miała dowodu. Trunk zaświecił się na czerwono, lądowanie T-31 odmówiło startu, a nie
    # było ani jednego padającego testu. Fałszywe oskarżenie kodu za zajętą maszynę jest gorsze
    # niż wolna bramka, bo wysyła człowieka szukać wady, której nie ma (tu: półtorej godziny).
    #
    # Sufit poziomu i tak wybacza wyłącznie ten czas, który to nadpisanie NAPRAWDĘ zjadło
    # (`ceiling_for`), więc podniesienie budżetu nie czyni bramki wolną — czyni ją cierpliwą
    # dokładnie tam, gdzie zmierzyliśmy, że trzeba.
    # 9000 s NIE jest zapasem "na wszelki wypadek", tylko liczba. ZMIERZONE 2026-08-17,
    # dwoma niezaleznymi sposobami:
    #   - pelny przebieg: 122 cele testowe, ~60-99 s na cel, w sumie ~2 h;
    #   - kontrolowany pomiar jednego celu po dotknieciu commands/run.rs: 60 s i 62 s.
    # Same testy trwaja **6,0 s**. Cala reszta to budowanie 122 OSOBNYCH binariow, z ktorych
    # kazde linkuje cala biblioteke razem z zaleznosciami Tauri.
    #
    # Dlatego kazda zmiana dotykajaca commands/ uniewazniala wszystkie cele i full-test NIE
    # MIAL JAK zmiescic sie w 1800 s -- nigdy, na zadnej maszynie. Tak padly T-29, T-32
    # i ladowanie po T-33, a bramka meldowala "waiting for something that is not going to
    # arrive", czyli diagnoze bez dowodu (Q-6).
    #
    # Sprawdzone i ODRZUCONE: [profile.test] debug = "line-tables-only". Na macOS informacja
    # debugowania zwykle dominuje czas linkera, ale nie tutaj -- 60/62 s wobec 62/71 s, czyli
    # roznica w granicach szumu. Zmiana bez zmierzonego zysku nie zostaje w repo.
    #
    # PRAWDZIWA naprawa to mniej celow testowych (Q-7): 122 pliki w tests/ to 122 linkowania
    # tej samej biblioteki. Scalenie ich w kilkanascie celow tnie ten czas o rzad wielkosci
    # i jest refaktorem na spokojna glowe, a nie zmiana do zrobienia miedzy zadaniami.
    "full-test": 9000.0,
}

# Kryteria akceptacji są per zadanie, więc tabela wyżej nie umie ich nazwać. A budżet warstwy
# `before` to 20 s — mniej, niż trwa ZIMNY build cargo. Zmierzone na kontrakcie T-01:
# `cargo test --test shell_logging` wywalił się na 20 s, retry zmieścił się w 10,3 s. Retry
# uratował, ale na zimniejszym cache by nie uratował, a wtedy kryterium wyglądałoby na
# niedowiedzione z powodu, który nie ma nic wspólnego z kodem.
#
# Regułą jest KSZTAŁT KOMENDY, nie nazwa: kryterium wołające cargo dostaje budżet cargo, bo
# kompilacja nie jest tym, co mierzymy. Kryteria vitest zostają przy 20 s i mają zostać —
# tam wolne znaczy naprawdę wolne.
CARGO_BUDGET = 420.0


class HeavyTierLock(object):
    """Jedna bramka `full` na maszynie naraz -- i czekanie na to LEZY POZA zegarem sprawdzen.

    Lane szeregowy wyzej usuwa kolizje WEWNATRZ fali. Miedzy falami muteks cargo dalej jest
    wspolny dla maszyny, wiec dwie bramki `full` obok siebie odtwarzaja incydent T-36 co do
    joty: przegrany przesiaduje cudze 512 s na wlasnym zegarze i oddaje 2.

    Naprawa nie moze polegac na dluzszym czekaniu W sprawdzeniu -- wtedy czekanie i tak zjada
    budzet, ktory jest twierdzeniem o KODZIE. Wiec czekamy tutaj, zanim ruszy jakikolwiek
    zegar. Bramka, ktora stoi w kolejce, nie klamie o niczyim kodzie; po prostu jeszcze nie
    zaczela.

    To NIE jest ten sam fakt, co muteks w checks/_cargo-serialize.sh (niezmiennik 13): tamten
    chroni pojedyncze `cargo` przed drugim `cargo` i obowiazuje takze petle `quick` agenta,
    ten obowiazuje CALA bramke `full` i tylko ja. Rozne ziarno, rozne zamki, rozne nazwy.
    """

    def __init__(self, tier, patience=None):
        # Cierpliwosc z ENV takze po to, zeby straznik mogl ja skrocic i sprawdzic
        # zachowanie na granicy, nie tylko szczesliwa sciezke.
        if patience is None:
            patience = float(os.environ.get("LOADOUT_GATE_LOCK_PATIENCE", 2400.0))
        self.path = os.path.join(TMPDIR, "loadout-gate-full.lock")
        self.tier = tier
        self.patience = patience
        self.held = False

    def __enter__(self):
        if self.tier != "full":
            return self
        waited = 0.0
        while True:
            try:
                os.mkdir(self.path)
                break
            except OSError:
                # Pytamy o ZYCIE wlasciciela, nie o wiek zamka: bramka bywa zabijana razem
                # z grupa procesow, wiec trap na wyjsciu bywa nieosiagalny i zamek zostaje
                # po trupie. To ta sama lekcja, co w checks/_cargo-serialize.sh.
                owner = ""
                try:
                    with open(os.path.join(self.path, "pid"), encoding="utf-8") as fh:
                        owner = fh.read().strip()
                except OSError:
                    pass
                if owner and not _alive(int(owner) if owner.isdigit() else -1):
                    print("  note the other full gate (pid %s) is gone -- taking the lock"
                          % owner)
                    _rm_lock(self.path)
                    continue
                if waited >= self.patience:
                    # Cierpliwosc konczy sie GLOSNO. Cicha rezygnacja czyta sie identycznie
                    # jak bramka, ktora po prostu zadzialala -- czyli jest ta sama awaria,
                    # przed ktora stoi checks/MANIFEST.
                    print("  note waited %.0fs for the other full gate and gave up -- "
                          "running anyway, expect cargo mutex contention" % waited)
                    return self
                if waited == 0.0:
                    print("  note another full gate holds the machine (pid %s) -- waiting "
                          "OUTSIDE the checks' clocks" % (owner or "unknown"))
                time.sleep(2.0)
                waited += 2.0
        with open(os.path.join(self.path, "pid"), "w", encoding="utf-8") as fh:
            fh.write(str(os.getpid()))
        self.held = True
        if waited:
            print("  note waited %.0fs for the other full gate; no check paid for it" % waited)
        return self

    def __exit__(self, *exc):
        if self.held:
            _rm_lock(self.path)
        return False


def _alive(pid):
    if pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    except OSError:
        return True
    return True


def _rm_lock(path):
    try:
        os.remove(os.path.join(path, "pid"))
    except OSError:
        pass
    try:
        os.rmdir(path)
    except OSError:
        pass


def takes_the_cargo_mutex(argv):
    """Czy to sprawdzenie stanie w kolejce po muteks cargo (niezmiennik 26)?

    Pytamy o to PLIK SPRAWDZENIA, a nie druga liste tutaj. Lista w oracle'u rozjechalaby sie
    przy pierwszym nowym sprawdzeniu z `cargo`, a plik rozjechac sie nie moze -- to ten sam
    powod, dla ktorego polityka lintow siedzi w Cargo.toml, a nie w checks/ (niezmiennik 13).
    """
    for a in argv:
        p = str(a)
        if not p.endswith(".sh"):
            continue
        full = p if os.path.isabs(p) else os.path.join(ROOT, p)
        try:
            with open(full, encoding="utf-8", errors="replace") as fh:
                if "cargo_serialize" in fh.read():
                    return True
        except OSError:
            continue
    return False


def budget_for(check_id, argv, default):
    if check_id in CHECK_TIMEOUT_OVERRIDE:
        return CHECK_TIMEOUT_OVERRIDE[check_id]
    if any("cargo" in str(a) for a in argv):
        return max(default, CARGO_BUDGET)
    return default

# Ile wyjścia padającego kroku widzi wołający. NA KROK, nie na bieg: globalny ogon
# regularnie nie zawiera ani jednej asercji, więc naprawa naprawia coś, czego nie widziała.
TAIL = 1200

# Kryteria lubią mieć port, bazę albo okno. Siedem naraz wyczerpało maszynę źródłową i każde
# padało co bieg z innego powodu — czerwone, które nic nie mówi o kodzie.
ACCEPTANCE_JOBS = 2

CRITERION = re.compile(r"^##\s+(AC-(\d+))\b\s*(.*)$")
CHECK = re.compile(r"^\s*check:\s*(.+?)\s*$")
EXPECT = re.compile(r"^\s*expect:\s*(.+?)\s*$")

# Zielone sprawdzenie musi udowodnić, że biegło (niezmiennik 19). Kod testowany ładuje się
# do tego samego procesu, którego kod wyjścia czytamy, więc `os._exit(0)` na poziomie modułu
# zazielenia całą suitę bez jednej asercji. Żądamy LICZNIKA PRZEJŚĆ; `expect: none` to jedyna,
# recenzowalna furtka. Pokryte: vitest "Tests 9 passed (9)" · cargo "test result: ok. 7 passed"
# · pytest "5 passed" · unittest "Ran 3 tests" · jest "Tests: 9 passed, 9 total".
DEFAULT_EXPECT = r"(?:Ran\s+(\d+)\s+tests?|(\d+)\s+(?:passed|tests?\s+passed))"

# W `before` sprawdzenie musi paść dlatego, że BRAKUJE ZACHOWANIA — nie dlatego, że brakuje
# samego sprawdzenia. Oba dają kod != 0 i tylko jedno certyfikuje oracle; bez tej listy
# `verify.sh before` przechodzi na pustym repo i pętla implementuje pod nic.
#
# Konsekwencja dla Rusta, świadoma: test, który się NIE KOMPILUJE, nic nie uruchomił. Kontrakt
# pisze się więc tak, żeby test się kompilował i padał w runtime — najpierw `todo!()`.
NOT_A_REAL_RED = re.compile(
    r"(no tests? (?:were )?found|found no tests|no tests ran|No test files found|"
    r"ModuleNotFoundError|ImportError|No module named|command not found|"
    r"No such file or directory|cannot find module|ERR_MODULE_NOT_FOUND|SyntaxError|"
    r"collection error|Cannot find configuration|error: no test files|no test files found|"
    # vitest z filtrem po NAZWIE, który nic nie złapał: wszystko pominięte, a podsumowanie
    # ani razu nie mówi "failed". Bieg mieszany drukuje obok "1 failed | 3 passed".
    r"Tests\s+\d+\s+skipped\s+\(\d+\)|file or directory not found|"
    r"unrecognized arguments|Testing pattern .* did not match|"
    # Aplikacja była nieosiągalna, więc nic nie zostało wykonane.
    r"ERR_CONNECTION_REFUSED|ECONNREFUSED|EADDRINUSE|ENOENT|"
    r"webServer was not able to start|connection refused|"
    # ...i przeglądarka, która nie wstała: bieg źródłowy scertyfikował oracle na Chromium,
    # które nie startowało — siedem kryteriów "padło", czyli w `before` siedem zaliczonych.
    r"bootstrap_check_in|browserType\.launch|Failed to launch|browser could not launch|"
    r"Executable doesn't exist|Target page, context or browser has been closed|"
    r"Host system is missing dependencies|"
    # Rust: KAŻDY błąd kompilatora znaczy, że test nie doszedł do runtime. Lista dwóch
    # kodów importu przepuściła 2026-08-24 E0308 (`mismatched types`) i scertyfikowała
    # pięć kryteriów T-112 na jednym zepsutym pliku. `could not compile` domyka też
    # diagnostyki bez numerowanego kodu, np. błędy składni; panika testu nie ma żadnego
    # z tych podpisów i nadal jest uczciwą czerwienią.
    r"(?:^|\n)error\[E\d+\]|could not compile|unresolved import|"
    r"no targets specified|error: no bin target|could not find `Cargo.toml`|"
    r"error: no test target|the following required arguments were not provided|"
    # Workspace, który się nie parsuje albo nie ma zadeklarowanej biblioteki: cargo nigdy nie
    # doszło do kompilacji, więc kryterium "padło", nie uruchamiając niczego. Trafione naprawdę,
    # przy pierwszym biegu bramki na pustym drzewie Loadouta: src-tauri/Cargo.toml deklaruje
    # lib `loadout_lib`, a src/lib.rs jeszcze nie istniał — i `before` zaliczyło to jako czerwone.
    r"failed to load manifest|failed to parse manifest|can't find library)",
    re.IGNORECASE,
)

# Filtry po NAZWIE testu są zakazane w `check:`. Filtr, który nic nie dopasował, raportuje
# sukces — i tak vitest dwa razy pokonał poziom `before` w repo źródłowym. Kryterium wskazuje
# JEDEN plik, po ścieżce. Dłuższe warianty przed krótszymi, bo alternatywa łapie pierwszą.
NAME_FILTER = re.compile(
    r"(?:^|\s)(--test-name-pattern|--testNamePattern|--test-name|--grep|-t|-g)(?:[=\s]|$)")

# Co uznajemy za "ścieżkę specyfikacji". Vitest wskazuje plik wprost; cargo wskazuje MODUŁ
# wewnątrz jedynego celu integracyjnego, więc oba są jednoznaczne i oba dają się porównać
# między zadaniami.
SPEC_PATH = re.compile(r"(?:^|[\s=\"'])((?:[\w.@-]+/)+[\w.@-]+\.(?:test|spec)\.[jt]sx?"
                       r"|(?:[\w.@-]+/)+[\w.@-]+\.rs)")

# 2026-08-17 — CEL PRZESTAŁ IDENTYFIKOWAĆ SPECYFIKACJĘ, i to jest cała treść tej zmiany.
#
# Do tego dnia `--test <nazwa>` wskazywał dokładnie jeden plik `tests/<nazwa>.rs`, bo Rust
# robi z każdego pliku w `tests/` osobne binarium. To było jednoznaczne i to samo było
# przyczyną, dla której `full-test` trwał godziny: 122 pliki = 122 programy, z których każdy
# statycznie linkuje 527 skrzyń, żeby uruchomić 6 sekund testów.
#
# Po scaleniu (`tests/it/main.rs` z modułami) cel jest JEDEN — `it` — więc sam cel nie
# odróżnia kryteriów. Rozróżnia je FILTR MODUŁU: `cargo test --test it store_pragmas::`
# uruchamia dokładnie te testy, co dawne `cargo test --test store_pragmas`. Ten filtr jest
# więc teraz tożsamością specyfikacji i to po nim liczy się reguła „jedna specyfikacja,
# jedno kryterium".
#
# Filtr BEZ `::` odrzucamy świadomie: `cargo test --test it store` łapie także
# `store_pragmas` i `storage_x`, czyli jedno kryterium sądziłoby cudze testy. Dwukropki
# czynią z tego prefiks ścieżki modułu, a nie podciąg nazwy.
# DWA KSZTAŁTY, bo są dwa rodzaje celów — i ta dwoistość jest zamierzona, nie przejściowa:
#   `--test it <moduł>::`  — moduł scalonego celu; tożsamością jest MODUŁ,
#   `--test <cel>`         — własny cel; tożsamością jest sam cel.
# Osobny cel dostaje test, który mierzy albo zmienia stan CAŁEGO PROCESU (deskryptory, hak
# paniki, `env::set_var`) — w scalonym binarium mierzyłby 285 cudzych testów naraz.
CARGO_TARGET = re.compile(r"--test[= ]+([\w-]+)(?:\s+([\w]+)::)?")


# ---------------------------------------------------------------- kontrakt zadania

def read_task(path):
    # -> [{id, num, title, check, expect}]. Jedyny parser tego formatu w harnessie. Kryteria
    # bez `check:` odsiewa dopiero discover(), żeby reguła "AC 1..n bez dziur" zobaczyła
    # kryterium, które zgubiło linię check:, zamiast liczyć je za nieistniejące.
    out, cur = [], None
    if not os.path.isfile(path):
        return out
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = CRITERION.match(line)
            if m:
                cur = {"id": m.group(1), "num": int(m.group(2)), "title": m.group(3).strip(),
                       "check": "", "expect": DEFAULT_EXPECT}
                out.append(cur)
                continue
            if cur is None:
                continue
            c = CHECK.match(line)
            if c:
                cur["check"] = c.group(1)
            e = EXPECT.match(line)
            if e:
                # `expect: none` = świadoma rezygnacja z dowodu wykonania. Formy z regexem
                # celowo nie reklamujemy: w repo źródłowym `expect:` było udokumentowane
                # i użyte w 0 z 83 kryteriów.
                cur["expect"] = "" if e.group(1).strip() == "none" else e.group(1)
    return out


def spec_tokens(cmd):
    """Ścieżki specyfikacji, jakie ta komenda uruchamia. Cel cargo liczy się jako ścieżka."""
    hits = [m.group(1) for m in SPEC_PATH.finditer(cmd)]
    # Ścieżka modułu, nie nazwa celu: po scaleniu cel jest jeden dla wszystkich kryteriów.
    for m in CARGO_TARGET.finditer(cmd):
        target, module = m.group(1), m.group(2)
        hits.append(
            "src-tauri/tests/it/%s.rs" % module
            if module
            else "src-tauri/tests/%s.rs" % target
        )
    # RÓŻNE ścieżki, nie wystąpienia. Komenda, która wymienia ten sam plik dwa razy
    # (`test -f x.test.ts && vitest run x.test.ts`), uruchamia JEDNĄ specyfikację —
    # licząc wystąpienia bramka meldowała "names 2 spec paths" i "run by AC-1, AC-1".
    seen, out = set(), []
    for h in hits:
        if h not in seen:
            seen.add(h)
            out.append(h)
    return out


def contract_problems(criteria):
    # Dwie tanie reguły zamiast 121-liniowego lintera planu, plus zakaz filtrów po nazwie.
    # Każda jest blizną: kryteria numerowane od AC-4 po podziale zadania, ten sam plik spec
    # uruchamiany przez dwa zadania, filtr `-t`, który nic nie złapał i zgłosił sukces.
    # Wynik to defekt KONTRAKTU (exit 2), nigdy padające sprawdzenie (exit 1).
    problems = []

    nums = [c["num"] for c in criteria]
    if nums and nums != list(range(1, len(nums) + 1)):
        problems.append("criteria are AC-%s, not AC-1..AC-%d with no gaps"
                        % (", AC-".join(str(n) for n in nums), len(nums)))

    mine = {}
    for c in criteria:
        if not c["check"]:
            problems.append("%s has no `check:` line, so nothing can prove it" % c["id"])
            continue
        m = NAME_FILTER.search(c["check"])
        if m:
            problems.append("%s uses the test-name filter %s -- a filter that matches "
                            "nothing reports success; name the spec file by path"
                            % (c["id"], m.group(1)))
        hits = spec_tokens(c["check"])
        if len(hits) != 1:
            problems.append("%s names %d spec paths, not exactly one: %s"
                            % (c["id"], len(hits), c["check"][:70]))
        for h in hits:
            mine.setdefault(h, []).append(c["id"])
    for path, who in sorted(mine.items()):
        if len(who) > 1:
            problems.append("%s is run by %s -- one spec path, one criterion"
                            % (path, ", ".join(who)))

    # Globalna unikalność po tasks/*.md. To jest cała reguła, która czyni równoległe zadania
    # bezpiecznymi: dwa zadania piszące pod ten sam plik testu ścigają się o jego treść.
    owner = {}
    for p in sorted(glob.glob(os.path.join(ROOT, "tasks", "*.md"))):
        stem = os.path.basename(p)[:-3]
        for c in read_task(p):
            for h in spec_tokens(c["check"]):
                owner.setdefault(h, set()).add(stem)
    for path, who in sorted(owner.items()):
        if len(who) > 1:
            problems.append("%s is named by tasks %s -- one spec path, one task"
                            % (path, ", ".join(sorted(who))))
    return problems


# ---------------------------------------------------------------- odkrywanie

class ContractError(Exception):
    """Nasza konfiguracja jest zepsuta (exit 2), nie kod (exit 1). Nigdy tego nie mieszamy."""


def discover(tier):
    """-> ([(id, kind, argv, expect)], [pominięte pliki]). Najpierw projektowe, potem AC."""
    want = TIER_RANK[tier]
    found, ignored = [], []
    cdir = os.path.join(ROOT, "checks")
    if os.path.isdir(cdir):
        for name in sorted(os.listdir(cdir)):
            # `_` i `.` na początku to konwencja dla plików pomocniczych, nie sprawdzeń.
            if not name.endswith(".sh") or name.startswith(("_", ".")):
                continue
            ctier, _, cid = name[:-3].partition("-")
            if not cid or ctier not in TIER_RANK:
                # Nie milczymy. Sprawdzenie, którego bramka nie odkryła, czyta się dokładnie
                # tak samo jak sprawdzenie, które przeszło.
                ignored.append(name)
                continue
            if TIER_RANK[ctier] <= want:
                # Bit wykonywalności nas nie obchodzi: wołamy `bash <ścieżka>`, więc brak
                # chmod +x nie jest cichym pominięciem sprawdzenia.
                found.append((name[:-3], "project",
                              ["bash", os.path.join("checks", name)], ""))
    # N-13: pin nie na regułach, tylko na DOMACH. Plik, który zgubił prefiks warstwy, dostaje
    # notkę wyżej; plik SKASOWANY nie produkował nic. Zmierzone: usunięcie
    # checks/quick-permissions.sh dało „7 checks, 0 failed" i exit 0 — zniknęło sprawdzenie
    # napisane po incydencie za $6,98, a bramka tego nie zauważyła. MANIFEST mieszka w checks/,
    # czyli tam, gdzie bieg nie pisze (co po N-06 jest znowu prawdą).
    man = os.path.join(cdir, "MANIFEST")
    if os.path.isfile(man):
        expected = {l.strip() for l in open(man, encoding="utf-8")
                    if l.strip() and not l.startswith("#")}
        actual = {i for i, k, _a, _e in found if k == "project"} | {
            n[:-3] for n in os.listdir(cdir)
            if n.endswith(".sh") and not n.startswith(("_", "."))}
        missing, extra = sorted(expected - actual), sorted(actual - expected)
        if missing or extra:
            raise ContractError(
                "checks/MANIFEST and checks/ disagree" +
                ("".join("\n  missing: " + m for m in missing)) +
                ("".join("\n  unlisted: " + e for e in extra)) +
                "\nA deleted check is silent otherwise. Add it to MANIFEST or restore the file.")

    for c in read_task(TASK_PATH):
        if c["check"]:
            found.append((c["id"], "acceptance", ["bash", "-c", c["check"]], c["expect"]))
    return found, ignored


# ---------------------------------------------------------------- czytanie wyjścia

# Sekwencje ANSI zdejmowane u źródła (patrz _run): kolorowany licznik przejść nie dopasowuje
# się do DEFAULT_EXPECT, a paragon z \x1b[31m w polu `reason` jest nieczytelny dla człowieka
# i dla widoku sesji, który go czyta.
_ANSI = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]")

# Linie, które nazywają porażkę, i linie, którymi runner odchrząkuje. Bez odsiewu banner
# uruchomieniowy albo stos node/cargo wypełnia cały ogon i runda naprawcza nie widzi asercji.
_SIGNAL = re.compile(
    r"(AssertionError|assertion\s+.?\w*\s*failed|panicked at|thread '.*' panicked|"
    r"error\[E\d+\]|error:|Error:|Expected|Received|expect\(|✘|✗|FAIL|"
    r"test result: FAILED|Timeout.*exceeded|did not match)")
_NOISE = re.compile(
    r"(^\s*$|^\s*at [\w./]+:\d|node:internal|^\s*(Compiling|Downloaded|Downloading|"
    r"Updating|Blocking|Fresh|Installing) |^\s*Finished\s|^\s*Running \d+ test|^\s*RUN\s+v)")


def extract_reason(out):
    """Najbardziej informacyjne linie z okna wokół pierwszego sygnału. Nie `tail -N`."""
    lines = [l.rstrip() for l in (out or "").splitlines() if not _NOISE.search(l)]
    hits = [i for i, l in enumerate(lines) if _SIGNAL.search(l)]
    if hits:
        picked = lines[max(0, hits[0] - 1):hits[0] + 6]
    else:
        picked = lines[-6:]
    return "\n".join(picked)[:TAIL]


# vitest raportuje pliki i testy w osobnych liniach, pliki pierwsze; cargo ma własną linię
# podsumowania. Którą byśmy nie czytali, musi to być ta o TESTACH.
_TESTS_LINE = re.compile(r"^\s*Tests\s+.*$", re.MULTILINE)
_CARGO_LINE = re.compile(r"^\s*test result:.*$", re.MULTILINE)


def reported_count(out, expect):
    # Ile testów runner powiedział, że przeszło, albo None, gdy nie powiedział nic. W zwykłym
    # poziomie to dowód, że zielone sprawdzenie biegło; w `before` to, co odróżnia porażkę
    # z braku zachowania od porażki z braku runnera.
    body = out or ""
    # Tam, gdzie runner ma własną linię podsumowania, jest ona JEDYNYM źródłem. Sięganie do
    # całego wyjścia jest tym, jak "Test Files 1 passed (1)" nad "Tests 4 skipped (4)"
    # zameldowało jeden przechodzący test w biegu, w którym nie wykonało się nic.
    lines = _TESTS_LINE.findall(body) or _CARGO_LINE.findall(body)
    if lines:
        body = "\n".join(lines)
    counts = []
    for m in re.finditer(expect or DEFAULT_EXPECT, body):
        got = next((g for g in m.groups() if g), None)
        if got and got.isdigit():
            counts.append(int(got))
    if not counts:
        return None
    # MAKSIMUM, nie pierwsze trafienie: `cargo test` drukuje "test result: ok. 0 passed" dla
    # każdego pustego celu, zanim dojdzie do tego, który coś uruchomił.
    return max(counts)


# ---------------------------------------------------------------- uruchamianie

def _run(argv, timeout):
    # Uruchom sprawdzenie, a przy timeoucie zabij CAŁĄ GRUPĘ, którą odpaliło: subprocess.run
    # zabija tylko proces, który uruchomił, a `TimeoutExpired` nie niesie pid-u. Motyw — sześć
    # osieroconych procesów testowych z PPID 1, najstarszy sprzed 21 godzin (niezmiennik 6).
    # NO_COLOR/FORCE_COLOR=0, bo FORCE_COLOR wycieka z sesji Claude Code do każdej powłoki,
    # którą ona odpala, i vitest koloruje mimo potoku. Kolor rozbija regexy licznika przejść
    # ("Tests \x1b[1m4 skipped") i zaśmieca paragon — to samo dwoma pasami, bo żaden runner
    # nie honoruje obu.
    p = subprocess.Popen(argv, cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         text=True, start_new_session=True,
                         env=dict(os.environ, CI="1", PYTHONDONTWRITEBYTECODE="1",
                                  NO_COLOR="1", FORCE_COLOR="0"))
    try:
        out, err = p.communicate(timeout=timeout)
        return p.returncode, _ANSI.sub("", (out or "") + (err or ""))
    except subprocess.TimeoutExpired:
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(os.getpgid(p.pid), sig)
            except (ProcessLookupError, PermissionError, OSError):
                break
            time.sleep(0.2)
            if p.poll() is not None:
                break
        try:
            p.communicate(timeout=5)
        except Exception:
            pass
        raise


def run_one(check, timeout):
    """Uruchom jedno sprawdzenie. Zwraca rekord; nigdy nie rzuca."""
    cid, kind, argv, expect = check
    t0 = time.time()
    retried = ""
    try:
        rc, out = _run(argv, timeout)
    except subprocess.TimeoutExpired:
        # Timeout mówi "to nie skończyło", nie mówi dlaczego: zmierzone, sprawdzenie 0,69 s
        # trafiło w sufit 90 s, bo maszynę zajmował inny bieg (2257 s zamiast 5,6 s). Więc
        # pytamy drugi raz — i mówimy to NA GŁOS, bo cichy retry to sposób, w jaki flaky żyje.
        try:
            t1 = time.time()
            rc, out = _run(argv, timeout)
            retried = ("timed out at %.0fs, finished in %.1fs on a second run"
                       % (timeout, time.time() - t1))
            out += "\n[gate] %s -- the machine was contended, not the code" % retried
        except subprocess.TimeoutExpired:
            rc, out = 124, ("check exceeded its %.0fs budget TWICE -- it is waiting for "
                            "something that is not going to arrive" % timeout)
    except OSError as exc:
        rc, out = 127, "could not run %r: %s" % (argv, exc)

    reason, rc_raw = "", rc
    if rc == 0 and expect:
        # Reguła dowodu (niezmiennik 19). Dotyczy wyłącznie kryteriów: sprawdzenie
        # projektowe (fmt, clippy) nie ma licznika przejść i nie powinno go udawać.
        if not re.search(expect, out):
            reason = ("exit 0 but no evidence of execution -- the output carries no "
                      "passing count, so nothing ran")
            rc = 1
        elif reported_count(out, expect) in (0, None):
            # `== 0` przepuszczało None, a None jest odczytem, który MA ZNACZENIE: tak
            # wygląda bieg vitest, w którym wszystko zostało pominięte.
            reason, rc = "exit 0 but the runner reports no passing tests", 1
    if rc != 0 and not reason:
        reason = extract_reason(out)
    # rc_raw obok rc, bo reguła dowodu właśnie przepisała rc na 1, a `before` musi odwracać
    # SUROWY kod: bez tego sprawdzenie, które wyszło zerem i nic nie uruchomiło, dostawało
    # "czerwone z właściwego powodu". Repo źródłowe ma tę dziurę do dziś.
    # `full` zostaje w całości do klasyfikacji: runner drukujący po błędzie log dostępu
    # ukryłby tę jedną linię, która mówi, że nic nie biegło.
    return {"id": cid, "kind": kind, "rc": rc, "rc_raw": rc_raw,
            "seconds": time.time() - t0, "reason": reason, "retried": retried, "full": out}


def verdict(tier, r):
    """(ok, note) — werdykt POZIOMU, nie surowy kod wyjścia. W `before` to przeciwieństwa."""
    if tier != "before":
        return r["rc"] == 0, r["reason"]
    if r["kind"] == "project":
        # Sprawdzenia projektowe NIE są odwracane: to one pozwalają kryteriom cokolwiek
        # dosięgnąć, więc czytanie ich sukcesu jako porażki wywala poziom na tym jednym
        # sprawdzeniu, które musiało się udać.
        return r["rc"] == 0, r["reason"]
    if r["rc_raw"] == 0:
        # Dwa różne zdania, bo prowadzą do dwóch różnych napraw: albo kryterium przechodzi
        # bez kodu (asercja jest za słaba), albo runner wyszedł zerem, nie uruchamiając nic.
        return False, ("exit 0 but no evidence of execution -- it certifies nothing"
                       if r["rc"] != 0 else
                       "PASSES before implementation -- it certifies nothing")
    if r["rc"] in (124, 127):
        # 124 = nasz podwójny timeout, 127 = nie dało się uruchomić. W `before` kod != 0 jest
        # warunkiem zaliczenia, więc bez tego coś, co nigdy nie skończyło, certyfikuje oracle.
        return False, "did not FINISH -- it hung or could not start, so it certifies nothing"
    full = r.get("full") or r["reason"] or ""
    m = NOT_A_REAL_RED.search(full)
    # ...chyba że runner zameldował, że coś uruchomił: test napisany przed implementacją pada
    # na brakującym imporcie dokładnie tak jak brakujący runner, a licznik je odróżnia.
    if m and not (reported_count(full, None) or 0):
        return False, ("did not RUN (%s) -- the check itself is missing or broken, so it "
                       "certifies nothing" % m.group(0).strip())
    return True, ""


# ---------------------------------------------------------------- poziom

def ceiling_for(tier, results, argv_by_id, per_check):
    """Sufit poziomu: norma + czas, ktory oracle SAM przyznal ponad nia.

    Sufit wybacza WYLACZNIE ten czas, ktory sprawdzenie naprawde zjadlo w granicach
    budzetu przyznanego mu przez `budget_for` -- z jednym dopiskiem, ktory kosztowal caly
    bieg. `run_one` po timeoucie pyta DRUGI raz (zamierzone: timeout mowi "nie skonczylo",
    nie mowi dlaczego), wiec oracle autoryzuje do 2*b, nie do b.

    Bez tego dopisku poziom przewracal sie na 3 ("GATE TOO SLOW") za czas, ktory sam wydal,
    i PRZYKRYWAL jedyna wiadomosc, po ktorej dalo sie cos zrobic. Zmierzone na T-06
    (2026-08-16): AC-2 zawislo na zakleszczeniu kanalu tokio, zjadlo 420 s + 420 s retry,
    bramka zapisala je uczciwie jako `failed` z powodem "did not FINISH" -- po czym zwrocila
    3 zamiast 1. Kod 3 znaczy "przerwane albo maszyna" i kieruje orchestratora do szukania
    osieroconych procesow. Kosztowalo to noc na hipotezie o zamku SQLite.

    To NIE rozluznia bramki. `granted` dalej liczy wylacznie czas NAPRAWDE zjedzony, wiec
    poziom wolny bez powodu -- czterdziesci sprawdzen po piec sekund, zadne nie dotkniete
    timeoutem -- nadal przewraca sie na 3. Zmienia sie tylko to, ze czerwien przestaje
    chowac sie za sufitem, ktory sama wypelnila.
    """
    granted = 0.0
    for r in results:
        b = budget_for(r["id"], argv_by_id.get(r["id"], []), per_check)
        authorised = 2.0 * b if (r.get("retried") or r.get("rc") == 124) else b
        if authorised > per_check:
            granted += min(max(0.0, r["seconds"] - per_check), authorised - per_check)
    return CEILING[tier] + max(0.0, granted)


def run(tier, jobs, only=None):
    if not shutil.which("bash"):
        sys.stderr.write("bash is not on PATH; every check is run as `bash <path>`\n")
        return 2

    # N-08 (audyt 2026-08-15, odtworzone). ship-task.sh kopiuje kontrakt i commituje go, po czym
    # nikt już na niego nie patrzy, a gate.py parsuje TASK.md z dysku przy każdym uruchomieniu.
    # Dopisanie `## AC-7 free win / check: true / expect: none` dało kryterium, które bramka
    # przyjęła z WYŁĄCZONĄ regułą dowodu — a quick-scope po commicie meldował 0 zmian.
    # Exit 2, nie 1: zepsuł się nasz kontrakt, nie kod.
    if os.path.isfile(TASK_PATH):
        stem = ""
        for line in open(TASK_PATH, encoding="utf-8"):
            m = re.match(r"^#\s+([ST]-\d+)\b", line.strip())
            if m:
                stem = m.group(1)
                break
        origin = os.path.join(ROOT, "tasks", stem + ".md") if stem else ""
        if origin and os.path.isfile(origin):
            if open(origin, "rb").read() != open(TASK_PATH, "rb").read():
                sys.stderr.write(
                    "TASK.md no longer matches tasks/%s.md\n" % stem +
                    "The contract is frozen at the branch's first commit. A criterion added or\n"
                    "relaxed here changes the terms of passing, and nothing downstream would see it.\n"
                    "  diff tasks/%s.md TASK.md\n" % stem)
                return 2

    criteria = read_task(TASK_PATH)
    if criteria:
        problems = contract_problems(criteria)
        if problems:
            sys.stderr.write("the task contract disagrees with itself:\n")
            for x in problems:
                sys.stderr.write("  %s\n" % x)
            sys.stderr.write("\nThis is a contract defect, not a failing check. Fix TASK.md.\n")
            return 2

    try:
        checks, ignored = discover(tier)
    except ContractError as exc:
        sys.stderr.write("%s\n" % exc)
        return 2
    if only:
        # Bramka per-podzadanie: wszystkie kryteria po każdym podzadaniu zrobiły ze "szybkiej"
        # bramki 33 sekundy.
        wanted = set(x.strip() for x in only.split(",") if x.strip())
        known = set(c[0] for c in checks if c[1] == "acceptance")
        unknown = sorted(wanted - known)
        if unknown:
            # Cicha literówka w --only zostawiłaby same sprawdzenia projektowe i zielone,
            # które nie osądziło ani jednego kryterium.
            sys.stderr.write("--only names %s, which TASK.md does not declare (have: %s)\n"
                             % (", ".join(unknown), " ".join(sorted(known)) or "none"))
            return 2
        checks = [c for c in checks if c[1] == "project" or c[0] in wanted]

    if not checks:
        sys.stderr.write("no checks found: add checks/<tier>-<id>.sh, or write TASK.md "
                         "criteria with `check:` lines\n")
        return 2

    # Zielona bramka, która nic nie osądziła, jest gorsza niż czerwona: fmt, zakres i typy
    # chętnie przechodzą, więc brak kryteriów dawał exit 0 na repo bez pracy — czyli w stanie,
    # w którym startuje każdy świeży projekt.
    #
    # ...ale dotyczy to wyłącznie warstw, które COŚ TWIERDZĄ O ZADANIU. `before` bez kryteriów
    # nie ma czego odwracać, a zielony `full` znaczy „praca zrobiona" — obie muszą odmówić.
    # `quick` to higiena wewnętrznej pętli (fmt, clippy, typy, zakres, słownictwo, tokeny) i jest
    # sensowna sama z siebie: uruchomiona w głównym repo, poza worktree zadania, ma powiedzieć
    # „higiena OK", a nie „nie wiem, kim jesteś". Odmowa w `quick` zamykała jedyną szybką pętlę,
    # jaką ma człowiek pracujący poza zadaniem — a jej zielone nigdy nie twierdziło, że praca
    # jest skończona, więc nie ma tu czego udawać.
    # ...i nie dotyczy TRUNKA. Na gałęzi brak kryteriów znaczy „kontrakt jest pusty" i jest
    # defektem. Na trunku znaczy „nie ma tu zadania", co jest stanem poprawnym i docelowym:
    # integrate.sh kasuje TASK.md przy lądowaniu, bo to artefakt gałęzi. Kryteria wylądowanego
    # zadania są już udowodnione — udowodniła je bramka gałęzi, zanim cokolwiek zmergowano.
    # Zadaniem bramki trunka jest „czy scalona całość dalej się buduje i przechodzi sprawdzenia
    # projektowe", nie „czy to zadanie zrobiło, co obiecało".
    # Zmierzone: pierwsze lądowanie S-2 skończyło się exit 2 na tym strażniku, dwie minuty po
    # tym, jak sam dodałem kasowanie TASK.md. Ta sama para poprawek widziana z dwóch stron.
    on_trunk = False
    try:
        on_trunk = subprocess.run(
            ["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=ROOT,
            capture_output=True, text=True, check=False,
        ).stdout.strip() == os.environ.get("LOADOUT_TRUNK", "main")
    except Exception:
        pass

    NEEDS_CRITERIA = () if on_trunk else ("before", "full")
    if not only and not any(c[1] == "acceptance" for c in checks):
        why = ("there is no TASK.md here" if not os.path.isfile(TASK_PATH)
               else "TASK.md declares no acceptance criteria")
        if tier in NEEDS_CRITERIA:
            sys.stderr.write(
                "%s, so this gate can only report on\n"
                "itself. Write `## AC-n` sections with `check:` lines before trusting it.\n" % why)
            return 2
        sys.stderr.write(
            "note: %s. Running project checks only —\n"
            "this tier reports hygiene, never that a task is done.\n" % why)

    # Sprawdzenie ma prawo wiedziec, na ktorym poziomie biegnie -- i tylko po to, zeby moc
    # powiedziec "jestem tu zbedne". `full-clippy` z `--all-targets` ZAWIERA `--lib`, wiec
    # w `full` oba clippy robia te sama prace i jeszcze bija sie o muteks cargo (niezmiennik 26).
    # Zmierzone 2026-08-16 przy ladowaniu T-27: drugie clippy czekalo 300 s i oddalo 2, przez co
    # trunk zaswiecil sie "MISCONFIGURED" na pustej maszynie. Sam `checks/quick-clippy.sh` pisze
    # w naglowku, ze pelna forma "biegnie raz, w bramce" -- implementacja tego nie odzwierciedlala.
    #
    # Decyzja mieszka w SPRAWDZENIU, nie tutaj (niezmiennik 23): to ono wie, co robi i co je
    # zastepuje. Bramka podaje fakt, nie polityke.
    os.environ["LOADOUT_TIER"] = tier
    per_check = CHECK_TIMEOUT[tier]
    head = "── verify %s " % tier
    print(head + "─" * max(3, 63 - len(head)))
    gate_lock = HeavyTierLock(tier)
    gate_lock.__enter__()
    t0 = time.time()
    for name in ignored:
        print("  note checks/%s has no <tier>- prefix, so it is NOT a check" % name)

    # DWIE FALE: projektowe do końca, potem kryteria. Wewnątrz fali równolegle (ścianą jest
    # maksimum, nie suma), między falami sekwencyjnie — bo sprawdzenie projektowe bywa tym,
    # co dopiero czyni kryteria wykonywalnymi.
    results = []
    for wave in ("project", "acceptance"):
        batch = [c for c in checks if c[1] == wave]
        if not batch:
            continue
        width = max(1, jobs if wave == "project" else min(jobs, ACCEPTANCE_JOBS))

        # LANE SZEREGOWY dla sprawdzen, ktore biora muteks cargo (niezmiennik 26).
        #
        # Zmierzone 2026-08-17 na T-36: `full-test` trzymal muteks 512 s, a `full-clippy`,
        # puszczony w tej samej fali, PRZESPAL na nim 242,88 s -- na WLASNYM zegarze -- po czym
        # oddal 2 i cala bramka zaswiecila sie "MISCONFIGURED". Kod byl w porzadku, oba kryteria
        # przeszly; bramka po prostu nie osadzila go przez wlasna kolejke.
        #
        # Sufit czekania nie da sie tego naprawic i to jest sedno: zeby `full-clippy` doczekal,
        # cap musialby byc >=512 s, a zeby zmiescil sie we WLASNYM budzecie (600 s) razem z
        # zimnym buildem -- <=360 s. Te dwa warunki sa sprzeczne, wiec kazda wartosc capa jest
        # zla. Zmienna do ruszenia nie jest wiec cap, tylko ROWNOLEGLOSC, ktora go wymusza.
        #
        # Puszczone szeregowo czekanie znika calkiem: nikt nie spi w kolejce, bo kolejki nie ma.
        # To jest takze SZYBSZE od poprzedniego zachowania -- tamte 242,88 s byly czystym snem.
        # Muteks zostaje jako siatka MIEDZY procesami (petla `quick` agenta obok bramki); tutaj
        # przestaje byc potrzebny, bo bramka juz nie tworzy kolizji, ktora on lagodzil.
        serial_ids = {c[0] for c in batch if takes_the_cargo_mutex(c[2])}
        heavy = [c for c in batch if c[0] in serial_ids]
        light = [c for c in batch if c[0] not in serial_ids]

        def one(c, pc=per_check):
            return run_one(c, budget_for(c[0], c[2], pc))

        done = {}
        with concurrent.futures.ThreadPoolExecutor(
                max_workers=width + (1 if heavy else 0)) as pool:
            futs = [(c[0], pool.submit(one, c)) for c in light]
            lane = pool.submit(lambda items: [one(c) for c in items], heavy) if heavy else None
            for cid, fut in futs:
                done[cid] = fut.result()
            if lane is not None:
                for r in lane.result():
                    done[r["id"]] = r
        results.extend(done[c[0]] for c in batch)

    failed, misconfigured = [], []
    for r in results:
        ok, note = verdict(tier, r)
        r["ok"], r["note"] = ok, note
        if not ok:
            failed.append(r["id"])
        # Sprawdzenie PROJEKTOWE, które wyszło dwójką, mówi "nie umiem sprawdzić" (brak
        # prettiera, brak cargo, brak tsconfigu) — to jest ten sam kod 2, co nasz własny,
        # i tak samo nie wolno go zlać z 1. Bez tego `npm install`, którego nikt nie zrobił,
        # czytał się w ship-task.sh jako czerwone zadanie. Kryteriów NIE dotyczy: `check:`
        # to dowolna komenda, a runnery używają 2 na zwykłą porażkę.
        state = "ok" if ok else "FAIL"
        if r["kind"] == "project" and r["rc"] == 2:
            misconfigured.append(r)
            state = "MISC"
        print("  %-4s %-28s %6.2fs  %s"
              % (state, r["id"], r["seconds"], note.replace("\n", " ")[:110]))
        if r["retried"]:
            # Retry, który tylko siedzi w paragonie, jest retry, którego nikt nie znajdzie.
            print("  !    %-28s %s" % (r["id"], r["retried"]))
        if not ok and note and len(note) > 110:
            for line in note.splitlines()[-12:]:
                print("       | %s" % line[:150])

    total = time.time() - t0
    gate_lock.__exit__(None, None, None)
    record(tier, results, failed, total)

    # Sufit poziomu wybacza WYŁĄCZNIE ten czas, który oracle sam przyznał ponad normę, i
    # tylko tyle, ile sprawdzenie z nadpisaniem naprawdę zjadło. Bez tego nadpisanie limitu
    # jest martwe (quick-clippy dostaje 420 s, po czym poziom przewraca się na 45 s i każdy
    # pierwszy zimny build kończy się kodem 3), a z płaskim doliczeniem całego nadpisania
    # sufit `quick` urósłby do 445 s także wtedy, gdy clippy nie miało czego kompilować.
    # Liczone przez `budget_for`, nie przez samą tabelę — inaczej kryterium wołające cargo
    # dostaje 420 s na sprawdzenie i wywraca się na 60-sekundowym suficie warstwy `before`,
    # czyli nadpisanie budżetu jest martwe dokładnie tam, gdzie powstało.
    ceiling = ceiling_for(tier, results, {c[0]: c[2] for c in checks}, per_check)
    granted = ceiling - CEILING[tier]
    print("─" * 63)
    print("  %d checks, %d failed, %.2fs (ceiling %.0fs%s)"
          % (len(results), len(failed), total, ceiling,
             ", %.0fs of it granted by CHECK_TIMEOUT_OVERRIDE" % granted if granted else ""))

    # Bez tego stdout (blokowo buforowany w potoku) ląduje PO stderr i log biegu czyta się
    # od końca. Ogon `RED: …` ma stać pod tabelą, którą tłumaczy.
    sys.stdout.flush()
    if misconfigured:
        # 2 wygrywa z 1 i z 3: skoro jedno sprawdzenie nie umiało się wykonać, werdykt
        # poziomu nie jest twierdzeniem o kodzie i nie wolno go tak przedstawić.
        sys.stderr.write("MISCONFIGURED: this tier could not judge the code\n")
        for r in misconfigured:
            first = (r["note"] or r["reason"] or "").splitlines()
            sys.stderr.write("  %s exited 2: %s\n" % (r["id"], first[0] if first else ""))
        sys.stderr.write("That is our setup, not a failing check -- exit 2 is never a red.\n")
        return 2
    if total > ceiling:
        sys.stderr.write("GATE TOO SLOW: %s took %.1fs, ceiling %.0fs\n"
                         % (tier, total, ceiling))
        return 3
    if failed:
        sys.stderr.write("RED: %s\n" % " ".join(failed))
        return 1
    return 0


# ---------------------------------------------------------------- paragon

def _git(*args):
    # Kod wyjścia SPRAWDZANY, nie zignorowany. Na gałęzi bez ani jednego commita
    # `git rev-parse HEAD` drukuje na stdout dosłowne "HEAD" i wychodzi 128 — paragon
    # zapisywał wtedy commit "HEAD", czyli proweniencję, która wygląda jak proweniencja
    # i nie jest nią. Złapane na świeżym repo Loadout, 2026-08-15.
    try:
        r = subprocess.run(("git",) + args, cwd=ROOT, capture_output=True,
                           text=True, timeout=5)
    except Exception:
        return None
    return r.stdout.strip() or None if r.returncode == 0 else None


def record(tier, results, failed, total):
    # Zapisz, co bramka zobaczyła; raport musi dać się odtworzyć z dysku. Zmierzone:
    # 36-minutowy bieg skończył się na `error_max_turns` z PUSTYM wynikiem — praca przeżyła,
    # relacja z niej nie.
    d = os.path.join(ROOT, "runs")
    os.makedirs(d, exist_ok=True)
    payload = {
        "tier": tier,
        "at": time.strftime("%Y-%m-%dT%H:%M:%S"),
        # Związany z drzewem, które osądził: "18/18 zielone" bez odpowiedzi CO było zielone
        # jest twierdzeniem o niczym.
        "commit": _git("rev-parse", "HEAD"),
        "tree": _git("rev-parse", "HEAD^{tree}"),
        "dirty": bool(_git("status", "--porcelain", "-uall")),
        "seconds": round(total, 2),
        "failed": failed,
        # KTORE sprawdzenia oddaly 2, osobno od `failed`. Paragon mowil dotad tylko "ok/nie ok",
        # a to zlewa dwie rozne rzeczy: "kod jest zly" (1) i "MY jestesmy zle skonfigurowani" (2).
        # `integrate.sh` musi je rozroznic, bo wybacza kod 2 przed pierwszym merge'em -- i bez tej
        # listy wybaczal takze sprzecznosc konfiguracji, czyli ladowal na drzewie, o ktorym bramka
        # wlasnie powiedziala, ze nie umie go osadzic. Dopisane 2026-08-19 po T-53.
        "misconfigured": [r["id"] for r in results if r["rc"] == 2],
        # Werdykt POZIOMU, nie surowy kod wyjścia. W `before` to przeciwieństwa dla każdego
        # kryterium, a to jest plik, który czyta --report.
        "checks": [{"id": r["id"], "kind": r["kind"],
                    "ok": r.get("ok", r["rc"] == 0),
                    "seconds": round(r["seconds"], 2),
                    "reason": (r.get("note") or r["reason"] or "")[:400],
                    "retried": r.get("retried", "")} for r in results],
    }
    # tmp + os.replace: zabity bieg nie zostawia rozdartego paragonu.
    tmp = os.path.join(d, ".last.tmp")
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, indent=1)
    os.replace(tmp, os.path.join(d, "last.json"))


def _receipt_is_current(d):
    # Paragon jest twierdzeniem o drzewie; powiedz, kiedy jest o innym. Zmierzone: last.json
    # mówił 18/18 zielone dla commita, na którym nikt już nie stał, a --report wydrukował to
    # bez słowa. Zapisana i nieporównywana proweniencja wygląda jak proweniencja.
    head = _git("rev-parse", "HEAD")
    notes, other_tree = [], False
    if d.get("commit") and head and d["commit"] != head:
        notes.append("this receipt describes %s and you are on %s -- it is stale"
                     % (d["commit"][:8], head[:8]))
        other_tree = True
    if d.get("commit") is None:
        notes.append("this receipt is tied to no commit (unborn branch, or written before "
                     "commit recording), so it cannot be checked against your tree")
        other_tree = True
    if d.get("dirty"):
        notes.append("the tree was dirty when this ran, so it describes uncommitted work")
    # Flaga obok tekstu, nie zamiast: wołający pytał wcześniej `"stale" in note`, więc
    # przeredagowanie komunikatu po cichu wyłączało odmowę (niezmiennik 20 — sprawdzaj
    # zachowanie, nie obecność stringa).
    return notes, other_tree


def report():
    """Wydrukuj stan z dysku. Działa po crashu, po zabiciu i po pustej odpowiedzi modelu."""
    path = os.path.join(ROOT, "runs", "last.json")
    if not os.path.isfile(path):
        print("no run recorded yet -- run ./verify.sh")
        return 2
    with open(path, encoding="utf-8") as fh:
        d = json.load(fh)
    titles = {c["id"]: c["title"] for c in read_task(TASK_PATH)}
    bad = [c for c in d["checks"] if not c["ok"]]

    print("last run: %s tier, %s, %.0fs" % (d["tier"], d["at"], d["seconds"]))
    notes, other_tree = _receipt_is_current(d)
    for note in notes:
        print("  ! %s" % note)
    for c in d["checks"]:
        if c.get("retried"):
            print("  ! %s %s" % (c["id"], c["retried"]))
    print()

    if other_tree:
        # Brudne drzewo to notatka; INNE drzewo to nie jest zaliczenie.
        print("this report describes a tree you are not on -- re-run the gate")
        return 2
    if bad:
        print("NOT DONE (%d):" % len(bad))
        for c in bad:
            print("  %-8s %s" % (c["id"], titles.get(c["id"], c["kind"])))
            for line in (c["reason"] or "").splitlines()[:3]:
                print("           %s" % line[:110])
    else:
        print("NOT DONE: nothing -- every check passed.")
    print()
    good = [c["id"] for c in d["checks"] if c["ok"]]
    print("passing (%d): %s" % (len(good), " ".join(good)))
    return 1 if bad else 0


# ---------------------------------------------------------------- wejście

def main(argv=None):
    ap = argparse.ArgumentParser(prog="verify.sh")
    ap.add_argument("tier", nargs="?", default="quick", choices=["before", "quick", "full"])
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--only", default=None,
                    help="run just these acceptance checks (comma separated) plus the "
                         "project checks -- the per-subtask gate")
    ap.add_argument("--report", action="store_true",
                    help="print the last run from disk; survives an empty model reply")
    ap.add_argument("--ids", action="store_true",
                    help="print criterion ids and exit (the one parser for TASK.md)")
    a = ap.parse_args(argv)
    if a.report:
        return report()
    if a.ids:
        for c in read_task(TASK_PATH):
            if c["check"]:
                print(c["id"])
        return 0
    return run(a.tier, a.jobs, a.only)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        # Przerwane to nie to samo co czerwone. 3 mówi "nie wiemy", 1 mówi "wiemy, że źle".
        sys.stderr.write("\ninterrupted\n")
        sys.exit(3)
