# T-47 — Ekran Run w Quiet Glass: „teraz" jest coralowe

Trzecie zadanie fali. T-45 dal slownik, T-46 powloke; to zadanie zamyka **slownik stanow**
na ekranie, ktory jest calym produktem. Po nim `--accent` nigdzie na tym ekranie nie znaczy
„teraz", a to, co sie dzieje, jest coralowe.

Kierunek: `docs/superpowers/specs/2026-08-19-quiet-glass-design.md` §3.4 i §6.

## Co jest dzis sprzeczne — wymienione, nie oszacowane

T-45 rozszczepil token: `--accent` znaczy „to jest interaktywne", `--live` znaczy „to sie dzieje
teraz". Piec miejsc na tym ekranie **nie poszlo za ta zmiana** i mowi „teraz" akcentem:

| gdzie | co mowi dzis | co ma mowic |
|---|---|---|
| `strip.tsx:90` aktywny segment paska | `bg-accent` | `bg-live` |
| `strip.tsx:103` etykieta aktywnego kroku | `text-accent` | `text-live` |
| `tabs/tab.tsx:99` kropka zywego biegu w karcie w tle | `bg-accent` | `bg-live` |
| makieta `.card[data-live]` lewy pasek kafelka agenta | `var(--accent)` | `var(--live)` |
| makieta `.blk[data-s="now"]` wypelnienie segmentu | `var(--accent)` | `var(--live)` |

Szoste miejsce mowi cos innego niz mysli: `feed/line.tsx:77` daje przedrostkowi `You →`
kolor `--accent`, a ten wiersz odpowiada na pytanie „co zrobil czlowiek, nie maszyna" —
czyli nalezy mu sie `--human`. Akcent na wypowiedzi czlowieka mowi „to jest klikalne".

Siodme: **strefa „teraz" nie oznacza zywego wiersza w ogole.** `feed/now.tsx` renderuje trzy
wiersze jednakowo, wiec ten, ktory naprawde pracuje, nie rozni sie od tych, ktore czekaja —
a to jest jedyne pytanie, na ktore ta strefa istnieje, zeby odpowiadac.

## Ryzyko, ktore trzeba zmierzyc, a nie ocenic okiem

`--live #ff7a5c` i `--fail #ff6b6b` roznia sie odcieniem o **~13 stopni**, a w strumieniu stoja
w sasiednich wierszach — czego system, z ktorego wzielismy wartosci, nigdy nie musi pokazac.
Rozstrzyga to **forma, nie barwa**, i DESIGN §3 zapisuje to jako regule:

- `--live` wylacznie jako: podklad aktywnego wiersza strefy „teraz", jego obrys, aktywny segment
  paska, pulsujaca kropka, kropka karty w tle;
- `--fail` wylacznie jako: glif `✕`, obrys chipa, lewa krawedz bloku bledu.

**Read first:**
`docs/design/DESIGN.md` §3 (regula formy) i §6 · `src/sections/run/feed/line.tsx`
(`marker()` — glify sa juz poprawne, strzalki NIE MA w aplikacji) · `src/sections/run/feed/now.tsx`
· `src/sections/run/strip/strip.tsx` (`STRIP_HEIGHT`, mapy klas) · `src/sections/run/tabs/tab.tsx`
· `src/ui/shell/only-the-nav-floats.test.ts` (wzorzec enumeratora regul z kontrola przeciw
parzystosci — ten sam problem wraca tutaj).

## Kto to robi

- **Agent:** `react-ui`
- **Druga opinia:** inny vendor niz pisarz (D3).
- **Artefakty biegu:** `runs/T-47/`

## Zalezy od

**T-45** (tokeny `--live-*`) i **T-46** (klasa materialu `.glass`, `STRIP_HEIGHT`).

## Co to zadanie posiada

