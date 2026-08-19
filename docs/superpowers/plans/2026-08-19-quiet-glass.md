# Loadout Quiet Glass — plan wdrożenia

> **Dla agentów wykonawczych:** jednostką wykonania w tym repo **nie jest** ten plik, tylko
> `tasks/T-NN.md`. Ten dokument ustala kolejność, granice i interfejsy między zadaniami; plik
> zadania jest kontraktem, który czyta bramka. Pętla pracy: `AGENTS.md` §2.

**Cel:** zamienić wygląd Loadouta na Loadout Quiet Glass — paleta, kroje, promienie, materiał,
powłoka, formy komponentów, znak i ikona — bez ani jednej czerwonej bramki pomiędzy.

**Architektura:** tokeny wchodzą **addytywnie** (nowe nazwy dochodzą, stare żyją jako aliasy),
potem migruje się powierzchnia po powierzchni, a stare tokeny giną na końcu. Każda powierzchnia
niesie **razem** swój fragment makiety i swoje komponenty, bo wyrocznie porównują te dwie rzeczy
w tym samym biegu testu.

**Stack:** React 19 + Tailwind v4 (`@theme` w `src/styles/theme.css`), vitest w środowisku
**node** (bez jsdom — patrz Global Constraints), Tauri 2, Rust.

**Spec:** [`docs/superpowers/specs/2026-08-19-quiet-glass-design.md`](../specs/2026-08-19-quiet-glass-design.md)

---

## Global Constraints

Obowiązują w **każdym** zadaniu tej fali. Wartości przepisane ze specu dosłownie.

- **Akcent** `#6e76ff` — znaczy wyłącznie „to jest interaktywne". **Nigdy** nie znaczy „teraz".
- **„Teraz"** `--live #ff7a5c`. **Czeka na ciebie** `--attend #f5b14c`. **Zepsute**
  `--fail #ff6b6b`. **Człowiek** `--human #9d7bff`. Piąty stan nie istnieje.
- **`--live` i `--fail` nigdy nie dzielą formy.** `--live`: podkład aktywnego wiersza, jego obrys,
  aktywny segment paska, pulsująca kropka, kropka karty w tle. `--fail`: glif `✕`, obrys chipa,
  lewa krawędź bloku błędu.
- **Promienie:** wyłącznie `--radius-sm 9px`, `--radius-md 13px`, `--radius-lg 18px`,
  `--radius-pill 999px`. **24 px nie istnieje.** Wiersz strumienia nie ma promienia wcale.
- **Cień wyłącznie pod tym, co pływa.** W całej aplikacji pływa jedna rzecz: panel nawigacji.
- **Szkło jest chrome, treść jest papierem.** Szkło nie wchodzi pod tekst ani pod kod.
- **Sufit chrome nad pierwszą treścią: 96 px.** Wersja, która wchodzi: `6 + 1 + 32 + 52 = 91`.
- **Kroje:** `--font-ui "Hanken Grotesk"`, `--font-mono "JetBrains Mono"`, oba jako `.woff2`
  w repo. Mono znaczy „to wyprodukowała maszyna".
- **`--t-label` jest zdaniowe, `--t-eyebrow` jest w wersalikach.** Etykieta pola nigdy nie jest
  w wersalikach.
- **Żaden literał w `src/**`:** `checks/quick-tokens.sh` odrzuca hex, liczbowe `font-size`
  i `border-radius` bez `var(`, oraz arbitralne wartości Tailwinda (`text-[13px]`,
  `rounded-[9px]`, `bg-[#07070b]`, `fill-[…]`, `stroke-[…]`). Gradienty ikony mieszkają
  **poza** `src/`.
- **Vitest biegnie w node.** Repo nie ma `jsdom` ani `environment` w `vite.config.ts`
  (sprawdzone 2026-08-19). Testy renderują `renderToStaticMarkup`; **`getComputedStyle`
  i efekty Reacta nie istnieją**. Każde kryterium musi być spełnialne bez DOM.
- **Kontrakt kryterium:** jedna linia `check:` na kryterium, dokładnie jeden plik testu wskazany
  ścieżką, ścieżka globalnie unikalna we wszystkich `tasks/*.md`, `## AC-n` bez luk,
  `expect: (\d+) passed`. Nigdy filtr po nazwie testu.
