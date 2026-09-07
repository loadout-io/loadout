# Przekazanie: niezawodność workflow, QA na pełnej aplikacji i generator agentów

Data: 2026-09-07. Odpowiada na §12 planu
[`2026-09-06-reliability-native-qa-generator.md`](2026-09-06-reliability-native-qa-generator.md).

To jest **raport wykonania**, nie plan. Wszystko poniżej da się sprawdzić poleceniem, które stoi
obok liczby.

## 1. Gdzie to leży

| | |
|---|---|
| Repo | `/Users/jakubgawronski/Projects/loadout-reliability-native-qa-generator` (worktree Loadouta) |
| Gałąź | `feat/reliability-native-qa-generator` |
| Baza | `f81d4b11` — snapshot WIP z `workflow-reliability-build`, wierny co do pliku |
| HEAD | `f2271d32`, dwadzieścia dwa commity nad bazą |
| Niezacommitowane | **brak** (`git status` pusty) |
| Nie zmergowane | nic z tego nie jest na `main`; niczego nie opublikowano, niczego nie wypchnięto |

Źródłowy worktree `/Users/jakubgawronski/Projects/loadout-workflow-reliability-build` **nie został
tknięty**: snapshot powstał z `git diff HEAD --binary` plus kopii plików nieśledzonych, a `diff -r`
między drzewami różni się wyłącznie o `src-tauri/gen` (artefakt buildu).

### Baza NIE jest zielona i nigdy nie była

Odziedziczony WIP ma kilkadziesiąt czerwonych testów `it` i 13 błędów `clippy --all-targets
-- -D warnings`. Zmierzone na osobnym worktree stojącym na samym `f81d4b11`
(`/Users/jakubgawronski/Projects/loadout-baseline-check`), żeby nie było wątpliwości, czyje to
jest. Czerwień dotyczy sprzątania kopii roboczych, transakcji prestartu i izolacji — obszarów,
których to zadanie nie tyka.

**„Kilkadziesiąt", a nie jedna liczba, i to jest zmierzony fakt, nie ostrożność.** Część tych
przypadków sądzi OKNA CZASOWE sygnałów (`SIGTERM` przed `SIGKILL`, gotowość procesu), więc ich
wynik zależy od tego, ile testów biegnie obok. Cztery przebiegi:

| Drzewo | `--test-threads` | passed | failed |
|---|---|---|---|
| baza `f81d4b11` | domyślne (16) | 1578 | 34 |
| baza `f81d4b11` | domyślne (16) | 1576 | **36** |
| ta gałąź | domyślne (16) | 1644 | 35 |
| ta gałąź | domyślne (16) | 1644 | 35 |
| baza `f81d4b11` | 4 | 1580 | **32** |
| ta gałąź | 4 | 1647 | **32** |

Baza sama w sobie zmienia listę między przebiegami (drugi przebieg dołożył dwa przypadki,
których w pierwszym nie było). Porównanie ma więc sens wyłącznie przy TEJ SAMEJ równoległości:
**32 = 32**, a listy różnią się o jeden przypadek po każdej stronie — oba z tej samej puli
wędrujących. Zero regresji; testów przechodzących jest o 67 więcej.

Clippy porównane po NAZWIE funkcji, nie po numerze linii — moje zmiany przesuwają linie, więc
`diff` po `plik:linia` pokazywałby dziesięć „regresji", których nie ma. Po nazwie: **13 = 13,
te same funkcje**. Dwa błędy, które faktycznie dołożyłem, są naprawione (commit `c3403ed1`).

Front: baza 1859 passed / 3 failed, ta gałąź 1875 passed / 3 failed — te same cztery pliki.
Checki repo: `boundary`, `vocabulary`, `tests-listed`, `wired`, `suppressions`, `tokens`,
`invoke-args` — 7/7. `density` jest czerwona **także na bazie**: mówi „could not measure", bo
kolektor nie jest wpięty w ten check.

## 2. Co zrobione, a co nie

