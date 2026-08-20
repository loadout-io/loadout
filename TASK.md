# T-58 — Wiersz wejscia jest terminalem: kursor, historia, echo, adresat

Zgloszenie wlasciciela 2026-08-20, cztery wady w jednym wierszu ekranu pracy:
kursor nie stoi w polu (trzeba kliknac, za kazdym razem), strzalka w gore nie cofa do
poprzedniej linii, komendy nie zostawiaja po sobie ani jednego wiersza w strumieniu,
a proza w trakcie biegu znika z rozmowy z liderem, bo leci do pracujacego agenta.

**Co juz dziala i czego to zadanie NIE dotyka.** Proza ma nosnik na drucie od 2026-08-19:
`Line::Told` (`engine/line.rs`) wystawia `commands/chat.rs` i `commands/run.rs`, a widok
podpisuje ja `You ->` (`feed/line.tsx`, autorytet `you` z `rail/say.ts`). Tego wiersza NIE
dublujemy lokalnie -- dwa wiersze o jednym zdaniu to dwa miejsca prawdy (niezmiennik 13).
Dziura jest w tym, czego drut nie widzi nigdy: `/run`, `/open`, `/stop` i odpowiedzi samego
wiersza (`NOT_KNOWN`, `NOTHING_RUNS`, odmowy) obsluguje `send()` w oknie i nic po nich nie
zostaje.

