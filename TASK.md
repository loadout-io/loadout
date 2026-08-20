# T-64 — Linear: „przypisany na mnie" dociera do Loadouta, dokładnie raz

Zgłoszenie właściciela 2026-08-20: *„jak wpadnie przypisany na nas task to uruchom workflow
i w prompt dajemy to zadanie"*. Drut do biegu **już stoi** — `RunRequest::task`
(`src-tauri/src/commands/mod.rs:568`) powstał 2026-08-19 z poprzedniego zgłoszenia tego samego
człowieka i nie ma dziś innego czytelnika niż wiersz wejścia. Brakuje wyłącznie źródła zdarzeń.

**To zadanie nie dotyka ani jednej linii silnika.** Kończy się w miejscu, w którym okno umie
zapytać Rusta „czy coś nowego?" i dostać odpowiedź **najwyżej raz na issue**. Zegar, sekcja
i odpalenie biegu należą do T-65.

## Dlaczego nie MCP, i dlaczego to nie jest do dyskusji

MCP jest protokołem **pull**: model woła narzędzie, kiedy chce. Nie ma w nim „obudź aplikację".
Do tego w tym drzewie MCP jest odcięty z premedytacją i w trzech miejscach naraz:
`engine/drivers/claude.rs:140` startuje każdą sesję z `--strict-mcp-config`,
`workflow/check.rs:44` trzyma tę flagę na liście zarezerwowanych, więc przelotka vendorowa nie
może jej cofnąć, a `tests/it/inherit_agents_are_text.rs:19` filtruje `mcpServers`
z dziedziczonych definicji z podanym powodem: **startuje proces poza grupą procesów Loadouta**,
czyli niezmiennika 6 nie da się utrzymać (zmierzone 2026-08-19: jeden bieg zostawił 14 sierot
z `ppid=1`). `checks/quick-vocabulary.sh` zakazuje zresztą samego słowa `MCP` w tekście
widocznym dla użytkownika.

## Dwa ograniczenia, które są twarde. Przeczytaj je, zanim napiszesz pierwszą linię

**1. Nowej skrzyni nie da się dodać.** `.claude/settings.json` odmawia `Write(Cargo.toml)`
i `Write(src-tauri/Cargo.toml)`. `reqwest`, `ureq` i `hyper` nie są w drzewie i nie będą.
Wyjście na sieć jedzie przez **`curl`**, dokładnie tym wzorcem, który stoi już
w `skills::ingest::build_fetch_command` (`src-tauri/src/skills/ingest.rs:909`): `--config -`,
konfiguracja **rurą na stdin**, `env_clear` plus samo `PATH`, `--proto '=https'`, `--max-time`.
Próba dodania zależności to spalona runda, nie „mniejsza wersja zadania".

**2. Żadne kryterium nie wolno oprzeć na żywym wywołaniu sieciowym.** W pętli nie ma klucza
i nie ma prawa być: bieg bez sieci dałby `before` czerwone z powodu rc 124/127, a to jest
podpis z `NOT_A_REAL_RED` — czyli kryterium, które nie sprawdza niczego. Prawdą o kształcie
odpowiedzi jest **plik złoty** `docs/research/fixtures/linear-assigned.json`, a każdy test
podaje go tam, gdzie kod normalnie czyta stdout `curl`-a. Kod składający komendę jest sądzony
osobno, na samej komendzie.

**Czego to zadanie w związku z tym NIE dowodzi, i mówię to wprost:** że zapytanie, które
wysyłamy, jest tym, które Linear naprawdę przyjmuje. To jest jedno sprawdzenie z prawdziwym
kluczem, ręką człowieka, po wylądowaniu — i tak ma być zapisane w `docs/STATUS.md`. Kryterium
udające ten dowód byłoby gorsze niż jego brak.

## Cicha porażka, przed którą stoi cały ten kontrakt