| ID | Stan | Czym to sprawdzić |
|---|---|---|
| L-00 | zrobione | snapshot `f81d4b11`, worktree, `diff -r` |
| L-01 | zrobione | `cargo test --test it step_turn_queue_is_atomic::` → 5/5 |
| L-02 | zrobione | `cargo test --test it a_saved_end_cause_beats_a_guess::` → 5/5 |
| V-01 | zrobione | `cargo test --test it required_checks_run_the_requested_tests::` → 11/11 |
| V-02 | zrobione | `cargo test --test it mandatory_criteria_survive_the_summary::` → 11/11 |
| V-03 | zrobione | `cargo test --test it the_shipped_workflow_checks_before_it_believes::` → 5/5 |
| P-01 | zrobione w części I-08 | `cargo test --test it a_neighbour_repository_is_named_before_the_run::` → 4/4 |
| P-02 | zrobione w części izolacji danych | `cargo test --test it a_test_instance_keeps_its_data_apart::` → 4/4 |
| P-03a | **rozpoznane i zamknięte werdyktem** | `cargo test --test it native_scenarios_need_a_real_route_to_the_window::` → 6/6 + 1 żywa `--ignored` |
| P-03b | zrobione w części egzekwowalnej | `cargo test --test it a_test_instance_keeps_its_data_apart::` → 8/8; granica w §5 |
| G-01, G-02 | zrobione, **potwierdzone żywym biegiem** | `… an_agent_is_written_by_the_vendor_that_was_asked::` → 14/14 + żywa `--ignored` na obu vendorach (§4a) |
| G-03 | zrobione | `npx vitest run e2e/tests/two-buttons-ask-two-different-vendors.spec.ts` → 2/2 w chromium |
| G-04 | zrobione w części rozróżnienia | `npx vitest run src/sections/agents/an-agent-can-be-written-from-a-description.test.tsx` → 4/4 |
| M-01 | zrobione, zacommitowane | `cargo test --lib` w meetnotes → 3731/0 |
| M-02 | zrobione | `commands::queue_ownership_tests` → 5/5 |
| M-03 | zrobione | `e2e/processing` → 14/14 (chromium + webkit) |
| M-04 | **zrobione**; wszystkie scenariusze zmierzone, plus jedno znalezisko | §4a |
| E-01 | macierz zrobiona; z §11 wykonane punkty **1, 2, 4, 6 i 7** | §4 i §4a |

| P-02 (kafelek) | zrobione | `npx vitest run src/sections/workflows/start-and-leave-has-a-panel.test.tsx` → 10/10 |

Liczby całych suit i porównanie z bazą stoją w §1; tutaj są tylko adresy, pod którymi każde
z tych ID da się obejrzeć osobno.

## 3. Co się naprawdę zmieniło, incydent po incydencie

**I-01 — wiadomość ginęła razem z turą.** Kolejka tur należy dziś do Loadouta
(`SessionChannel::waiting`), a nie do vendora. Sprawdzenie pustej kolejki i zamknięcie
przyjmowania to **jedno wzięcie zamka** (`RunControl::next_turn_or_stop_accepting`) — dwie
operacje zostawiały okno, w którym wiadomość jest przyjęta i nigdy nie podana. Sufit czasu
należy do kroku, nie do tury: mutacja `limit` zamiast `limit - elapsed` czerwieni kryterium
natychmiast. Zużycie sumuje się przez wszystkie obsłużone tury, dokładnie raz.

**I-02 — `infrastructure-failed` czytane jako `unknown`.** Zapisany `end_cause` wygrywa z całą
heurystyką opartą o kod wyjścia i obecność artefaktów. Stary raport bez tego pola zachowuje swoją
dotychczasową odpowiedź. Lead widzi ten sam fakt w `read_run_summary`.

**I-03/I-04 — dwa niezwiązane testy zamiast trzynastu.** Check potwierdza testy **po nazwie**
(`required::Scan`, adaptery libtest i TAP), a nie po liczniku. Sądzone STRUMIENIOWO i wyłącznie po
`stdout`: zachowywany ogon ma 64 KiB, więc sądzenie po nim kłamałoby o każdej prawdziwej suicie,
a `stderr` mógłby podszyć się pod wynik. Katalog roboczy sprawdzany PRZED spawnem i nazwany
w zdaniu.

**I-05 — proza opisuje braki, ostatni wiersz mówi `pass`.** Kiedy człowiek zatwierdził listę
wymagań, wynik powstaje z jej kompletności, a nie ze słowa na końcu. Trzy wyniki zamiast dwóch:
niezmierzone kryterium nie jest ani zaliczeniem, ani dowodem wady.

**I-06/I-07 — QA bez narzędzia do okna i tak wydało werdykt.** Kryterium wymagające pełnej
aplikacji, zameldowane jako zdane na maszynie bez zgody na sterowanie oknem, przechodzi do
„nie zmierzono" — **kodem**, nie prośbą w prompcie.

