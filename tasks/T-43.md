# T-43 — Powiedz, czego chcesz, a twoj agent napisze umiejetnosc; ty ja czytasz przed zapisem

**Loadout ma dwa sterowniki agentow, zywy nadzor procesow i dowod smierci grupy — i ani jednej
drogi, ktora zamienia jedno zdanie czlowieka w tekst od modelu.** Kazde uruchomienie agenta w tej
aplikacji przechodzi dzis przez plik workflow, katalog biegu, `Dag` i planiste. Umiejetnosc, ktorej
czlowiek chce, nie jest biegiem: to jedna tura, jeden prompt, jedna odpowiedz.

Zmierzone 2026-08-19 na wyladowanym trunku:

```
engine/drivers/mod.rs:288-305   trait AgentDriver { start(spec, tx) -> Box<dyn AgentHandle> }
engine/drivers/mod.rs:345-393   AgentHandle { send, wait -> Outcome, cancel -> GroupProof, close }
engine/drivers/mod.rs:209-230   Outcome { text: String, ... }   // dokladnie koncowa wypowiedz
commands/run.rs:1495-1570       jedyny wolacz `start` w produkcji — wewnatrz Live::run_agent
```

Czyli mechanizm dostarczenia ISTNIEJE w calosci i jest publiczny. Brakuje **jednej funkcji
w warstwie komend**, ktora sklada `RunSpec`, drenuje kanal zdarzen, czeka na `Outcome` i oddaje
`Outcome.text`. Reszta tego zadania to szew do okna i uczciwosc wobec trzech niezmiennikow, ktore
przy jednorazowym wywolaniu lamie sie najlatwiej: 6 (dowod smierci), 7 (anulowanie jest wartoscia)
i 10 (limit czasu nie jest `tokio::time::timeout`).

## Dlaczego to nie jest „odpal agenta i wez wyjscie"

**Nie wolno zrobic tego przez podstawienie jednokrokowego workflow.** Synteza pliku workflow
w Ruscie, zeby zawolac planiste, jest etapem biegu zaszytym w kodzie — czyli dokladnie tym, co
zabrania niezmiennik 27 i decyzja D7. Droga przez `AgentDriver::start` jest od tego wolna
z definicji: warstwa sterownikow nie zna slowa „krok".

**Trzy rzeczy urwa sie po cichu, jesli skopiowac naiwnie:**

1. **Kanal.** `start` bierze `mpsc::Sender<DecodedEvent>` i pcha w niego zdarzenia. Nieodbierany
   kanal staje na 256 pozycjach (`EVENT_QUEUE`, run.rs:188) i tura nigdy sie nie konczy. Draft nie
   potrzebuje ani jednej z tych linii na ekranie, ale MUSI je odebrac.
2. **Limit czasu.** `tokio::time::timeout(limit, handle.wait())` ubija zadanie Rusta i zostawia
   zywego `claude`, ktory dalej pali limit dostawcy (niezmiennik 10, blad finansowy z niezmiennika 6).
   Wzor jest w `Live::one_turn` (run.rs:1620-1690): `select!` z `biased`, a `Overdue` i `Stopped`
   ida przez `handle.cancel().await` i **czytaja** `GroupProof`.
3. **Uchwyt anulowania.** `AppState.live` jest PODMIENIANY przy kazdym Start (`begin_run`,
   ipc.rs:452-459). Draft trzymajacy sie `deps().control` traci swoj token w chwili, w ktorej
   czlowiek uruchomi bieg w innej karcie — i `Stop` na drafcie przestaje cokolwiek robic.

**I jedna rzecz, ktorej NIE robimy, zeby nie klamac o „ile naraz".** Limit rownoleglosci jest dzis
**per bieg**, nie globalny: `run_workflow_inner` robi sobie wlasny `Limiter::new(how_many_at_once)`
(run.rs:232-237), a `run_workflow_with_slots(..., slots)` — funkcja przyjmujaca wspolna pule — **nie
ma w produkcji ani jednego wolajacego**. Wiec draft nie ma z czego wziac slotu i udawanie, ze bierze,
byloby czwartym miejscem, w ktorym ta liczba nie znaczy tego, co mowi. Zamiast tego draft ma wlasna,
jawna granice: **jeden naraz** (AC-2 d). Brakujaca wspolna pula jest znaleziskiem dla czlowieka,
zapisanym na koncu tego pliku.

