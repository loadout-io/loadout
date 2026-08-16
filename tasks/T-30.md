# T-30 — Start dochodzi do silnika, a linie dochodza do okna

Dzis nie ma ani jednej dzialajacej sciezki od kliknięcia do agenta. Zmierzone na wyladowanym
trunku (przeglad zewnetrzny 2026-08-16):

- **Dwa konce pompy nie pasuja typami.** `run_workflow_inner` przyjmuje `mpsc::Sender<Vec<Line>>`
  (`commands/run.rs:174`), a `LineSink::send` bierze pojedyncza `Line` (`ipc.rs:153`). Miedzy nimi
  nie ma mostu. Pompa 16 ms / 2000 linii istnieje, jest zgodna z benchmarkiem z T7 §5.3 i jest
  **martwym kodem wolanym wylacznie z testow**.
- Komendy biegu nie sa zarejestrowane (T-27 rejestruje cztery powierzchnie biblioteki; biegu nie,
  bo bez mostu nie ma czego rejestrowac).

Kierunek naprawy nie jest dowolny. Ksztalt pompy jest **udowodniony pomiarem** (T-07: linia po
linii do `LineSink`, sklejanie po stronie pompy, jedna aktualizacja stanu na paczke). To bieg ma
sie dostosowac do szwu, nie szew do biegu — odwrotnie skasowalibysmy wlasnosc, dla ktorej ta pompa
powstala.

**Read first:**
`src-tauri/src/ipc.rs` (T-07 — `LineSink`, `try_send`, bilans przyjetych i odrzuconych) ·
`src-tauri/src/commands/run.rs` (T-15 — `the_whole_run`, `stop_run_inner`, dowod zejscia grupy) ·
`docs/ARCHITECTURE.md` §4 (przeplyw danych podczas biegu) · `AGENTS.md` niezmienniki 6, 13, 23.

## Kto to robi

- **Agent:** `rust-core` + `react-ui`
- **Druga opinia:** inny vendor niz pisarz (D3).
- **Artefakty biegu:** `runs/T-30/`

## Co to zadanie posiada

- `src-tauri/src/commands/run.rs` — **waski mandat**: zamienic kanal wyjsciowy biegu na szew
  pompy i nic wiecej. Planisty, eskalacji zabijania ani `the_whole_run` nie przepisujemy.
- `src-tauri/src/ipc.rs` — **waski mandat**: skorupy `#[tauri::command]` dla trzech komend biegu
  i dopisanie ich do listy `generate_handler!`, ktora zaklada T-27. Pompy nie dotykamy.
- `src-tauri/commands.golden.txt` — trzy nazwy dopisane do listy z T-27.
- `src/sections/run/io.ts`, `src/sections/workflows/io.ts` — **waski mandat**: wywolanie Start
  i Stop. Reszta cial nalezy do T-27.
- Cztery pliki testow wymienione przy `check:`.
- `src-tauri/tests/runcmd_end_to_end.rs`, `src-tauri/tests/runcmd_snapshot.rs`,
  `src-tauri/tests/runcmd_checkpoint.rs` — **najwezszy mozliwy mandat**: dopasowac WYWOLANIE
  `run_workflow_inner` do nowego typu trzeciego argumentu. Ani jednej asercji nie wolno tu
  usunac, oslabic ani przenumerowac; jesli dopasowanie wymaga czegos wiecej niz zmiany
  konstrukcji kanalu przy wywolaniu, to znaczy, ze zmiana w `run.rs` jest za szeroka —
  zwez `run.rs`, a nie test.

  **Dlaczego to jest w OWNS (§5c).** Zmierzone 2026-08-17 na galezi `task-T-30`:
  `cargo check --all-targets` daje `error[E0308]` w tych trzech plikach, w czterech
  wywolaniach (`runcmd_end_to_end.rs:326`, `runcmd_snapshot.rs:275`, `runcmd_checkpoint.rs:145`
  i `:207`) — wszystkie podaja `mpsc::Sender<Vec<Line>>` tam, gdzie stoi juz `LineSink`.
  Bez tych plikow zadanie jest NIEWYKONALNE: `full-clippy` i `full-test` nie kompiluja sie
  nigdy, wiec zadne kryterium nie ma jak zaswiecic na zielono. Petla `quick` pisarza tego nie
  widziala, bo `--all-targets` sadzi takze `tests/`, a `quick-clippy` chodzi po `--lib`.

## Niezmienniki

- **23 — adaptery po piec linii.** Skorupa `#[tauri::command]` rozpakowuje stan i wola `*_inner`.
  Logika w skorupie to logika, ktorej nie da sie przetestowac bez Tauri.
