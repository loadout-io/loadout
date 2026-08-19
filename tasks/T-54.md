# T-54 — Dziedziczenie wiedzy repo gospodarza przez PRZEPISANIE: umiejętności i learnings jako tekst, nigdy jako maszyneria

Zasada nadrzędna całej tej fali, powiedziana wprost i moimi słowami: **harness jest NASZ.
Z repo gospodarza dziedziczymy TEKST, nigdy MASZYNERIĘ — i dziedziczymy go przez PRZEPISANIE do
siebie, nie przez wczytanie jego ustawień.** „Przepisanie" jest tu czasownikiem dosłownym: czytamy
cudze pliki, przenosimy ich bajty do katalogu, który sami stworzyliśmy, i podajemy sesji **nasz**
katalog. Nie wołamy `--setting-sources project`, nie ładujemy `.claude/settings.json` gospodarza,
nie uruchamiamy jego haków i nie dziedziczymy ani jednej linii, która potrafi wystartować proces.

Dlaczego to jest niepodważalne, a nie estetyczne [wszystko zmierzone 2026-08-19]: hak `PreToolUse`
gospodarza startuje proces we **własnej grupie procesów**, jego dziecko dostaje `ppid=1` i przeżywa
wyjście `claude` — jeden bieg zostawił **14 sierot**, eksperymenty łącznie **30**. Przy załadowanych
ustawieniach gospodarza niezmiennik 6 jest NIE DO UTRZYMANIA: nie ma czego zabić, bo grupa nie jest
nasza, i nie ma czego dowieść, bo `kill(-pgid, 0)` pyta o cudzą grupę. Drugi zmierzony wypadek tej
samej klasy: agent Loadouta wywołał projektowego podagenta repo gospodarza, ten wystartował jako
osobny proces i spalił **38–41 tys. tokenów całkowicie poza widokiem i rozliczeniem Loadouta**.
Nikt tego nie zauważył, bo bieg wyglądał normalnie i skończył się na zielono.

Ten plik odpowiada za **wiedzę**: umiejętności (`SKILL.md`) i learnings (`.claude/learnings/*.md`),
plus podagentów gospodarza sprowadzonych do samego tekstu. Odcięcie ustawień, uprawnienia i argv
sterownika należą do sąsiedniego zadania tej fali; tu przenosimy treść i dowodzimy, że przeniosła
się **tylko** treść.

**Cicha porażka numer jeden, zmierzona i wyglądająca na sukces.** Katalog pluginu zbudowany
o jeden poziom za płytko — `<katalog>/alpha/SKILL.md` zamiast `<katalog>/skills/alpha/SKILL.md`.
Plugin **ładuje się**, pojawia się w `init.plugins` jako pełnoprawny wpis i rejestruje **zero
umiejętności** [S1 §2, przebieg M3: 54 → 54]. Nie ma błędu, nie ma ostrzeżenia, jest zielony wpis
w zdarzeniu startowym. Agent po prostu nie zna umiejętności, o której człowiek zaznaczył, że ma ją
znać — a „agent nie wie o umiejętności" jest nieodróżnialne od „model nie uznał, że warto po nią
sięgnąć". Dokładnie ta klasa awarii zabiła pół `skills/place.rs` i dlatego ten plik istnieje.

