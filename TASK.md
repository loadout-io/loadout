# T-69 — Jedna droga do biegu, jeden uchwyt: zaden start nie osieroca poprzednika

Znalezisko drugiej opinii przy T-62, powaznosc finansowa. T-62 dolozylo `begin_a_run`
z warunkiem `is_working()`, wiec drugie `/ask` nie potrafi juz podmienic uchwytu pierwszego.
**Stare `begin_run` zostalo bez warunku** — a wola je `run_workflow`, czyli przycisk Start,
komenda `/run` w wierszu wejscia i zielony `Run` w edytorze workflow.

Sciezka awarii, przesledzona przez recenzenta koniec-koniec (`entry.tsx` -> `startFromLine` ->
`launchRun` -> `start()` -> `invoke('run_workflow')` -> `begin_run`): odpalasz agenta przez
`/ask`, potem startujesz workflow. `AppState.live` zostaje podmieniony, Stop czyta `self.live`
i dosiega **wylacznie nowego** biegu, a agent z `/ask` pracuje dalej i dalej placi. Z okna nie
ma juz zadnej drogi, zeby go zatrzymac.

**Dlaczego to jest niezmiennik 6, a nie niedogodnosc.** AGENTS.md nazywa osierocony proces
agenta bledem **finansowym**, nie higienicznym: „osierocony `claude` pali limit w tle". Tu nie
ma nawet dowodu smierci grupy, bo nie ma komu go zazadac — uchwyt, ktory jako jedyny wiedzial
o tamtym biegu, zostal nadpisany.

**Dlaczego to osobne zadanie, a nie poprawka w T-62.** Bo kryterium T-62 AC-2 pilnuje wylacznie
przypadku „drugie `/ask`", a naprawa bez wyroczni jest mechanizmem, ktory zgnije. Planista rundy
naprawczej ma prawo taka naprawe odrzucic i **ma racje**, kiedy to robi. Wyrocznia nalezy do
zadania, ktore ja opisze — czyli do tego.

**Cicha porazka, przed ktora stoi ten kontrakt:** naprawa jednej strony. Warunek dopisany tylko
do `begin_run` zamyka `/ask` -> `/run`, a zostawia `/run` -> `/run` (dwa Starty pod rzad) i wciaz
pozwala na `/run` -> `/ask`, jesli tamta droga uzywa innej funkcji. Kryterium przechodzi wiec
przez **wszystkie pary drog**, nie przez jedna.

**Read first:**
`src-tauri/src/ipc.rs` (`AppState.live`, `begin_run`, `begin_a_run` z T-62, `stop_run`),
`src-tauri/src/commands/run.rs` (`RunControl`, `is_working`, `settle`),
`src/sections/run/io.ts` (zapadka `going` — dziala tylko w JEDNYM oknie i tylko dla `run_workflow`),
`src/sections/run/launch.ts` i `run-command.ts` (trzy drogi do `run_workflow`),
`AGENTS.md` niezmienniki 6, 7, 11.

## Niezmienniki, ktorych to dotyczy

- **6 — zabijamy grupe i dowodzimy, ze nie zyje.** Uchwyt nadpisany to grupa, ktorej nikt nie
  ubije, bo nikt o niej nie wie.
- **11 — „ile naraz" musi znaczyc naraz.** Bieg poza uchwytem jest biegiem poza pula.
- **7 — anulowanie jest wartoscia.** Odmowa startu przy zajetym uchwycie jest zdaniem dla
  czlowieka, nie bledem i nie cisza.

## Kryteria akceptacji

## AC-1 Zaden start nie podmienia uchwytu zywego biegu
check: cargo test --test it no_start_orphans_the_previous::
expect: (\d+) passed

Asercje przez **wszystkie pary drog** startu (`run_workflow` i `run_agent`, w obu kolejnosciach,
plus kazda z nich sama ze soba): (a) drugi start przy zywym pierwszym **nie podmienia** uchwytu;
(b) konczy sie **odmowa nazywajaca nastepny ruch** („zatrzymaj to, co idzie, albo poczekaj"),
nie cisza i nie panika; (c) po odmowie `stop_run` dalej dosiega PIERWSZEGO biegu — to jest cala
tresc tej naprawy; (d) kiedy pierwszy bieg zszedl (`settle`), drugi start przechodzi normalnie,
bo blokada na zawsze byla by gorsza od wady; (e) kontrola przeciw pustemu przejsciu: test
sprawdza, ze w scenariuszu bez zadnego zywego biegu KAZDA z drog startuje.

*Slaba asercja:* test wylacznie na parze `/ask` -> `/run`. Przechodzi dla warunku dopisanego
do jednej funkcji i zostawia dwie pozostale pary otwarte — a wystarczy jedna, zeby agent placil
w tle. Rozroznia to (a) razem z macierza par z naglowka.

## AC-2 Odmowa dochodzi do okna, a nie tylko do dziennika
check: npx --no-install vitest run src/sections/run/second-start-says-why.test.ts
expect: (\d+) passed

Asercje: (a) krawedz startu oddaje zdanie odmowy, kiedy Rust odmowil z powodu zywego biegu —
nie `null` i nie wyjatek pozerany po drodze; (b) zdanie nazywa nastepny ruch (Stop albo
czekanie), bo odmowa bez wyjscia zostawia czlowieka tam, gdzie byl (DESIGN §8); (c) zdanie nie
jest enumem z drutu ani napisem od vendora (niezmiennik 14); (d) kontrola: przy powodzeniu ta
sama krawedz oddaje `null` — inaczej test przechodzi dla implementacji, ktora odmawia zawsze.

*Slaba asercja:* sprawdzenie, ze cokolwiek wrocilo. Przechodzi dla surowego napisu z Rusta
wypchnietego na ekran. Rozrozniaja to (b) i (c).

<!-- OWNS
src-tauri/src/ipc.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/no_start_orphans_the_previous.rs
src/sections/run/io.ts
src/sections/run/second-start-says-why.test.ts
-->
