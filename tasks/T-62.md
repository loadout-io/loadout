# T-62 — `/ask <agent> <zadanie>`: jeden agent bez robienia workflow

Zamowienie wlasciciela 2026-08-20: „odpalac nasze workflows/agents". Workflow ma dzis droge
(`/run`), agent nie ma zadnej. Jednostka pracy jest graf: `run_workflow` bierze nazwe PLIKU,
wiec zeby puscic jednego agenta z jednym zdaniem, czlowiek musi wejsc do edytora, zalozyc
workflow, postawic jeden kafelek, zapisac go i wrocic. To jest cena, ktora placi sie za
najczestsza czynnosc dnia.

**Czym to NIE jest.** Nie jest druga maszyneria obok biegu i nie ma prawa nia byc. Bieg
jednokrokowy to **zwykly bieg**: ma plan, katalog `runs/<ts>__<id>/`, transkrypt, miejsce
w puli i dowod smierci grupy na koncu. Druga sciezka wykonania — „lekki tryb bez katalogu" —
bylaby dokladnie tym, co `docs/ARCHITECTURE.md` opisuje jako osiem rodzajow autorytetu
w repo zrodlowym.

**Cicha porazka, przed ktora stoi ten kontrakt:** bieg jednokrokowy, ktory omija limiter.
Wygladalby jak wygoda („to tylko jeden agent"), a znaczylby, ze `atOnce` przestaje byc prawda
o maszynie — czlowiek ustawia trzech, a pracuje piatka, bo dwa `/ask` przeszly bokiem
(niezmiennik 11: „ile naraz" musi znaczyc naraz).

**Read first:**
`src-tauri/src/ipc.rs` (`run_workflow`, `run_request`, `begin_run`, `project_for`),
`src-tauri/src/commands/run.rs` (budowa planu z pliku, `policy_of`, `with_the_task`,
`lay_out_the_run_dir`),
`src-tauri/src/library/agents.rs` (definicja agenta — z niej powstaje krok),
`src-tauri/commands.golden.txt` (zlota lista nazw komend, czytana z obu stron granicy),
`src/sections/run/entry/entry.tsx` (`KNOWN` — lista, ktora wiersz WYKONUJE i POKAZUJE),
`src/sections/run/run-command.ts` (`startFromLine` — wzor rozbioru linii i odmow),
`src/sections/run/io.ts` (krawedz do Rusta),
`AGENTS.md` niezmienniki 6, 11, 13, 16.

## Niezmienniki, ktorych to dotyczy

- **11 — „ile naraz" musi znaczyc naraz.** Bieg jednokrokowy bierze miejsce w tej samej puli.
- **6 — dowod smierci grupy.** Stop dziala na nim tak samo, jak na kazdym innym biegu.
- **13 — jeden fakt, jedno miejsce.** Lista komend wiersza wejscia jest wartoscia (`KNOWN`),
  a nie napisem: komendy nie da sie dopisac do zachety, nie uczac jej wiersza.

## Waskie mandaty na cudze pliki

`src/sections/run/io.ts` i `src-tauri/src/ipc.rs` maja swoich wlascicieli w kolejce (T-41,
T-60). To zadanie startuje **dopiero po ich wyladowaniu** i wtedy dopisuje do nich jedna nowa
krawedz oraz jedna nowa komende. Zadna istniejaca sygnatura nie jest przy tym zmieniana; jesli
okaze sie, ze trzeba — **stoj i zglos**.

`src/sections/commands-wired.test.ts` (kryterium T-27/T-41) wypisuje wiersz dla KAZDEJ
krawedzi sekcji i kazdej nazwy komendy, i asertuje, ze zadna nie zostala bez wiersza. Nowa
komenda `run_agent` z nowa krawedzia w `io.ts` przewraca je z definicji. Wolno ci dopisac tam
**jeden wiersz dla tej jednej komendy** — z wartosciami, ktore ta krawedz naprawde wysyla —
i nic wiecej: zadna asercja nie znika, zaden istniejacy wiersz nie jest przepisywany.

To lustro dziala dokladnie tak, jak ma dzialac: nowa komenda bez wiersza jest komenda, ktorej
nikt nie sprawdzil na granicy. Plik ma wlasciciela w niewyladowanym T-41 i w T-64 — jesli
okaze sie, ze trzeba tknac cokolwiek poza tym jednym wierszem, **stoj i zglos** (AGENTS.md §7).

## Szkielet, bez ktorego `before` nie jest czerwone

Rust: `run_agent` z `todo!()` i wpis w zlotej liscie. TypeScript: `src/sections/run/ask-command.ts`
jako pusty szkielet rzucajacy `throw new Error('not implemented')`.

## Kryteria akceptacji

## AC-1 Jeden agent, jedno zdanie, zwykly bieg
check: cargo test --test it ask_one_agent::
expect: (\d+) passed

Asercje: (a) plan zbudowany z definicji agenta ma **dokladnie jeden** krok; (b) krok bierze
vendora, model i polityke z tej definicji, przez te sama tabele, ktorej uzywa bieg z pliku
(`policy_of`) — nie przez druga kopie; (c) zdanie czlowieka lezy w promptcie kroku; (d) bieg
zaklada katalog i wpis w indeksie jak kazdy inny — plik jest prawda, wiec bieg bez sladu na
dysku jest biegiem, ktorego nie da sie potem wyjasnic (niezmiennik 4); (e) kontrola:
identyfikator agenta, ktorego nie ma w bibliotece, jest **odmowa nazywajaca, gdzie sa agenci**,
nie cichym startem czegokolwiek.

*Slaba asercja:* sprawdzenie samej liczby krokow. Przechodzi dla implementacji, ktora bierze
domyslnego agenta zamiast wskazanego — czyli odpala nie tego, o kogo poprosil czlowiek.
Rozroznia to (b).

## AC-2 `/ask` nie omija puli ani Stopu
check: cargo test --test it ask_respects_the_pool::
expect: (\d+) passed

Asercje: (a) bieg jednokrokowy przechodzi przez ten sam limiter, co bieg z pliku: przy puli
zajetej krok czeka, zamiast ruszyc obok; (b) Stop zatrzymuje go i wraca dopiero z dowodem, ze
grupa nie zyje; (c) drugie `/ask`, zanim pierwsze zeszlo, nie osierocia pierwszego uchwytu —
albo czeka, albo odmawia zdaniem, nigdy nie podmienia po cichu (`begin_run` podmienia
`RunControl`, i to jest dzis prawdziwa pulapka); (d) kontrola: test sam sprawdza, ze pula
w fiksturze naprawde ma sufit mniejszy niz liczba krokow, ktore probuja ruszyc — inaczej
mierzy limiter, ktory nie ma czego ograniczac.

*Slaba asercja:* test wylacznie na (b). Przechodzi dla implementacji, ktora startuje bieg
poza pula — Stop dalej dziala, a `atOnce` przestaje byc prawda. Rozroznia to (a).

## AC-3 Wiersz wejscia zna `/ask`, podpowiada agentow i odmawia po ludzku
check: npx --no-install vitest run src/sections/run/ask-command.test.ts
expect: (\d+) passed

Asercje: (a) `/ask` jest w `KNOWN`, wiec stoi w zachecie i w podpowiedziach — lista, ktora
wiersz wykonuje, i lista, ktora pokazuje, sa jedna wartoscia; (b) po `/ask ` podpowiadaja sie
NAZWY AGENTOW z biblioteki, tak jak po `/run ` podpowiadaja sie workflow; (c) rozbior oddaje
pare (agent, zadanie) i zachowuje zadanie co do znaku, razem z wielokrotnymi spacjami;
(d) `/ask` bez zadania jest odmowa nazywajaca nastepny ruch, nie cichym startem agenta bez
polecenia; (e) kontrola: nazwa agenta, ktorej nie ma, konczy sie odmowa **wymieniajaca
istniejace nazwy** — bo to jedyna odpowiedz, ktora da sie wykonac.

*Slaba asercja:* sprawdzenie samego `KNOWN`. Przechodzi dla komendy, ktora stoi w zachecie
i nie jest rozumiana — czyli dla obietnicy w napisie (niezmiennik 16). Rozroznia to (c).

<!-- OWNS
src-tauri/src/ipc.rs
src-tauri/src/commands/run.rs
src-tauri/commands.golden.txt
src/sections/commands-wired.test.ts
src-tauri/tests/it/main.rs
src-tauri/tests/it/ask_one_agent.rs
src-tauri/tests/it/ask_respects_the_pool.rs
src/sections/run/entry/entry.tsx
src/sections/run/ask-command.ts
src/sections/run/ask-command.test.ts
src/sections/run/io.ts
-->