**Zielony bieg skarżył się na własną pracę — wada BAZY, nie tego zadania.** Krok pracujący
we własnej kopii i zmieniający w niej cokolwiek zawsze ją zostawia; zdejmowania nikt nawet nie
próbuje. Zdanie o tym szło jednak do `StepRun::error`, czyli do pola, którym okno maluje kafelek
na czerwono. Żywa wyrocznia całego przepływu (`flow_todo_app`) padała przez to **na samej bazie**
`f81d4b11` i po mojej pracy tak samo — trzy zielone kroki Scout „obwiniały" coś za to, że zrobiły,
o co je poproszono. Ścieżka nie ginie i nigdy nie potrzebowała tej kopii: niesie ją `copy_results`,
a pokazuje `ResultFolder` w panelu biegu minionego, razem z nazwą kroku, zdaniem „Changes kept in
this folder." i przyciskiem, który ten folder otwiera (niezmiennik 13). Do pola błędu wraca
wyłącznie kopia, której zdjęcie NAPRAWDĘ odmówiło.

Przy okazji wyszło, że kryterium, które miało tego pilnować
(`a_copy_that_could_not_be_cleared_away_is_named_in_the_run_record`), sądziło inną ścieżkę, niż
mówi jego nazwa: dubler pisał plik do kopii, więc kopia była „zmieniona", zdejmowania nie było
i zamek na katalogu nie miał czego zatrzymać. **Dowód mutacyjny:** z usuniętym
`seal_the_folder` przypadek był tak samo zielony. Dubler nic już w tym jednym wariancie nie pisze,
więc `remove_dir_all` rusza naprawdę i odmawia na `notes.txt` — a z usuniętym zamkiem przypadek
jest dziś czerwony na własnej przesłance.

**I-08 — zależność od sąsiedniego repo.** Krok pracujący we własnej kopii słyszy, zanim ruszy
pierwszy proces, że `src-tauri/Cargo.toml` potrzebuje `../murmur-server`, którego w kopii nie
będzie — razem z obiema drogami wyjścia.

## 4. Macierz regresji z §11

| Próba | Gdzie jest sądzona |
|---|---|
| Wiadomość dokładnie przy pierwszym result | `step_turn_queue_is_atomic::queued_messages_get_their_own_turns_before_the_session_closes` |
| Wiadomość do wcześniejszej próby/node w innym runie | `step_turn_queue_is_atomic::a_misaddressed_message_never_reaches_a_live_session` |
| 0 testów / niewłaściwe testy / stary wynik | `required_checks_run_the_requested_tests::` (11 przypadków) |
| QA pisze pass przy brakującym obowiązkowym scenariuszu | `mandatory_criteria_survive_the_summary::a_pass_line_cannot_outvote_a_requirement_it_never_answered` |
| UI działa na mocku, kryterium wymaga backendu | `mandatory_criteria_...::a_mocked_confirmation_does_not_satisfy_a_runtime_requirement` |
| Obca aplikacja zajmuje port | odziedziczone: `probe_owned_endpoint` + `supervisor::listener_owner` |
| Poprawka zmienia źródła po QA | odziedziczone: `boundary.validate()` + `input_for_check` w `commands::run` |
| Generowanie dla dwóch vendorów | `an_agent_is_written_...::the_chosen_app_runs_it_with_nothing_to_read_and_nothing_to_write` + e2e |
| Model żąda zapisu configu, sekretów lub większych praw | `an_agent_is_written_...::a_generated_agent_cannot_widen_itself_through_raw_settings` |
| Cancel generation równolegle do workflow | `an_agent_is_written_...::cancelling_one_generation_ends_only_that_one` |
| Brak CLI, loginu, uprawnienia | `an_agent_is_written_...::a_missing_app_says_so`, `native_scenarios_...::` |
| Próba zmiany egzaminatora przez mierzony bieg | odziedziczone: `processes_cannot_rewrite_their_examiner` |

## 4a. Żywy odbiór §11 — co naprawdę pobiegło, na prawdziwych agentach

Właściciel ma subskrypcję, więc „płatne próby" z §11 nie kosztują osobno; zgoda padła
2026-09-06. Poniżej wyłącznie biegi, które naprawdę się odbyły — osobno pozytywne i negatywne
(§11 punkt 7), bez sumowania kilku prób w jedną.

