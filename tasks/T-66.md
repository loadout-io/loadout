# T-66 — Wiersz zlozony w oknie nie jest agentem i nie bije kafelka w szynie

Znalezisko drugiej opinii przy T-58, potwierdzone lektura kodu. `rail/roster.ts` bije **jeden
kafelek na kazde odrebne `row.agent`** w historii (`for (const row of state.view.history)`),
a stan bierze z planu: `statusOf(null, false) === 'working'`, czyli agent, ktorego w planie nie
ma, jest na zawsze „working". Po T-58 kazda komenda wpisana w wiersz wejscia — `/run`, `/open`,
`/stop` i kazda odpowiedz, ktora wiersz daje sam sobie — sklada wiersz podpisany oknem. Skutek
widzi kazdy po pierwszym `/stop`: w szynie „Agents" siada kafelek, ktorego nikt nie uruchomil,
i stoi tam „working" do konca sesji.

**To nie jest kosmetyka, tylko niezmiennik 17 zlamany dokladnie tam, gdzie kod sie na niego
powoluje.** Komentarz przy `railCard` tlumaczy, ze wymyslona rola bylaby relacja, ktorej
w danych nie ma — a widmowy agent jest cala taka relacja. Wada istniala przed T-58 (rozmowa
z liderem podpisuje sie `Lead`), tylko z czestotliwoscia „raz na rozmowe"; T-58 podniosl ja do
„raz na komende" i dlatego zamyka ja osobne zadanie, a nie przypis.

**Nosnik juz istnieje i nie trzeba go budowac.** T-58 AC-2(c) wymusil, zeby wiersz skladany
w oknie mial **ujemny identyfikator** — bo pompa biegu i pompa rozmowy stempluja od 1 kazda
z osobna i dodatni licznik w oknie zderzylby sie z ich numerami. „Sklad okna" jest wiec faktem
zapisanym w danych, a nie domyslem z nazwy: naprawa pyta o pochodzenie wiersza, nigdy o to, jak
sie nazywa jego autor. Lista zakazanych nazw byla by druga tabela prawdy i rozjechalaby sie
przy pierwszym agencie nazwanym „Loadout".

**Cicha porazka, przed ktora stoi ten kontrakt:** naprawa przez wyciecie za duzo. Pod-agent
rozpuszczony w trakcie biegu **nie ma kroku w planie** i tez dostaje `status: 'working'`
z tej samej galezi — wiec „nie pokazuj kafelkow bez kroku" wyglada jak naprawa i kasuje
jedyny slad po pod-agentach. Kryterium ma kontrole dokladnie na to.

**Read first:**
`src/sections/run/rail/roster.ts` (petla po `view.history`, `statusOf`, `railCard`),
`src/sections/run/entry/echo.ts` (sklad wiersza okna i ujemny identyfikator — po T-58),
`src/sections/run/feed/model.ts` (`HistoryRow`: `id`, `agent`, `kind`, `label`),
`src/sections/run/rail/say.ts` (`authorityOf` — trzy autorytety, bez galezi domyslnej),
`AGENTS.md` niezmienniki 13, 16, 17.

## Niezmienniki, ktorych to dotyczy

- **17 — UI nie rysuje relacji, ktorych nie ma w danych.** Kafelek agenta, ktorego nikt nie
  uruchomil, jest wlasnie taka relacja.
- **13 — jeden fakt, jedno miejsce.** „Czy ten wiersz zlozylo okno" ma jedna odpowiedz i niesie
  ja identyfikator wiersza. Druga odpowiedz w postaci listy nazw autorow to pierwsza rzecz,
  ktora sie rozjedzie.

## Szkielet, bez ktorego `before` nie jest czerwone

Nie ma nowego modulu, wiec nie ma czego szkieletowac: plik testu importuje `roster` i `railCard`,
ktore istnieja. Kryterium ma paść na ASERCJI o liczbie kafelkow, nie na imporcie.

## Kryteria akceptacji

## AC-1 Historia zlozona przez okno nie produkuje ani jednego kafelka
check: npx --no-install vitest run src/sections/run/rail/window-rows-are-not-agents.test.ts
expect: (\d+) passed

Asercje: (a) historia zlozona wylacznie z wierszy okna (ujemne identyfikatory, dowolny podpis)
daje **zero** kafelkow; (b) ta sama historia plus jedna linia prawdziwego agenta daje
**dokladnie jeden** kafelek i jest nim ten agent; (c) **kontrola przeciw naprawie przez
wyciecie:** pod-agent spoza planu — dodatni identyfikator, brak kroku w `agents` — DALEJ dostaje
kafelek ze stanem `working`, bo to jest jedyny slad, jaki po nim zostaje; (d) rozstrzyga
pochodzenie wiersza, nie nazwa autora: wiersz okna podpisany **ta sama nazwa**, co prawdziwy
agent, nie dokłada temu agentowi ani jednej wypowiedzi; (e) kontrola przeciw pustemu przejsciu:
test sprawdza, ze fikstura naprawde niesie oba rodzaje wierszy.

*Slaba asercja:* `expect(roster(...)).toHaveLength(0)` na samej historii z okna. Przechodzi dla
implementacji, ktora nie rysuje kafelkow w ogole, i dla tej, ktora tnie po braku kroku w planie
— czyli kasuje pod-agentow. Rozrozniaja to (b) i (c).

## AC-2 Liczba kafelkow rowna sie liczbie agentow, ktorzy naprawde nadali
check: npx --no-install vitest run src/sections/run/rail/rail-counts-only-agents.test.ts
expect: (\d+) passed

Zdanie o calosci, nie o gałęzi: strumien mieszany — echo komend z okna, proza lidera, dwie
linie dwoch roznych krokow biegu i jedna linia pod-agenta — daje **dokladnie tyle** kafelkow,
ilu jest w nim agentow, ktorzy nadali. Asercje: (a) liczba kafelkow zgadza sie co do sztuki;
(b) zaden kafelek nie nazywa sie tak, jak podpis wierszy okna; (c) kolejnosc kafelkow zostaje
kolejnoscia pierwszego pojawienia sie w strumieniu — to jest wlasnosc, ktora ta naprawa moze
po cichu zepsuc, jesli przefiltruje historie po zbudowaniu mapy; (d) kontrola: fikstura ma
wiecej wierszy okna niz agentow, wiec implementacja liczaca wiersze zamiast agentow oblewa.

*Slaba asercja:* porownanie samej liczby. Przechodzi dla implementacji, ktora zgubila pod-agenta
i dolozyla widmo — dwa bledy znoszace sie w jednej liczbie. Rozrozniaja to (b) razem z (c).

<!-- OWNS
src/sections/run/rail/roster.ts
src/sections/run/rail/window-rows-are-not-agents.test.ts
src/sections/run/rail/rail-counts-only-agents.test.ts
-->