- **UI po angielsku, dokumentacja po polsku** (D5). Zero żargonu z tabeli `00-SYNTHESIS.md` §2.2.

---

## Dlaczego kolejność ze specu §8.2 się zmienia

Spec kazał: makieta → powłoka → …, z uzasadnieniem „makieta jest wyrocznią". To jest prawda
o **kierunku prawdy**, ale nie o **kolejności lądowania**. Trzy wyrocznie porównują makietę
z wyrenderowaną aplikacją w tym samym biegu:

| Wyrocznia | Co z czym porównuje |
|---|---|
| `type-ladder.test.ts` | `:root` makiety ↔ skompilowany arkusz Tailwinda |
| `shell-matches-mockup.test.tsx` | reguły `.app`/`.nav` makiety ↔ wyrenderowany `App` |
| `run-matches-mockup.test.tsx` | reguły `.work`/`.feedcol` makiety ↔ wyrenderowany ekran Run |

Zmiana makiety bez komponentu (albo odwrotnie) **przewraca bramkę pomiędzy**, a `./integrate.sh`
wymaga pełnej zieleni po każdej gałęzi. Stąd dwie reguły tej fali:

1. **Powierzchnia jedzie razem ze swoim fragmentem makiety.** Jedno zadanie = jedna powierzchnia
   + jej reguły w makiecie + jej komponenty + jej wyrocznie.
2. **Tokeny wchodzą addytywnie.** T-45 dokłada nowe nazwy i zmienia wartości, ale
   **`--radius-sq` i `--radius-dot` zostają żywe jako aliasy**, bo trzy powierzchnie jeszcze ich
   używają. Giną w T-50, kiedy nikt ich już nie woła. To jest niezmiennik 25 zastosowany do CSS:
   migracja addytywna i idempotentna, nigdy `DROP`.

---

## Struktura plików

### Zmieniane

| Plik | Odpowiedzialność po fali | Zadanie |
|---|---|---|
| `docs/design/DESIGN.md` | źródło systemu projektowego | T-45 (§3–4), T-46 (§5, §7), T-47/48 (§6), T-50 (§9) |
| `src/styles/theme.css` | lustro DESIGN.md, jedyny plik z heksami | T-45, T-50 |
| `docs/mockup/index.html` | wyrocznia układu | T-45 (`:root`), T-46 (powłoka), T-47 (Run), T-48 (sekcje) |
| `src/App.tsx`, `src/ui/shell/*` | rama okna, nawigacja, karty, pasek | T-46 |
| `src/sections/run/**` | strumień, strefa „teraz", wiersz wejścia, szyna | T-47 |
| `src/sections/{agents,skills,memory,workflows}/**` | listy, formularze, inspektory | T-48 |
| `docs/DECISIONS-LOCKED.md` | D1 | T-50 |

### Nowe

| Plik | Odpowiedzialność | Zadanie |
|---|---|---|
| `src/styles/fonts/hanken-grotesk-{400,500,600,700}.woff2` | krój UI | T-45 |
| `src/styles/fonts/jetbrains-mono-{400,700}.woff2` | krój wartości maszynowych | T-45 |
| `src/ui/shell/nav-icons.tsx` | pięć glifów nawigacji, `currentColor` | T-46 |
| `src/ui/brand/mark.tsx` | znak, dwa tokeny, zero literałów | T-49 |
| `docs/branding/loadout-{icon,icon-32,icon-16,logo,mark}.svg` | marka | T-49 |
| `src-tauri/icons/*` | zestaw ikon aplikacji | T-49 |

---

## Zadania

### T-45 — Tokeny, kroje i `:root` makiety

**Powierzchnia:** żadna. To zadanie zmienia **wyłącznie słownik**, addytywnie.

**Pliki**
- Modify: `docs/design/DESIGN.md` §3 (kolor), §4 (typografia)
- Modify: `src/styles/theme.css`
- Modify: `docs/mockup/index.html` — wyłącznie blok `:root`
- Create: `src/styles/fonts/*.woff2` (6 plików), `@font-face` w `theme.css`
- Test: `src/styles/tokens-mirror-the-house.test.ts`, `src/styles/fonts-are-really-here.test.ts`