**Cicha porażka numer dwa: wstrzyknięcie zdania O regułach zamiast reguł.** Wzorzec z urc-monorepo
nie ma **ani jednej linii kodu** — wstrzykiwanie learnings jest prozą w `SKILL.md`, którą model
wykonuje ręcznie narzędziem Read [`ship-task/SKILL.md` §„Learnings injection", kroki 1–2]. Nie ma
czego portować; piszemy własny wstrzykiwacz i przez to **nie powstaje żadna zależność od harnessu
gospodarza**. Ale własny wstrzykiwacz ma jedną pułapkę i jest ona zmierzona: każdy prawdziwy plik
w `.claude/learnings/` niesie w trzeciej linii cytat *o* sekcji —
`> Auto-loaded by ship-task orchestrator. \`## Recurring patterns\` is BINDING …`. Naiwne
`text.find("## Recurring patterns")` trafia w ten cytat, a nie w nagłówek: na `backend-dev.md`
daje **131 bajtów** zdania o tym, że reguły są wiążące, zamiast **1701 bajtów** reguł. Prompt jest
wtedy dłuższy, agent nie dostaje żadnej reguły i nikt tego nie widzi, bo pole „lekcje" jest
niepuste.

**Cicha porażka numer trzy, najdroższa: podagent gospodarza wzięty razem z front-matterem.**
`.claude/agents/e2e-author.md` gospodarza ma w nagłówku `mcpServers: playwright: command: npx,
args: ["-y", "@playwright/mcp@0.0.75"]` [zmierzone 2026-08-19, trzy pliki na trzynaście]. Jedno
pole YAML-a, a znaczy „uruchom `npx` i pobierz z sieci paczkę". Proces startuje **poza grupą
procesów Loadouta**, więc nie wchodzi do żadnego dowodu śmierci i nie wchodzi do żadnego licznika
kosztu. To jest ten sam wypadek co 38–41 tys. tokenów wyżej, tylko wpisany w plik, który wygląda
na dokumentację. **Front-matter jest granicą maszynerii** — po naszej stronie zostaje wyłącznie
ciało pliku.

**Read first:**
`src-tauri/src/skills/ingest.rs` (`review` — dekoduj → normalizuj → skanuj, publiczne; `from_folder`
— czyta `SKILL.md` z katalogu, nakłada limity `FILE_CAP`/`TOTAL_CAP` i buduje `Skill`;
`parse_doc` — **prywatny** parser front-mattera, patrz niżej),
`src-tauri/src/skills/place.rs` (`emit` i `apply` — czym NIE jest przepisanie, patrz „Co
odziedziczamy z `skills/`, a czego świadomie nie"; `first_line` — prywatna, ale jej DLACZEGO
obowiązuje tu bez zmian: cytujemy dosłownie, bo zdanie wymyślone w zastępstwie zostałoby pokazane
człowiekowi tak, jakby stało w pliku),
`src-tauri/src/skills/mod.rs` (`Skill`, `SkillDoc`, `Error`, `Result` — typy, których nie
powielasz),
`docs/research/topics/S1-skill-subsetting.md` (**cały, to jest wyrocznia tego zadania**: §2 tabela
sześciu przebiegów, dwa zdania pod tabelą o obowiązkowym poziomie `skills/` i o nazwie katalogu
stającej się przedrostkiem, §3 o `.claude-plugin/plugin.json`, blok `answer` z polem `layout`),
`~/Projects/urc-monorepo/.claude/learnings/README.md` i jeden plik roli, np.
`.../learnings/code-reviewer.md` (**wyłącznie do odczytu, to cudze repo** — kształt źródła: nagłówek
`## Recurring patterns (BINDING — do NOT repeat)` i `## Run journal`),
`~/Projects/urc-monorepo/.claude/skills/ship-task/SKILL.md`, sekcja „Learnings injection"
(dowód, że wzorzec nie ma kodu),
`AGENTS.md` (§2a kontrakt kryterium, §3 niezmienniki 3, 4, 5, 6, 9, 20, 21, 23, 24).

**To nie jest dziedziczenie z `docs/patterns/05-inherit-dont-copy.md`.** Tamten wzorzec mówi
o różnicy kroku wobec szablonu agenta wewnątrz Loadouta. Tu „dziedziczenie" znaczy przeniesienie
tekstu z **cudzego repozytorium**, a rozstrzygnięcie jest odwrotne: tam trzymamy różnicę i liczymy
wartość przy uruchomieniu, tu kopiujemy bajty raz i nigdy nie sięgamy po źródło w trakcie biegu.
Zdanie stoi tutaj, bo czytelnik szukający słowa „dziedziczenie" trafi najpierw tam.

## Co to zadanie posiada

- `src-tauri/src/inherit/mod.rs` — typy i nic poza nimi: wpis skanu (nazwa katalogu + pierwsza
  linia `SKILL.md`), wynik przepisania (ścieżka katalogu pluginu + lista przepisanych nazw), enum
  błędu. Ten sam podział co `skills/mod.rs` wobec `skills/place.rs`: dane tutaj, zachowanie obok.
- `src-tauri/src/inherit/scan.rs` — **czytanie gospodarza, zero zapisu**: lista umiejętności
  z `<projekt>/.claude/skills/**`, wycięcie sekcji `## Recurring patterns` z pliku learnings,
  wyjęcie ciała podagenta zza front-mattera. Wszystkie trzy to czyste funkcje nad tekstem albo nad
  katalogiem podanym argumentem — żadna nie zna pojęcia „bieżący katalog".
- `src-tauri/src/inherit/rewrite.rs` — **pisanie do siebie**: budowa katalogu pluginu biegu
  i złożenie flagi `--plugin-dir`. Jedyne miejsce w tym zadaniu, które dotyka dysku zapisem.
- `src-tauri/tests/it/inherit_scan_skills.rs`, `inherit_plugin_dir.rs`, `inherit_argv_plugin.rs`,
  `inherit_recurring_patterns.rs`, `inherit_agents_are_text.rs` — po jednym pliku na kryterium, bo
  `check:` wskazuje **jeden** plik po ścieżce.

**`src-tauri/src/lib.rs` masz w OWNS WYŁĄCZNIE po to, żeby dopisać jeden wiersz
`pub mod inherit;`** (z komentarzem dokumentacyjnym w konwencji sąsiadów: „Dziedziczenie wiedzy
repo gospodarza… Wypełnia T-54"). Żadnej innej zmiany w tym pliku — ani `.manage`, ani fabryki
sterowników, ani niczego w `setup`. Bez tego wiersza żaden plik z `tests/it/` się nie skompiluje,
a `unresolved import` jest na liście `NOT_A_REAL_RED`, więc `./verify.sh before` nie powiedziałby
ci niczego prawdziwego.

**`src-tauri/tests/it/main.rs` masz w OWNS WYŁĄCZNIE po to, żeby dopisać pięć wierszy `mod`**
w porządku alfabetycznym (`mod inherit_agents_are_text;` … `mod inherit_scan_skills;`). Plik bez
wpisu kompiluje się do niczego i nie uruchamia ani jednego testu — czyli wygląda dokładnie jak
zestaw, który przeszedł; pilnuje tego `checks/quick-tests-listed.sh`. Nic poza tymi pięcioma
wierszami.

**Czego potrzebujesz, żeby `before` było czerwone z właściwego powodu.** `todo!()` odpada:
`todo = "deny"` stoi w `[workspace.lints]` w `Cargo.toml` i wywróci `clippy`, zanim test ruszy.
Pisz więc zaślepki, które **się kompilują i zwracają pustą wartość albo `Err`** — sygnatura
istnieje, moduł się rozwiązuje, test pada na asercji. I jedna rzecz, którą trzeba zrobić celowo:
zaślepka zwracająca pustkę **przechodzi połowę** trzech z tych kryteriów (AC-3 „nic nie
odziedziczono → brak flagi", AC-4 „brak sekcji → pusty wynik", AC-1 „brak katalogu → pusta lista").
Dlatego w każdym z tych trzech plików **przypadek pozytywny stoi PIERWSZY**: zestaw ma padać na
pierwszej asercji, a nie wyglądać na w połowie zielony.

**Widoczność.** Plik w `src-tauri/tests/` widzi wyłącznie **publiczną** powierzchnię
`loadout_lib`, więc wszystko, czego dotyka test, musi być `pub`.

## Co odziedziczamy z `skills/`, a czego świadomie nie

`ingest.rs` i `place.rs` już umieją czytać i pisać `SKILL.md`. Nie powielasz tego — ale „użyj"
znaczy tu dwie różne rzeczy po dwóch stronach i pomylenie ich jest osobną cichą porażką:

- **Czytanie — używasz.** Wszystko, co potrzebuje sparsowanej umiejętności, idzie przez
  `ingest::from_folder`; drugi parser front-mattera w tym repo to drugie znaczenie słowa
  „umiejętność" (niezmiennik 23). Tekst gospodarza jest **nieufany dokładnie tak samo** jak
  umiejętność pobrana z sieci — to cudze repo, w którym nikt nie audytował komentarzy HTML — więc
  ciało, które jedzie do promptu, przechodzi przez `ingest::review` i to samo dotyczy ciała
  podagenta z AC-5. `review` zdejmuje komentarze HTML i znaki niewidzialne, a znaleziska są
  faktem o imporcie, nie powodem do wywalenia biegu (niezmiennik 5).
- **Pisanie — NIE używasz, i to jest rozstrzygnięcie, nie przeoczenie.** `place::emit` **normalizuje**:
  zdejmuje czternaście pól spoza specyfikacji, przepisuje cytowanie skalarów YAML-a i ustawia
  kolejność pól. `place::apply` pisze do **dwóch katalogów vendorów użytkownika** i do sidecara.
  Obie te rzeczy są poprawne dla umiejętności, którą Loadout **posiada**, i obie są złe dla
  umiejętności, którą Loadout **cytuje**. Przepisanie ma przenieść cudzy plik bajt w bajt: człowiek
  ma móc porównać `diff` i zobaczyć zero różnic, a każda nasza „poprawka" w cudzym pliku jest
  zmianą treści promptu, o której autor umiejętności się nie dowie. Dlatego `rewrite.rs` kopiuje
  bajty (AC-2 to asertuje) i **nie instaluje niczego u użytkownika**.
- **`place::first_line` i `ingest::parse_doc` są prywatne, a `place.rs`/`ingest.rs` NIE są w OWNS.**
  Więc podziału na front-matter i ciało nie da się stamtąd wziąć i `scan.rs` robi go u siebie —
  **raz**, jedną funkcją, z której korzystają obie ścieżki (umiejętność i podagent). Reguła jest
  lustrem `parse_doc` i przepisz ją świadomie: front-matter bez domknięcia **nie jest**
  front-matterem, `---` w pierwszej linii pliku, który nigdy się nie domyka, to pozioma kreska.

## Niezmienniki

- **6 — zabijamy grupę procesów i dowodzimy, że nie żyje.** To zadanie nie zabija nic, ale jest
  jedynym miejscem, w którym da się wpuścić coś, czego zabić nie umiemy. Cicho łamie się tak:
  front-matter podagenta przechodzi „na razie w całości, przefiltrujemy przy użyciu" — a `mcpServers`
  startuje `npx` poza naszą grupą i dowód śmierci przestaje cokolwiek znaczyć.
- **9 — prompt i sekrety wyłącznie przez stdin, `env_clear()` plus jawna lista.** Front-matter
  gospodarza niesie też `model`, `permissionMode` i `memory`; przepuszczone dalej zmieniają
  politykę biegu z miejsca, którego nie widać w naszym UI. Cicho łamie się tak: „to tylko metadane".
- **5 — nigdy nie wywalaj biegu na nieznanym zdarzeniu.** Nieznane pole front-mattera gospodarza,
  katalog bez `SKILL.md`, plik learnings bez sekcji, brak całego `.claude/` — każde z tych czterech
  jest **normalnym stanem cudzego repo**, nie błędem. Cicho łamie się odwrotnie niż zwykle: przez
  `?`, który zamienia „ten host nie ma umiejętności" w odmowę startu biegu.
- **20 — test sprawdza zachowanie, nie obecność stringa.** Największe ryzyko całego tego pliku:
  `assert!(argv.contains("--plugin-dir"))` przechodzi dla flagi bez wartości i dla katalogu, który
  nie istnieje. Każde kryterium niżej ma akapit o tym, co dokładnie to dyskryminuje.
- **21 — nie pisz artefaktu, którego żaden skrypt nie czyta.** Katalog pluginu ma dokładnie jednego
  czytelnika, `claude --plugin-dir`, i dokładnie jeden kształt, który ten czytelnik rozumie
  (poziom `skills/` jest obowiązkowy). Cicho łamie się tak: dokładamy `commands/`, `hooks/` albo
  `agents/`, bo „plugin i tak to ma" — a S-1 nie zmierzył żadnej z tych powierzchni.
- **23 — polityka w jednym rdzeniu, adaptery po pięć linii.** Reguła „co jest tekstem, a co
  maszynerią" mieszka w `scan.rs` i tylko tam. Cicho łamie się tak: druga lista pól do zdjęcia,
  dopisana w `rewrite.rs`, bo „przy zapisie i tak trzeba sprawdzić".
- **3 i 4 — kod platformowy tylko w `supervisor.rs`; pliki są prawdą.** Bit wykonywalności
  wykrywasz **nie wykrywając go**: `rewrite.rs` zapisuje wyłącznie to, co sam postanowił zapisać,
  więc żaden `PermissionsExt` ani `#[cfg(unix)]` nie jest potrzebny. Katalog pluginu jest wyjściem
  builda i musi dać się skasować bez straty — źródłem jest repo gospodarza.
- **24 — komentuj DLACZEGO, zwłaszcza incydent.** Trzy liczby z tego pliku mają wylądować
  w komentarzach przy liniach, których dotyczą: `54 → 54` przy poziomie `skills/`, `131 vs 1701`
  przy szukaniu nagłówka, `npx -y @playwright/mcp` przy odrzucaniu front-mattera.

## Kryteria akceptacji

## AC-1 Skan umiejętności gospodarza cytuje dosłownie, a katalog bez `SKILL.md` jest normalnym stanem
check: cargo test --test it inherit_scan_skills::
expect: (\d+) passed

Fikstura w `tempfile::tempdir()` ma odwzorować kształt prawdziwego hosta: `<projekt>/.claude/skills/`
z **co najmniej trzema katalogami**, z czego **jeden bez `SKILL.md`** (u gospodarza taki katalog
powstaje po ręcznym usunięciu pliku i po nieudanym `git checkout` — jest tam i nie znaczy awarii),
plus zwykły plik obok katalogów (np. `README.md`). Skan zwraca listę wpisów, każdy z **nazwą
katalogu** i **pierwszą linią jego `SKILL.md` dosłownie**. Asercje: (a) porównanie **całej** listy
z oczekiwanym wektorem wpisów, para po parze, a nie „czy zawiera"; (b) `entries.len() == 2` przy
trzech katalogach na dysku; (c) katalog bez `SKILL.md` **nie ma** wpisu, a skan zwrócił `Ok`, nie
`Err`; (d) katalog bez `SKILL.md` **dalej istnieje** po skanie — skan niczego nie sprząta;
(e) pierwsze linie obu wpisów są **różne** i jedna z nich to `---`, czyli kształt prawdziwego
`SKILL.md` z front-matterem; (f) drugie wywołanie na projekcie **bez** katalogu `.claude/skills`
zwraca pustą listę i `Ok` — repo, które nie ma umiejętności, jest większością repozytoriów.

*Słaba asercja:* `assert!(!entries.is_empty())` albo `assert!(entries.iter().any(|e| e.name ==
"log-sweep"))`. Oba przechodzą dla skanu, który wypisuje **wszystkie** katalogi i wkłada pusty napis
w miejsce pierwszej linii — czyli dla implementacji, w której „pomiń katalog bez `SKILL.md`" nie
istnieje, a człowiek dostaje na ekranie umiejętność, której nie ma. Dyskryminuje to porównanie
**całego wektora** razem z pierwszymi liniami (a) plus twarde `entries.len() == 2` (b): długość
listy jest jedynym miejscem, w którym „pominięty" różni się od „wypisany z pustą treścią".
Punkt (e) domyka drugą stronę: wektor porównany z listą, w której obie pierwsze linie są takie same,
przechodziłby też dla skanu, który wpisuje w to pole **nazwę katalogu** zamiast czytać plik.

## AC-2 Katalog pluginu ma dokładnie kształt, który vendor rozumie, i dokładnie tę treść, która przyszła
check: cargo test --test it inherit_plugin_dir::
expect: (\d+) passed

Przepisanie wybranej listy umiejętności do katalogu pluginu biegu
(`<projekt>/.loadout/runs/<ts>__<id>/plugin/`, katalog podany argumentem — funkcja nie liczy go
sama). Powstaje `.claude-plugin/plugin.json` oraz `skills/<nazwa>/SKILL.md` na **każdą wybraną**
umiejętność. Fikstura hosta ma trzy umiejętności, wybieramy dwie, a obok umiejętności leży **plik
wykonywalny gospodarza** w kształcie, który tam naprawdę jest (`.claude/hooks/format.sh`, `0755`
— u gospodarza jest ich dziesięć) oraz `references/anti-patterns.md` wewnątrz jednej z wybranych
umiejętności. Asercje: (a) `skills/<nazwa>/SKILL.md` istnieje dla obu wybranych; (b) bajty każdego
z nich są **identyczne** z bajtami pliku u gospodarza — porównanie `Vec<u8>`, nie `String` po
`trim`; (c) `.claude-plugin/plugin.json` istnieje i parsuje się jako JSON z polem nazwy; (d) rekurencyjny
spis **wszystkich** ścieżek względnych pod katalogiem pluginu jest **równy** zbiorowi
`{.claude-plugin/plugin.json, skills/<a>/SKILL.md, skills/<b>/SKILL.md}` — ani jednej ścieżki
więcej; (e) w tym spisie nie ma `format.sh`, nie ma nazwy trzeciej, niewybranej umiejętności,
i nie ma `references/anti-patterns.md` — dołączone pliki są nazwanym kosztem, patrz „Świadomie
poza zakresem"; (f) katalog gospodarza jest po operacji **bajt w bajt taki jak przed** —
przepisanie czyta źródło i niczego w nim nie dotyka.

Poziom `skills/` jest **obowiązkowy** i to jest zmierzone: `<katalog>/alpha/SKILL.md` daje plugin,
który się ładuje i rejestruje zero umiejętności [S1 §2, M3: 54 → 54], a `skills/alpha/SKILL.md`
rejestruje obie [M3a: 54 → 56]. `plugin.json` **nie jest** warunkiem działania na CLI 2.1.233 [S1 §3]
i piszemy go mimo to, z konkretnego powodu: umiejętności wracają w `system/init` **z przedrostkiem
od nazwy katalogu** (`s1-plugin-a:alpha`), a nasz katalog nazywa się od biegu — bez przypiętej nazwy
przedrostek zmieniałby się co bieg i żaden ekran nie mógłby go pokazać stabilnie. Zapisz oba te
zdania jako komentarz przy odpowiednich liniach (niezmiennik 24).

*Słaba asercja:* `assert!(dir.join("skills").join(name).join("SKILL.md").exists())`. Przechodzi dla
implementacji, która obok wybranych plików wsypała do katalogu **cały** `.claude/` gospodarza —
razem z `format.sh`, `settings.json` i trzecią umiejętnością — bo pytanie „czy jest" nigdy nie pyta
„czy tylko". To jest dokładnie ta droga, którą maszyneria gospodarza wchodzi do naszego biegu.
Dyskryminuje to (d): **równość zbioru** wszystkich ścieżek, nie zawieranie. Drugi wariant słabości:
`assert_eq!(fs::read_to_string(..).unwrap().trim(), oczekiwane.trim())` — przechodzi dla
implementacji, która przepuściła plik przez `place::emit`, bo `emit` zwraca poprawny `SKILL.md`,
tylko **inny** (przestawione pola, zdjęte `argument-hint`, przecytowane skalary). Dyskryminuje to
(b): porównanie surowych bajtów, bez `trim`, bez `String`.

## AC-3 Flaga `--plugin-dir` powstaje tylko wtedy, gdy jest co odziedziczyć — i nigdy bez wartości
check: cargo test --test it inherit_argv_plugin::
expect: (\d+) passed

**To kryterium sądzi funkcję w module `inherit`, nie `engine/drivers/claude.rs`.** Sterownik składa
argv w `ClaudeDriver::command` i ten plik należy do sąsiedniego zadania fali (odcięcie ustawień,
`--setting-sources ""`, przepisany `permissions.deny`) — dwa zadania piszące do jednego pliku to
kolizja, której ta fala unika z premedytacją. Tutaj powstaje **kompozytor**: funkcja biorąca wynik
przepisania i zwracająca fragment argv (`Vec<String>`), który sterownik dopnie do swojego.
Wiring jest tamtego zadania; ten test nie zna słowa `ClaudeDriver`.

Trzy przypadki, w tej kolejności: (a) po przepisaniu **dwóch** umiejętności fragment jest dokładnie
dwuelementowy — `["--plugin-dir", <ścieżka>]` — a `<ścieżka>` jest **tą samą** ścieżką, którą
przepisanie zapisało jako katalog pluginu i pod którą w tym samym teście leży
`skills/<nazwa>/SKILL.md`; porównanie jako `Path`, nie jako fragment napisu; (b) gdy nie odziedziczono
**niczego** (pusta lista wybranych albo host bez `.claude/skills`), fragment jest **pusty** —
`Vec::is_empty()`, zero elementów; (c) w przypadku (b) katalog pluginu **nie powstał** na dysku:
pusty katalog przekazany vendorowi to plugin ładujący się z zerem umiejętności, czyli ta sama zieleń,
o którą chodzi w AC-2. Dodatkowo: fragment **nigdy** nie zawiera `--plugin-dir` z wartością o zerowej
długości — asercja mówi to wprost, bo `--setting-sources ""` w sąsiednim zadaniu jest flagą, której
pusty argument jest **poprawny**, i pomylenie tych dwóch kształtów jest realne.

*Słaba asercja:* `assert!(argv.contains(&"--plugin-dir".to_string()))`. Przechodzi dla kompozytora,
który wypisuje flagę **zawsze** — także przy pustym dziedziczeniu — i przechodzi dla flagi bez
wartości, bo `contains` pyta o jeden element, a `--plugin-dir` bez wartości połknie następną flagę
sterownika jako swój argument. Dyskryminują to trzy rzeczy razem: dokładna **długość** fragmentu
(2 albo 0, nigdy 1), porównanie drugiego elementu z realną ścieżką z AC-2 jako `Path`, oraz (c) —
nieistnienie katalogu w przypadku pustym. Bez (c) „pusty katalog nie trafia do argv" jest spełnialne
przez kompozytor, który katalog i tak stworzył, tylko go nie wymienił.

## AC-4 Do promptu jedzie wyłącznie `## Recurring patterns` — a naiwne szukanie trafia w zdanie o nich
check: cargo test --test it inherit_recurring_patterns::
expect: (\d+) passed

Wycięcie sekcji: z pliku learnings bierzemy **wyłącznie** tekst między nagłówkiem
`## Recurring patterns` a następnym nagłówkiem `## `. Fikstura ma odwzorować prawdziwy plik roli
i to jest połowa wartości tego kryterium:

1. w trzeciej linii, **przed** prawdziwym nagłówkiem, stoi cytat blokowy zawierający dosłownie
   `` `## Recurring patterns` `` — u gospodarza jest w każdym z dziewięciu plików ról;
2. prawdziwy nagłówek niesie przyrostek: `## Recurring patterns (BINDING — do NOT repeat)` —
   nagłówka **równego** dosłownie `## Recurring patterns` nie ma w żadnym z dziesięciu plików
   gospodarza [zmierzone 2026-08-19];
3. sekcja patterns zawiera zdanie ze znacznikiem `PATTERNS-ONLY-9ac7`;
4. sekcja `## Run journal` jest **co najmniej dziesięć razy dłuższa** i jej **pierwsza linia po
   nagłówku** zawiera `JOURNAL-ONLY-4b21`.

Asercje: (a) wynik zawiera `PATTERNS-ONLY-9ac7`; (b) wynik **nie** zawiera `JOURNAL-ONLY-4b21`;
(c) wynik nie zawiera napisu `## Run journal` ani nie zaczyna się od `## ` — nagłówki są granicami,
nie treścią; (d) `wynik.len() * 5 < plik.len()`, czyli mniej niż 20% bajtów pliku; (e) plik **bez**
sekcji daje wynik pusty i `Ok`, nigdy błąd; (f) sekcja będąca **ostatnią** w pliku (bez następnego
`## `) jest cięta do końca pliku, a nie gubiona.

Kontekst budżetowy, który tu rozstrzyga i który zapisz w komentarzu: zmierzone u gospodarza
2026-08-19 — `backend-dev.md` **1701 z 32922 bajtów (5,2%)**, `orchestrator.md` **2016 z 73258
bajtów (2,8%)**. Reszta pliku, do 73 KB `## Run journal`, **nigdy** nie wchodzi do budżetu tokenów,
i to jest cała różnica między wstrzykiwaczem a wklejaniem pliku.

*Słaba asercja:* `assert!(!wynik.is_empty())` albo samo (a). Oba przechodzą dla implementacji, która
zwraca **cały plik** — bo cały plik zawiera zdanie z patterns. Przechodzą też dla implementacji
z naiwnym `text.find("## Recurring patterns")`, jeśli fikstura nie ma cytatu z punktu 1: wtedy
wycięcie jest przypadkiem poprawne i test milczy o pułapce, która na prawdziwym pliku daje **131
bajtów zdania o regułach zamiast 1701 bajtów reguł**. Dyskryminują to trzy asercje naraz: (b) —
znacznik z journalu, którego w wyniku być nie może; (d) — próg 20% bajtów, którego „cały plik" nie
przechodzi przy dziesięciokrotnie dłuższym journalu; oraz punkt 1 fikstury, bez którego (a) jest
spełnialne przez trafienie w cytat blokowy. Czwarta pułapka jest po drugiej stronie: implementacja
porównująca nagłówek **dosłownie** (`line == "## Recurring patterns"`) przechodzi (e) — bo nigdy nic
nie znajduje — i pada na (a), i dlatego punkt 2 fikstury (przyrostek w nagłówku) nie jest ozdobą.

## AC-5 Podagent gospodarza jedzie jako tekst; front-matter zostaje po jego stronie granicy
check: cargo test --test it inherit_agents_are_text::
expect: (\d+) passed

Podagenci gospodarza (`<projekt>/.claude/agents/*.md`) są dziedziczeni **wyłącznie jako tekst**:
bierzemy **ciało** pliku (wszystko po drugim `---`), a **cały** front-matter jest odrzucany.
Fikstura odwzorowuje `.claude/agents/e2e-author.md` gospodarza i musi mieć w nagłówku pięć pól:
`tools`, `model`, `permissionMode`, `memory` oraz `mcpServers` z zagnieżdżonym
`command: npx` i `args: ["-y", "@playwright/mcp@0.0.75"]`. Ciało zawiera zdanie ze znacznikiem
`BODY-ONLY-7d10`.

Asercje: (a) wynik zawiera `BODY-ONLY-7d10`; (b) wynik **nie zawiera żadnej** z pięciu nazw pól —
pięć osobnych asercji, po jednej na pole, żeby komunikat porażki nazywał **które** przeszło;
(c) wynik nie zawiera `npx` ani `@playwright/mcp` — wartości, nie tylko klucze, bo `args:` w cudzym
pliku może stać pod inną nazwą klucza; (d) wynik nie zawiera separatora `---` z nagłówka;
(e) plik **bez** front-mattera zwraca całe swoje ciało nietknięte, a plik z `---` w pierwszej linii,
które **nigdy się nie domyka**, zwraca całą treść razem z tą kreską — to jest lustro reguły
`ingest::parse_doc` i front-matter bez domknięcia nie jest front-matterem; (f) kontrola przeciw
pustemu czytaniu: wynik ma co najmniej dwie linie i jest krótszy niż plik.

Powód, dla którego to jest **granica maszynerii**, a nie sprzątanie: `mcpServers` z tego pliku
uruchamia proces (`npx -y @playwright/mcp@0.0.75`) **poza grupą procesów Loadouta**. Taki proces nie
wchodzi ani do dowodu śmierci grupy (niezmiennik 6), ani do żadnego licznika kosztu — a zmierzone
2026-08-19 osierocenia (14 w jednym biegu, 30 łącznie) i 38–41 tys. tokenów spalonych poza
rozliczeniem to dokładnie ten sam wypadek, tylko wywołany z innej strony. `tools` i `permissionMode`
przepisują politykę biegu z miejsca, którego nasze UI nie pokazuje; `memory` wskazuje cudzy katalog
pamięci; `model` cicho zmienia rachunek. Front-matter jest granicą, po naszej stronie zostaje ciało.

*Słaba asercja:* `assert!(!wynik.contains("mcpServers"))` jako jedyna asercja negatywna. Przechodzi
dla implementacji, która front-mattera **nie wycina**, tylko **filtruje z niego znane pola**:
`mcpServers` znika, a `tools`, `memory` i pierwsze pole dołożone przez vendora jadą dalej. To jest ta
sama wada, co czarna lista ścieżek startu procesu z sąsiedniego zadania — **czarna lista jest
z definicji niekompletna i cicho pęknie przy następnym wydaniu CLI.** Przechodzi też dla
implementacji, która zdejmuje sam **wiersz** `mcpServers:` i zostawia w wyniku jego wcięte dzieci:
`command: npx` i `args: ["-y", "@playwright/mcp@0.0.75"]`, czyli dokładnie te dwie wartości, które
uruchamiają proces. Dyskryminują to trzy asercje naraz: (b) — pięć osobnych sprawdzeń, po jednym na
pole, więc komunikat porażki nazywa to, które przeszło; (c) — sprawdzenie **wartości**, nie tylko
nazw kluczy; (d) — brak separatora `---` w wyniku, bo tylko wycięcie **całego** bloku zdejmuje
jednocześnie klucze, wartości i kreskę. Filtr pól zostawia przynajmniej kreskę.

## Świadomie poza zakresem

- **Odcięcie ustawień gospodarza, `--setting-sources ""`, przepisany `permissions.deny`, biała lista
  `--tools`.** To sąsiednie zadanie tej fali i to ono posiada `engine/drivers/claude.rs`. Tutaj
  powstaje wyłącznie fragment argv z `--plugin-dir` (AC-3) — kompozytor, nie wiring. Jeśli zauważysz,
  że sterownik potrzebuje z `inherit` czegoś więcej niż tego fragmentu, **zapisz to w uwagach**
  zamiast dopisywać do `claude.rs` (AGENTS.md §7).
- **Dołączone pliki umiejętności (`references/`, `assets/`).** Do katalogu pluginu jedzie sam
  `SKILL.md` i AC-2 to asertuje. To jest **nazwany koszt**, nie przeoczenie: umiejętność gospodarza,
  której proza mówi „przeczytaj `references/flow-backend.md`", degraduje się do samego `SKILL.md`
  i model nie znajdzie pliku. Zgłoś to w uwagach z listą umiejętności, których to dotyczy —
  u gospodarza takich katalogów jest cztery na siedemnaście. Rozstrzygnięcie „przenosimy też
  `references/`" wymaga decyzji o budżecie i należy do człowieka.
- **`scripts/` wewnątrz umiejętności gospodarza — nigdy, i to nie jest zakres do rozszerzenia.**
  Skrypt jest maszynerią z definicji. `SKILL.md`, który każe uruchomić `scripts/run.sh`, ma w naszym
  biegu nie znaleźć tego pliku i to jest zachowanie zamierzone.
- **Pozostałe powierzchnie pluginu (`commands/`, `agents/`, `hooks/`, `mcp.json`).** S-1 nie zmierzył
  żadnej z nich [S1 §3: „this spike measured no other plugin surface"]. Katalog pluginu ma dokładnie
  dwie rzeczy z AC-2 i ani jednej więcej (niezmiennik 21).
- **Ekran wyboru, checkboxy „All skills / Only these", podgląd wyciętych patterns.** UI należy do
  T-13; tutaj powstają funkcje, które ten ekran zawoła. Kontrolka bez handlera nie wchodzi do repo
  (niezmiennik 16), ale handler bez kontrolki jest normalnym stanem przez jedno zadanie.
- **Podłączenie `inherit` do biegu — kto woła skan, kiedy powstaje katalog pluginu, co ląduje
  w `run.json`.** `lib.rs` dostaje **jeden wiersz `pub mod inherit;`** i nic więcej. Ścieżka biegu
  (`commands/run.rs`) nie jest w OWNS.
- **Podpięcie learnings pod konkretnego agenta i zapis nowych lekcji.** Wycinamy sekcję (AC-4);
  **kto** dostaje **czyj** plik i czy Loadout kiedykolwiek do niego dopisuje, to osobna decyzja.
  Zapis do `.claude/learnings/` gospodarza jest tu zakazany bez wyjątku: dziedziczymy przez
  czytanie, a repo gospodarza jest dla nas tylko do odczytu.
- **16 umiejętności, które przeżywają `--setting-sources ""`** (`deep-research`, `dataviz`, `code-review`,
  `run`, …) [S1 §0]. Nie są nasze, nie da się ich zdjąć niczym poza `--disable-slash-commands`,
  które zeruje wszystko, i **nie są materiałem na checkbox**. Uczciwa obietnica w UI brzmi „only
  these, plus the CLI's own bundled skills" — to jest tekst dla T-13, nie zachowanie dla tego pliku.

<!-- OWNS
src-tauri/src/inherit/mod.rs
src-tauri/src/inherit/scan.rs
src-tauri/src/inherit/rewrite.rs
src-tauri/src/lib.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/inherit_scan_skills.rs
src-tauri/tests/it/inherit_plugin_dir.rs
src-tauri/tests/it/inherit_argv_plugin.rs
src-tauri/tests/it/inherit_recurring_patterns.rs
src-tauri/tests/it/inherit_agents_are_text.rs
-->
