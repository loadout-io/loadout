# T-45 — Slownik Quiet Glass: kolory, kroje, drabinka

Pierwsze zadanie fali redesignu. **Zmienia wylacznie slownik, addytywnie** — ani jednego
komponentu `.tsx`. Po nim aplikacja ma nowa palete, nowe kroje i nowe promienie, ale wciaz stare
FORMY: kazda powierzchnia dostaje nowy promien przez alias, nie przez wlasna decyzje. Formy
migruja powierzchnia po powierzchni w T-46/T-47/T-48, a aliasy gina w T-50
(niezmiennik 25 — migracje sa addytywne).

Kierunek i pelna specyfikacja: `docs/superpowers/specs/2026-08-19-quiet-glass-design.md`,
kolejnosc i granice: `docs/superpowers/plans/2026-08-19-quiet-glass.md`.

## Co jest zepsute dzisiaj — zmierzone, nie ocenione

**1. Kroj zadeklarowany i nieistniejacy.** `src/styles/theme.css` deklaruje Intera od pierwszego
dnia, a `find src public -name '*.woff*'` daje **zero**, katalogu `public/` nie ma, `@font-face`
nie ma ani jednego. Aplikacja przez caly ten czas rysowala sie krojem systemowym — po cichu,
bez ani jednego bledu. Komentarz w `theme.css` sam to przyznaje i zglasza jako dlug.

**2. Paleta jest statystyczna srednia.** `#6ee0b0` na `#06090b` to mint-na-czerni; research
nazywa „near-black z jednym kwasowym akcentem" jednym z trzech domyslnych wygladow generowanych
przez modele. Nie chodzi o gust — chodzi o czestotliwosc.

**3. Jeden token robi dwie prace.** `DESIGN.md` §3 mowi jednoczesnie „`--accent` jest **jedynym
kolorem interaktywnym**" i „`--accent` znaczy **teraz**". To sa dwa rozne pytania i od tego
zadania maja dwa rozne tokeny: `--accent` (interaktywne) i `--live` (teraz).

**4. Etykieta pola i nadoczko sekcji dziela jeden stopien drabinki.** `--t-label` obsluguje
i `Name` nad polem, i `AGENTS` nad szyna. Skutek jest widoczny w wyroczni: `type-ladder.test.ts`
wymaga dzis `text-transform:uppercase` na **pieciu** selektorach makiety, w tym na `.fld label`
i `.card .role`, czyli na etykiecie pola i roli agenta. Wersaliki na kazdej etykiecie pola sa
najczestszym ruchem domyslnego panelu admina i pierwsza rzecza, po ktorej formularz przestaje
wygladac jak macOS.

## Skad biora sie wartosci

Z domu: `../meetnotes/src/design-tokens/{colors,layout,typography,glass}.css`. Regula tamtego
systemu brzmi doslownie *„QUIET GLASS: glass is CHROME, content is paper"*. Bierzemy **wartosci**,
nie inspiracje — dwie nasze aplikacje maja w Docku wygladac na rodzenstwo.

**Wartosci wchodza do repo jako wersjonowana migawka `docs/design/house-values.json`, nie jako
odczyt sciezki `../meetnotes` w czasie testu.** Powod jest mechaniczny: `scripts/ci.sh` jest
jedynym zrodlem prawdy o zieleni, a w GitHub CI katalogu obok nie ma. Test, ktory „grzecznie
sie pomija", daje `Tests N skipped (N)` — podpis z listy `NOT_A_REAL_RED`, czyli sprawdzenie,
ktore nie chroni niczego dokladnie tam, gdzie ma chronic. Odswiezenie migawki jest czynnoscia
swiadoma: jedna linia w naglowku pliku mowi, z ktorego dnia jest kopia.

Kroje tez sa z domu i tez sa **zmienne**: `hanken-grotesk.woff2` (34 704 B, waga 100–900)
i `jetbrains-mono.woff2` (40 404 B, waga 100–800), oba OFL. Dwa pliki, nie szesc.

**Read first:**
`docs/superpowers/specs/2026-08-19-quiet-glass-design.md` §3–§4 · `src/ui/shell/type-ladder.test.ts`
(wyrocznia drabinki — to ja przepisujemy, i to jest jedyna zmiana kontraktu w tym zadaniu)
· `src/styles/theme.css` blok `@layer components` (`.text-label` w wierszu 221, `.field` w 239)
· `checks/quick-tokens.sh` (co bramka egzekwuje i czego NIE widzi)
· `../meetnotes/src/design-tokens/colors.css` i `layout.css` (zrodlo migawki)
· `../meetnotes/src/styles.css` wiersze 22–34 (przepis na `@font-face`).

