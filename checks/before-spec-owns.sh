#!/usr/bin/env bash
# ~0,2 s. Kontrakt, który nie dotyka ani jednym kryterium kodu, który sam zmienia.
#
# CO TO ŁAPIE. Zadanie, którego CAŁA specyfikacja mierzy cudzy podsystem. Wtedy kryteria
# potrafią być zielone, gałąź potrafi być zielona, a rzecz, którą to zadanie miało zbudować,
# nie jest sprawdzona przez nic. To jest ta sama rodzina, co reszta szwów z przeglądu
# 2026-08-16: mechanizm istnieje, wygląda na osądzony, i nie jest.
#
# CZEGO TO NIE ŁAPIE, i mówię to wprost, bo check obiecujący więcej niż robi jest gorszy niż
# jego brak. NIE łapie kryterium, które dotyka własnego kodu, ale od spodu — sterując warstwę
# niżej niż to, co proza zadania obiecuje. Na to stoi tier `before`: przy pierwszej wersji T-28
# bramka powiedziała „AC-2 passes before implementation -- it certifies nothing" i to zadziałało.
# Tu chodzi o drugą dziurę: spec wycelowany w NIE TEN podsystem.
#
# REGUŁA. Przynajmniej jedno kryterium musi mieć spec odwołujący się do symbolu zadeklarowanego
# w PRODUKCYJNYCH plikach z bloku OWNS. Nie każde — zmierzone 2026-08-17 na 159 istniejących
# specach: 157 dotyka własnego kodu, a dwa wyjątki (`T-08 AC-8`, `T-25 AC-2`) montują `App`
# i `sectionEntry`, czyli prawdziwy kod produkcyjny należący do INNEGO zadania. To jest legalny
# wzorzec — ekran dowodzi swojego istnienia przez powłokę — więc reguła „każdy spec" karałaby
# uczciwą robotę. Wersja „przynajmniej jedno" ma na tych samych danych ZERO fałszywych trafień
# na 38 zadaniach.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

# Bez kontraktu nie ma czego sądzić: na trunku ten check milczy i to jest poprawne.
[ -f TASK.md ] || { echo "spec-owns: no TASK.md here, nothing to judge"; exit 0; }
command -v python3 >/dev/null 2>&1 || { echo "python3 is not on PATH" >&2; exit 2; }

exec python3 - <<'PY'
import os, re, sys

SPEC_PATH = re.compile(r"(?:^|[\s=\"'])((?:[\w.@-]+/)+[\w.@-]+\.(?:test|spec)\.[jt]sx?)")
CARGO_TARGET = re.compile(r"--test[= ]+([\w-]+)")
RS_DECL = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?"
    r"(?:fn|struct|enum|trait|const|static|type)\s+([A-Za-z_]\w*)", re.M)
TS_DECL = re.compile(
    r"^\s*export\s+(?:default\s+)?(?:async\s+)?"
    r"(?:function|const|class|type|interface)\s+([A-Za-z_]\w*)", re.M)

md = open("TASK.md", encoding="utf-8", errors="replace").read()

# TERMINATOR MOZE BYC SKLEJONY Z OSTATNIA SCIEZKA i przez lata byl -- 42 z 60 plikow zadan
# koncza blok bajtami `...cancel.rs-->`, bez nowej linii. Stara forma `<!-- OWNS\n(.*?)\n-->`
# NIE DOPASOWYWALA ich w ogole, wiec ten check wychodzil zerem z napisem "nothing to judge"
# i NIE SADZIL NICZEGO na 42 zadaniach (niezmiennik 19: zielone bez dowodu jest czerwone).
# Ta sama forma co `quick-permissions.sh:78` i `harness/task-spine.py:42` -- cztery konsumenty
# OWNS musza czytac ten blok identycznie. Zmierzone 2026-08-19 na T-10.
owns = re.search(r"<!--\s*OWNS(.*?)-->", md, re.S)
if not owns:
    # Brak bloku OWNS to defekt kontraktu, ale ma go czyj inny check; tutaj milczymy,
    # zeby jedna wada nie swiecila w dwoch miejscach pod dwiema nazwami.
    print("spec-owns: TASK.md carries no OWNS block, nothing to judge")
    raise SystemExit(0)

# Produkcyjne, czyli KOD, ktory ten check umie czytac. Lista jest DODATNIA, nie odejmujaca,
# i to jest poprawka z 2026-08-19 po fałszywym trafieniu na T-45.
#
# CO SIE STALO. Stara wersja odejmowala testy, `.md` i `.txt`, wiec `docs/mockup/index.html`
# — dokument-wyrocznia, nie kod — przechodzil jako "produkcyjny". Potem `<script>` z makiety
# byl parsowany REGEXEM RUSTA (bo plik nie konczy sie na .ts/.tsx), wiec jego trzy `const`
# ladowaly w `symbols["rs"]`. Kryteria T-45 sa w TypeScripcie, czyli szukaja w `symbols["ts"]`,
# ktory zostawal PUSTY. Warunek nie dal sie spelnic ANI RAZ, przy kontrakcie, ktory calym
# swoim specem mierzy wlasny kod (theme.css, DESIGN.md, blok :root makiety, dwa pliki krojow).
#
# DLACZEGO TA ZMIANA JEST BEZPIECZNA, a nie tylko wygodna. Ekstrakcja symboli NIZEJ i tak umie
# wylacznie dwa jezyki: `.ts/.tsx` do `symbols["ts"]`, wszystko inne do `symbols["rs"]`. Plik,
# ktorego zaden z dwoch parserow nie rozumie, wnosil do tego zbioru wylacznie szum. Zwezenie
# listy moze wiec zamienic falszywa czerwien na zielen, i NIGDY zieleni na czerwien: mniej
# plikow to mniej symboli, a zero symboli wpada w jawna furtke ponizej ("declare nothing yet").
# Kontrola negatywna do tej poprawki: kontrakt, ktory posiada prawdziwy plik `.ts` z symbolami
# i celuje kryterium w cudzy spec, dalej wychodzi 1.
CODE_EXT = (".rs", ".ts", ".tsx")
prod = [f for f in owns.group(1).split()
        if not re.search(r"(^|/)tests?/|\.(test|spec)\.[jt]sx?$", f)
        and (f.endswith(CODE_EXT) or os.path.isdir(f))]