Czyli zadanie polega na **jednej turze poza grafem, z uczciwym wyjsciem, anulowaniem i limitem**,
nie na dopisaniu przycisku „napisz mi to".

**Read first:**
`src-tauri/src/engine/drivers/mod.rs:288-393` (oba traity, `Outcome`, `RunSpec`, `Policy`) ·
`src-tauri/src/commands/run.rs:1495-1570` (jak sklada sie `RunSpec` i kto odbiera kanal) ·
`src-tauri/src/commands/run.rs:1620-1690` (`Ended`, `biased`, `Overdue` -> `cancel` -> `GroupProof`) ·
`src-tauri/src/commands/run.rs:856-900` (`plan_agent` — skad biora sie model, instrukcje i dial) ·
`src-tauri/src/commands/agents.rs:85` (`list_agents_inner` — publiczna lista agentow z dysku) ·
`src-tauri/src/library/agents.rs:258` (`resolve`) · `src-tauri/src/engine/drivers/absent.rs`
(vendor, ktorego nie ma) · `src-tauri/tests/it/runcmd_parallel.rs` (wzor atrapy `AgentDriver`
wewnatrz pliku testu) · `tasks/T-42.md` (droga, w ktora draft wpada) ·
`AGENTS.md` niezmienniki 5, 6, 7, 8, 9, 10, 16, 27, 28.

## Kto to robi

- **Agent:** `rust-engine` na warstwie komend, potem `react-ui` na panelu — jeden worktree,
  dwa kroki, jedna bramka.
- **Druga opinia:** inny vendor niz pisarz (D3); recenzentowi powiedz wprost, zeby szukal
  `tokio::time::timeout` wokol tury i uchwytu anulowania wzietego z `deps().control`.
- **Artefakty biegu:** `runs/T-43/`

## Zalezy od

**T-42.** Draft nie zapisuje niczego sam: oddaje trzy pola, ktore wypelniaja formularz z T-42,
i dopiero zapis z tamtej strony sklada plik, skanuje go i odklada kopie kanoniczna. Zbudowany
wczesniej musialby miec wlasna droge zapisu — czyli drugi potok obok tego, ktory T-42 wlasnie
otwiera (niezmiennik 23). Kolejnosc ladowania: T-42, potem T-43.

## Co to zadanie posiada

- `src-tauri/src/commands/skills.rs` — `draft_skill_inner`, `enum DraftOutcome { Wrote(..),
  Cancelled }`, prompt jako **dane** w tej warstwie (precedens: `HANDOFF_INDEX_OPENS`
  i `with_the_task` w `commands/run.rs`), drenaz kanalu, `select!` z `biased`, wybor agenta przez
  `commands::agents::list_agents_inner` + `library::agents::resolve`.
- `src-tauri/src/ipc.rs` — jedno nowe pole w `AppState` na token anulowania draftu (obok `live`,
  z komentarzem o niezmienniku 8 na samym polu), dwie nowe skorupy **`async`** i dwa wiersze
  w `generate_handler!`. Skorupy skills sa dzis synchroniczne i to jest jawnie zapisany dlug
  (ipc.rs:588-593): synchroniczna skorupa zamrozilaby okno na czas pisania przez model, czyli
  dziesiatki sekund, a nie 20 ms.
- `src-tauri/commands.golden.txt` — **waski mandat**: dwie nowe nazwy, alfabetycznie. Ani jednej
  istniejacej nie wolno usunac ani przestawic.
- `src/sections/skills/index.tsx`, `src/sections/skills/io.ts`, `src/state/skills.ts` — trzecie
  wejscie w tym samym panelu, stan „pisze" i droga zatrzymania.
- `src/sections/commands-wired.test.ts` — **waski mandat**: DWA nowe wiersze w tabeli `WIRES`.
  Ani jednego istniejacego nie wolno zmienic ani usunac.
- `src-tauri/tests/it/main.rs` — **waski mandat**: ten plik masz w OWNS WYLACZNIE po to, zeby
  dopisac dwa wiersze `mod skills_draft_asks_an_agent;` i `mod skills_draft_stops_dead;`
  w porzadku alfabetycznym. Zadnej innej zmiany; bez nich pliki kompiluja sie do niczego,
  a zestaw wyglada jak przeszly.