## Kto to robi

- **Agent:** `react-ui`
- **Druga opinia:** inny vendor niz pisarz (D3).
- **Artefakty biegu:** `runs/T-45/`

## Zalezy od

Niczego. To jest korzen fali.

## Co to zadanie posiada

- `src/styles/theme.css` — wartosci tokenow, nowe nazwy, dwa `@font-face`, rozszczepienie
  stopnia etykiety.
- `docs/design/DESIGN.md` — **wylacznie §3 (kolor) i §4 (typografia)**. Sekcje §5, §6, §7 i §9
  naleza do zadan powierzchniowych i T-50; nie ruszamy ich.
- `docs/mockup/index.html` — **blok `:root`**, dwa bloki `@font-face` (makieta jest wyrocznia
  wygladu, wiec musi rysowac sie TYM krojem, a nie systemowym) oraz dwie reguly, ktore przestaja
  byc nadoczkiem: `.fld label` i `.card .role` traca wersaliki, rozstrzelenie i rodzine `mono`.
  Reszta makiety nalezy do T-46/T-47/T-48.
- `docs/design/house-values.json` — **nowy**, migawka wartosci domu.
- `src/styles/fonts/hanken-grotesk.woff2`, `src/styles/fonts/jetbrains-mono.woff2` — **nowe**.
- `src/ui/shell/type-ladder.test.ts` — przepisanie jednego punktu tej wyroczni.
- **Trzy komponenty, ktore niosa nadoczka sekcji** — `src/sections/run/rail/rail.tsx`,
  `src/sections/run/session/session.tsx`, `src/sections/memory/index.tsx`. Waski mandat: cztery
  miejsca przechodza z `text-label` na `text-eyebrow` i trzy tracz `uppercase` wpisane z palca.
  Ani jednej innej zmiany w tych plikach. Bez tego rozszczepienie drabinki jest MARTWE i szkodliwe
  naraz — patrz AC-6.
- `src/ui/shell/palette.test.ts` — dwie listy strazy rosna razem z paleta. `rounded-lg`
  i `shadow-lg` przechodza z listy „obce" na „nasze", bo pasmo domu uzywa DOKLADNIE tych nazw;
  w ich miejsce wchodza `rounded-3xl` i `shadow-2xl`, ktore obce pozostaja. Do listy pozytywnej
  dochodza `bg-live`, `text-eyebrow`, `rounded-md`, `shadow-md`.
- Szesc plikow testow wymienionych przy `check:`.

**Czego to zadanie NIE dotyka:** ani jednego komponentu `.tsx` poza testami, ani jednego pliku
w `src-tauri/**`, ani `docs/DECISIONS-LOCKED.md`. Stare nazwy promieni **zyja**: `--radius-sq`
i `--radius-dot` wskazuja teraz na nowe pasmo, wiec kazde istniejace uzycie `rounded-sq`
dostaje 9 px bez dotykania komponentu. To jest cala migracja promieni w tym zadaniu.

## Poprawka harnessu, ktora to zadanie wymusilo

`checks/before-spec-owns.sh` bral `docs/mockup/index.html` za kod produkcyjny (`.html` nie bylo
na jego liscie wykluczen), potem parsowal `const` z jej `<script>` regexem RUSTA, wiec trafialy
do `symbols["rs"]`. Kryteria tego zadania sa w TypeScripcie, czyli szukaja w `symbols["ts"]`,
ktory zostawal PUSTY — warunek nie dal sie spelnic ani raz. Naprawione na `main` osobnym
commitem, z trzema kontrolami (negatywna dalej wychodzi 1). **Nie jest w OWNS tego zadania
i nie ma byc:** poprawki harnessu naleza do orkiestratora, nie do pisarza.

## Niezmienniki

- **25 — migracje sa addytywne i idempotentne.** Zastosowany do CSS: nowa nazwa dochodzi, stara
  zyje jako alias. Nazwa skasowana pod trzema powierzchniami, ktore ja jeszcze wolaja, zostawia
  element bez ani jednej reguly CSS — awarie, ktora nie rzuca wyjatku.