**Interfejsy**
- Produkuje nazwy tokenów, których wołają T-46, T-47, T-48: `--bg`, `--panel`, `--raised`,
  `--well`, `--overlay`, `--solid`, `--hover`, `--scrim`, `--ink`, `--body`, `--muted`,
  `--line`, `--line-strong`, `--line-subtle`, `--accent`, `--accent-hover`, `--accent-active`,
  `--accent-soft`, `--accent-ring`, `--live`, `--live-soft`, `--live-edge`, `--attend`,
  `--attend-soft`, `--attend-edge`, `--fail`, `--fail-soft`, `--fail-edge`, `--human`,
  `--human-soft`, `--human-edge`, `--id-1..5`, `--radius-sm/md/lg/pill`, `--t-*`.
- **Zachowuje** `--radius-sq` i `--radius-dot` jako aliasy (`--radius-sq: var(--radius-sm)`).
  Znikają w T-50.

**Kryteria**

- **AC-1 — tokeny zgadzają się z domem, wartość po wartości.**
  Test czyta `../meetnotes/src/design-tokens/{colors,layout,typography,glass}.css` **w tym samym
  biegu** i porównuje z `src/styles/theme.css` po tabeli mapowania nazw
  (`--surface-base` → `--color-bg` itd.). Pada, gdy którakolwiek wartość się rozjedzie, **i pada
  też wtedy, gdy tabela mapowania nic nie dopasowała** — kontrola przeciw pustemu porównaniu.
  *Słaba wersja:* wpisanie heksów z palca w test. Przechodzi także wtedy, gdy dom zmieni paletę,
  a my zostaniemy w tyle — czyli mierzy nas, nie spójność.
  Czego test **nie** obejmuje i to jest nazwane: `--id-1..5` i `--human` są **nasze**, więc nie
  mają odpowiednika w domu; test wymienia je jawnie jako wyłączone, żeby nikt nie „naprawił"
  spójności, kasując świadomą różnicę.

- **AC-2 — kroje naprawdę są, a deklaracja nie wystarcza.**
  Trzy asercje łańcuchowe: (a) każdy plik z `@font-face` **istnieje na dysku** i ma niezerowy
  rozmiar; (b) rodzina z `@font-face` jest **pierwszym członem** `--font-ui` / `--font-mono`;
  (c) w `theme.css` nie ma ani jednej rodziny zadeklarowanej bez `@font-face`.
  *Słaba wersja:* `expect(css).toContain('Hanken Grotesk')`. Przechodzi dokładnie na tym
  defekcie, który to zadanie zamyka — Inter był zadeklarowany od pierwszego dnia i nie istniał.

- **AC-3 — lustro DESIGN.md ↔ theme.css widzi też `rgba()`.**
  `checks/quick-tokens.sh` porównuje wzorcem `#[0-9a-fA-F]{6}`, który `rgba()` **nie widzi**.
  W Quiet Glass to ponad połowa palety. Kryterium: sonda sadzi rozjazd w tokenie `rgba`
  (`--line` w DESIGN.md ≠ `--color-line` w theme.css) i wymaga **czerwieni**, potem przywraca.
  **Wymaga zgody człowieka** na zmianę w `checks/` (`AGENTS.md` §7). Bez zgody: AC-3 wypada
  z zadania, a dług idzie do `docs/HARNESS-QUEUE.md` jako świadomy, nazwany.

- **AC-4 — `--radius-sq` i `--radius-dot` żyją i wskazują na nowe pasmo.**
  Migracja addytywna: test wymaga, żeby oba aliasy istniały **i** rozwijały się do
  `--radius-sm` / `--radius-pill`. Bez tego trzy powierzchnie, które ich jeszcze wołają,
  zostają bez ani jednej reguły CSS — awaria, która nie rzuca wyjątku.

---

### T-46 — Powłoka Quiet Glass

**Powierzchnia:** rama okna, nawigacja, karty workspace, pasek loadoutu.

**Pliki**
- Modify: `docs/mockup/index.html` — `.app`, `.nav`, `.brand`, `.tabs`, `.strip`, `.blk`
- Modify: `src/App.tsx`, `src/ui/shell/titlebar.tsx`, `src/ui/shell/window.tsx`,
  `src/sections/run/tabs/*`, `src/sections/run/strip/*`
