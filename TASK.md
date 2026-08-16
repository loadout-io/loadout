# T-09 — Szyna agentów i widok sesji: „co dostał" i „co wyprodukował" przed transkryptem

Widok sesji agenta ma jedną oczywistą, płynną i średnią wersję: transkrypt na całą wysokość, bo
transkrypt jest tym, co mamy pod ręką. Ta wersja odpowiada na pytanie „co ten agent gadał", a
człowiek otwiera agenta, żeby dowiedzieć się dwóch innych rzeczy: **co ten agent dostał na wejściu**
i **co po nim zostało**. Cicha porażka numer jeden: blok „co wyprodukował" karmiony ostatnią
wiadomością agenta — agent pisze „naprawiłem wszystko", nie zmieniwszy ani jednego pliku, a UI
podaje jego deklarację w miejscu, w którym człowiek czyta fakty (`agent said` ≠ `happened`,
00-SYNTHESIS §2.2). Cicha porażka numer dwa: kolor tożsamości agenta wzięty z tej samej palety co
kolor stanu — w referencyjnym redesign poprzedniego prototypu agent Forge ma `#ffb45b`, czyli dokładnie ten hex,
który na sąsiednim kafelku znaczy „czeka na twoją decyzję" (DESIGN.md §3, „Tożsamość ≠ stan"). Nikt
tego nie zgłosi jako błędu; ludzie po prostu przestaną ufać kolorom. Cicha porażka numer trzy:
szyna zbudowana z listy agentów w workflow, a nie ze strumienia — pokazuje kafelki agentów, którzy
nigdy nie wystartują, i nie pokazuje pod-agentów, którzy wystartowali naprawdę.

**Read first:**
`docs/research/topics/T2-terminal-ux.md` (§9.1 — widok sesji to **ten sam** `<Feed>` z filtrem, i to
jest największa oszczędność zakresu w całym ekranie; §9.2 — szyna bierze się z `parent_tool_use_id`,
czyli ze strumienia, nie z planu; §7.2 — rodzaje linii, których sesja nie ma prawa renderować
inaczej niż strumień główny),
`docs/design/DESIGN.md` (§3 „Tożsamość ≠ stan" — dwie rozłączne palety i powód, dla którego ta
reguła w ogóle powstała; §6 `agent-card` — maksymalnie cztery linie tekstu, piąta to błąd
projektowy),
`docs/mockup/index.html` (linie 403–430 — szyna z czterema kafelkami i notatką „TOŻSAMOŚĆ ≠ STAN";
linie 438–477 — sesja: `What Forge was given` / `What Forge produced` / `What Forge said`, w tej
kolejności),
`docs/research/projects/00-SYNTHESIS.md` (§2.2 — wiążąca tabela nazw: `agent said` vs `happened`,
`latest note from this agent`, `what this agent was told`, trzy autorytety),
`docs/ARCHITECTURE.md` (§7 — sufit gęstości; kafelek agenta ma limit 4 linii wpisany do tabeli),
`AGENTS.md` (§3 niezmienniki 13, 14, 16, 17, 18).

## Kto to robi

- **Agent:** `react-ui`
- **Druga opinia:** `claude` (pisze `codex` — decyzja D3 wymaga, żeby działały wszystkie cztery pary)
- **Artefakty biegu:** `runs/T-09/` (transkrypt, plik wyników, plan) — nigdy `$TMPDIR`

## Co to zadanie posiada

- `src/sections/run/rail/**` — model kafelka (`card.ts`), przydział kolorów tożsamości i stanu
  (`colour.ts`), budowa listy kafelków ze strumienia (`roster.ts`), komponenty `Rail`, `AgentCard`.
  Ograniczenie na cały katalog: **kafelek nie liczy, kafelek pokazuje**. Cztery sloty tekstu i
  koniec.
- `src/sections/run/session/**` — model sekcji sesji (`layout.ts`), filtr strumienia po agencie
  (`filter.ts`), komponenty `Session`, `GivenBlock`, `ProducedBlock`.

Testy leżą wewnątrz tych dwóch katalogów. `vitest` biegnie w środowisku `node`: nie ma `jsdom` ani
`@testing-library/react`, a `package.json` jest na liście `DENIED` w `checks/quick-scope.sh`, więc
logika testowana kryteriami mieszka w czystych modułach `.ts`, a `.tsx` tylko je renderuje.
Zaślepka modułu (eksport zwracający pustą wartość) powstaje **przed** testem — nierozwiązywalny
import jest na liście `NOT_A_REAL_RED` w `harness/gate.py` i nie liczy się jako czerwone.