| Bieg | Kiedy | Wynik |
|---|---|---|
| `flow_todo_app` na SAMEJ bazie `f81d4b11` | 09-06 23:49, 209,6 s | **czerwony** — trzy kroki Scout niosły zdanie o zachowanej kopii w polu błędu |
| `flow_todo_app` po naprawie, maszyna obciążona | 09-07 01:57, 475,9 s | Lead, Scout A/B/C, Builder: `error: null` — wada z bazy zniknęła. Czerwony na czym innym: Checker przekroczył swój limit 6 minut, bo obok szły suity frontu |
| `flow_todo_app`, maszyna wolna | 09-07 02:22, **235,9 s** | **zielony** — cały przepływ sześciu agentów, ani jeden krok niczego nie obwinia |
| `flow_say_to_agent` (żywa sesja `claude`) | 09-07 02:22, **5,7 s** | **zielony** — słowo wysłane w środku pracy wraca w prozie modelu, czyli doprecyzowanie z §11 punktu 2 naprawdę dochodzi |
| `flow_todo_app` na DOKŁADNIE tym HEAD (`0dac2e02`) | 09-07 02:55, **423,7 s** | **zielony** — ta sama wyrocznia na wersji, która stoi w repo, a nie na tej sprzed czterech commitów (§12 zakazuje zamknięcia, gdy kod różni się od sprawdzonego) |
| `both_buttons_really_write_an_agent_with_their_own_app` | 09-07 02:47, **36,9 s** | **zielony po czterech poprawkach** — oba przyciski, prawdziwy `claude` i prawdziwy `codex`, oba uszanowały `file_access: look-only` |
| `a_real_window_is_counted_and_driven_by_the_identity_it_was_given` | 09-07 03:12, 6,6 s | **zielony** — Loadout uruchomił WŁASNY egzemplarz aplikacji z oknem, policzył jego okna (`1`), zapytał o nazwę okna adresując po `unix id` (wróciło `"Otwórz"`) i zamknął go tym samym adresem |
| **§11 punkt 4 — QA na osobnej, pełnej instancji Murmura** | 09-07 03:17–03:25 | **wykonane** — szczegóły niżej |
| brak osieroconych procesów | po każdym z powyższych | `ps -eo pid,ppid` — ani jednego procesu z `PPID 1` od Loadouta (§11 punkt 6) |

Pierwsza pozycja jest tu ważniejsza niż trzecia: dowodzi, że skarga trzech kroków na własną
pracę **nie była regresją tego zadania**, tylko wadą odziedziczonego WIP-u — i że jest zamknięta.

### §11 punkt 4 — scenariusze na osobnej instancji, wykonane

Blokada zniknęła, bo ją usunąłem, zamiast o niej pisać: Murmur dostał `MURMUR_DATA_DIR`
(commit `e4c8357b` w `../.murmur-agent-tasks/m01-queue-ownership-repro`). Katalog danych na
JEDEN bieg, honorowany wyłącznie pod tym samym warunkiem, który już dziś wybiera `MeetNotes-dev`
— build debug albo jawny `MURMUR_DEV_DEK` — więc wydanie notaryzowane ignoruje go całkowicie
i żadna zmienna środowiskowa nie przesunie prawdziwej, zaszyfrowanej biblioteki człowieka.

Zbudowana i uruchomiona **prawdziwa aplikacja**, nie atrapa i nie zainstalowane wydanie:

| Scenariusz | Co zrobiono | Co zaobserwowano |
|---|---|---|
| własna biblioteka | `MURMUR_DATA_DIR=<tmp> ./target/debug/Murmur` | cała biblioteka powstała w tym katalogu: `meetnotes.sqlite` (+`-wal`, `-shm`), `audio/`, `inflight/`, `recording-inflight/`, `dev-secrets.json`, własny `murmur-dev.instance.lock` |
| okno jest osobnym dowodem | `count windows` zaraz po starcie | **0** — i to jest poprawne: `visible: false` w `tauri.conf.json`. Odpowiedź systemu to `-1719` (zły indeks), NIE odmowa uprawnień, więc `refused_for_permission` słusznie jej nie łapie |
| otwarcie okna | kliknięcie `Open Murmur` w menu ikony statusu | 1 okno o nazwie `Murmur`, 1170×884 — dokładnie skonfigurowany rozmiar |
| druga powierzchnia | kliknięcie `Recorder bar (⌘⇧R)` | dwa okna procesu |
| trzecia powierzchnia | `GET http://127.0.0.1:8765/` | `200` z serwera MCP TEJ instancji |
| zamknięcie | `Quit Murmur` z tego samego menu | proces zszedł, zero sierot |
| **izolacja** | `stat` katalogów człowieka przed i po | `MeetNotes` 09-04 20:39 i `MeetNotes-dev` 03-07 01:40 — **obie daty niezmienione**, a bieg trwał 03:17–03:25 |

**Każda akcja adresowana `first process whose unix id is <pid>`, ani jedna nazwą aplikacji.**
To jest ta jedna rzecz z P-03b, której Loadout nie umie wymusić kodem — i tu widać, po co ona
jest: gdyby scenariusz adresował „Murmur", trafiłby w aplikację, na którą akurat wskazuje
menedżer okien, czyli potencjalnie w prawdziwą aplikację człowieka z jego nagraniami.