- Create: `src/ui/shell/nav-icons.tsx`
- Modify: `docs/design/DESIGN.md` §5 (przestrzeń i kształt), §7 (ruch)
- Test: `src/ui/shell/floating-pane-fits-the-ceiling.test.ts`,
  `src/ui/shell/nav-icons-carry-the-accent.test.tsx`

**Interfejsy**
- Konsumuje: wszystkie tokeny z T-45.
- Produkuje: `NAV_WIDTH = 208`, `CHROME_INSET_TOP = 38`, `WINDOW_INSET = 6`,
  `TABS_HEIGHT = 32`, `STRIP_HEIGHT = 52` — czytane przez T-47.
- Produkuje `NavIcon` z `src/ui/shell/nav-icons.tsx`: `(section: Section) => ReactElement`.

**Kryteria**

- **AC-1 — chrome nad pierwszą treścią mieści się w 96 px, i liczba jest policzona, nie wpisana.**
  Test czyta z makiety `padding` ramy, `border` kartki treści, `height` kart i `height` paska,
  **sumuje je** i porównuje z sufitem przeczytanym z `docs/ARCHITECTURE.md` §7. Cztery odczyty,
  każdy z osobną asercją na to, że coś dopasował.
  *Słaba wersja:* `expect(TABS_HEIGHT + STRIP_HEIGHT).toBeLessThanOrEqual(96)`. To jest **ta
  sama wada**, którą `docs/STATUS.md` nazywa wzorcowym przykładem: asercja `TITLEBAR_HEIGHT <= 96`
  była zielona przy 138 px realnego chrome, bo mierzyła jeden pasek z trzech.

- **AC-2 — `CHROME_INSET_TOP` i `trafficLightPosition` są mierzone razem.**
  `38 = 16 (trafficLightPosition.y z tauri.conf.json) + 20 (wysokość świateł) + 8 (odstęp)
  − 6 (odstęp okna)`. Test czyta `trafficLightPosition.y` z `src-tauri/tauri.conf.json`
  i odstęp z makiety, liczy, i porównuje ze stałą. Zmiana jednej liczby bez drugiej jest
  czerwienią; osobno każda wygląda rozsądnie.

- **AC-3 — panel nawigacji pływa, i jest jedyną rzeczą, która pływa.**
  Asercje: (a) panel ma cień; (b) **żaden inny** element powłoki w makiecie nie ma `box-shadow`
  z niezerowym przesunięciem — czytane z makiety, nie z listy w teście; (c) szyna agentów, pasek
  i karty mają wyłącznie cienie `inset` (refleks), bo leżą, nie pływają.

- **AC-4 — akcent bierze glif, nie tło.**
  Aktywny wiersz nawigacji: tło `--shell-active-bg` (neutralne), etykieta `--ink`, **glif
  `--accent`**. Asercje: (a) aktywny wiersz nie niesie `--accent` w tle ani w obrysie;
  (b) niesie go dokładnie raz, na glifie; (c) który wiersz jest aktywny, wynika z `aria-current`,
  a nie z drugiej kopii tej prawdy w klasie (niezmiennik 13).

- **AC-5 — pięć glifów, i gramatyka ikon jest sprawdzalna.**
  Węzły i krawędzie **wyłącznie** dla rzeczy, które są grafem. Asercje: (a) glif `Workflows`
  niesie okręgi **i** linie; (b) glify `Agents`, `Skills`, `Memory` nie niosą ani jednej linii
  łączącej okręgi; (c) `Run` jest jedną ścieżką zamkniętą. To niezmiennik 17 przeniesiony
  na ikonografię: nie rysujemy relacji tam, gdzie relacji nie ma.

---

### T-47 — Ekran Run w Quiet Glass

**Powierzchnia:** strumień, strefa „teraz", wiersz wejścia, szyna agentów.

**Pliki**
- Modify: `docs/mockup/index.html` — `.work`, `.feedcol`, `.feed`, `.ln`, `.detail`, `.now`,
  `.slots`, `.entry`, `.box`, `.rail`, `.card`, `.st`