- `src/sections/run/feed/now.tsx` — jeden zywy region, bramkowany faktem „bieg zyje".
- `src/sections/run/index.tsx` — **jedna linia**: przekazuje strefie flage `running`. Fakt „cos
  sie teraz dzieje" mieszka w wywolujacym, nie w `NowZone`, bo `doing` w modelu jest tylko
  DOPISYWANE i nigdy nie czyszczone — kropka bramkowana sama liczba wierszy pulsowalaby po
  zakonczeniu biegu, czyli mowilaby „dzieje sie" o czyms, co stoi.
- `src/sections/run/feed/line.tsx` — `You →` przestaje byc akcentem.
- `src/sections/run/strip/strip.tsx` — segmenty w jednym szklanym torku, aktywny `--live`.
- `src/sections/run/tabs/tab.tsx` — kropka zywego biegu `--live`.
- `src/sections/run/tabs/live-dot.test.tsx` — **wyrocznia, ktora uzasadnia sie zdaniem
  „accent znaczy teraz"**. T-45 to zdanie uniewaznil, wiec ten test sadzi dzis regule, ktorej
  DESIGN.md juz nie stawia.
- `src/sections/run/strip/strip.test.ts` — to samo, w komunikacie o „accent colour".
- `docs/mockup/index.html` — **wylacznie reguly ekranu Run**: `.now`, `.now .row`, `.think`,
  `.blocks`, `.blk`, `.tab .d`, `.card[data-live]`, `.ln .g` oraz usuniecie strzalki z markupu
  strumienia. Reguly powloki naleza do T-46, sekcji do T-48, `.mark` do T-49.
- `docs/design/DESIGN.md` — **wylacznie §6**, wiersze `stream-line`, `history-line`, `agent-card`,
  `loadout-strip`.
- Piec plikow testow wymienionych przy `check:`.

**Czego to zadanie NIE dotyka:** powloki (T-46 wyladowal), formularzy (T-48), znaku (T-49),
`docs/DECISIONS-LOCKED.md` (T-50), `checks/**`, `src-tauri/**`.

## Niezmienniki

- **13 — jeden fakt, jedno miejsce.** Ktory wiersz pracuje, mowi jedna rzecz.
- **17 — UI nie rysuje relacji, ktorych nie ma w danych.** Liczba segmentow paska rowna sie
  liczbie krokow z magazynu, nie dlugosci napisu.
- **ARCHITECTURE §7 — dwa regiony animujace sie od jednego zdarzenia.** Trzeci to regres.
- **DESIGN §3 — `--live` i `--fail` nie dziela formy.**

## Kryteria akceptacji

## AC-1 `--live` i `--fail` nie dziela ani jednej formy
check: npx --no-install vitest run src/sections/run/live-and-fail-never-share-a-form.test.ts
expect: (\d+) passed

Sprawdzenie jest **statyczne, na zrodle**, i to jest wymuszone, nie wybrane: repo nie ma `jsdom`
ani `environment` w `vite.config.ts`, wiec vitest biegnie w node i `getComputedStyle` nie istnieje.
Kryterium oparte na obliczonym stylu nie ruszylo by ani razu (`NOT_A_REAL_RED`).

Asercje: (a) zbior nazw klas stojacych na elementach niosacych `live-*` i zbior tych z `fail-*`
sa **rozlaczne**; (b) oba zbiory sa **niepuste** — pusty zbior daje rozlacznosc za darmo i jest
ta sama awaria „porownanie przeszlo na niczym"; (c) skaner widzi klasy podane takze
przez **mapy i zmienne**, nie tylko literalne `className="..."` — bo tak wlasnie ten kod podaje
barwy stanu (`BLOCK`, `LABEL`, `tone`), a skaner slepy na to sadzilby kilka recznie wybranych
napisow; (d) zdejmuje komentarze przed czytaniem.