Odpytywanie **działa**, trafienie wraca, kursor zapisuje się **po** oddaniu wyniku — i jeden
issue odpala dwa biegi: dwa procesy `claude`, dwa rachunki, dwie gałęzie na tym samym zadaniu.
Nic nie pada i nic nie krzyczy. Albo wersja odwrotna, droższa: pierwsze odpytanie po wpisaniu
klucza widzi pięćdziesiąt otwartych issues przypisanych na ciebie i odpala pięćdziesiąt biegów
naraz. Obie awarie są **finansowe**, nie higieniczne, i obie mieszkają w kolejności dwóch
instrukcji.

**Read first:**
`src-tauri/src/skills/ingest.rs` (`build_fetch_command`, `config_on_stdin`, `FetchError` —
wzorzec „konfiguracja `curl`-a rurą, adres nigdy w argv" i jego uzasadnienie),
`src-tauri/src/commands/mod.rs` (`RunRequest`, w szczególności doc-komentarz przy `task`),
`src-tauri/src/library/agents.rs` (`read_agent_file` — plik konfiguracyjny pisany ręką
człowieka, `deny_unknown_fields` i odmowy, które mówią co zrobić),
`src-tauri/src/ipc.rs` (czternaście bezstanowych skorup komend; `generate_handler!` w okolicy
1189; `AppState` niesie stan **wyłącznie** dla trzech komend biegu — ta komenda stanu nie ma),
`src-tauri/commands.golden.txt` (jedyna lista nazw komend, czytana z obu stron granicy),
`src/sections/workflows/io.ts` (krawędź okna po pięć linii, jedno miejsce z nazwami komend),
`src/sections/commands-wired.test.ts` (tabela `EDGES` — musi pokrywać cały eksport modułu io),
`AGENTS.md` niezmienniki 4, 5, 9, 21, 23.

## Niezmienniki, których to dotyka

- **9 — prompt i sekrety wyłącznie przez stdin.** Klucz do Lineara jest pierwszym prawdziwym
  sekretem w tym drzewie. Nigdy w argv (`ps` widzi argv każdego procesu na tej maszynie), nigdy
  w pliku tymczasowym, nigdy w linii dziennika, nigdy w zdaniu odmowy.
- **4 — pliki są prawdą, SQLite jest indeksem.** Kursor („co już widzieliśmy") jest **plikiem**.
  W bazie byłby polem, którego nie da się odtworzyć z plików, więc po skasowaniu `loadout.db`
  Loadout odpaliłby wszystko od nowa.
- **5 — nigdy nie wywalaj biegu na nieznanym zdarzeniu.** Linear dokłada pola do schematu bez
  zapowiedzi. `Option<T>` na wszystkim, co nie jest niezbędne, i żadnego `deny_unknown_fields`
  na tym, co przychodzi z drutu.
- **23 — polityka w jednym rdzeniu.** Decyzja „co jest trafieniem" mieszka w jednym miejscu.
  Druga kopia reguły w skorupie komendy jest tym, jak po cichu umarło skanowanie sekretów
  w repo źródłowym.
- **21 — nie pisz artefaktu, którego żaden skrypt nie czyta.** Plik złoty z AC-3 jest czytany
  przez cztery kryteria; nazwa komendy wchodzi do `commands.golden.txt`, który czytają dwa
  istniejące testy rejestracji.

## Rozstrzygnięcia, których nie podejmujesz sam

1. **Pierwsze odpytanie uzbraja, nie odpala** (AC-5). Konfiguracja bez kursora zapisuje kursor
   na najnowszym `updatedAt`, jaki widzi, i **nie oddaje trafienia**. To jest decyzja produktowa
   właściciela, nie optymalizacja.
2. **Trafienie, którego nie dało się zapisać, nie jest trafieniem** (AC-4c). Jeśli kursor nie
   idzie na dysk, komenda **odmawia**. Oddanie trafienia bez zapisu jest dokładnie tym podwójnym
   rachunkiem, o którym jest ten kontrakt.
3. **Jedno źródło w tym zadaniu: Linear.** Jira, ClickUp i Slack mają inny kształt odpowiedzi
   i inne pojęcie „przypisane na mnie". Struktura ma je dopuszczać (pole nazywające źródło,
   odmowa dla nieznanej nazwy), ale ani jednej ich linii tu nie piszesz.

## Kryteria akceptacji

## AC-1 Plik triggera wczytuje się, a jego odmowy nie wypisują klucza
check: cargo test --test it trigger_file_format::
expect: (\d+) passed

Podmiot: `~/.loadout/triggers/<slug>.json`, do T-65 pisany ręką człowieka. Asercje:
(a) obieg przez **prawdziwy plik** na dysku — zapis, wczytanie, równość pól; (b) literówka
w nazwie klucza jest odmową **nazywającą ten klucz** (`deny_unknown_fields`, ten sam powód co
przy pliku agenta: plik pisze człowiek i pomyłka ma zaboleć od razu); (c) brak klucza do
Lineara jest odmową, która mówi, **co zrobić**, a nie „missing field"; (d) **żadne** zdanie
odmowy, żaden `Debug` i żaden `Display` tej struktury nie zawiera wartości klucza; (e) nieznana
nazwa źródła jest odmową nazywającą tę nazwę, nie cichym pominięciem wpisu.

*Kontrola przeciw pustemu przejściu:* fikstura musi nieść klucz **wyglądający jak prawdziwy**
(prefiks `lin_api_` plus co najmniej 32 znaki) i test sam to sprawdza. Bez tego (d) mierzy
pusty napis i przechodzi na strukturze, która klucza w ogóle nie ma.

*Słaba asercja:* `assert!(load(path).is_ok())`. Przechodzi dla struktury, która ma
`#[serde(default)]` na wszystkim, więc pusty `{}` też się wczytuje — a wtedy człowiek dostaje
trigger bez klucza i bez ani jednego zdania o tym. Rozróżnia to (c) razem z (b).

## AC-2 Klucz jedzie rurą, a `ps` go nie widzi
check: cargo test --test it trigger_key_never_in_argv::
expect: (\d+) passed

Dwa podmioty, bo `Command` ze stdinem podpiętym do rury nie daje się przeczytać z powrotem:
**czysta funkcja** składająca tekst konfiguracji `curl`-a i **`Command`** składana wokół niej.
Asercje: (a) `Command::get_args()` nie zawiera ani jednego ciągu ≥ 8 znaków wspólnego
z kluczem; (b) argv niesie `--config` i `-`, a adres API **nie stoi w argv**; (c) tekst
konfiguracji niesie jednocześnie `url = "…"` i wiersz nagłówka z kluczem — czyli klucz
naprawdę jedzie tą drogą; (d) `Command::get_envs()` przepuszcza `PATH` i **nic więcej**
(`http_proxy` ani `CURL_CA_BUNDLE` nie mają prawa zmienić, skąd przyszła odpowiedź);
(e) w tekście konfiguracji stoi limit czasu i `--proto`/`url` po `https`, a nie po `http`.

*Kontrola przeciw pustemu przejściu:* (c) jest tą kontrolą i musi stać w tym samym pliku co
(a). Bez niej (a) przechodzi dla implementacji, która **nie wysyła klucza wcale** — a to jest
zielone kryterium nad zapytaniem, które Linear odrzuci.

*Słaba asercja:* `assert!(!format!("{command:?}").contains(key))`. Przechodzi dla klucza
pociętego na dwa argumenty argv, bo `Debug` wstawia między nie przecinek. Rozróżnia to
asercja o **fragmencie** z (a), nie o całym napisie.

## AC-3 Odpowiedź czyta się permisywnie, a nie „prawie na pewno tak wygląda"
check: cargo test --test it trigger_reads_the_answer::
expect: (\d+) passed

Podmiot: `docs/research/fixtures/linear-assigned.json` podany tam, gdzie kod czyta stdout
`curl`-a. Asercje: (a) trafienie niesie identyfikator sprawy, tytuł, adres i treść;
(b) pozycja z **nieznanym dodatkowym polem** wczytuje się bez straty pozostałych
(niezmiennik 5); (c) pozycja z `null` w opisie daje trafienie z pustą treścią, nie błąd;
(d) pusta lista pozycji to **brak trafienia**, nie awaria; (e) odpowiedź, która nie jest tym
JSON-em — pusty stdout, HTML strony błędu, poprawny JSON z listą błędów zamiast danych — jest
odmową z osobnym zdaniem na każdy z tych trzech przypadków, bo każdy naprawia się inaczej.

*Kontrola przeciw pustemu przejściu:* test sam sprawdza, że fikstura niesie **co najmniej trzy
pozycje**, co najmniej jedno nieznane pole i co najmniej jeden `null` w opisie. Inaczej mierzy
plik, w którym tych przypadków nie ma, i przechodzi na niczym.

*Słaba asercja:* parsowanie do `serde_json::Value` i czytanie po ścieżce. Przechodzi dla
odpowiedzi, w której brak pola daje `Value::Null` zamiast odmowy, więc trafienie z pustym
tytułem jedzie dalej i człowiek dostaje bieg o zadaniu bez nazwy. Rozróżnia to (a) razem z (e).

## AC-4 Ten sam issue nie odpala dwa razy, i nie da się tego obejść awarią zapisu
check: cargo test --test it trigger_never_fires_twice::
expect: (\d+) passed

Najważniejsze kryterium tego zadania. Asercje: (a) pierwsze wywołanie na uzbrojonym triggerze
i odpowiedzi z nowszą sprawą oddaje trafienie, a **plik kursora na dysku** niesie po powrocie
`updatedAt` tej sprawy; (b) **drugie wywołanie na tej samej odpowiedzi oddaje brak trafienia**;
(c) kiedy kursora **nie da się zapisać** (katalog tylko do odczytu), wywołanie **odmawia**
i nie oddaje trafienia — dowód, że zapis stoi **przed** oddaniem wyniku, a nie po nim;
(d) kursor jest plikiem pod `~/.loadout/triggers/`, a ta ścieżka kodu nie bierze ani jednego
uchwytu do `Store` — po skasowaniu `loadout.db` zachowanie jest identyczne (niezmiennik 4).

*Kontrola przeciw pustemu przejściu:* (b) musi paść na implementacji, która kursora nie
zapisuje wcale, a (c) na tej, która zapisuje go po oddaniu wyniku. Test ma **nazwać oba te
kształty w komunikacie asercji**, bo za rok to jest jedyne miejsce, w którym stoi, dlaczego ta
kolejność jest taka, a nie odwrotna.

*Słaba asercja:* dwa wywołania i sprawdzenie, że drugie zwróciło `None`. Przechodzi dla
implementacji trzymającej „co widziano" **w pamięci procesu** — czyli takiej, która po
restarcie aplikacji odpala wszystko po raz drugi, a to jest cały ten defekt, tylko rzadszy
i dlatego trudniejszy do znalezienia. Rozróżnia to (a): asercja o **pliku**, nie o wartości
zwróconej.

## AC-5 Pierwsze odpytanie uzbraja, nie odpala pięćdziesięciu biegów
check: cargo test --test it trigger_first_poll_arms::
expect: (\d+) passed

Asercje: (a) trigger **bez kursora**, przeciw odpowiedzi z pięćdziesięcioma przypisanymi
sprawami, oddaje **brak trafienia** i zapisuje kursor; (b) kursor ląduje na **najnowszym**
`updatedAt` z całej listy, a nie na pierwszym ani ostatnim wpisie — fikstura ma je w kolejności
przetasowanej, żeby implementacja czytająca „ostatni z listy" była czerwona; (c) następne
wywołanie, z jedną sprawą nowszą od kursora, oddaje **dokładnie jedno** trafienie i jest to ta
sprawa; (d) sprawa o `updatedAt` **równym** kursorowi nie jest trafieniem (granica jest ostra,
bo inaczej każda pozycja odpala się przy każdym tiku).

*Kontrola przeciw pustemu przejściu:* test sam sprawdza, że fikstura niesie pięćdziesiąt
pozycji i że ich `updatedAt` nie są posortowane. Bez tego (b) mierzy listę jednoelementową.

*Słaba asercja:* sprawdzenie, że pierwsze wywołanie zwróciło `None`. Przechodzi dla
implementacji, która przy braku kursora **nie robi nic i kursora nie zapisuje** — czyli nigdy
się nie uzbraja i trigger jest martwy na zawsze. Rozróżnia to (a) razem z (c).

## AC-6 Okno ma czym tę komendę zawołać, po nazwie i po nazwach argumentów
check: npx --no-install vitest run src/sections/triggers/asks-rust-once.test.ts
expect: (\d+) passed

Szew, na którym poległo dwadzieścia sześć zielonych zadań (T-27). Asercje: (a) nazwa komendy
stoi w `src-tauri/commands.golden.txt`, czytanym **z pliku**, nie z literału w teście;
(b) nazwy argumentów, które wysyła krawędź `src/sections/triggers/io.ts`, zgadzają się co do
znaku z tymi, które deklaruje `ipc.rs` — czytane przez `windowSideArguments` z
`src/sections/ipc-signature.ts`, bo Tauri dopasowuje argumenty **po nazwie** i odrzuca całe
wywołanie przy literówce; (c) każda funkcja eksportowana z tego modułu io jest **wykonana**
na atrapie `@tauri-apps/api/core` i żadna nie odmawia zdaniem „not implemented";
(d) `check_trigger` ma swój wiersz w tabeli `EDGES` w `src/sections/commands-wired.test.ts`,
więc funkcja dopisana jutro bez wpisu jest czerwona po tamtej stronie.

*Kontrola przeciw pustemu przejściu:* test odmawia, kiedy lista złota wczytała się pusta.
Porównanie dwóch pustych zbiorów przechodzi — i to jest dokładnie ten kształt zieleni, którego
to kryterium ma nie mieć.

*Słaba asercja:* grep po `invoke(` w źródle krawędzi. Przechodzi dla krawędzi wołającej
`invoke` w martwej gałęzi i dla takiej, która skleja nazwę komendy ze zmiennej. Rozróżnia to
wykonanie z (c).

## Świadomie poza zakresem

- **Zegar, sekcja i odpalenie biegu** — całość jest T-65. Ta komenda jest bezstanowa i nikt
  jej jeszcze cyklicznie nie woła; `checks/quick-wired.sh` uznaje ją za podłączoną, bo jej
  nazwa stoi w `commands.golden.txt` („sama jest wejściem — woła ją Tauri, nie nasz kod").
- **Jira, ClickUp, Slack.** Struktura ich dopuszcza (AC-1e), kodu nie ma.
- **Wspólny rdzeń budowniczego `curl`-a.** `skills::ingest::build_fetch_command` robi to samo
  o jedną politykę bezpieczeństwa i nie jest w bloku OWNS — należy do sekcji Umiejętności.
  Zostaje więc **druga kopia tych samych flag**, i to jest dług do zgłoszenia człowiekowi,
  nie rozwiązanie: dwa budowniczych rozjadą się przy pierwszej zmianie polityki, dokładnie tak
  jak rozjechało się skanowanie sekretów w repo źródłowym (niezmiennik 23). Zgłoś to zdaniem
  w `docs/STATUS.md`, nie refaktorem poza zakresem.
- **Keychain.** Klucz leży w `~/.loadout/triggers/<slug>.json` jawnym tekstem. Do
  `<repo>/.loadout/` nie ma prawa trafić nigdy, bo `ARCHITECTURE.md` §8 opisuje tamten katalog
  jako bezpieczny do commitowania. Przeniesienie do Keychaina jest osobną decyzją człowieka.

<!-- OWNS
src-tauri/src/commands/triggers.rs
src-tauri/src/commands/mod.rs
src-tauri/src/ipc.rs
src-tauri/commands.golden.txt
src-tauri/tests/it/main.rs
src-tauri/tests/it/trigger_file_format.rs
src-tauri/tests/it/trigger_key_never_in_argv.rs
src-tauri/tests/it/trigger_reads_the_answer.rs
src-tauri/tests/it/trigger_never_fires_twice.rs
src-tauri/tests/it/trigger_first_poll_arms.rs
docs/research/fixtures/linear-assigned.json
src/sections/triggers/io.ts
src/sections/triggers/asks-rust-once.test.ts
src/sections/commands-wired.test.ts
-->
