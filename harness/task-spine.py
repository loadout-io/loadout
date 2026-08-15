#!/usr/bin/env python3
"""Kazde zadanie, ktore tworzy modul Rusta, musi miec w OWNS plik z jego DEKLARACJA.

Bez `pub mod x;` w rodzicu modul nie wchodzi do skrzyni. Test integracyjny linkujacy
`loadout_lib` sie nie skompiluje, warstwa `before` da "brak modulu" -- a to jest podpis
z NOT_A_REAL_RED, wiec bramka odrzuci calosc jako falszywa czerwien. Zadanie jest wtedy
NIEWYKONALNE w sposob, ktorego agent nie moze obejsc: checks/quick-scope.sh nie przepusci
zapisu poza blokiem OWNS, a harness/ i tasks/ sa dla niego zabronione.

Zmierzone 2026-08-15: ta klasa zatrzymala petle na T-02, T-03, T-04 i T-05, za kazdym razem
z innym objawem, wiec za kazdym razem diagnozowalem ja od zera. Naprawilem wtedy zadania
silnika i zatrzymalem sie dokladnie tam, gdzie skonczyly sie zadania silnika -- dziesiec
pozniejszych zadan zostalo z ta sama wada i dowiedzialbym sie o nich po kolei, placac
za kazde osobno.

CZEGO TO SPRAWDZENIE *NIE* WYMAGA, i dlaczego to jest istotne:

Deklaracja nie musi pochodzic od tego zadania. Wystarczy, ze bedzie na miejscu, KIEDY
zadanie ruszy. Sa trzy zrodla i wszystkie trzy sa legalne:

  1. modul jest juz zadeklarowany w repo (np. `pub mod engine;` od T-01),
  2. zadeklaruje go zadanie, ktore laduje WCZESNIEJ w kolejnosci z build-loop.sh,
  3. zadanie samo ma rodzica w OWNS i dopisze wiersz.

Dlatego symulujemy kolejnosc landowania zamiast patrzec na kazde zadanie osobno. Pierwsza
wersja tego skryptu tego nie robila i zadala od T-17 wlasnosci `lib.rs` -- mimo ze T-17
tylko DOPISUJE do `memory/mod.rs`, ktory tworzy T-16 dwa zadania wczesniej. Sprawdzenie,
ktore kaze poszerzyc OWNS ponad potrzebe, jest gorsze niz jego brak: uczy, ze blok OWNS
to formalnosc, a to jedyna granica, jaka ma bieg.

Wolane przez scripts/ci.sh. Kod 1 = brak, 0 = komplet.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "src-tauri/src"


def owns_of(path):
    m = re.search(r"<!--\s*OWNS(.*?)-->", path.read_text(), re.S)
    return [l.strip().rstrip("/") for l in m.group(1).strip().splitlines() if l.strip()] if m else []


def parent_decl(p):
    """Plik, w ktorym MUSI stanac deklaracja modulu dla sciezki p. None = korzen albo nie Rust."""
    if not p.startswith("src-tauri/src/"):
        return None
    rest = p[len("src-tauri/src/"):]
    if rest in ("lib.rs", "main.rs"):
        return None
    parts = rest.split("/")
    if len(parts) == 1 or (len(parts) == 2 and parts[1] == "mod.rs"):
        return "src-tauri/src/lib.rs"
    return "src-tauri/src/" + "/".join(parts[:-1]) + "/mod.rs"


def mod_name(p):
    parts = p[len("src-tauri/src/"):].split("/")
    if len(parts) == 1:
        return parts[0][:-3] if parts[0].endswith(".rs") else parts[0]
    return parts[-2] if parts[-1] == "mod.rs" else parts[-1][:-3]


def declared_in_repo(decl_file, name):
    """Czy `mod name;` jest juz osiagalny -- w spodziewanym rodzicu albo przez #[path]."""
    pf = ROOT / decl_file
    if pf.exists() and re.search(rf"^\s*(pub )?mod {re.escape(name)};", pf.read_text(), re.M):
        return True
    # `#[path = "..."] pub mod x;` deklaruje modul SPOZA domyslnego rodzica. T-02 tak wlasnie
    # podpiel drivers/fake.rs z engine/mod.rs; bez tego wyjatku sprawdzenie zglaszalo brak
    # w module, ktory od godzin stoi w trunku i sie kompiluje.
    if SRC.exists():
        for f in SRC.rglob("*.rs"):
            if re.search(rf'#\[path\s*=\s*"[^"]*"\]\s*(pub )?mod {re.escape(name)};', f.read_text(), re.S):
                return True
    return False