`src/styles/theme.css` **definiuje już** `--color-id-1…--color-id-5`, a `checks/quick-tokens.sh`
jest zielony przed twoją gałęzią. Wniosek jest praktyczny: **każde czerwone `quick-tokens`
w tym biegu jest winą twojej zmiany** i naprawiasz je u siebie, nie zgłaszasz. Twoje moduły
operują na **nazwach** tokenów (`'--color-id-3'`), komponenty piszą `var(--color-id-3)` —
żadnego hexa w kodzie komponentu (DESIGN.md §9).

## Niezmienniki

- **13 — jeden fakt, jedno miejsce.** Stan agenta jest w kafelku. Nagłówek sesji go **nie**
  powtarza, tylko dokłada to, czego kafelek nie mieści. Cicho łamie się tak: nagłówek sesji dostaje
  własny chip „working", strefa TERAZ ma trzeci, a szyna czwarty — cztery żywe regiony na jeden
  fakt, przy limicie 1.
- **14 — zero żargonu.** `session`, `spawn`, `tool_use`, `parent_tool_use_id` nie mają prawa
  pojawić się na ekranie. Cicho łamie się tak: pusty stan bloku „co dostał" drukuje nazwę pola z
  danych, bo „i tak nikt tego nie zobaczy".
- **16 — kontrolka bez handlera nie wchodzi do repo.** `Open its files` i `Stop this agent` z
  makiety wchodzą tylko wtedy, gdy mają co wywołać. Jeśli nie mają — nie ma ich, a nie są wyszarzone.
- **17 — UI nie rysuje relacji, których nie ma w danych.** Kafelek istnieje wtedy i tylko wtedy, gdy
  agent pojawił się w strumieniu. Cicho łamie się tak: szyna renderowana z definicji workflow, żeby
  „było widać, co się będzie działo".
- **18 — sufit gęstości jest mierzony, nie oceniany okiem.** Kafelek: **4 linie tekstu** [ARCHITECTURE
  §7]. Ekran sesji: ≤ 8 oznaczonych regionów i ≤ 60 elementów niosących tekst. Baseline może tylko
  maleć.

## Kryteria akceptacji

## AC-1 Kafelek ma cztery sloty tekstu i ani jednego więcej
check: npx --no-install vitest run src/sections/run/rail/card.test.ts

`railCard(agent)` zwraca obiekt, którego klucze porównane jako posortowana tablica to dokładnie
`id, name, role, say, square, status`. Cztery z nich niosą tekst (`name`, `role`, `say.text`,
`status`), `square` to nazwa tokenu, `id` nie jest renderowane, a `say` niesie obok tekstu
autorytet `who` (pinuje go AC-6). `say.text` jest **gwarantowanie jednolinijkowy**: notatka z `\n`
na pozycji 10 daje jedną linię, ciągi białych znaków są zwinięte do pojedynczej spacji, wynik jest
przycięty z obu stron. Skracaniem zajmuje się CSS (`text-overflow: ellipsis`, makieta linia 185),
nie kod — nie obcinaj `say.text` do stałej liczby znaków.

*Słaba asercja:* `expect(card.say.text).not.toContain('\n')` przechodzi dla implementacji, która
dokłada piąty slot — „12 files · 2m 04s" jako wiersz metadanych pod stanem. Kafelek ma wtedy pięć
linii, wygląda dobrze na jednym agencie i rozjeżdża szynę przy czterech. Rozróżnia to porównanie
**pełnego** posortowanego zbioru kluczy z literałem powyżej, sprawdzone dla sześciu stanów agenta
(w tym `failed` z długą notatką i `needs you` z zadanym pytaniem) — nowe pole nie ma jak się
prześlizgnąć.

## AC-2 Kolor tożsamości i kolor stanu nigdy nie pochodzą z tego samego zbioru
check: npx --no-install vitest run src/sections/run/rail/colour.test.ts

`identityToken(agent)` zwraca nazwę z pięcioelementowego zbioru `--color-id-1 … --color-id-5`;
`statusToken(status)` zwraca nazwę z czteroelementowego zbioru `--color-accent`, `--color-attend`,
`--color-fail`, `--color-muted` i jest totalny na `working | waiting | needs you | failed | done |
stopped` (DESIGN.md §3: rzecz skończona jest cicha, więc `done` → `--color-muted`). Dla 40 różnych
agentów obraz `identityToken` zawiera się w zbiorze tożsamości, a przecięcie obu zbiorów jest puste.
Przydział jest **stabilny**: ten sam agent dostaje ten sam token, gdy poda się listę w innej
kolejności i gdy dojdzie do niej nowy agent.