- 3 pliki testow wymienione przy `check:`.

**Czego to zadanie NIE dotyka:** `src-tauri/src/engine/**` — ani jednej linii. Trait wystarcza
taki, jaki jest, a `Policy::ReadOnly` juz istnieje. Nie dotyka tez `commands/run.rs` (planista
i bieg zostaja czyje sa), `src-tauri/src/skills/**` (T-18, T-19) ani `src/sections/skills/mounted.test.tsx`
(T-26) — panel po otwarciu nie ma prawa wypisac nazwy zadnego vendora, bo tamten plik zamraza
brak tych nazw w markupie i ma do tego zmierzony powod.

## Niezmienniki

- **10 — `tokio::time::timeout` wokol kroku anuluje zadanie Rusta, nie proces.** *Jak sie lamie po
  cichu:* `timeout(limit, handle.wait())` zwraca `Err(Elapsed)`, okno pokazuje „nie udalo sie",
  a `claude` pisze dalej i placi za to czlowiek.
- **6 — zabijamy grupe i dowodzimy, ze nie zyje.** `handle.cancel()` oddaje `GroupProof` wlasnie po
  to, zeby `Ok(())` nie moglo znaczyc „wyslalem sygnal". `GroupProof::Alive` ma dac zdanie
  o tym, ze proces moze dalej dzialac — nigdy ciszy.
- **7 — anulowanie jest wartoscia, nie bledem.** `DraftOutcome::Cancelled`, nigdy
  `Err(Cancelled)`.
- **9 — prompt wylacznie przez stdin.** Prompt jedzie w `RunSpec.prompt`, a `ClaudeDriver` wklada
  go w koperte na stdin. Ani jednego znaku pytania czlowieka w argv.
- **8 — `std::sync::Mutex` nigdy przez `await`.** Nowe pole w `AppState` jest wlasnie takim
  mutexem: klon tokena bierz i oddawaj w jednym wyrazeniu, przed pierwszym `await`. Udokumentuj
  to na polu.
- **27 — zaden etap biegu nie jest zaszyty w Ruscie.** *Jak sie lamie po cichu:* draft zrobiony
  przez zlozenie jednokrokowego workflow i wolanie planisty. Wtedy „napisz mi umiejetnosc" jest
  etapem w kodzie, ktorego nie da sie wylaczyc konfiguracja.
- **28 — najpierw mechanizm, potem prompt.** Poprawnosc draftu **nie** stoi na dokladnosci
  instrukcji dla modelu: tekst i tak przechodzi `ingest::from_folder` i `place::validate_strict`
  po drodze z T-42. Prompt ma byc krotki, a sprawdzanie maszynowe.

## Kryteria akceptacji

**Jak zaczerwienic to poprawnie.** `clippy::todo` jest `deny`, wiec sygnatury zwracaja trywialnie
zla wartosc (`DraftOutcome::Cancelled`, pusty `String`), nigdy `todo!()`. Atrapa `AgentDriver`
mieszka **w pliku testu**, wzor gotowy w `src-tauri/tests/it/runcmd_parallel.rs:425-510` (impl
traitu, `mpsc::Sender<DecodedEvent>`, `AgentEvent::…::into()`). Nie uzywaj `engine::drivers::fake`
— to dubler PLANISTY i nie implementuje `AgentDriver`. Kazdy plik testu zaczyna sie od
`#![allow(clippy::unwrap_used, clippy::expect_used)]` z powodem. Testy z prawdziwym `claude`
w tym zadaniu **nie wystepuja**: kryterium wymagajace sieci czerwieni sie od cudzych awarii, a co
robi zywy proces, dowodza `claude_completion` i `claude_cancel_escalation` (T-04). Po stronie okna:
`renderToStaticMarkup`, magazyn zasiany `setState`, granica `@tauri-apps/api/core` podmieniona
atrapa; kazdy importowany modul musi istniec przed `./verify.sh before`.

## AC-1 Jedno pytanie dochodzi do sterownika wybranego agenta, a dial nie idzie w gore
check: cargo test --test it skills_draft_asks_an_agent::
expect: (\d+) passed