**Rozlacznosc dotyczy CALEJ formy, nie pojedynczych nazw klas.** Dwa elementy dziela forme, gdy
ich zestawy klas po odjeciu barwy sa IDENTYCZNE — bo wtedy jedyna roznica jest odcien, a on
wynosi 13 stopni. Wspolne slowo narzedziowe (`text-center`, `font-mono`) formy nie tworzy:
etykieta segmentu i glif bledu maja rozne stopnie drabinki i rozne zawijanie.

*Slaba wersja:* kryterium na `getComputedStyle`. Niewykonalne w tym repo — i to jest w naglowku
testu nazwane, a nie przemilczane.

## AC-2 Strefa „teraz" ma DOKLADNIE JEDEN zywy region i nie rosnie
check: npx --no-install vitest run src/sections/run/now-row-is-live-not-accent.test.tsx
expect: (\d+) passed

**Pierwotna wersja tego kryterium zadala „dokladnie jeden WIERSZ niosacy `--live`" i byla
zadaniem o fakcie, ktorego dane nie maja.** `NowRow` to `{ agent, text }`; ktory agent naprawde
pracuje, a ktory czeka, nie jest polem — jest tresc zdania („writing src/parser.rs" wobec
„waiting on Forge"). Wyprowadzenie tego w widoku przez szukanie slowa `waiting` w napisie
oznaczaloby dwie rzeczy naraz: wymyslenie faktu, ktorego nie ma w magazynie (niezmiennik 17),
i przeniesienie polityki „kto co robi" do komponentu, gdzie istnialaby drugi raz
(niezmiennik 23). Zglaszam to zamiast tak zrobic.

Regula, ktora dane UTRZYMUJA i ktora jest mocniejsza: **strefa jest JEDNYM zywym regionem na
jeden fakt** (niezmiennik 13, limit zywych regionow na fakt wynosi 1). Fakt brzmi „cos sie
teraz dzieje", a nie „ten konkretny agent pisze".

Asercje: (a) przy niepustej strefie `--live` wystepuje w niej **dokladnie raz**; (b) strefa
**nie niesie** ani jednej klasy `accent-*`; (c) przy zerze wierszy strefa nie renderuje ani
jednego wiersza **ani** `--live` — zero atrap i zero coralu, gdy nic nie chodzi (niezmiennik 17);
(d) zdania wierszy pochodza z przekazanego magazynu, nie z napisow w tescie.

**Punkt o stalej wysokosci zostal USUNIETY, a nie oslabiony.** Brzmial „strefa ma `shrink-0`
albo zadeklarowana stala wysokosc" i certyfikowal teze DESIGN §1, mierzac wlasciwosc, ktora jej
nie implikuje: `shrink-0` jest wrecz odwrotnoscia ograniczenia wysokosci. Mechanizmem, ktory tu
naprawde dziala, jest siatka kolumny strumienia (`minmax(0,1fr) auto auto`) — historia przewija
sie i pochlania miejsce. Tego pilnuje `run-matches-mockup.test.tsx` i jest to jego pytanie.

*Slaba wersja:* `expect(markup).toContain('live')`. Przechodzi, gdy coralowe jest wszystko —
czyli gdy strefa przestaje odrozniac „dzieje sie" od „stoi".
## AC-3 Kolumna glifow: cichy `✓`, czerwony `✕`, i ani jednej strzalki
check: npx --no-install vitest run src/sections/run/glyph-column-carries-no-arrow.test.tsx
expect: (\d+) passed

Asercje: (a) glif wiersza zakonczonego to `✓` w `--muted`, nie w zieleni — **rzecz skonczona jest
cicha**; (b) glif wiersza zepsutego to `✕` w `--fail`; (c) **zaden** glif nie jest strzalka, sprawdzone na wszystkich
rodzajach wiersza — strzalka to cytat z terminala, a nazwa agenta w tym samym wierszu juz
powiedziala, kto to zrobil. *Pierwotnie ten punkt brzmial „wiersz czynnosci ma glif pusty"
i opisywal rozroznienie, ktorego model nie ma: `marker()` zwraca `·` dla wszystkiego, co nie jest
skonczone ani zepsute, bo `HistoryRow` nie odroznia czynnosci od noty. Kryterium o polu, ktorego
nie ma, jest niespelnialne inaczej niz przez dopisanie pola — a to inna praca.* (d) **makieta tez
nie niesie strzalki**, czytane z niej, bo inaczej wyrocznia zadalaby czegos, czego aplikacja
slusznie nie robi; (e) przedrostek wypowiedzi czlowieka niesie `--human`, nie `--accent`.

*Slaba wersja:* sprawdzenie samej aplikacji. Makieta jest wyrocznia wygladu, wiec dopoki ona
niesie strzalke, roznica jest **jej** zdaniem, nie naszym bledem.

## AC-4 Pasek loadoutu to jeden szklany torek, a segmentow jest tyle, ile krokow
check: npx --no-install vitest run src/sections/run/strip-is-one-glass-track.test.tsx
expect: (\d+) passed

Asercje: (a) segmenty sa dziecmi **jednego** pojemnika, ktory niesie material szkla i promien
kapsuly; (b) **wszystkie trzy** stany segmentu zgadzaja sie z makieta, a wartosci sa z niej CZYTANE:
aktywny niesie `live`, skonczony wypelnienie `muted`, czekajacy obrys `line-strong`.
*Pierwotnie ten punkt mylil sie w obie strony i sadzil wylacznie segment aktywny — kryterium
nazywajace trzy tokeny i sadzace jeden sadzi trzecia czesc siebie.*
(c) aktywny segment **nie niesie** `accent-*`; (d) liczba segmentow rowna sie liczbie krokow
przekazanych w danych, sprawdzone na dwoch roznych dlugosciach — **nie ma paska procentowego,
bo kroki to nie procenty**; (e) przy zerze krokow pasek nie renderuje ani jednego segmentu.

*Slaba wersja:* policzenie segmentow przy jednej dlugosci. Przechodzi na komponencie, ktory
rysuje zawsze cztery.

## AC-5 Pulsuje dokladnie tyle, ile wolno, i to co ma
check: npx --no-install vitest run src/sections/run/exactly-one-thing-pulses.test.ts
expect: (\d+) passed

`ARCHITECTURE §7` daje **dwa** regiony animujace sie od jednego zdarzenia. Sufit jest czytany
z tego pliku, nie wpisany.

Asercje: (a) sufit jest liczba dodatnia, przeczytana z §7 po tresci wiersza; (b) liczba miejsc
w kodzie produkcyjnym `src/`, ktore niosa animacje, jest **mniejsza lub rowna** sufitowi;
(c) kazde z nich odpowiada na pytanie „co sie dzieje teraz" — niesie `live-*`; (d) kropka
gotowosci dostawcy **nie** pulsuje, bo dostepnosc nie jest ani interakcja, ani „teraz";
(e) definicja animacji jest **jedna** i mieszka w arkuszu, nie w komponencie (niezmiennik 13).

*Slaba wersja:* policzenie wystapien `animate-blip`. Nie mowi nic o tym, czy pulsuje wlasciwa
rzecz, ani czy definicja nie zdublowala sie w komponencie.

<!-- OWNS
src/sections/run/feed/now.tsx
src/sections/run/index.tsx
src/sections/run/feed/line.tsx
src/sections/run/strip/strip.tsx
src/sections/run/strip/strip.test.ts
src/sections/run/tabs/tab.tsx
src/sections/run/tabs/live-dot.test.tsx
docs/mockup/index.html
docs/design/DESIGN.md
src/sections/run/live-and-fail-never-share-a-form.test.ts
src/sections/run/now-row-is-live-not-accent.test.tsx
src/sections/run/glyph-column-carries-no-arrow.test.tsx
src/sections/run/strip-is-one-glass-track.test.tsx
src/sections/run/exactly-one-thing-pulses.test.ts
-->