- Modify: `src/sections/run/index.tsx`, `feed/line.tsx`, `feed/now.tsx`, `entry/entry.tsx`,
  `rail/rail.tsx`, `session/session.tsx`
- Modify: `docs/design/DESIGN.md` §6 — `stream-line`, `history-line`, `agent-card`
- Test: `src/sections/run/live-and-fail-never-share-a-form.test.ts`,
  `src/sections/run/glyph-column-carries-no-arrow.test.tsx`

**Interfejsy**
- Konsumuje `WINDOW_INSET`, `TABS_HEIGHT`, `STRIP_HEIGHT` z T-46.

**Kryteria**

- **AC-1 — `--live` i `--fail` nie dzielą formy, sprawdzone statycznie.**
  Test czyta źródła komponentów Run jako tekst (wzorzec `shell-matches-mockup`), zbiera dwa
  zbiory nazw klas: te na elementach niosących `live-*` i te na elementach niosących `fail-*`.
  Pada, gdy zbiory się przecinają, **i pada, gdy któryś jest pusty**.
  *Słaba wersja:* kryterium na `getComputedStyle`. **Niewykonalne** — repo biegnie vitest
  w node, bez jsdom, i taki test nie ruszyłby ani razu (`NOT_A_REAL_RED`).

- **AC-2 — aktywny wiersz strefy „teraz" jest coralowy, a nie akcentowy.**
  Asercje: (a) aktywny wiersz niesie `live-soft` i `live-edge`; (b) **nie niesie** żadnej klasy
  `accent-*`; (c) pozostałe wiersze nie niosą ani `live-*`, ani `accent-*`; (d) strefa „teraz"
  ma stałą wysokość przeczytaną z makiety — nie rośnie.

- **AC-3 — kolumna glifów nie niesie strzałki.**
  `→` wypada: nazwa agenta już powiedziała, kto to zrobił, a strzałka jest cytatem z terminala.
  Asercje: (a) wiersz czynności ma glif **pusty**; (b) `✓` niesie `--muted`, nie zieleń;
  (c) `✕` niesie `--fail`; (d) wiersz zakończony ma `opacity` z makiety.

- **AC-4 — pasek loadoutu jest jednym szklanym torkiem z segmentami.**
  Asercje: (a) segmenty są dziećmi jednego pojemnika z `--radius-pill`; (b) skończony niesie
  `--line-strong`, aktywny `--live`, czekający obrys `--line`; (c) **nie ma paska procentowego** —
  liczba segmentów równa się liczbie kroków, czytana z magazynu, nie z długości.

- **AC-5 — pulsuje dokładnie jedna rzecz.**
  Asercje: (a) kropka pracującego agenta ma animację; (b) kropka gotowości w stopce **nie ma**;
  (c) w całej makiecie liczba reguł `animation` na elementach widocznych w widoku domyślnym
  jest ≤ 2 (sufit z `ARCHITECTURE §7`), policzona z makiety.

---

### T-48 — Sekcje, listy i inspektory

**Powierzchnia:** Agents, Skills, Memory, Workflows.

**Pliki**
- Modify: `docs/mockup/index.html` — `.tile`, `.node`, `.side`, `.f`, `.toggle`, `.empty`, `.ask`
- Modify: `src/sections/agents/**`, `src/sections/skills/**`, `src/sections/memory/**`,
  `src/sections/workflows/**`
- Modify: `docs/design/DESIGN.md` §6 — `field`, `node-card`, `empty-state`, `chip`
- Test: `src/sections/field-labels-are-sentence-case.test.tsx`,
  `src/sections/inspector-is-two-columns.test.tsx`

**Kryteria**

- **AC-1 — etykieta pola nie jest w wersalikach, w żadnej z pięciu sekcji.**
  Asercje: (a) żaden element niosący `--t-label` nie ma `text-transform: uppercase`;
  (b) nadoczka sekcji **mają** wersaliki i tracking `.06em`; (c) oba stopnie istnieją w makiecie
  jako **osobne** tokeny — bo do 2026-08-19 jeden obsługiwał oba i dlatego wersaliki wchodziły
  wszędzie albo nigdzie.