- **13 — jeden fakt, jedno miejsce.** `--accent` przestaje odpowiadac na dwa pytania.
- **14 / D5 — UI po angielsku, zero zargonu.** Zadanie nie dodaje ani jednego napisu.
- **21 — nie pisz artefaktu, ktorego zaden skrypt nie czyta.** `house-values.json` jest czytany
  przez AC-1 w kazdym biegu, nie tylko zapisany.
- **DESIGN §3 — tozsamosc != stan.** `--id-1..5` zostaja **nasze** i przygaszone; domowe
  `--graph-*` sa nasycone, bo obsluguja legende grafu.

## Kryteria akceptacji

## AC-1 Tokeny zgadzaja sie z migawka domu, wartosc po wartosci
check: npx --no-install vitest run src/styles/tokens-mirror-the-house.test.ts
expect: (\d+) passed

Test czyta `docs/design/house-values.json` i `src/styles/theme.css` w tym samym biegu i porownuje
po tabeli mapowania nazw (`surface-base` na `--color-bg`, `text-tertiary` na `--color-muted`,
`accent` na `--color-accent`, `live` na `--color-live`, `radius-sm` na `--radius-sm`, ...).

Asercje: (a) kazda pozycja migawki ma odpowiednik w `theme.css` **o tej samej wartosci**;
(b) migawka zostala naprawde odczytana — niepusta i o spodziewanej liczbie pozycji, bo parser,
ktory cicho nic nie dopasowal, dalby dwa puste zbiory i porownanie przeszloby na niczym;
(c) tabela mapowania nie ma pozycji **martwej** — nazwy, ktorej nie ma w zadnym z dwoch plikow;
(d) lista naszych swiadomych roznic (`--color-id-1..5`, `--color-human`, `--color-live-edge`)
jest w tescie **wymieniona jawnie** i wylaczona z porownania.

Punkt (d) jest tam, zeby nikt nie „naprawil" spojnosci, kasujac swiadoma roznice. Kolor agenta
o nasyceniu koloru stanu jest dokladnie ta awaria, ktora `DESIGN.md` §3 opisuje na przykladzie
poprzedniego prototypu: Forge dostawal `#ffb45b`, czyli ten sam hex co „wymaga uwagi".

*Slaba wersja:* wpisanie heksow z palca w test. Przechodzi takze wtedy, gdy dom zmieni palete,
a my zostaniemy w tyle — czyli mierzy nas, nie spojnosc.

## AC-2 Kroje naprawde sa, a deklaracja nie wystarcza
check: npx --no-install vitest run src/styles/fonts-are-really-here.test.ts
expect: (\d+) passed

Trzy ogniwa lancucha, kazde osobno:

Asercje: (a) kazdy `src:url(...)` z `@font-face` w `theme.css` wskazuje na plik, ktory **istnieje
na dysku** i ma **niezerowy rozmiar**; (b) `font-family` z `@font-face` jest **pierwszym czlonem**
odpowiedniego tokenu `--font-ui` / `--font-mono` — deklaracja rodziny, ktorej `@font-face` nie
definiuje, daje po cichu kroj zapasowy; (c) w `theme.css` nie ma ani jednej rodziny cytowanej
w `--font-*`, ktora nie ma swojego `@font-face` — czyli warunek dziala w obie strony;
(d) `font-weight` w `@font-face` jest **zakresem** (dwie liczby), bo pliki sa zmienne i pojedyncza
waga zmarnowalaby 34 kB.

*Slaba wersja:* `expect(css).toContain('Hanken Grotesk')`. Przechodzi dokladnie na tym defekcie,
ktory to zadanie zamyka — Inter byl zadeklarowany od pierwszego dnia i nie istnial w drzewie
ani przez chwile.

## AC-3 Lustro DESIGN.md i theme.css widzi takze tokeny rgba
check: npx --no-install vitest run src/styles/mirror-sees-alpha-too.test.ts
expect: (\d+) passed

`checks/quick-tokens.sh` porownuje oba pliki wzorcem `#[0-9a-fA-F]{6}`. **Ten wzorzec nie widzi
`rgba()`.** W starej palecie wszystkie 21 tokenow bylo heksami, wiec luka nie miala znaczenia.
W Quiet Glass **wiekszosc powierzchni i wszystkie obrysy to biel-alfa**, czyli tamto sprawdzenie
przestaje pilnowac ponad polowy palety — meldujac przy tym zielono i wypisujac „N colour tokens
agree". Ten test domyka luke **po stronie testow, nie w `checks/`**: `AGENTS.md` §7 wymaga tam
zgody czlowieka, a szersza wyrocznia jest tanszym i mniej ryzykownym sposobem.

