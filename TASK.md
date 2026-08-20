# T-61 — Lider proponuje bieg, czlowiek go zaczyna

Rozstrzygniecie wlasciciela 2026-08-20, wariant A: lider ma podawac gotowa komende, a nie
sam startowac bieg. Wlasnosc, ktora to chroni, jest strukturalna i zostaje bez zadraśniecia:
`commands/chat.rs` nie zna `RunDeps`, nie importuje `run` i nie widzi bazy biegow, wiec
propozycja jest **tekstem**, a uruchomienie dalej nalezy do czlowieka — tak jak rozstrzygnal
2026-08-19 („tylko komendy determinuja akcje workflow").

Wartosc jest konkretna: lider patrzy na projekt, wiec umie powiedziec „to jest robota dla
Easy, z takim zadaniem" — a czlowiek nie musi pamietac nazw plikow workflow ani przepisywac
zdania. Jedno klikniecie zamiast przepisywania jest cala roznica miedzy czatem a formularzem.

**Dlaczego to nie moze byc rozpoznawane w oknie.** Wiersz strumienia jest decyzja projektowa
podjeta w Ruście, w mapowaniu zdarzenie -> linia (niezmiennik 15, decyzja D4). Okno, ktore
samo szuka `/run` w prozie agenta i dorysowuje przycisk, jest kuracja w CSS-ie: da sie ja
zepsuc arkuszem stylow, nie da sie jej sprawdzic bez przegladarki i nie ma jej w `run.json`.

**Cicha porazka, przed ktora stoi ten kontrakt:** propozycja rozpoznana u KAZDEGO agenta,
a nie tylko u lidera. Krok w srodku biegu, ktory napisze w prozie `/run ...`, dostalby wtedy
przycisk startujacy DRUGI bieg — a silnik prowadzi dzis jeden (`AppState::begin_run` podmienia
uchwyt, wiec pierwszy zostalby osierocony, niezmiennik 6). Dlatego rozpoznanie jest wlasnoscia
rozmowy, nie kuratora biegu.

**Read first:**
`src-tauri/src/engine/line.rs` (`Line`, `LineKind`, `Curator::observe` — i akapit przy
`Line::Told` o tym, dlaczego nowy rodzaj jest addytywny w obie strony),
`src-tauri/src/commands/chat.rs` (`read_along` — tu przechodza wiersze lidera),
`src/ipc/types.ts` i `src/ipc/line-wire.golden.json` (lustro drutu czytane z OBU stron),
`src/sections/run/feed/kinds.ts` (rejestr rodzajow: `Record<Kind, KindEntry>` nie skompiluje
sie bez nowego wpisu — i to jest cala obrona przed wierszem, ktorego nikt nie umie narysowac),
`src/sections/run/rail/say.ts` (`AUTHOR` — trzy autorytety, bez galezi domyslnej),
`src/sections/run/feed/line.tsx` (jak rysuje sie wiersz i skad bierze sie podpis),
`src/sections/run/rail/rail.tsx` (precedens: komponent wolajacy czynnosc wprost — `openAgent`),
`src/sections/run/run-command.ts` (`startFromLine` — CALA polityka `/run`, jedno miejsce),
`AGENTS.md` niezmienniki 13, 15, 16, 23.

## Niezmienniki, ktorych to dotyczy

- **15 — kuracja w Ruście, nie w CSS.** Rozpoznanie propozycji jest mapowaniem zdarzenie ->
  linia i mieszka po tamtej stronie granicy.
- **23 — polityka w jednym rdzeniu.** Przycisk woła **dokladnie ta sama** funkcje, co Enter
  w wierszu wejscia (`startFromLine`). Druga sciezka startu to druga odpowiedz na pytanie
  „ktory workflow, ile naraz, w ktorym folderze".
- **16 — kontrolka bez handlera.** Przycisk, ktory nie startuje, jest gorszy niz zdanie bez
  przycisku.

## Waskie mandaty na cudze pliki

1. `src-tauri/src/commands/chat.rs` nalezy do T-60. Wolno ci dopisac w nim **jedno wywolanie**
   nowej funkcji z `engine/line.rs` w petli czytajacej wiersze lidera. Nic wiecej.
2. `src-tauri/tests/it/ipc_line_wire_golden.rs` trzyma `const KINDS: [LineKind; 16]`. Wolno ci
   podniesc rozmiar tablicy o jeden i dopisac probke nowego rodzaju — **ani jednej asercji nie
   zdejmujesz i nie przepisujesz**. Jesli okaze sie, ze trzeba tknac cokolwiek innego, **stoj
   i zglos** (AGENTS.md §7).

## Szkielet, bez ktorego `before` nie jest czerwone

Rust: nowy wariant `Line` plus funkcja rozpoznajaca z `todo!()`. TypeScript:
`src/sections/run/feed/suggested.ts` musi istniec jako pusty szkielet rzucajacy
`throw new Error('not implemented')` — vitest przewraca sie na ZBIERANIU brakujacego importu,
a to jest podpis z `NOT_A_REAL_RED`.

## Kryteria akceptacji

## AC-1 Propozycja powstaje z prozy lidera i tylko z niej
check: cargo test --test it lead_suggests_a_run::
expect: (\d+) passed

Asercje: (a) proza lidera, ktorej linia zaczyna sie od `/run <nazwa> <zadanie>`, daje wiersz
nowego rodzaju, niosacy komende **co do znaku** — okno nie ma jej sklejac z powrotem;
(b) `/run` w SRODKU zdania („zrobilbym to przez /run easy") nie daje propozycji: to jest opis,
nie polecenie, a przycisk pod opisem startuje bieg, o ktory nikt nie prosil; (c) proza agenta
z BIEGU nie daje propozycji nigdy — powod w naglowku; (d) tekst wiersza zachowuje CALA proze
lidera, nie sama komende: czlowiek ma przeczytac, dlaczego lider to proponuje, zanim kliknie;
(e) kontrola przeciw pustemu przejsciu: fikstura niesie i zwykla proze, i propozycje, a test
wymaga, zeby powstaly DWA rozne rodzaje wierszy — inaczej mierzy implementacje, dla ktorej
wszystko jest propozycja.

*Slaba asercja:* `assert!(text.contains("/run"))`. Przechodzi dla implementacji, ktora zostawia
proze `Note` i niczego nie rozpoznaje — czyli dla dzisiejszego stanu. Rozroznia to (a) razem
z (e).

## AC-2 Nowy rodzaj przechodzi drut i ma miejsce w rejestrze
check: npx --no-install vitest run src/ipc/suggested-crosses-the-wire.test.ts
expect: (\d+) passed

Asercje: (a) lustro po stronie okna (`types.ts`) zna nowy rodzaj i jego zestaw kluczy zgadza
sie CO DO JEDNEGO ze zlotym plikiem; (b) rejestr `kinds()` ma dla niego wpis o trasie
`history` i rozwiniety domyslnie — propozycja zwinieta jest propozycja, ktorej nie widac;
(c) `authorityOf` tego rodzaju to `agent`: to sa slowa lidera, nie komunikat Loadouta ani nie
zdanie czlowieka; (d) kontrola: test sprawdza, ze rodzaj naprawde jest NOWY, czyli ze rejestr
urosl wzgledem zbioru rodzajow zapisanego w zlotym pliku sprzed zmiany — inaczej przechodzi
na rodzaju, ktory istnial.

*Slaba asercja:* sprawdzenie samego `kinds()`. Przechodzi dla rodzaju dopisanego wylacznie
w oknie, ktory z drutu nie przyjdzie nigdy — czyli dla wiersza-widma. Rozroznia to (a).

## AC-3 Wiersz propozycji niesie przycisk, ktory nazywa workflow
check: npx --no-install vitest run src/sections/run/feed/suggestion-has-a-button.test.tsx
expect: (\d+) passed

Asercje na markupie (`renderToStaticMarkup`): (a) wiersz tego rodzaju niesie `<button>`;
(b) dostepna nazwa przycisku zawiera nazwe workflow z komendy — „Run" bez nazwy nie mowi,
co sie stanie; (c) proza lidera stoi w tym samym wierszu i jest widoczna bez rozwijania;
(d) kontrola: wiersz rodzaju `note` z tym samym tekstem **nie** ma przycisku — inaczej test
przechodzi dla implementacji, ktora doklada przycisk kazdemu wierszowi.

*Slaba asercja:* `expect(markup).toContain('<button')`. Przechodzi dla przycisku rozwijania,
ktory w tym wierszu i tak stoi. Rozrozniaja to (b) i (d).

## AC-4 Klikniecie idzie ta sama droga, co Enter
check: npx --no-install vitest run src/sections/run/feed/suggested-runs-one-policy.test.ts
expect: (\d+) passed

Asercje: (a) czynnosc pod przyciskiem wola `startFromLine` z reszta linii po `/run`, co do
znaku; (b) nie wola `start` ani `invoke` z pominieciem tej polityki — limit „ile naraz",
folder zakresu i odmowy maja jedno miejsce (niezmiennik 23); (c) zdanie odmowy wraca i jest
pokazywane, a nie porzucane: propozycja z nazwa workflow, ktorego nie ma na dysku, konczy sie
zdaniem, nie cisza; (d) kontrola: test sprawdza, ze atrapa polityki w ogole zostala wolana —
zero wywolan przy zielonej asercji o „braku innych wywolan" jest przejsciem na pustce.

*Slaba asercja:* sam (b). Przechodzi dla implementacji, ktora nie robi NIC — bo wtedy tez nie
wola niczego zabronionego. Rozroznia to (a) razem z (d).

<!-- OWNS
src-tauri/src/engine/line.rs
src-tauri/src/commands/chat.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/lead_suggests_a_run.rs
src-tauri/tests/it/ipc_line_wire_golden.rs
src/ipc/types.ts
src/ipc/line-wire.golden.json
src/ipc/suggested-crosses-the-wire.test.ts
src/sections/run/feed/kinds.ts
src/sections/run/feed/line.tsx
src/sections/run/feed/suggested.ts
src/sections/run/feed/suggestion-has-a-button.test.tsx
src/sections/run/feed/suggested-runs-one-policy.test.ts
src/sections/run/rail/say.ts
-->