- **AC-2 — inspektor jest dwukolumnowy i etykieta stoi przed polem.**
  Wartości czytane z makiety. Asercje: (a) `grid-template-columns` inspektora ma dwa członki;
  (b) etykieta jest pierwszym dzieckiem; (c) pola dziedziczą `--radius-sm`, nie własny literał.

- **AC-3 — pusty ekran zaprasza, w każdej z pięciu sekcji.**
  Asercje: (a) każda sekcja z pustym magazynem niesie **co najmniej jedną czynną** kontrolkę;
  (b) zdanie przechodzi tabelę żargonu z `00-SYNTHESIS.md` §2.2; (c) nie ma wiersza zastępczego
  typu `not reported` — wiersz bez wartości po prostu nie istnieje.

- **AC-4 — chip stanu bierze `--radius-pill` i wash swojego stanu.**
  Cztery stany, cztery pary token-obrys/token-tło, sprawdzone po jednym elemencie każdy, plus
  wariant neutralny w `--line`/`--muted`.

---

### T-49 — Znak, ikona, logotyp

**Pliki**
- Create: `docs/branding/loadout-icon.svg`, `loadout-icon-32.svg`, `loadout-icon-16.svg`,
  `loadout-logo.svg`, `loadout-mark.svg`
- Create: `src/ui/brand/mark.tsx`
- Create/Modify: `src-tauri/icons/{32x32.png,128x128.png,128x128@2x.png,icon.icns,icon.png}`
- Modify: `src/ui/shell/titlebar.tsx` — znak i logotyp w nawigacji
- Modify: `docs/design/DESIGN.md` §2 (podpis wizualny)
- Test: `src/ui/brand/mark-is-the-smallest-true-graph.test.tsx`,
  `src/ui/brand/icon-set-is-three-drawings.test.ts`

**Kryteria**

- **AC-1 — znak jest najmniejszym prawdziwym grafem, i geometria jest czytana z pliku.**
  Test czyta `docs/branding/loadout-mark.svg` i sprawdza: cztery węzły, **cztery** krawędzie,
  węzeł syntezy większy od pozostałych, stosunek średnicy węzła do grubości linii ≥ 3.
  *Słaba wersja:* asercja na obecność `<circle>`. Cztery luźne okręgi bez krawędzi nie są grafem
  — i dokładnie tym jest dzisiejszy znak.

- **AC-2 — ikona to trzy rysunki, nie jeden przeskalowany.**
  Asercje: (a) trzy pliki istnieją; (b) `loadout-icon-16.svg` **nie** niesie ani jednego
  gradientu ani `sheen` — jedna barwa; (c) `loadout-icon-32.svg` ma krawędź grubszą niż pełny
  rysunek; (d) wszystkie trzy mają ten sam squircle `rx=232` na płótnie 1024.

- **AC-3 — znak w kodzie nie niesie ani jednego literału.**
  Asercje: (a) `src/ui/brand/mark.tsx` przechodzi `checks/quick-tokens.sh` (zero heksów, zero
  arbitralnych klas Tailwinda); (b) węzły biorą `fill-body`, krawędzie `stroke-line-strong`;
  (c) w chrome znak **nie niesie** ani `accent-*`, ani `live-*` — neutralność powłoki.

- **AC-4 — logotyp jest krzywymi, nie tekstem.**
  Asercje: (a) `loadout-logo.svg` nie ma elementu `<text>` ani atrybutu `font-family`;
  (b) niesie `<path>` o niezerowej długości danych. Powód: deklaracja wskazująca na krój,
  którego nie ma, daje po cichu krój zapasowy — i właśnie tak przez cały czas działał Inter.

- **AC-5 — `tauri.conf.json` wskazuje na pliki, które istnieją.**
  Każda ścieżka z `bundle.icon` istnieje na dysku i ma niezerowy rozmiar.

---

### T-50 — Nowa D1, śmierć aliasów, sprzątnięcie

**Ostatnie zadanie fali.** Dopóki nie jest zielone, D1 opisuje stan, który naprawdę stoi w trunku.

**Pliki**
- Modify: `docs/DECISIONS-LOCKED.md` — cała sekcja D1 (tekst: spec §2)
- Modify: `src/styles/theme.css` — **usunięcie** `--radius-sq`, `--radius-dot`
- Modify: `docs/design/DESIGN.md` §9 (kontrola jakości)
- Delete: `docs/superpowers/specs/2026-08-19-quiet-glass-design.md`,
  `docs/superpowers/plans/2026-08-19-quiet-glass.md`