Asercje: (a) kazdy token `rgba` w DESIGN.md §3 ma w `theme.css` **te sama** wartosc po
znormalizowaniu odstepow i zapisu alfy (`.09` i `0.09` sa rowne); (b) w obie strony — token
obecny tylko w jednym pliku jest porazka; (c) liczba porownanych tokenow `rgba` jest **wieksza
od zera** i test wypisuje ja w komunikacie, bo zero znaczy „nic nie zmierzono", nie „wszystko
sie zgadza"; (d) test sadzi takze heksy, wiec jest scislym nadzbiorem polowy 1 z `quick-tokens.sh`
— dwa egzekutory jednej reguly, nigdy dwie reguly.

Nadmiarowosc wobec `quick-tokens.sh` jest **nazwana w naglowku testu**, a nie przemilczana,
razem z warunkiem jej znikniecia: kiedy czlowiek zgodzi sie rozszerzyc tamten wzorzec o `rgba`,
polowa tego testu staje sie zbedna i ma zostac usunieta.

## AC-4 Aliasy promieni zyja i rozwijaja sie do nowego pasma
check: npx --no-install vitest run src/styles/radius-aliases-still-resolve.test.ts
expect: (\d+) passed

Asercje: (a) `--radius-sq` i `--radius-dot` **istnieja** w `theme.css`; (b) rozwijaja sie
odpowiednio do wartosci `--radius-sm` i `--radius-pill` — nie do wlasnych literalow;
(c) nowe pasmo `--radius-sm/md/lg/pill` istnieje w calosci; (d) `--radius-*` o wartosci
**24 px nie istnieje** — pasmo domu ma ten stopien, my go swiadomie nie bierzemy, bo narzedzie
o tej gestosci przy 24 px wyglada jak aplikacja na iPada.

Punkt (b) jest sedno: alias, ktory wskazuje na wlasna kopie liczby, przy nastepnej zmianie pasma
rozjedzie sie po cichu. Punkt (a) jest tym, ktory odroznia migracje addytywna od `DROP`.

*Slaba wersja:* sprawdzenie, ze `--radius-sm` istnieje. Nie mowi nic o tym, czy trzy powierzchnie,
ktore wolaja jeszcze `--radius-sq`, dalej maja regule CSS.

## AC-5 Wersaliki nosi nadoczko, nie etykieta pola
check: npx --no-install vitest run src/ui/shell/eyebrow-carries-the-capitals.test.ts
expect: (\d+) passed

To jest **jedyna zmiana kontraktu istniejacej wyroczni w tym zadaniu** i dlatego jest osobnym
kryterium, a nie dopiskiem. `src/ui/shell/type-ladder.test.ts` wymaga dzis
`text-transform:uppercase` na pieciu regulach makiety i argumentuje to wlasnym komentarzem,
cytujac `DESIGN.md` §4: „etykieta pola, WERSALIKI". Po tym zadaniu DESIGN §4 mowi co innego,
wiec tamten punkt egzekwowalby regule, ktorej wyrocznia **przestala chciec**.

Podzial: **trzy** reguly zostaja w wersalikach, bo sa nadoczkami sekcji (`.side h3`, `.rail h2`,
`.ctx .ch`). **Dwie** je traca, bo sa etykieta pola i rola agenta (`.fld label`, `.card .role`).

Asercje: (a) `--text-eyebrow` istnieje w `theme.css`, ma `text-transform:uppercase` **dokladnie
raz** i stoi w warstwie `@layer components`, zeby dalo sie go zniesc klasa `normal-case`;
(b) `--text-label` **nie** niesie wersalikow w zadnej regule arkusza; (c) trzy nadoczka w makiecie
**maja** `text-transform:uppercase`, czytane z makiety; (d) dwie reguly etykiet w makiecie
**nie maja** go, czytane z makiety; (e) `type-ladder.test.ts` po przepisaniu dalej niesie
kontrole przeciw pustemu porownaniu na kazdym odczycie z makiety — punkt czyta zrodlo tego
testu i wymaga, zeby liczba asercji „nothing was read out of" nie zmalala.

