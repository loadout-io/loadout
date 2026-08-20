# T-71 — Plusik otwiera NOWY TERMINAL w zakresie, ktory juz wybrales

Zgloszenie wlasciciela 2026-08-20: „jak klikam plusik to powinno po prostu odpalac nowy nasz
terminal i sobie tam mozemy kolejne workflow w naszym scope co mamy zaznaczone, a nie tak jak
teraz ze scope wybieramy znowu".

**Stan jest gorszy, niz brzmi zgloszenie, i to zmierzone z kodu.** `＋` (`tabs/tab-bar.tsx`)
wola `openFolder` (`index.tsx`), czyli systemowe okno wyboru katalogu; wybor konczy sie
`useWorkspaces.add(...)`, a to USTAWIA nowy zakres jako aktywny (`state/workspaces.ts`). Pasek
kart pokazuje karty aktywnego zakresu — a w swiezym zakresie nie ma zadnej. Czyli klikniecie
w `＋` nie dokłada karty **nigdy**: wymienia zakres i pasek robi sie pusty. Karty zaklada
wylacznie start biegu (`tabs/store.ts`, `cardForRun`).

**Co stoi na przeszkodzie.** Karta JEST dzis folderem: `id === folder`, a komentarz magazynu
mowi wprost „w jednym zakresie moze stac najwyzej jedna karta". Sesja strumienia
(`feed/live.ts`, `feedFor`) i magazyn biegu (`state/run.ts`, `runFor`) sa kluczowane tym samym
folderem. Terminal potrzebuje wiec **wlasnej tozsamosci**, a folder staje sie jego polem.

**Zakres tego zadania: ETAP A.** Nowy terminal rozmawia z liderem i ma wlasna historie. Bieg
dalej jest jeden na aplikacje (`AppState.live`, zapadka `going` w `io.ts`), wiec start
z drugiego terminalu, kiedy pierwszy bieg idzie, **odmawia zdaniem** — nigdy po cichu nie
podmienia uchwytu. Etap B (biegi rownolegle) wymaga tozsamosci biegu na drucie i `stop_run(id)`;
T-69 jest jego warunkiem wstepnym i dlatego nie jest tu.

**Cicha porazka, przed ktora stoi ten kontrakt:** terminal, ktory wyglada na osobny i dzieli
strumien. Dwie karty pokazujace te sama historie sa gorsze niz jedna, bo czlowiek wpisuje
zdanie w jedna i widzi je w obu — i przestaje wierzyc, ze cokolwiek na tym ekranie nalezy do
czegokolwiek.