Fikstura: biblioteka w `tempfile::TempDir` z dwoma zapisanymi agentami — jeden `work-freely`
z modelem `opus`, drugi `look-only`. Atrapa `AgentDriver` zapamietuje `RunSpec`, ktory dostala,
i oddaje `Outcome { text: "---\nname: pr-review\n…", .. }`. Fabryka sterownikow oddaje atrape dla
`claude` i `Absent::new("codex", "T-10")` dla `codex`.

Asercje: (a) draft wola `start` **dokladnie raz**, a `RunSpec.prompt` niesie zdanie, ktore napisal
czlowiek — porownane z tym, co podano wywolaniu, nie z literalem; (b) `RunSpec.policy` to
`Policy::ReadOnly` dla OBU agentow, takze dla `work-freely` — tekst wraca strumieniem, wiec do
pisania po dysku nie ma powodu, a dial wolno tylko obnizyc (D6: „przelotka nie omija diala
bezpieczenstwa"); (c) `model` i `system_append` sa **te z definicji wybranego agenta**, wziete
z `resolve`, a nie wpisane w draft — porownane z `Resolved`, nie z napisem; (d) zwrocone trzy pola
to `name`/`description`/`body` przeczytane z tekstu modelu tym samym rdzeniem, co przy linku
(`ingest::from_folder`), a nie wlasnym parserem front-mattera; (e) vendor, ktorego nie ma
(`codex` -> `Absent`), jest zdaniem dla czlowieka, nie panika i nie cisza.

*Slaba wersja:* asercja, ze funkcja zwrocila niepusty `String`. Przechodzi implementacja, ktora
sklada `RunSpec` z wlasnym modelem, z `Policy::Unrestricted` i z promptem w argv. Rozstrzyga
porownanie CALEGO `RunSpec` z tym, co daje `resolve` na zapisanej definicji, plus asercja o polu
`policy`.

## AC-2 Zatrzymanie i limit czasu ubijaja grupe, dowodza tego, i sa wartoscia
check: cargo test --test it skills_draft_stops_dead::
expect: (\d+) passed

Fikstura: atrapa, ktorej `wait()` **nigdy nie wraca**, i ktora zapisuje, czy zawolano na niej
`cancel()` oraz co oddala jako `GroupProof`. Drugi wariant atrapy oddaje `GroupProof::Alive`.

Asercje: (a) zatrzymanie w trakcie pisania daje `DraftOutcome::Cancelled` — wartosc, nie `Err` —
i na uchwycie **zostalo zawolane `cancel()`**; (b) draft, ktory przekroczyl swoj limit czasu,
konczy sie ta sama droga: `cancel()` zawolane, `GroupProof` odczytany; asercja o samym czasie nie
wystarcza, bo `tokio::time::timeout` wokol `wait()` konczy sie tak samo szybko i **nie wola
`cancel()`** — to jest jedyna rzecz, ktora te dwie implementacje rozroznia; (c) `GroupProof::Alive`
daje zdanie mowiace, ze proces moze dalej dzialac, i nie melduje sukcesu; (d) drugie pytanie
zadane w chwili, gdy pierwsze jeszcze pisze, jest odmowa ze zdaniem, a pierwsze zostaje nietkniete
— jeden draft naraz, bo wspolnej puli miejsc w produkcji nie ma; (e) po zakonczeniu draftu, jakkolwiek
sie skonczyl, w bibliotece nie zostaje ani jeden katalog roboczy draftu.

*Slaba wersja:* asercja, ze funkcja wrocila w mniej niz N sekund. Przechodzi na `tokio::time::timeout`,
czyli na implementacji, ktora zostawia zywego `claude` palacego limit dostawcy — to jest dokladnie
niezmiennik 10 i strata pieniedzy, nie estetyka. Rozstrzyga: zapis wywolania `cancel()` na uchwycie.

## AC-3 Draft wpada w trzy pytania, czlowiek go czyta i poprawia PRZED zapisem
check: npx --no-install vitest run src/sections/skills/the-agent-writes-it.test.tsx
expect: (\d+) passed

Fikstura: `renderToStaticMarkup` na calym `<SkillsScreen>`, magazyn zasiany `setState` (lista
zapisanych agentow, stan „pisze", gotowy draft), granica IPC podmieniona atrapa liczaca wywolania.

Asercje: (a) panel niesie trzecie wejscie: pole na zdanie „czego chcesz" i wybor **spisanego
agenta** — pozycje z magazynu, nie nazwy vendorow wpisane w kod (`mounted.test.tsx` zamraza brak
nazw vendorow w markupie tej sekcji i ma do tego zmierzony powod); (b) oddanie tego pytania wysyla
do Rusta wywolanie nazwa ze `src-tauri/commands.golden.txt` — czytana z tego pliku — a zdanie
czlowieka i wybrany agent sa w argumentach; (c) w stanie „pisze" na ekranie stoi zdanie o tym, co
sie dzieje, ORAZ kontrolka zatrzymania, ktora **tez opuszcza okno** drugim wywolaniem z tej samej
listy; kontrolki „napisz mi to" w tym stanie nie ma (podmiana kontrolki, jak `Start`/`Stop`
w `run/start.tsx:263-276`), a animacji nie ma zadnej (DESIGN §7: jedyna w aplikacji to kropka
zywej karty); (d) gotowy draft stoi w trzech polach formularza z T-42, edytowalny, i **nic jeszcze
nie jest zapisane** — zapis idzie ta sama droga co tekst wpisany reka, wiec tekst poprawiony po
drafcie zostaje przeskanowany jeszcze raz; (e) odmowa (zaden agent nie jest zapisany, vendor
niedostepny, model oddal cos, co nie jest umiejetnoscia) stawia zdanie na ekranie i **zostawia
tekst czlowieka w polu**.

*Slaba wersja:* asercja, ze w markupie jest przycisk z napisem o pisaniu. Przechodzi na przycisku
bez handlera i na stanie „pisze", ktorego nie da sie zatrzymac — czyli na kontrolce, ktora klamie
(niezmiennik 16), i to w miejscu, gdzie klamstwo kosztuje pieniadze. Rozstrzyga: policzenie DWOCH
wywolan na atrapie granicy (pisz, zatrzymaj) i asercja o nieobecnosci kontrolki „napisz" w stanie
pisania.

## Swiadomie poza zakresem

- **Wspolna pula miejsc dla wszystkiego, co odpala agenta.** Poza tym zadaniem; opisane nizej jako
  znalezisko.
- **Linie od agenta na ekranie sekcji Skills.** Draft drenuje kanal i porzuca zdarzenia. Widok
  strumienia ma jednego wlasciciela (sekcja Praca) i drugi zywy region na ten sam fakt lamalby
  niezmiennik 13.
- **Kolejka draftow.** Jeden naraz, odmowa dla drugiego. Kolejka jest stanem, ktorego nikt nie
  zamowil.
- **Codex jako autor draftu.** `codex.rs` nie istnieje, `Absent` odmawia z nazwa zadania T-10.
  AC-1 (e) pilnuje wylacznie tego, zeby odmowa byla zdaniem.

**Znalezisko, ktorego to zadanie NIE naprawia (AGENTS.md §7).** Limit „ile naraz" nie jest dzis
globalny w produkcji. `run_workflow_with_slots(deps, request, lines, slots)` — jedyna funkcja
przyjmujaca wspolna pule — nie ma ani jednego wolajacego poza testami, a `run_workflow_inner`
zaklada wlasny `Limiter` per bieg (run.rs:232-237). Trzy karty po trzech agentach to dziewieciu
agentow przy suwaku ustawionym na trzy. `workspace::Registry::slots()` jest przewidzianym zrodlem
wspolnej puli i jest konstruowany wylacznie w `src-tauri/tests/it/**`. To jest ten sam ksztalt
defektu, co linie, ktore nie dochodzily do okna przed T-38: mechanizm wyladowal, ma testy, nikt go
nie zawolal. Naprawa dotyka `ipc.rs`, `commands/run.rs` i `workspace.rs` naraz i nalezy do
osobnego zadania.

<!-- OWNS
src-tauri/src/commands/skills.rs
src-tauri/src/ipc.rs
src-tauri/commands.golden.txt
src-tauri/tests/it/skills_draft_asks_an_agent.rs
src-tauri/tests/it/skills_draft_stops_dead.rs
src-tauri/tests/it/main.rs
src/sections/skills/index.tsx
src/sections/skills/io.ts
src/state/skills.ts
src/sections/commands-wired.test.ts
src/sections/skills/the-agent-writes-it.test.tsx
-->