Punkt (e) jest tam, bo przepisanie wyroczni jest najlatwiejszym miejscem na ciche oslabienie:
test, ktory po zmianie porownuje dwa puste napisy, jest zielony i nic nie sprawdza.

*Slaba wersja:* skasowanie punktu o wersalikach z `type-ladder.test.ts` i napisanie nowego
o `--text-eyebrow`. Przechodzi, a jednoczesnie zdejmuje ochrone z dwoch regul, ktore MAJA
zostac w wersalikach — bo nikt ich juz nie sadzi.

## AC-6 Stopien nadoczka ma nosniki, a wersaliki nie sa wpisane z palca
check: npx --no-install vitest run src/ui/shell/eyebrow-has-carriers.test.ts
expect: (\d+) passed

**To kryterium dopisala DRUGA OPINIA, a nie zaden z pieciu pozostalych, i to jest cala jego
historia.** Zmierzone 2026-08-19 na gotowym, ZIELONYM zestawie: `theme.css` skasowal
`.text-label { text-transform: uppercase }` spod 42 uzyc `text-label`, a `--text-eyebrow` mial
ZERO nosnikow w komponentach. Skutek: makieta dalej zadala `AGENTS`, aplikacja rysowala `Agents`,
bramka byla zielona, i nic tego nie zglaszalo.

To jest dokladnie awaria z niezmiennika 25, ktora ten plik cytuje trzy razy: deklaracja skasowana
spod niezmigrowanych powierzchni, ktora nie rzuca wyjatku i nie pojawia sie w zadnym logu.
Pasmo promieni dostalo na to alias. Rozszczepienie drabinki aliasu dostac NIE MOZE — z klasy
`text-label` nie da sie odczytac, czy stoi na nadoczku sekcji, czy na etykiecie pola — wiec
zamiast aliasu ma to kryterium.

Asercje: (a) skaner odwiedzil wiecej niz 20 plikow produkcyjnych, bo inaczej nic nie zmierzyl;
(b) `--text-eyebrow` ma **co najmniej jeden** nosnik — stopien, ktorego nikt nie niesie, jest
martwy, a punkt sadzacy sam arkusz jest wtedy spelniony klasa, ktora nie jest napisany zaden
ekran; (c) **zaden `<h2>`/`<h3>` w kodzie produkcyjnym nie nosi `text-label`** — naglowek nie
jest etykieta, wiec stopien etykiety nie ma prawa na nim stac; (d) zaden komponent nie ma
`uppercase` wpisanego z palca, bo druga kopia jednego faktu (niezmiennik 13) przezywa zmiane
stopnia po cichu — i wlasnie tak trzy naglowki zachowaly wersaliki w tym zadaniu, gdy dwa inne
je stracily i nic nie zzielenialo na czerwono; (e) skaner rozwija stale klas
(`className={ZONE_TITLE}`), sprawdzone sonda — bez tego trzy naglowki strefy w Memory sa dla
niego niewidoczne i punkt (c) jest zielony na dziurze.

Punkt (c) jest tym, ktory lapie prawdziwy defekt, i dlatego jest sformulowany o SEMANTYCE,
nie o wygladzie: `rail.tsx` niosl `<h2 className="... text-label ...">Agents</h2>`. Taki naglowek
czyta sie poprawnie dokladnie tak dlugo, jak dlugo oba stopnie wygladaja jednakowo — a w chwili
rozszczepienia zmienia wyglad i nie ma czego zapytac.

*Slaba wersja:* asercja, ze `--text-eyebrow` istnieje w arkuszu. Dokladnie to sprawdza juz AC-5
i dokladnie to bylo zielone, gdy aplikacja przestala krzyczec.

<!-- OWNS
src/styles/theme.css
src/styles/fonts/hanken-grotesk.woff2
src/styles/fonts/jetbrains-mono.woff2
docs/design/DESIGN.md
docs/design/house-values.json
docs/mockup/index.html
src/ui/shell/type-ladder.test.ts
src/ui/shell/palette.test.ts
src/styles/tokens-mirror-the-house.test.ts
src/styles/fonts-are-really-here.test.ts
src/styles/mirror-sees-alpha-too.test.ts
src/styles/radius-aliases-still-resolve.test.ts
src/ui/shell/eyebrow-carries-the-capitals.test.ts
src/ui/shell/eyebrow-has-carriers.test.ts
src/sections/run/rail/rail.tsx
src/sections/run/session/session.tsx
src/sections/memory/index.tsx
-->