*Słaba asercja:* `expect(identityToken('forge')).not.toBe(statusToken('running'))` na kilku parach
przechodzi dla implementacji z błędem zawijania, która przy szóstym agencie sięga po
`--color-attend` — czyli robi dokładnie ten błąd, przez który ta reguła w ogóle powstała.
Rozróżniają to: przeliczenie wszystkich 40 agentów i asercja na **zbiorach** (zawieranie i puste
przecięcie), asercja `statusToken` na wszystkich sześciu stanach (zbiór wartości ma dokładnie cztery
elementy) oraz asercja, że `card.square` nigdy nie jest tokenem stanu, nawet dla agenta `failed` —
stan jest **słowem**, nie kolorem kwadratu.

## AC-3 Sesja prowadzi dwoma blokami, transkrypt jest trzeci i pokazuje tylko to, co istnieje
check: npx --no-install vitest run src/sections/run/session/layout.test.ts

`sessionSections(agent, run)` zwraca tablicę, której `map(s => s.id)` to dokładnie
`['given', 'produced', 'transcript']`, z nagłówkami `What <Name> was given`, `What <Name> produced`,
`What Forge said` (makieta, linie 449–467). Wiersze `given` mają rodzaje wyłącznie ze zbioru
`step, handoff, note, files`; wiersze `produced` — wyłącznie `changes, handoff`. Agent bez
przychodzącego przekazania i bez notatek dostaje `given` z samymi wierszami, które ma; **żaden
wiersz nie ma pustej ani zastępczej wartości** (poprzedni prototyp renderował `SPEND: not reported`).

*Słaba asercja:* `expect(sections[0].id).toBe('given')` przechodzi dla bloku z pięcioma wierszami
wpisanymi na stałe za makietą, z których trzy są puste. Rozróżniają to dwie asercje:
`given.rows.every(r => r.value !== '' && r.value !== '—')` dla agenta o minimalnym wejściu, oraz —
ważniejsza — agent, którego ostatnia wiadomość brzmi „I fixed everything", a który nie wyprodukował
żadnej zmiany pliku, ma `produced.rows.length === 0` i pusty stan; jego deklaracja pojawia się
wyłącznie w transkrypcie, jako linia `note` z autorytetem `agent`. Deklaracja w rubryce faktów jest
dokładnie tym błędem, którego to rozdzielenie ma nie dopuścić.

## AC-4 Sesja to ten sam strumień z filtrem, a nie druga jego derywacja
check: npx --no-install vitest run src/sections/run/session/filter.test.ts

Scena: skrypt, w którym Forge i Needle na przemian robią po trzy `read` w oknie 2 s, a między nimi
pada jedna `note` Forge'a. `sessionFeed(state, 'forge').map(l => l.id)` jest **podciągiem**
`feedView(state).history.map(l => l.id)` — te same identyfikatory, ta sama kolejność, te same
granice grup sklejania i te same flagi rozwinięcia. Linia innego agenta nie pojawia się nigdy.
Linia pod-agenta (z rodzicem) trafia do sesji dziecka i **jednym** wierszem echa do strumienia
głównego [T2 §9.3], ale do sesji dziecka nie trafia dwa razy.

*Słaba asercja:* `expect(rows.every(r => r.agent === 'forge')).toBe(true)` przechodzi dla
implementacji, która przelicza strumień od nowa dla jednego agenta — i wtedy sesja pokazuje
`Read 3 files`, a strumień główny `Read 6 files`, bo w globalnej kolejności odczyty obu agentów
sąsiadują. Rozróżnia to porównanie **par (identyfikator grupy, licznik)** między obiema derywacjami:
muszą być identyczne. Jeśli okaże się, że nie da się ich uzgodnić, bo model T-08 skleja ponad
agentami, to jest defekt T-08 (jego AC-4) i zgłaszasz go, zamiast obchodzić własnym przeliczeniem.

## AC-5 Szyna bierze się ze strumienia, nie z planu
check: npx --no-install vitest run src/sections/run/rail/roster.test.ts

Bieg czterokrokowy, w którym krok 4 zostaje `skipped`, a agent kroku 2 rozpuszcza pod-agenta,
którego nie ma w żadnym workflow. Wynik: `roster(state)` **nie** ma kafelka agenta kroku 4, **ma**
kafelek pod-agenta, a kolejność jest kolejnością pierwszego pojawienia się w strumieniu. Agent,
którego krok został `cancelled` po tym, jak coś nadał, zachowuje kafelek ze stanem `stopped`.