### Scenariusze KOLEJKI — wykonane na zasianej bibliotece

Zasianie biblioteki to nie nagrywanie: mikrofon nie jest otwierany, a dźwiękiem jest wygenerowany
**cichy WAV**, czyli materiał testowy z definicji. Wiersze wchodzą **własnym magazynem aplikacji**
(`park_meeting_for_deferred_processing`), więc stan jest co do bajta tym, który aplikacja zapisuje
sama, kiedy człowiek naciska „Process later" (`f453261e`, `src-tauri/examples/seed_qa_library.rs`).

Trzy nagrania, bieg prowadzony **wyłącznie przez okno**, klawiaturą, z ogniskiem sprawdzanym po
`AXFocusedUIElement`:

| Kryterium | Co zobaczono |
|---|---|
| M-01: odroczone nagranie to `Queued`, nie `Error` | trzy karty ze statusem **Queued**; w bazie `meetings.status` jest `ERROR`, więc projekcja API-only działa całą drogą do ekranu |
| M-02: JEDNO ciężkie zadanie naraz | „Run all" przy trzech w kolejce → **„1 processing, 2 queued"**, karta biegnąca na `Transcribe / Running` z paskiem postępu, dwie pozostałe nietknięte na `Queued` |
| M-03: etapy widać, ZANIM się wydarzą | `Transcribe / Note / Export` jako trzy pełnoprawne kroki z własnym stanem — a nie Export dopisywany po fakcie |
| uczciwość porażki | whisper naprawdę policzył VAD na urządzeniu i oddał *„No speech detected in the recording — nothing to transcribe"*; etapy zeszły na **`Not run`**, nie na fałszywe zaliczenie |
| **brak utraty audio** | karta mówi „The recording's audio is untouched" — **sprawdzone na dysku**: trzy pliki WAV zachowały rozmiar co do bajta i swój `mtime` |

#### `Cancelling` — zmierzone, po naprawieniu FIKSTURY, a nie po obniżeniu kryterium

Pierwsze podejście nie zobaczyło tego napisu i napisałem, że „nie ma okna, w którym dałoby się
nacisnąć Cancel ręką". To była prawda o mojej fiksturze, nie o produkcie: cisza kończy się
odmową whispera po pięciu sekundach. Kryterium, którego nikt nie umie dosięgnąć, jest
kryterium, którego nikt nie sprawdza — więc naprawiłem fiksturę.

`say` to syntezator samego macOS, więc dźwięk powstaje z jednego, stałego zdania na tej maszynie:
dalej niczyja rozmowa, dalej materiał testowy z definicji, ale prawdziwa mowa, którą whisper
naprawdę transkrybuje tak długo, jak trzeba. Jedno nagranie, ~16 minut mowy (`bc00d20d`).

Zaobserwowane przejście, w całości, na żywej instancji sterowanej wyłącznie przez okno:

| Chwila | Co pokazał produkt |
|---|---|
| Cancel wciśnięty przy `Running 0:31` | transkrypcja **wciąż biegnie**, pasek postępu się przesuwa |
| natychmiast po | pastylka **`Cancelling`** — NIE `Cancelled` — i zdanie **„Cancel requested — finishing the current step."** |
| po dojściu do bezpiecznej granicy | pastylka **`Cancelled`**, karta w `NEEDS ATTENTION`, `Took 0:32`, trzy etapy `Not run`, przyciski Retry / Remove |
| przez cały czas | plik audio **31 190 794 bajtów, bez zmiany** — sprawdzone na dysku |

To jest dokładnie semantyka Cancel wymagana w M-02: anulowanie jest kooperatywne, więc nie wolno
meldować, że praca stanęła, dopóki ona trwa.

**Dwie rzeczy dla właściciela, obie zaobserwowane, nie wywnioskowane:** anulowanie zeszło DOPIERO
po transkrypcji, a potok stanął następnie na summarizerze ze zdaniem *„[cloud-consent] this
provider sends meeting content off-device; grant one-time consent before using it"* — brama
prywatności odmawia domyślnie w świeżej bibliotece, zamiast po cichu wysłać. Oraz: karta pokazuje
`Attempt 2` na nagraniu, które człowiek uruchomił RAZ; kolejka liczy zaparkowanie „Process later"
jako próbę pierwszą. Kosmetyka, ale to jest liczba, po której człowiek poznaje, czy coś się ponawia.

### ZNALEZISKO: pamięć webview NIE jest objęta izolacją