**Read first:**
`src/sections/run/tabs/store.ts` (`cardForRun`, `cardsIn`, akapit „ile kart moze byc"),
`src/state/run-tabs.ts` (`WorkspaceTab`, `id` z rejestru Rusta),
`src/sections/run/feed/live.ts` (rejestr `feeds`, `feedFor`, `runFeed` jako UCHWYT),
`src/state/run.ts` (`runFor` — ten sam klucz),
`src/sections/run/tabs/tab-bar.tsx` (`＋` i jego dzisiejszy handler),
`src/sections/run/index.tsx` (`openFolder`, `shown`, `onTop`),
`src-tauri/src/commands/chat.rs` i `src-tauri/src/ipc.rs` (watek lidera per zakres z T-60 —
tu staje sie per terminal),
`AGENTS.md` niezmienniki 4, 13, 16, 17.

## Niezmienniki, ktorych to dotyczy

- **13 — jeden fakt, jedno miejsce.** „Ktory terminal widac" ma jedna odpowiedz; „gdzie pracujemy"
  zostaje w magazynie zakresow i terminal go tylko NIESIE.
- **4 — pliki sa prawda.** Terminal jest stanem UI (`~/.loadout/ui.json`), wiec jego skasowanie
  ma kosztowac uklad kart i nic wiecej.
- **16 — kontrolka bez handlera.** `＋`, ktore nie dokłada nic widocznego, jest dzis dokladnie
  taka kontrolka.

## Waski mandat na zlota liste komend

`src-tauri/commands.golden.txt` **wchodzi do OWNS z jednym dozwolonym uzyciem: dopisujesz nazwe
JEDNEJ nowej komendy — tej, ktora konczy watek zamykanego terminalu.** Bez tego AC-3(c) nie da sie
spelnic koniec-koniec: `Threads::close_at` istnieje, ale zaden `#[tauri::command]` go nie opakowuje,
a `tests/it/ipc_commands_registered.rs` porownuje liste handlera z tym plikiem **co do sztuki** —
wiec komendy nie da sie dodac bez wpisu tutaj.

Skutek dzisiejszego stanu jest finansowy, nie kosmetyczny: kazdy terminal otwarty plusikiem
i zamkniety krzyzykiem zostawia swojego lidera zywego i placacego do zamkniecia okna
(niezmiennik 6 — „osierocony `claude` pali limit w tle"). Zadna inna zmiana w tym pliku nie nalezy
do tego zadania; plik ma tez wlasciciela w niewyladowanym T-41. Jesli okaze sie, ze trzeba tknac
cokolwiek wiecej, **stoj i zglos** (AGENTS.md §7).

## Waski mandat na kryterium starego modelu kart

`src/sections/run/tabs/cards-are-runs.test.ts` (wyladowane) adresuje karty FOLDEREM —
`cardForRun(nazwa, folder)` plus `requestClose(folder)` — bo do dzis karta ZNACZYLA bieg
w zakresie, a jej `id` bylo folderem. To zadanie zmienia ten model na zamowienie wlasciciela
(„plusik otwiera nowy terminal w zakresie, ktory juz wybrales"), wiec tamta fikstura przestaje
trafiac w karte.

**Wolno ci przepisac wylacznie ADRESOWANIE: fikstura ma brac identyfikator terminalu zamiast
zakladac, ze jest nim folder.** Ani jedna asercja, ani jedno zdanie uzasadnienia i ani jeden
komunikat bledu nie zmienia sie — bo tresc tego kryterium przezywa zmiane modelu w calosci
i jest wazna: zamkniecie karty, na ktorej NIC nie idzie, nie ma prawa zawolac `stop_run`, bo
przy jednym biegu naraz po stronie Rusta to zabija bieg w INNYM zakresie.

Jesli ktorakolwiek asercja przestaje dac sie utrzymac przy karcie-terminalu, **stoj i zglos**
(AGENTS.md §7) — wtedy to nie jest przepisanie adresu, a zmiana kryterium, i rozstrzyga czlowiek.

## Waski mandat na lustro komend okna

`src/sections/commands-wired.test.ts` wchodzi do OWNS z jednym dozwolonym uzyciem: dopisujesz
**jeden addytywny wiersz** dla `run.closeTerminal` → `close_terminal`, z identyfikatorem terminalu
jako argumentem. T-71 dodaje eksport do `src/sections/run/io.ts`, a to lustro z premedytacja
odmawia kazdemu eksportowi bez wykonywalnej krawedzi. Nie wolno usunac ani przepisac zadnego
istniejacego wiersza. Bez tego mandatu pelna bramka jest czerwona na poprawnym lustrze poza
zakresem, mimo ze AC-3 wymaga tej samej komendy koniec-koniec.

## Szkielet, bez ktorego `before` nie jest czerwone

`src/sections/run/tabs/terminal.ts` (tozsamosc terminalu) musi istniec jako pusty szkielet
rzucajacy `throw new Error('not implemented')` — vitest przewraca sie na ZBIERANIU brakujacego
importu, a to jest podpis z `NOT_A_REAL_RED`.

## Kryteria akceptacji

## AC-1 Terminal ma wlasna tozsamosc, a folder jest jego polem
check: npx --no-install vitest run src/sections/run/tabs/terminal-has-its-own-identity.test.ts
expect: (\d+) passed

Asercje: (a) dwa terminale zalozone w JEDNYM zakresie maja rozne identyfikatory i ten sam
folder; (b) `cardsIn` oddaje OBA dla tego zakresu — dzisiejszy filtr po `id === folder` oddawal
najwyzej jeden; (c) zamkniecie jednego zostawia drugi, razem z jego folderem; (d) terminal
w innym zakresie nie jest widoczny w tym — to jest wlasnosc, ktora ta zmiana najlatwiej psuje;
(e) kontrola przeciw pustemu przejsciu: test sprawdza, ze fikstura ma dwa zakresy i trzy
terminale, inaczej mierzy liste jednoelementowa.

*Slaba asercja:* `expect(cardsIn(...)).toHaveLength(2)`. Przechodzi dla implementacji, ktora
przestala filtrowac po zakresie w ogole — czyli pokazuje w tym zakresie karty cudzego.
Rozroznia to (d).

## AC-2 Kazdy terminal ma wlasna historie
check: npx --no-install vitest run src/sections/run/feed/session-per-terminal.test.ts
expect: (\d+) passed

Asercje: (a) linie wpuszczone do sesji terminalu A nie pojawiaja sie w widoku terminalu B, choc
oba stoja w tym samym folderze; (b) przelaczenie widoku na B i powrot do A oddaje historie A
w calosci — sesja nie jest kasowana przy przelaczeniu (to jest wymog wlasciciela z 2026-08-18);
(c) sesja powstaje na zadanie i nie ginie: rejestr nie ma usuwania; (d) kontrola: obie sesje
dostaja po linii i test sprawdza, ze KAZDA widzi swoja — inaczej przechodzi dla implementacji,
ktora nie wpuszcza nigdzie niczego.

*Slaba asercja:* test na samym (a). Przechodzi dla implementacji gubiacej historie przy
przelaczeniu — czyli dla wady, ktora wlasciciel zglosil dwa dni wczesniej. Rozroznia to (b).

## AC-3 Watek lidera nalezy do terminalu
check: cargo test --test it lead_thread_per_terminal::
expect: (\d+) passed

T-60 dalo watek per ZAKRES; terminal jest jednostka drobniejsza. Asercje: (a) dwa terminale
w jednym folderze zakladaja DWIE sesje lidera, kazda ze swoim `cwd` rownym temu folderowi;
(b) powrot do terminalu A trafia w te sama sesje, co za pierwszym razem; (c) zamkniecie
terminalu konczy JEGO sesje i dowodzi smierci grupy (niezmiennik 6) — rozmowa osierocona pali
limit tak samo jak bieg; (d) zamkniecie okna konczy wszystkie; (e) kontrola: test sprawdza, ze
oba terminale maja ten sam folder — inaczej mierzy to, co T-60 juz dowiodlo.

*Slaba asercja:* liczenie sesji. Przechodzi dla implementacji zakladajacej nowa sesje przy
kazdej turze — czyli dla lidera bez pamieci, ktory placi za kazde zdanie. Rozroznia to (b).

## AC-4 Plusik naprawde otwiera terminal, i widzi to czlowiek
check: npx --no-install vitest run e2e/tests/plus-opens-a-terminal.spec.ts
expect: (\d+) passed

Niezmiennik 29: skutek `＋` musi byc widoczny tam, gdzie patrzy czlowiek, a nie tylko w magazynie.
Asercje na prawdziwym froncie (`e2e/harness.ts`): (a) przy wybranym zakresie klikniecie `＋`
NIE otwiera systemowego okna wyboru katalogu — atrapa `__TAURI_INTERNALS__` nie widzi wywolania
wyboru folderu; (b) po kliknieciu na pasku stoi o jedna karte wiecej niz przed, a **drugie**
klikniecie przy juz otwartym terminalu doklada kolejna zamiast podmienic pierwsza; (c) nowa karta
po kazdym z tych klikniec jest na wierzchu i pole wejscia dalej ma kursor (T-58 AC-3 nie ma sie zepsuc przy okazji);
(d) kontrola przeciw pustemu przejsciu: przy BRAKU zakresu `＋` dalej pyta o folder, bo terminal
bez miejsca pracy nie ma gdzie stanac — i test sprawdza oba przypadki.

*Slaba asercja:* sam (b). Przechodzi dla implementacji, ktora dokłada karte i przy okazji nadal
otwiera okno wyboru folderu — czyli zostawia dokladnie te wade, ktora zglosil wlasciciel.
Rozroznia to (a) razem z (d).

## AC-5 Zywa komenda idzie przez rejestr watkow, a nie przez jeden uchwyt
check: cargo test --test it live_chat_goes_through_the_registry::
expect: (\d+) passed

**To jest zdjecie blokady, ktora T-60 opisalo i ktorej nie moglo tknac.** `commands::chat::Threads`
trzyma watek na zakres od 2026-08-20 i **zywa aplikacja go nie konstruuje**: `AppState.chat` jest
`Mutex<Option<Chat>>`, wiec `say_to_orchestrator` chodzi stara droga. Pisarz T-60 zapisal powod
wprost (`ipc.rs`, akapit „WATEK PER ZAKRES ISTNIEJE I NIE STOI TUTAJ"): `Threads::say` wymaga
WSKAZANEGO lidera, a wskazania nie ma czym dowiezc z okna — brakuje klucza obok `folder`, czyli
zmiany w `io.ts`, na ktora mandat T-60 nie pozwalal. **Odmowil podstawienia polowy i mial racje.**
To zadanie posiada `io.ts`, `ipc.rs` i `chat.rs`, wiec zdejmuje te blokade w calosci.

Asercje: (a) `say_to_orchestrator` rozstrzyga watek **przez rejestr**, nie przez pojedyncze pole
— test woła te sama funkcje, ktora wola okno, a nie konstruktor uzywany wylacznie w testach;
(b) dwa zdania do dwoch terminali tego samego folderu, oba wyslane TA DROGA, zakladaja dwa watki;
(c) `AppState` nie trzyma juz pojedynczego `Chat` — pole znika, nie zostaje obok jako martwe
(niezmiennik 13: dwa domy dla „gdzie mieszka rozmowa" to pierwsza rzecz, ktora sie rozjedzie);
(d) wskazanie lidera dojezdza z okna: wywolanie bez niego jest **odmowa nazywajaca nastepny ruch**,
nie cichym wroceniem do zaszytego Claude'a; (e) kontrola przeciw pustemu przejsciu: test sprawdza,
ze fikstura ma dwa terminale i JEDEN folder — inaczej mierzy to, co T-60 juz dowiodlo.

*Slaba asercja:* test na `Threads` konstruowanym w tescie. Przechodzi dzis — i to jest dokladnie
ta wada, ktora znalazl recenzent T-70: mechanizm dowiedziony, produkt go nie wola. Rozroznia to
(a) razem z (c), bo pole `chat` musi ZNIKNAC, a nie dostac drugiego mieszkanca.

<!-- OWNS
src/sections/run/tabs/store.ts
src/sections/run/tabs/cards-are-runs.test.ts
src/sections/run/tabs/terminal.ts
src/sections/run/tabs/tab-bar.tsx
src/state/run-tabs.ts
src/sections/run/feed/live.ts
src/state/run.ts
src/sections/run/index.tsx
src/sections/run/io.ts
src-tauri/src/commands/chat.rs
src-tauri/src/ipc.rs
src-tauri/commands.golden.txt
src-tauri/tests/it/main.rs
src-tauri/tests/it/lead_thread_per_terminal.rs
src-tauri/tests/it/live_chat_goes_through_the_registry.rs
src/sections/run/tabs/terminal-has-its-own-identity.test.ts
src/sections/run/feed/session-per-terminal.test.ts
e2e/tests/plus-opens-a-terminal.spec.ts
src/sections/commands-wired.test.ts
-->