- Test: `src/styles/d1-and-design-agree.test.ts`

**Kryteria**

- **AC-1 — D1 i DESIGN.md nie mogą się rozjechać.**
  Test czyta oba pliki i porównuje: hex akcentu nazwany w D1 równa się `--accent` w DESIGN.md,
  hex „teraz" równa się `--live`, a nazwa systemu („Quiet Glass") występuje w obu. Pada też,
  gdy którykolwiek odczyt nic nie dopasował.
  *Słaba wersja:* asercja, że D1 zawiera słowo „Quiet". Przechodzi na D1, która nazywa akcent
  innym heksem niż system.

- **AC-2 — aliasy promieni zniknęły, i nikt ich nie woła.**
  Asercje: (a) `--radius-sq` i `--radius-dot` nie istnieją w `theme.css`; (b) nie występują
  w żadnym pliku pod `src/`; (c) `--radius-sm/md/lg/pill` występują. Punkt (b) jest tym, który
  odróżnia „skasowane" od „skasowane i zepsute".

- **AC-3 — spec i plan tej fali nie istnieją.**
  Asercje: oba pliki są nieobecne, a `grep` po `docs/` i `tasks/` nie znajduje do nich odwołania.
  Powód: dokument przejściowy, który przeżył falę, dalej wygląda na źródło prawdy i dalej jest
  cytowany w recenzjach — a jest już nieaktualny.

---

## Kolejność i zależności

```
T-45 (tokeny, kroje)  ──┬──► T-46 (powłoka) ──┬──► T-47 (Run)
                        │                     └──► T-48 (sekcje)
                        └──► T-49 (marka)
                                                    │
        T-50 (D1, śmierć aliasów) ◄──────────────────┘  po WSZYSTKICH
```

T-46 i T-49 mogą jechać równolegle po T-45 (rozłączne pliki poza `titlebar.tsx` — dlatego
`titlebar.tsx` należy do T-46, a T-49 tylko wstawia do niego `Mark`, co jest kolizją i **musi**
być sekwencyjne). T-47 i T-48 mogą jechać równolegle po T-46.

**Jedna gałąź naraz przy lądowaniu**, pełna bramka po każdej (`./integrate.sh`).

---

## Self-review planu wobec specu

| Sekcja specu | Zadanie, które ją realizuje |
|---|---|
| §2 nowa D1 | T-50 AC-1 |
| §3.1–3.3 powierzchnie, tekst, linie | T-45 AC-1 |
| §3.4 stany + rozłączność form | T-45 AC-1, T-47 AC-1, AC-2 |
| §3.5 tożsamość | T-45 AC-1 (jawnie wyłączona z porównania z domem) |
| §3.6 promienie, odstępy, cienie, ruch | T-45 AC-4, T-46 AC-3, T-47 AC-5 |
| §3.7 szkło i aurora | T-46 AC-3, AC-4 |
| §3.8 luka `rgba()` w lustrze | T-45 AC-3 (**wymaga zgody człowieka**) |
| §4 typografia i kroje | T-45 AC-2, T-48 AC-1 |
| §5.1–5.3 rama i budżet chrome | T-46 AC-1, AC-2 |
| §5.4 ikony nawigacji | T-46 AC-5 |
| §5.5 ruch | T-47 AC-5 |
| §6 komponenty | T-47 AC-2–4, T-48 AC-2–4 |
| §7 marka | T-49 AC-1–5 |
| §8 kolejność | **zmieniona** — uzasadnienie wyżej |
| §9 kontrola jakości | T-50, plus po jednym punkcie w każdym zadaniu |
| §10 poza zakresem | nie realizowane świadomie |

**Luka, którą self-review znalazł:** spec §3.7 wymaga
`@media (prefers-reduced-transparency: reduce)` zamieniającego szkło na `--solid`, a żadne
kryterium tego nie dotykało. Dopisane do **T-46 AC-3** jako punkt (d): reguła istnieje w makiecie
i w arkuszu, i podmienia **wszystkie trzy** powierzchnie szklane, nie jedną.