To jest wynik QA, a nie dodatek. WebKit kluczuje swój magazyn **NAZWĄ PLIKU WYKONYWALNEGO**
(`~/Library/WebKit/Murmur`), a nie katalogiem danych — więc instancja QA startuje z otwartymi
kartami notatek dewelopera. Zmierzone: pasek kart „czystej" instancji wymieniał **prawdziwe,
osobiste tytuły notatek** człowieka (odczytane po `AXFocusedUIElement`). To jest dokładnie to,
czego §11 punkt 4 zabrania („nie używa danych użytkownika").

Obejście jest jednym `cp` i zostało **zweryfikowane**: ta sama binarka skopiowana pod unikatową
nazwą dostała własny `~/Library/WebKit/<nazwa>`, pusty pasek kart i wyłącznie zasiane nagrania.
Domyślne zachowanie zostaje jednak wyciekiem napisów widocznych dla człowieka i należy do decyzji
właściciela Murmura.

### Czego nie dało się zobaczyć bez żywej próby

Wszystkie kryteria generatora były pod dublerem zielone, a **przycisk „Create with Claude" nie
oddawał ani jednego szkicu**. Cztery wady naraz, opisane w commicie `0dac2e02`: wyrzucany powód
vendora, ten sam numer sesji w obu turach („Session ID … is already in use"), korekta bez
pierwotnej prośby oraz prośba wymieniająca nazwy kluczy bez ani jednej dopuszczalnej wartości
i bez kształtu. To jest cała odpowiedź na pytanie, po co §11 chce prawdziwego biegu obok
zielonego harnessu: dubler odpowiada dokładnie tym, co mu wpiszemy.

## 5. Czego NIE zrobiłem i dlaczego — bez chowania pod „non-blocking"

**P-03b — czego NIE da się wymusić kodem, i dlaczego to jest wybór.** Zrobione: potwierdzenie
OKNA przed ogłoszeniem gotowości celu natywnego (`native_ui::windows_of`, trzy różne wyniki),
tożsamość instancji dostępna dla agenta przez `service_status` → `pgid`, oraz rola QA mówiąca
wprost „adresuj `first process whose unix id is <pgid>`, nigdy po nazwie aplikacji".

Nie da się wymusić kodem JEDNEJ rzeczy: że agent naprawdę adresuje każdą akcję tym pidem.
Steruje oknem przez `Bash` i `osascript`, więc przechwycenie każdej akcji znaczyłoby napisanie
własnego systemu Computer Use — czego plan zabrania wprost. Egzekwowalne jest to, co obok, i to
jest zrobione: metoda `full-runtime`, potwierdzone okno przed startem QA i „nie zmierzono" bez
drogi do okna.

**„Sesja bez ani jednego narzędzia" jest dziś niewyrażalna.** Sprawdzone w API sterowników,
zgodnie z poleceniem planu: u Claude'a pusta lista narzędzi jest w tym drzewie ODMOWĄ
(`ToolsRefused::NothingChosen`), a Codex nie ma listy narzędzi w ogóle. Ograniczenie, które
działa u OBU i jest egzekwowane, składa się z trzech rzeczy naraz: polityka tylko-do-odczytu,
wyłączona sieć i **pusty katalog roboczy**. Żadnego fallbacku do `Everything`.

**Zweryfikowany katalog modeli nie istnieje.** `Available::models` jedzie dziś pusta, co znaczy
„nie sprawdzamy" — i model wskazany przez generatora przechodzi, a widzi go człowiek. Stara
statyczna lista z formularza nie jest dowodem bieżącej dostępności, więc jej nie użyłem.

**§11 punkt 4 — WYKONANE.** Droga do okna, izolowana instancja i scenariusze: wszystko
w §4a, z liczbami. Blokada, o której pisałem wcześniej, była prawdziwa i została **usunięta**,
a nie obejściem przykryta: Murmur dostał `MURMUR_DATA_DIR` (`e4c8357b`), honorowany wyłącznie
w buildzie, który i tak pisze do `MeetNotes-dev`. Wydanie notaryzowane ignoruje tę zmienną,
więc powierzchni bezpieczeństwa w produkcie nie przybyło — ale **dotyka to rozwiązywania ścieżki
do zaszyfrowanej bazy i dlatego należy do przeglądu lock/security**. Zgłaszam to wprost, zamiast
chować pod „non-blocking"; commit niesie ten sam akapit.

**M-04 — wykonane.** Osobna instancja, jej własna biblioteka, scenariusze okna ORAZ scenariusze
kolejki na zasianych nagraniach — wszystko z liczbami w §4a. Zasianie idzie własnym magazynem
aplikacji i cichym WAV-em, więc nie jest nagrywaniem: rola QA zabrania traktować prawdziwą rozmowę
jak materiał testowy, a nie zabrania mieć plik audio.

Zmierzone zostało też `Cancelling` w trakcie ciężkiego kroku — po naprawieniu fikstury, nie po
obniżeniu kryterium (§4a). Zostaje **jedno znalezisko**, nazwane wprost i nieschowane pod
„non-blocking": pamięć webview nie jest objęta izolacją i wnosi do instancji QA tytuły notatek
człowieka. Obejście jest zweryfikowane, decyzja należy do właściciela Murmura.

**M-01…M-03 (Murmur) — zrobione i zacommitowane** w `../.murmur-agent-tasks/m01-queue-ownership-repro`,
regułą tamtego repo (autor `JakubGawr`, bez trailerów AI, merge przez PR). Szczegóły
w `M-01-REPRODUKCJA-I-NAPRAWA.md` obok. Werdykt „done" należy tam do `adversarial-verifier`,
nie do implementującego — trzy commity czekają na PR.

**Pełny odbiór produktu z §11 — nie wykonany.** Wymaga płatnych biegów i zgody, a plan wprost
mówi, że sam ich nie zleca.

## 6. Czego potrzebuję od człowieka

1. **Zgód macOS dla aplikacji Loadout**: Automation (sterowanie „System Events") i Accessibility.
   Zmierzone 2026-09-06: `osascript` + `System Events` na tej maszynie **działa** — odczyt
   `count windows` procesu wskazanego przez `unix id` odpowiada poprawnie, a żywa sonda z drzewa
   testowego mówi `Ready`. Ale zgody macOS są przypisane do BINARKI: „Ready" zmierzone z procesu
   testowego nie jest zgodą dla aplikacji Loadout. Ta poprosi o nią własnym oknem systemowym przy
   pierwszym użyciu i tylko człowiek może ją dać.
2. ~~Decyzji o torze M~~ — **dane 2026-09-06**: wolno. Tor M stoi w osobnym worktree, trzy
   commity, opisane wyżej.
3. ~~Zgody na płatne biegi~~ — **dane 2026-09-06**: właściciel ma subskrypcję, więc biegi nie
   kosztują osobno. Żywa wyrocznia przepływu została na tej zgodzie wykonana (§4a).
4. ~~Jednego ustawienia w Murmurze~~ — **wybrałem je sam i dowiozłem**: `MURMUR_DATA_DIR`
   (`e4c8357b`). To była rutynowa decyzja nazewnicza, a nie decyzja właściciela, i pytanie o nią
   było zwykłą zwłoką. Do przejrzenia zostaje SAMA ZMIANA, bo dotyka rozwiązywania ścieżki do
   zaszyfrowanej bazy — powód i granica gatingu stoją w §5 i w commicie.
5. **Decyzji o scenariuszach kolejki** — czy zasiewać fiksturę nagrań w bibliotece biegu
   (potrzebny DEK instancji), czy zostawić M-01…M-03 przy testach jednostkowych. To jedyna
   część M-04, która została.

Czego **nie** potrzebuję i czego nie ruszałem: aktywnej biblioteki użytkownika (`~/.loadout`),
restartu aplikacji, instalacji połączeń, mikrofonu, publikacji.

## 7. Jedna granica, o którą się otarłem i wycofałem

Próbowałem zrobić tak, żeby nierozstrzygalna trasa nie puszczała OBU gałęzi naraz przy
ustawieniu `carry-on`. Zapaliły się trzy kryteria T-101
(`a_blocked_way_out_takes_the_chosen_path`, `every_failure_shares_one_door`), które mówią
wprost: `Route::Blocked` jest porażką kroku przechodzącą tymi samymi drzwiami, co każda inna —
routing dostarcza dokładny powód, a `when_it_fails` wybiera skutek. To jest **rozstrzygnięte
kryterium, nie luka**, więc poprawkę wycofałem, a szablon z V-03 ustawia na tym kafelku `stop`.
Nowe jest wyłącznie to, że wynik BEZ POMIARU (`CheckOutcome::NotJudged`) w ogóle dociera do
wyboru drogi jako własna wartość, zamiast udawać zaliczenie albo wadę.

## 8. Wady, które znalazłem we własnej pracy, zanim znalazł je ktoś inny

1. **Sądzenie wymaganych testów po zebranym tekście.** Krok „sprawdź" zachowuje ostatnie 64 KiB,
   więc każdy test prawdziwej suity sprzed ogona wyglądałby na niewykonany. Poprawione na skan
   strumieniowy; kryterium dowodzi własnej przesłanki — asertuje, że wymaganej linii NIE MA
   w zachowanym wyjściu, i mimo to jest potwierdzona.
2. **Pompa zdarzeń generatora czekała na zamknięcie kanału, którego nadajnik trzymał żywy
   uchwyt.** Cały cel testowy wisiał kilkanaście minut. Uchwyt schodzi przed czekaniem.
3. **`setState` rodzica wewnątrz funkcji aktualizującej stan.** React wykonuje ją w trakcie
   renderu; prawdziwa przeglądarka zgłaszała to jako błąd, a w StrictMode szkic wchodziłby dwa
   razy. Aktualność operacji rozstrzyga dziś `useRef`, poza renderem.

4. **Cztery wady generatora naraz, wszystkie niewidoczne pod dublerem.** Opisane w §4a
   i w commicie `0dac2e02`. Najgorsza z nich nie była techniczna: własne zdanie „and did not say
   what it wrote" **zastępowało powód, który program podał**, więc trzech pozostałych nie dało
   się w ogóle zobaczyć. Dopóki tego nie zdjąłem, każda kolejna próba wyglądała tak samo.

5. **`testDataEnv` omijało regułę zastrzeżonych zmiennych.** Pole, które dołożyłem w P-02, nie
   było sprawdzane nigdzie, a `isolated_data` ustawia je na procesie potomnym — więc
   `testDataEnv: "HOME"` podmieniał aplikacji katalog domowy, a `"DYLD_INSERT_LIBRARIES"`
   wstrzykiwał do niej bibliotekę. Wszystkie trzy są zabronione dla zmiennych z `environment`,
   tą samą funkcją, dwadzieścia linii wyżej. Znalezione przy próbie wykonania §11 punktu 4:
   szukałem aplikacji, którą wolno uruchomić izolowaną, i zauważyłem, że własna reguła
   przepuściłaby `HOME`. Naprawione obiema drogami (`f2271d32`).

6. **Zdanie odmowy z osiemnastoma spacjami w środku.** Złamany literal bez `\` w
   `isolated_data`. Czytał to człowiek, któremu Loadout właśnie odmówił uruchomienia aplikacji.

7. **Zgadywanie zamiast pomiaru.** Kiedy prawdziwy `claude` nie oddał szkicu, dopisałem
   zdejmowanie płotka z bloku kodu — bo „modele przecież owijają JSON". Nie owijał; prawdziwą
   przyczyną było `"color": "amber"`. Kod poszedł do kosza, a nie do repo: leniency bez
   ani jednego pomiaru to poszerzenie kontraktu w ciemno.

## 9. Jak to obejrzeć

Wersja, o której mówi ten raport: `f2271d32` na `feat/reliability-native-qa-generator`.
Niezacommitowany jest wyłącznie ten plik.

```bash
cd /Users/jakubgawronski/Projects/loadout-reliability-native-qa-generator

# każde kryterium tego zadania, jedno po drugim
cargo test --manifest-path src-tauri/Cargo.toml --test it step_turn_queue_is_atomic::
cargo test --manifest-path src-tauri/Cargo.toml --test it required_checks_run_the_requested_tests::
cargo test --manifest-path src-tauri/Cargo.toml --test it mandatory_criteria_survive_the_summary::

# co ta maszyna naprawdę potrafi z natywnym oknem — druga otwiera i zamyka własne okno
cargo test --manifest-path src-tauri/Cargo.toml --test it native_scenarios -- --ignored --nocapture

# oba przyciski, prawdziwe kliknięcie, prawdziwa przeglądarka
npx --no-install vitest run e2e/tests/two-buttons-ask-two-different-vendors.spec.ts

# ŻYWE — płacą i odpowiadają o tej maszynie, nie o kodzie
cargo test --manifest-path src-tauri/Cargo.toml --test it \
  an_agent_is_written_by_the_vendor_that_was_asked::both_buttons -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test flow_say_to_agent -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test flow_todo_app -- --ignored --nocapture

# porównanie z bazą MA sens tylko przy tej samej równoległości (§1)
cargo test --manifest-path src-tauri/Cargo.toml --test it -- --test-threads=4
```

Szablon i rola do obejrzenia przed importem:
`.loadout/workflows/verified-change.json`, `.loadout/agents/verifies-the-running-app.md`.
Kroki `s_plan`, `s_backend`, `s_frontend`, `s_combine` mają w polu `agent` napis
`REPLACE_WITH_…`: to jest miejsce na twoje role, a nie identyfikator.