**Cicha porazka, przed ktora stoi ten kontrakt:** terminal, w ktorym wpisana komenda nie
zostawia sladu, jest nieodrozninalny od terminala, ktory tej komendy nie przyjal. Wlasciciel
mial dokladnie te watpliwosc 2026-08-19 przy prozie („a moze odpisuje on, ale na pewno nie
widac moich wiadomosci") i wtedy powstal `Line::Told`. Komendy zostaly z ta sama wada.

**Read first:**
`src/sections/run/entry/entry.tsx` (`send()`, `onKeyDown` obsluguje dzis wylacznie Tab,
`whereItGoes`, `data-entry-said`),
`src/sections/run/index.tsx` (`sayIt` -- to on wybiera dzis agenta zamiast lidera; `openFolder`),
`src/sections/run/feed/model.ts` (`Incoming`, `appendLines`, sklejanie po `agent`+`kind`),
`src/sections/run/feed/live.ts` (rejestr sesji per zakres, `runFeed`),
`src/sections/run/rail/say.ts` (`AUTHOR`, `authorityOf` -- autorytet `loadout` istnieje),
`src/sections/run/io.ts` (`start`/`openChat` stempluja wiersze licznikiem od 1 -- OSOBNYM dla
biegu i dla rozmowy, wiec identyfikatory w jednej sesji juz dzis potrafia sie powtorzyc),
`e2e/harness.ts` (prawdziwa przegladarka, prawdziwy React, atrapa `__TAURI_INTERNALS__`),
`AGENTS.md` §2a i niezmienniki 4, 13, 14, 16.

## Niezmienniki, ktorych to dotyczy

- **13 — jeden fakt, jedno miejsce.** Odpowiedz wiersza przenosi sie POD POLEM do strumienia
  i zostaje w jednym miejscu. `data-entry-hint` (gdzie pojdzie to, co piszesz) zostaje bez
  zmiany: to inny fakt, mowiony PRZED Enterem. `data-entry-said` znika razem z przeprowadzka.
- **16 — kontrolka bez handlera.** Pole, w ktorym nie stoi kursor i ktorego strzalka nie
  obsluguje, obiecuje terminal i go nie dowozi.
- **4 — pliki sa prawda.** Wiersz dopisany przez OKNO nie ma prawa udawac zdarzenia biegu:
  nie ma go w `run.json` i nie przezyje przeladowania. Niesie to jego identyfikator (AC-2c).
- **14 — zero zargonu.** Wiersze skladane w oknie sa zdaniami po angielsku, nigdy enumem.

## Szkielet, bez ktorego `before` nie jest czerwone

`vitest` przewraca sie na ZBIERANIU pliku, ktorego import nie istnieje, a „Cannot find module"
jest podpisem z `NOT_A_REAL_RED` (AGENTS.md §2a). Przed pierwszym `./verify.sh before` musza
wiec istniec jako puste szkielety: `entry/history.ts`, `entry/echo.ts`, `addressee.ts` --
kazda eksportowana funkcja rzuca `throw new Error('not implemented')`. Kryterium ma paść na
ASERCJI, nie na imporcie.

## Kryteria akceptacji

## AC-1 Strzalka cofa do poprzedniej linii i nie gubi szkicu
check: npx --no-install vitest run src/sections/run/entry/history.test.ts
expect: (\d+) passed

Czysty modul, bo w tym repo nie ma jsdom i Enter jest dla kryterium nieosiagalny -- to samo
rozumowanie, co przy `suggestions` i `run-command.ts`. Asercje: (a) po trzech zapamietanych
liniach pierwszy krok wstecz oddaje NAJMLODSZA, drugi przedostatnia, a krok naprzod wraca
w druga strone; (b) krok naprzod ponizej najmlodszej oddaje SZKIC, czyli to, co stalo w polu,
zanim czlowiek pierwszy raz siegnal wstecz -- nie pusty napis; (c) dwie identyczne linie pod
rzad zajmuja jeden wpis; (d) historia ma sufit i wypada z niej NAJSTARSZA; (e) kontrola: przy
pustej historii krok wstecz oddaje `null`, a nie pusty napis -- pole ma zostac nietkniete.

*Slaba asercja:* test wylacznie na (a). Przechodzi dla implementacji, ktora przy kroku naprzod
czysci pole -- czyli kasuje zdanie, ktore czlowiek wlasnie pisal, i robi to cicho. Rozroznia
to (b), i dlatego stoi w tym samym pliku, a nie „gdzies obok".

## AC-2 Kazda linia, ktora wysylasz, zostawia wiersz — i nie udaje zdarzenia biegu
check: npx --no-install vitest run src/sections/run/entry/echo.test.ts
expect: (\d+) passed

Czysty modul sklada wiersz, ktory okno dopisuje samo. Asercje: (a) `/run easy zbuduj X` daje
wiersz, ktorego tekst niesie CALA linie co do znaku; (b) odpowiedz wiersza (nieznana komenda,
„Nothing is running.", odmowa startu) daje wiersz tego samego ksztaltu -- rozmowa z Loadoutem
jest jedna historia, nie polowa pod polem; (c) identyfikator wiersza jest UJEMNY: pompa biegu
i pompa rozmowy stempluja od 1 osobno (`io.ts`), wiec dodatni licznik w oknie zderzylby sie
z ich numerami w tej samej sesji -- ujemny nie moze; (d) `authorityOf(kind)` tego wiersza to
`loadout`, sprawdzone WOLANIEM tej funkcji, nie zalozeniem o rodzaju -- wiersz podpisany
`agent` bylby cytatem przypisanym komus, kto go nie wypowiedzial; (e) proza bez ukosnika NIE
daje wiersza z okna: jej wiersz przychodzi z drutu jako `told`.

*Slaba asercja:* sprawdzenie, ze tekst jest niepusty. Przechodzi dla implementacji, ktora
sklada wiersz podpisany agentem i z identyfikatorem 1 -- czyli takiej, ktora psuje klucze
Reacta i przypisuje Twoje slowa komus innemu. Rozrozniaja to (c) i (d).

## AC-3 Pole ma kursor od pierwszej sekundy
check: npx --no-install vitest run src/sections/run/entry/caret.test.tsx
expect: (\d+) passed

Markup, bo `renderToStaticMarkup` wypisuje `autofocus=""` (sprawdzone). Asercje: (a) markup
`<Entry>` niesie `autofocus` na polu z `aria-label="Command line"`; (b) w calym tym markupie
DOKLADNIE jeden element ma `autofocus` -- dwa ogniska to zero ognisk; (c) kontrola przeciw
pustemu przejsciu: test sam sprawdza, ze markup w ogole zawiera pole `Command line`, inaczej
mierzy pustke i przechodzi na niczym.

*Slaba asercja:* `expect(markup).toContain('autofocus')`. Przechodzi, gdy atrybut wyladuje na
dowolnym elemencie -- takze na przycisku Tab-completion. Rozroznia to (a) razem z (b).

## AC-4 Zdanie bez ukosnika idzie do lidera; do agenta wylacznie po nazwie
check: npx --no-install vitest run src/sections/run/addressee.test.ts
expect: (\d+) passed

To jest zmiana polityki, nie porzadki: do dzis proza przy pracujacym agencie szla do NIEGO
(`index.tsx`, `sayIt`), wiec lider znikal na czas biegu -- czyli dokladnie wtedy, kiedy
czlowiek chce zapytac, co sie dzieje. Konwencja „nazwa na poczatku linii" juz istnieje i to
nia odmawia Rust przy kilku pracujacych (`RunError::SeveralAreWorking`).

Asercje: (a) przy zerze pracujacych adresatem jest lider; (b) przy JEDNYM pracujacym adresatem
dalej jest lider; (c) linia zaczynajaca sie nazwa pracujacego kroku adresuje TEN krok, a nazwa
jest zdejmowana z tresci, ktora do niego pojdzie; (d) nazwa kroku, ktory nie pracuje, nie jest
adresem -- zdanie idzie do lidera w calosci, razem z tym slowem; (e) dopasowanie jest na calym
slowie, nie na prefiksie: `Plan` nie adresuje kroku `Planner`.

*Slaba asercja:* test wylacznie na (a). Przechodzi dla dzisiejszej implementacji, ktora przy
jednym pracujacym wysyla do agenta -- czyli dla wady, ktora to kryterium zamyka. Rozroznia
to (b).

## AC-5 Prawdziwa klawiatura w prawdziwej przegladarce
check: npx --no-install vitest run e2e/tests/terminal-behaves.spec.ts
expect: (\d+) passed

`renderToStaticMarkup` nie odpala ani jednego zdarzenia, wiec kursor i strzalka moga byc
sadzone WYLACZNIE tutaj (`e2e/harness.ts`: prawdziwy chromium, prawdziwy React, atrapa
`__TAURI_INTERNALS__`). Asercje na swiezo otwartej aplikacji, na ekranie pracy: (a) bez ani
jednego klikniecia `document.activeElement` ma `aria-label="Command line"`; (b) po wpisaniu
`/stop` i `Enter` (przy braku biegu) w strumieniu stoi wiersz `[data-line]` z ta linia;
(c) `ArrowUp` wstawia te linie z powrotem do pola; (d) klik w kolumne strumienia w miejsce
bez kontrolki oddaje kursor polu; (e) kontrola przeciw „focus kradniemy zawsze": klik
w PRZYCISK wewnatrz strumienia zostawia ognisko na tym przycisku -- kontrolka, w ktora
czlowiek celowal, nie ma prawa stracic klikniecia.

*Slaba asercja:* sam (a). Przechodzi dla implementacji z `autoFocus` i bez ani jednej z
pozostalych trzech rzeczy -- czyli dla pola, ktore traci kursor przy pierwszym kliknieciu
i nigdy go nie odzyskuje.

<!-- OWNS
src/sections/run/entry/entry.tsx
src/sections/run/entry/history.ts
src/sections/run/entry/history.test.ts
src/sections/run/entry/echo.ts
src/sections/run/entry/echo.test.ts
src/sections/run/entry/caret.test.tsx
src/sections/run/addressee.ts
src/sections/run/addressee.test.ts
src/sections/run/index.tsx
e2e/tests/terminal-behaves.spec.ts
-->