# OWNS wskazuje takze KATALOGI (src-tauri/src/store). Bez rozwiniecia check czytalby
# wylacznie lib.rs -- czyli liste modulow -- i oskarzal T-06 oraz T-12 na pusto.
expanded = []
for f in prod:
    if os.path.isfile(f):
        expanded.append(f)
    elif os.path.isdir(f):
        for root, _, names in os.walk(f):
            expanded += [os.path.join(root, n) for n in names
                         if n.endswith((".rs", ".ts", ".tsx"))]

# PAROWANIE JEZYKOW. Spec w Rustzie sadzimy symbolami z plikow .rs, spec w TS -- z .ts/.tsx.
# Bez tego check przechodzil na kontrakcie, ktorego wszystkie kryteria wskazywaly spec FRONTOWY
# przy zadaniu posiadajacym wylacznie Rusta: zderzyly sie `read` i `serve` -- zwykle angielskie
# slowa, ktore sa nazwami funkcji po jednej stronie i zwyklym tekstem po drugiej.
symbols = {"rs": set(), "ts": set()}
for f in expanded:
    text = open(f, encoding="utf-8", errors="replace").read()
    if f.endswith((".ts", ".tsx")):
        symbols["ts"] |= {m.group(1) for m in TS_DECL.finditer(text)}
    else:
        symbols["rs"] |= {m.group(1) for m in RS_DECL.finditer(text)}
# Krotkie nazwy (`new`, `id`, `run`) trafiaja w kazdy plik i zamienilyby ten check w ozdobe.
for k in symbols:
    symbols[k] = {s for s in symbols[k] if len(s) > 3}

if not (symbols["rs"] or symbols["ts"]):
    print("spec-owns: the owned production files declare nothing yet -- the skeleton is not "
          "written, so there is nothing for a spec to reach")
    raise SystemExit(0)

specs = []
for cid, check in re.findall(r"^## (AC-\d+)[^\n]*\ncheck:\s*(.+)$", md, re.M):
    for m in SPEC_PATH.finditer(check):
        specs.append((cid, m.group(1)))
    for m in CARGO_TARGET.finditer(check):
        specs.append((cid, "src-tauri/tests/%s.rs" % m.group(1)))

present = [(cid, p) for cid, p in specs if os.path.isfile(p)]
if not present:
    # Faza wstepna: kryteria istnieja, plikow jeszcze nie ma. To jest stan OCZEKIWANY przed
    # commitem kontraktowym i nie jest twierdzeniem o niczyim kodzie.
    print("spec-owns: no spec file exists yet (%d named) -- the contract phase writes them"
          % len(specs))
    raise SystemExit(0)

def used_as_code(sym, text, lang):
    """Czy to WYWOLANIE/uzycie, a nie samo slowo w komentarzu?

    `re.search(r"\bread\b")` trafia w kazde zdanie, ktore mowi "read". Zadamy wiec
    skladniowego sasiada: nawiasu, sciezki modulu, literalu struktury albo klamry importu.
    """
    e = re.escape(sym)
    if lang == "rs":
        pats = [r"\b%s\s*[({<]" % e, r"::\s*%s\b" % e, r"\b%s\s*::" % e]
    else:
        pats = [r"\b%s\s*[({<]" % e,
                r"import\s*(?:type\s*)?\{[^}]*\b%s\b[^}]*\}" % e,
                r"<\s*%s\b" % e]
    return any(re.search(p, text) for p in pats)


touching = []
for cid, p in present:
    text = open(p, encoding="utf-8", errors="replace").read()
    lang = "ts" if p.endswith((".ts", ".tsx", ".js", ".jsx")) else "rs"
    hit = next((s for s in sorted(symbols[lang]) if used_as_code(s, text, lang)), None)
    if hit:
        touching.append((cid, p, hit))

if not touching:
    sys.stderr.write(
        "not one criterion reaches the code this task owns:\n")
    for cid, p in present:
        sys.stderr.write("  %-6s %s\n" % (cid, p))
    sys.stderr.write(
        "\nthe owned production files declare %d symbol(s) and no spec mentions any of them.\n"
        "A contract that only measures somebody else's subsystem can go green while the thing\n"
        "this task was written to build is checked by nothing. Point at least one criterion at\n"
        "the code in the OWNS block -- or the OWNS block is naming files this task does not need.\n"
        % (len(symbols["rs"]) + len(symbols["ts"])))
    raise SystemExit(1)

cid, p, hit = touching[0]
print("spec-owns: %d of %d existing spec(s) reach the owned code (%s touches %s)"
      % (len(touching), len(present), cid, hit))
PY
