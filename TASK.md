# T-60 — Lider zamiast listy workflow: kim jest, na czym stoi i gdzie patrzy

Rozstrzygniecie wlasciciela 2026-08-20: lista wyboru workflow obok Startu oddaje swoje miejsce
LIDEROWI. Powod nie jest kosmetyczny — ta lista jest slabsza z dwoch drog do jednej czynnosci
(nie umie przyjac zadania, ktore `/run <workflow> <co zbudowac>` przyjmuje), a zajmuje miejsce
w pasku na stale, przy suficie chrome 96 px z `docs/ARCHITECTURE.md` §7.

**Dlug, ktory przy okazji znika.** `src/sections/run/chosen-workflow.ts` zostal wyniesiony na
poziom modulu z jednym konkretnym uzasadnieniem: proza bez ukosnika miala uruchamiac ten
workflow, ktory czlowiek widzi wybrany. Wlasciciel skasowal to zachowanie tego samego dnia
(2026-08-19, „nie powinno byc tak, ze jak pisze bez komendy... to sie na nowo cale workflow
odpala"), a modul zostal. Jego jedynym konsumentem jest dzis `start.tsx`.

**Trzy wady lidera, ktore to zadanie zamyka.**

1. **Jest zaszyty.** `AppState::chat_driver` (`ipc.rs`) zwraca `Vendor::ClaudeCode` na sztywno,
   a `RunSpec.model` przy starcie rozmowy to `None`. Wlasna dokumentacja tej funkcji zapowiada
   ten dzien wprost: „w dniu, w ktorym orchestrator stanie sie konfigurowalny, ta funkcja
   zniknie na rzecz jego zapisanej definicji".
2. **Nie chodzi za zakresem.** `Chat::say` uzywa `cwd` **wylacznie przy zakladaniu sesji**;
   kazda nastepna tura leci do zywego procesu, ktory siedzi w folderze sprzed przelaczenia.
   Rozmowa jest przy tym jedna na cala aplikacje (`AppState.chat`), co jej wlasny komentarz
   nazywa „do przemyslenia, kiedy zakresy dostana wlasne sesje". Skutek widzi czlowiek:
   rozmawia o projekcie A, przelacza sie na B i dostaje odpowiedzi o A, bez ani jednego zdania
   ostrzezenia.
3. **Obiecuje wiecej, niz mu wolno.** `BRIEF` jest jedna stala i mowi „You may read files and
   write draft files when asked". Przy liderze, ktoremu czlowiek da `look only`, to zdanie
   staje sie nieprawda — a model, ktory obieca zapis i go nie wykona, zostawia czlowieka
   czekajacego na plik, ktory nie powstanie.

**Cicha porazka, przed ktora stoi ten kontrakt:** lider, ktory odpowiada o innym katalogu,
wyglada dokladnie jak lider, ktory sie myli. Nie ma zadnego sygnalu, po ktorym czlowiek moglby
to odroznic — a jedyna rzecza, ktora sie zmienila, byl jego wlasny klik w bocznym menu.

**Read first:**
`src-tauri/src/commands/chat.rs` (`Chat`, `Session`, `begin`, `BRIEF`, `lines_go_to`),
`src-tauri/src/ipc.rs` (`AppState.chat`, `chat_driver`, `open_chat`, `say_to_orchestrator`,
`project_for`),
`src-tauri/src/library/agents.rs` (`Agent`: `vendor`, `model`, `file_access`, `instructions`),
`src-tauri/src/commands/run.rs` (`policy_of` — jedyna tabela `FileAccess` -> `Policy`;
TEGO PLIKU NIE POSIADASZ, czytasz go, zeby uzyc tej samej tabeli, a nie napisac drugiej),
`src/sections/run/start.tsx` (lista workflow, `chosen-workflow`, `requested`, Start/Stop),
`src/sections/run/requested.ts` (zielony `Run` z edytora — jego JEDYNY konsument to `start.tsx`),
`src/sections/run/io.ts` (`openChat`, `sayToOrchestrator` — obie juz przyjmuja folder),
`AGENTS.md` niezmienniki 13, 16, 23.

## Niezmienniki, ktorych to dotyczy

- **23 — polityka w jednym rdzeniu.** Tlumaczenie `FileAccess` -> `Policy` mieszka w
  `commands/run.rs` (`policy_of`) i lider ma z niego korzystac, nigdy napisac swoje.
- **13 — jeden fakt, jedno miejsce.** „Kim jest lider" ma dokladnie jedno zrodlo: zapisana
  definicja agenta. Kopia vendora czy modelu trzymana obok w stanie okna jest pierwsza
  rzecza, ktora sie rozjedzie.
- **16 — kontrolka bez handlera.** Zielony `Run` z edytora workflow ma dzis JEDNEGO
  konsumenta i jest nim znikajaca kontrolka. Bez nowego odbiorcy staje sie martwym
  przyciskiem — i zlapie to `e2e/tests/no-dead-controls.spec.ts`, tylko o jeden bieg za pozno.

## Szkielet, bez ktorego `before` nie jest czerwone

Rust: sygnatury z `todo!()`, zeby testy sie kompilowaly i padly w czasie wykonania.
TypeScript: `src/sections/run/lead.ts` i `src/sections/run/requested-launch.ts` musza istniec
jako puste szkielety (funkcje rzucajace `throw new Error('not implemented')`), bo vitest
przewraca sie na ZBIERANIU brakujacego importu, a to jest `NOT_A_REAL_RED`.

## Kryteria akceptacji

## AC-1 Lider bierze vendora, model i polityke z zapisanej definicji agenta
check: cargo test --test it lead_comes_from_the_agent::
expect: (\d+) passed

Asercje: (a) rozmowa z liderem wskazanym na agenta o `vendor: codex` startuje sterownikiem
Codeksa, a nie Claude'a — `chat_driver` znika, nie zostaje jako gałąź domyślna; (b) `model`
z definicji dojezdza do `RunSpec.model` (dzis zawsze `None`); (c) `policy` pochodzi z tej
samej tabeli, ktorej uzywa bieg (`policy_of`), a nie z drugiej kopii: agent `look-only`
startuje rozmowe z `Policy::ReadOnly`, `work-freely` z `Policy::Unrestricted`; (d) `instructions`
agenta dojezdzaja do promptu systemowego RAZEM z `BRIEF`, a nie zamiast niego — lider bez
zdania „nie uruchamiasz biegow" jest liderem, ktory obieca start; (e) kontrola: brak wskazanego
lidera to **odmowa nazywajaca nastepny ruch**, nie ciche wrocenie do zaszytego Claude'a.

*Slaba asercja:* sprawdzenie samego `model`. Przechodzi dla implementacji, ktora czyta
definicje i dalej startuje Claude'em dla kazdego vendora — czyli dla wyboru, ktory wyglada
na dzialajacy i nie dziala. Rozroznia to (a).

## AC-2 Watek lidera nalezy do zakresu, nie do aplikacji
check: cargo test --test it lead_thread_per_scope::
expect: (\d+) passed

Asercje: (a) zdanie powiedziane w zakresie A i zdanie powiedziane w zakresie B zakladaja DWIE
sesje, kazda z `cwd` swojego zakresu — dzis druga tura idzie do procesu pierwszego zakresu;
(b) powrot do zakresu A trafia w te SAMA sesje, co za pierwszym razem (rozmowa nie zaczyna sie
od nowa); (c) sesja zakresu B zyje dalej, kiedy okno patrzy na A — zamkniecie cudzej rozmowy
przy przelaczeniu bylo by zgubieniem watku, o ktory chodzi cale to zadanie; (d) zamkniecie
okna konczy WSZYSTKIE sesje i kazda z nich dowodzi smierci swojej grupy procesow
(niezmiennik 6) — rozmowa osierocona pali limit tak samo jak bieg; (e) kontrola: test sam
sprawdza, ze fikstura ma dwa ROZNE katalogi, inaczej mierzy jedna sesje dwa razy.

*Slaba asercja:* test na dwoch `cwd` przy PIERWSZYM zdaniu w kazdym zakresie. Przechodzi dla
implementacji, ktora zaklada nowa sesje przy kazdej turze — czyli dla lidera bez pamieci,
ktory za kazdym zdaniem zaczyna rozmowe od zera i placi za to u dostawcy. Rozroznia to (b).

## AC-3 Prompt systemowy nie obiecuje wiecej, niz polityka pozwala
check: cargo test --test it brief_matches_the_policy::
expect: (\d+) passed

Asercje: (a) przy `Policy::ReadOnly` prompt systemowy nie zawiera obietnicy zapisu plikow
(dzisiejsze „write draft files"); (b) przy `EditInFolder` i `Unrestricted` ta obietnica jest;
(c) zdanie „nie uruchamiasz biegow, robi to czlowiek przez /run" stoi w KAZDEJ z trzech
wersji — to jest wlasnosc struktury (`commands/chat` nie zna biegu) i prompt ma jej nie
zaprzeczac; (d) kontrola przeciw pustemu przejsciu: test porownuje trzy wersje miedzy soba
i wymaga, zeby przynajmniej dwie sie ROZNILY — inaczej mierzy jedna stala trzy razy.

*Slaba asercja:* `assert!(brief.contains("/run"))`. Przechodzi dla jednej stalej, czyli dla
dzisiejszego stanu, ktory to kryterium ma zmienic. Rozroznia to (a) razem z (d).

## AC-4 Pasek niesie lidera, a zielony `Run` z edytora dalej ma odbiorce
check: npx --no-install vitest run src/sections/run/lead-replaces-the-picker.test.tsx
expect: (\d+) passed

Asercje: (a) markup kontrolek biegu NIE zawiera juz listy `aria-label="Workflow to run"`;
(b) zawiera kontrolke, ktora nazywa lidera i ma etykiete dostepnosciowa — wybor bez nazwy
jest zagadka; (c) `requested-launch` wolany z zadaniem z edytora naprawde uruchamia bieg
(atrapa `launchRun` widzi wywolanie z tym plikiem workflow) — to jest dowod, ze zielony `Run`
nie zostal martwym przyciskiem po zabraniu jego jedynego konsumenta; (d) zadanie zdjete raz
nie uruchamia sie drugi raz przy nastepnym renderze — zapadka `takeRequestedRun` jest cala
ochrona przed dwoma biegami z jednego klikniecia; (e) kontrola: test sprawdza, ze markup
w ogole zawiera grupe kontrolek biegu, inaczej (a) przechodzi na pustce.

*Slaba asercja:* sam (a). Przechodzi dla zmiany, ktora kasuje liste i nie stawia w jej miejsce
niczego — a wtedy zielony `Run` w edytorze przestaje cokolwiek robic, ekran pracy traci jedyna
mysia droge do biegu, i dowiaduje sie o tym czlowiek. Rozrozniaja to (b) i (c).

## Waski mandat na cudzy plik

`src/sections/run/io.ts` nalezy do niewyladowanego T-41. Wolno ci w nim dopisac **wylacznie**
klucz `folder` do wywolania `open_chat` — bez niego Rust nie ma czym rozroznic strumieni dwoch
zakresow i AC-2 jest niewykonalny. Kazda inna zmiana w tym pliku jest cudza: jesli okaze sie
potrzebna, **stoj i zglos** (AGENTS.md §7).

<!-- OWNS
src-tauri/src/commands/chat.rs
src-tauri/src/ipc.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/lead_comes_from_the_agent.rs
src-tauri/tests/it/lead_thread_per_scope.rs
src-tauri/tests/it/brief_matches_the_policy.rs
src/sections/run/start.tsx
src/sections/run/lead.ts
src/sections/run/chosen-workflow.ts
src/sections/run/requested-launch.ts
src/sections/run/lead-replaces-the-picker.test.tsx
src/sections/run/io.ts
-->