def task_order():
    """Kolejnosc landowania -- z build-loop.sh, zeby byla JEDNA lista, nie dwie (niezmiennik 23)."""
    text = (ROOT / "scripts/build-loop.sh").read_text()
    m = re.search(r"^TASKS=\((.*?)^\)", text, re.S | re.M)
    order = m.group(1).split() if m else []
    # S-3 i T-10 sa w BLOCKED, wiec nie ma ich w TASKS -- doklejamy na koniec, bo kiedys ruszą.
    blocked = re.search(r'^BLOCKED="([^"]*)"', text, re.M)
    for t in (blocked.group(1).split() if blocked else []):
        if t not in order:
            order.append(t)
    return order


# Zdania, ktorymi proza zadania odsyla agenta do czlowieka. Kazde z nich jest poprawne,
# kiedy sciezka NIE jest w OWNS -- i jest defektem, kiedy jest.
STOP_PHRASES = (
    r"nie\*{0,2}\s*posiadasz",
    r"zatrzymaj\s+się\s+i\s*\n?\s*zapytaj",
    r"to jest sytuacja z AGENTS\.md §7",
)


def prose_contradictions():
    """Proza mowiaca 'nie posiadasz X / zapytaj czlowieka' o pliku, ktory JEST w OWNS.

    Ten defekt jest gorszy niz sam brak wpisu w OWNS, bo jest niewidoczny dla wszystkich
    pozostalych sprawdzen: blok OWNS wyglada poprawnie, quick-scope przepusci zapis, a agent
    i tak stanie -- albo, co gorsza, zrobi to wbrew jawnej instrukcji wlasnego zadania.
    Zmierzone 2026-08-15 na T-05: dopisalem kregoslup do OWNS i nie tknalem tekstu obok.
    """
    bad = []
    for f in sorted((ROOT / "tasks").glob("[ST]-*.md")):
        s = f.read_text()
        owns = set(owns_of(f))
        decls = {p for p in owns if p.endswith(("lib.rs", "mod.rs")) and p.startswith("src-tauri/src/")}
        if not decls:
            continue
        # Akapit = blok do pustej linii. Sprzecznosc liczy sie w obrebie jednego akapitu,
        # bo "nie posiadasz" trzy akapity dalej dotyczy czegos innego.
        for para in re.split(r"\n\s*\n", s):
            if not any(re.search(p, para, re.I) for p in STOP_PHRASES):
                continue
            for d in decls:
                short = d[len("src-tauri/src/"):]
                if short in para or d in para:
                    ln = s[: s.index(para)].count("\n") + 1 if para in s else 0
                    bad.append((f.stem, d, ln, " ".join(para.split())[:150]))
                    break
    return bad