- **6 — zabijamy grupe i dowodzimy, ze nie zyje.** `stop_run` wraca dopiero po dowodzie, nie po
  wyslaniu sygnalu. Osierocony `claude` pali limit w tle.
- **13 — jeden fakt, jedno miejsce.** Bilans linii (przyjete / odrzucone) liczy pompa; bieg go nie
  powtarza.

## Kryteria akceptacji

## AC-1 Kazda linia biegu dochodzi do pompy, w kolejnosci i bez gubienia
check: cargo test --test run_reaches_the_pump

Pusc bieg na `FakeDriver` emitujacy **300** linii przez prawdziwa sciezke `run_workflow_inner`,
z pompa po drugiej stronie. Asercje: pompa oddala **300** linii; w tej samej kolejnosci; tresci
sa rowne co do znaku. Bilans pompy zgadza sie z liczba wyslanych — przyjete plus odrzucone
rowna sie 300.

*Slaba asercja:* sprawdzenie, ze cokolwiek dotarlo. Przechodzi na moscie, ktory gubi paczki pod
obciazeniem — czyli w jedynym warunku, dla ktorego ta pompa istnieje. Dyskryminuje: **rownosc
licznikow** i porownanie tresci, nie obecnosci.

## AC-2 Trzy komendy biegu sa zarejestrowane, a ich skorupy nie niosa logiki
check: cargo test --test run_commands_registered

`run_workflow`, `stop_run` i `continue_run` sa w `commands.golden.txt` **i** w `generate_handler!`.
Do tego asercja o ksztalcie: cialo kazdej skorupy ma **najwyzej trzy** instrukcje — rozpakowanie
stanu, wywolanie `*_inner`, zwrot. Skorupa z galezia `if` jest logika bez testu, bo `State<'_,
AppState>` nie da sie zbudowac w tescie jednostkowym.

*Slaba asercja:* sprawdzenie samej obecnosci nazw. Przechodzi na skorupie, ktora niesie polowe
planisty. Dyskryminuje: **limit instrukcji** w ciele.

## AC-3 Stop wraca dopiero po dowodzie, ze grupa nie zyje
check: cargo test --test run_stop_waits_for_proof

Odpal bieg z krokiem, ktory nie konczy sie sam, zawolaj `stop_run_inner` i zmierz: funkcja wraca
**po** tym, jak `kill(-pgid, 0)` oddalo `ESRCH`, nie przed. Kontrola dodatnia: ta sama sonda przed
Stopem oddaje sukces — inaczej `ESRCH` znaczy „procesu nigdy nie bylo".

*Slaba asercja:* `assert!(stop.is_ok())`. Przechodzi na implementacji, ktora wysyla SIGTERM
i wraca — a wtedy UI mowi „zatrzymane", kiedy agent dalej pisze i dalej placi (niezmiennik 6).
Dyskryminuje: kolejnosc **dowodu** wzgledem powrotu.

## AC-4 Start w aplikacji wola komende biegu z otwartym workflow
check: npx --no-install vitest run src/sections/run/start-invokes.test.tsx

`@tauri-apps/api/core` podmienione atrapa. Klikniecie Start: **dokladnie jedno** wywolanie
`invoke`, nazwa `run_workflow` z listy zlotej, a argumenty niosa identyfikator otwartego workflow
i limit „ile naraz" ze stanu. Drugie klikniecie w trakcie biegu **nie** wola drugi raz.

*Slaba asercja:* sprawdzenie, ze `invoke` zostalo zawolane. Przechodzi na przycisku, ktory wysyla
pusty workflow albo wysyla go dwa razy. Dyskryminuje: **tresc argumentow** i brak drugiego
wywolania.

## Swiadomie poza zakresem

- **Limit globalny i pauza na rate limit** — T-31.
- **Przekazania w promptcie** — T-32. **Izolacja kroku** — T-33. **Limit czasu** — T-35.
- **Cztery powierzchnie biblioteki** — T-27, ta sama lista zlota.

<!-- OWNS
src-tauri/src/commands/run.rs
src-tauri/src/ipc.rs
src-tauri/commands.golden.txt
src/sections/run/io.ts
src/sections/workflows/io.ts
src-tauri/tests/runcmd_end_to_end.rs
src-tauri/tests/runcmd_snapshot.rs
src-tauri/tests/runcmd_checkpoint.rs
src-tauri/tests/run_reaches_the_pump.rs
src-tauri/tests/run_commands_registered.rs
src-tauri/tests/run_stop_waits_for_proof.rs
src/sections/run/start-invokes.test.tsx
-->