*Słaba asercja:* `expect(roster(state).length).toBe(3)` przechodzi dla implementacji, która bierze
listę agentów z definicji workflow i odfiltrowuje te w stanie `pending` — liczba się zgadza, a
kafelka pod-agenta nie ma i nigdy nie będzie. Rozróżniają to dwie asercje w przeciwnych kierunkach:
kafelek agenta pominiętego kroku **nie istnieje** oraz kafelek pod-agenta spoza planu **istnieje**.
Obie naraz przechodzą tylko wtedy, gdy źródłem jest strumień.

## AC-6 „Latest note from this agent" jest cytatem agenta i jest tak oznaczone
check: npx --no-install vitest run src/sections/run/rail/say.test.ts

`card.say.text` pochodzi z ostatniej linii `note` tego agenta i wtedy `say.who === 'agent'`. Gdy
agent nie powiedział jeszcze nic prozą, `say.text` to zdanie Loadouta o bieżącej czynności (np.
`writing src/parser.rs`) z `who: 'loadout'` — nigdy pusty string i nigdy zdanie zmyślone. Po linii
`ran` z `ok: false` `say` jest zdaniem Loadouta (`3 of 40 tests failed`, `who: 'loadout'`), bo
sprawdzenia to Loadout, nie agent [00-SYNTHESIS §2.2]. Zbiór wartości `who` ma dokładnie trzy
elementy: `agent`, `loadout`, `you`.

*Słaba asercja:* `expect(card.say.text).toBe(lastNote.text)` przechodzi dla implementacji, która za
„ostatnią wypowiedź agenta" bierze także `problem` i podsumowanie sprawdzeń — i wtedy zdanie
Loadouta jest podane jako cytat agenta, co jest tym samym błędem co w AC-3, tylko mniejszą czcionką.
Rozróżniają to: przypadek `ran ok:false` po `note` (`say.who === 'loadout'`, a `lastNote` dalej
należy do agenta) i asercja na zamkniętym, trzyelementowym zbiorze `who`.

## AC-7 Ekran sesji mieści się w suficie gęstości
check: npx --no-install vitest run src/sections/run/session/density.test.ts

Ustalona scena: agent z jednym przekazaniem na wejściu, jedną notatką „w użyciu", dwiema zmienionymi
ścieżkami, jednym przekazaniem na wyjściu i dwunastoma liniami transkryptu w stanie domyślnym.
`countTextNodes(sessionSections(...))` liczy rekurencyjnie każdy niosący tekst element modelu
(nagłówek sekcji, etykietę wiersza, wartość, chip, jedną zwiniętą linię = 1) i musi być
`<= DENSITY_BASELINE`, gdzie `DENSITY_BASELINE` to stała w pliku testu, nie większa niż **60**
[ARCHITECTURE §7]. Oznaczonych regionów: `<= 8`.

*Słaba asercja:* licznik przechodzący tylko po kluczach najwyższego poziomu zwraca 3 i zawsze będzie
zielony. Rozróżnia to test kontrolny w tym samym pliku: sztucznie rozwiń wszystkie dwanaście linii
transkryptu i wymagaj, żeby licznik **przekroczył** baseline. Licznik, który nie umie się przewrócić,
nie jest pomiarem — jest ozdobą (niezmiennik 19: zielone bez dowodu wykonania jest czerwone).

## Świadomie poza zakresem

- **Model strumienia, reguły zwijania i sklejania, limit 2000 linii** — T-08. Tutaj wyłącznie
  filtrujemy i układamy.
- **Rozmowa z jednym agentem (`AgentComposer`)** — wysyłanie wiadomości w trakcie biegu jest
  odłożone [T2 §8.3, §10], a kontrolka bez handlera nie wchodzi do repo (niezmiennik 16). Nie
  renderuj pola tekstowego „na przyszłość".
- **`Open its files` i panel zmian** — panel szczegółów jest osobną powierzchnią; blok „co
  wyprodukował" wystawia `detailId`, otwieranie nie jest tutaj.
- **Zagnieżdżenie pod-agentów głębsze niż jeden poziom** — T2 §12 pytanie 5 zostawia to otwarte.
  Model przechowuje rodzica, szyna pokazuje wszystkie, które nadały; jeśli okaże się to hałaśliwe,
  to jest zmiana na podstawie skargi, nie hipotezy.
- **Kolory tożsamości w `theme.css`** — plik nie należy do tego zadania (patrz wyżej). Twoje moduły
  zwracają nazwy tokenów; brak definicji jest zgłoszeniem, nie poprawką.
- **Perzystencja przydziału kolorów między biegami** — przydział jest stabilny w obrębie biegu
  (AC-2). Zapamiętywanie „Forge zawsze ma id-3" wymaga miejsca w definicji agenta i należy do T-11.

<!-- OWNS
src/sections/run/rail
src/sections/run/session
-->