def unowned_test_files():
    """Plik testu wskazany przez `check:` musi byc w OWNS -- inaczej kontrakt nie moze go napisac.

    To ta sama rodzina co brak deklaracji modulu, tylko o krok wczesniej: faza kontraktu probuje
    utworzyc plik specyfikacji, quick-scope.sh odrzuca zapis poza blokiem OWNS, i petla staje
    przed pierwszym `./verify.sh before` -- zanim ktokolwiek zobaczy jedno kryterium.
    Zmierzone 2026-08-15 na T-24: trzy kryteria wolaja `cargo test --test workspace_*`,
    a zadnego z tych plikow nie ma w OWNS.
    """
    bad = []
    for f in sorted((ROOT / "tasks").glob("[ST]-*.md")):
        s = f.read_text()
        owns = owns_of(f)
        for line in s.splitlines():
            if not line.strip().startswith("check:"):
                continue
            # `cargo test --test <nazwa>` -> src-tauri/tests/<nazwa>.rs (konwencja cargo,
            # nie nasza: integracyjny target o tej nazwie MUSI tam lezec).
            m = re.search(r"--test\s+(\S+)", line)
            want = f"src-tauri/tests/{m.group(1)}.rs" if m else None
            if want is None:
                # vitest i inne: sciezka stoi w linii wprost
                m2 = re.search(r"(\S+\.(?:ts|tsx|rs))(?:\s|$)", line)
                want = m2.group(1) if m2 else None
            if want is None:
                continue
            # Wpis w OWNS moze byc katalogiem ("src-tauri/tests" albo "src/sections/run").
            if any(want == o or want.startswith(o.rstrip("/") + "/") for o in owns):
                continue
            bad.append((f.stem, want, line.strip()))
    return bad


def main():
    problems = []
    # Modul jest "zalatwiony", gdy juz stoi w repo albo gdy zadeklaruje go WCZESNIEJSZE zadanie.
    satisfied = set()

    for task in task_order():
        f = ROOT / f"tasks/{task}.md"
        if not f.exists():
            continue
        owns = owns_of(f)
        creates = []
        for p in owns:
            decl = parent_decl(p)
            if decl is None:
                continue
            name = mod_name(p)
            creates.append((decl, name))
            if (decl, name) in satisfied or declared_in_repo(decl, name):
                continue
            if decl not in owns:
                problems.append((task, decl, p, name))
        # Cokolwiek to zadanie deklaruje, jest juz na miejscu dla nastepnych.
        satisfied.update(creates)

    for task, decl, p, name in problems:
        print(f"{task:6} brakuje w OWNS: {decl}")
        print(f"         bo tworzy: {p}  (pub mod {name};)")
    if problems:
        print()
        print(f"zadan z brakiem: {len({t for t, _, _, _ in problems})}")
        print("detail: bez deklaracji modul nie wchodzi do skrzyni, a bramka odrzuci")
        print("detail: 'brak modulu' jako falszywa czerwien -- zadanie jest niewykonalne.")
        return 1

    contra = prose_contradictions()
    if contra:
        print("proza zabrania tego, na co pozwala blok OWNS:")
        for task, decl, ln, para in contra:
            print(f"  {task} linia ~{ln}: {decl} JEST w OWNS, a tekst odsyla do czlowieka")
            print(f"      {para}")
        print()
        print("detail: agent posluszny tekstowi stanie przed napisaniem jednej linii kodu;")
        print("detail: agent ufajacy OWNS zadziala wbrew jawnej instrukcji wlasnego zadania.")
        print("detail: napisz pozytywnie -- 'ten plik masz w OWNS WYLACZNIE po to, zeby dopisac")
        print("detail: `pub mod x;` -- zadnej innej zmiany'.")
        return 1

    unowned = unowned_test_files()
    if unowned:
        print("plik testu z linii check: jest poza blokiem OWNS:")
        for task, want, line in unowned:
            print(f"  {task}: {want}")
            print(f"      {line}")
        print()
        print("detail: faza kontraktu nie ma prawa go utworzyc -- quick-scope.sh odrzuci zapis,")
        print("detail: a petla stanie przed pierwszym `./verify.sh before`.")
        return 1

    print(f"task spine: {len(task_order())} zadan, kazdy modul Rusta ma gdzie sie zadeklarowac,")
    print("            zadna proza nie zabrania tego, na co pozwala blok OWNS,")
    print("            a kazdy plik testu z check: jest w OWNS swojego zadania")
    return 0


if __name__ == "__main__":
    sys.exit(main())
