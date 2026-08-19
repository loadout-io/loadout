# T-53 — Powierzchnia narzędzi i ustawień sterownika `claude`: biała lista zamiast czarnej, własny plik ustawień zamiast cudzego

**Harness jest NASZ. Z repo gospodarza dziedziczymy TEKST, nigdy MASZYNERIĘ — i dziedziczymy go
przez PRZEPISANIE do siebie, nie przez wczytanie jego ustawień.** To zdanie jest zasadą całej tej
fali i to zadanie jest jej pierwszym egzekutorem: reguła zapisana w cudzym `settings.json` może
u nas zostać, ale wyłącznie jako **napis, który sami przepisaliśmy do własnego pliku**. Wszystko,
co w tamtym pliku jest maszyną — hooki, `env`, `sandbox`, lista `allow` — nie wsiada do naszego
biegu w ogóle, a jedynym sposobem, żeby tego dopilnować, jest **nie wczytać tamtego pliku**.

Cicha porażka numer jeden, i to ta droga. Sterownik startuje `claude` w projekcie gospodarza,
który ma własny `.claude/settings.json` z hakami. Hak `PreToolUse` gospodarza startuje proces we
**własnej grupie procesów**; jego dziecko dostaje `ppid=1` i **przeżywa wyjście `claude`**
[zmierzone 2026-08-19: jeden bieg zostawił 14 sierot, eksperymenty łącznie 30]. Krok się kończy,
zdarzenie `Finished` przychodzi, dowód śmierci grupy z niezmiennika 6 jest prawdziwy — i nic z tego
nie dotyczy procesu, który już nie jest w naszej grupie. Nic nie pęka. Bramka jest zielona. Sierota
pali limit w tle i dowiadujesz się o niej z rachunku. **Przy załadowanych ustawieniach gospodarza
niezmiennik 6 jest nie do utrzymania** — nie dlatego, że supervisor jest słaby, tylko dlatego, że
grupa procesów, którą zabija, nie jest tą, w której cudzy hak wystartował swoje dziecko.

Cicha porażka numer dwa: **czarna lista udająca ogranicznik**. `--allowedTools` to lista
**auto-zatwierdzania**, nie filtr dostępności — narzędzie spoza niej dalej jest w zestawie, tylko
zapyta. W biegu bez człowieka „zapyta" nie znaczy „nie zrobi": zmierzone 2026-08-19, agent Loadouta
wywołał **projektowego podagenta repo gospodarza**, ten wystartował jako osobny proces i spalił
**38–41 tys. tokenów całkowicie poza widokiem i rozliczeniem Loadouta**. Ani jednej czerwieni, ani
jednego wiersza na ekranie pracy, ani jednego dolara w podsumowaniu kroku. Koszt po prostu nie
pojawił się w paragonie.

Cicha porażka numer trzy, najgorsza, bo odnawialna: **czarna lista jest z definicji niekompletna**.
Domyślna powierzchnia narzędzi ma dziś **osiem ścieżek startu procesu** — `Task`, `Workflow`,
`SendMessage`, `CronCreate`, `RemoteTrigger`, `ScheduleWakeup`, `EnterWorktree`, `Monitor` — i lista
rzeczy zakazanych dostaje dziurę przy najbliższym wydaniu CLI, po cichu, bo nikt nie czyta changelogu
pod kątem „czy przybyło czasowników". Lista rzeczy **dozwolonych** dziury nie dostaje: nowe narzędzie
po prostu na nią nie wchodzi.

Cicha porażka numer cztery, ta, która wywraca intuicję: **ustawienia gospodarza mogą nas
ROZSZERZYĆ, nie tylko ograniczyć**. `sandbox.autoAllowBashIfSandboxed: true` przepuszcza dowolną
komendę **mimo** `--allowedTools`, a blok `env` gospodarza **nadpisuje** środowisko podane przez
Loadouta — czyli przewraca `env_clear()` z niezmiennika 9 od zewnątrz. „Wczytajmy jego ustawienia,
przecież on wie lepiej, czego u siebie zabrania" nie jest ostrożnością. To jest oddanie kierownicy.
Dlatego przepisujemy **wyłącznie `permissions.deny`** i robimy to do **swojego** pliku.

## Fakty zmierzone, na których to zadanie stoi

Wszystkie [zmierzone 2026-08-19]. Nie podważaj ich w implementacji — jeśli któryś okaże się
nieprawdziwy w twoim biegu, to jest **znalezisko dla człowieka**, nie powód do zmiany kryterium
(`AGENTS.md` §7).

| Fakt | Konsekwencja dla tego zadania |
|---|---|
| `--setting-sources ""` to **jedyna** dźwignia wyłączająca haki repo gospodarza | zostaje w `LEAN_CONTEXT` dokładnie raz i z argumentem o zerowej długości |
| `--settings <plik>` **sumuje się** z projektowym i **nie gasi** hooków, nawet podany z pustą listą `PreToolUse` | `--settings` nie jest izolacją; jest nośnikiem naszego `deny` i niczym więcej |
| `--settings <plik>` **działa samodzielnie** przy `--setting-sources ""` i egzekwuje przepisane `permissions.deny` | to jest cała droga, którą reguła gospodarza do nas wraca |
| `--bare` gasi haki, ale rozbija OAuth (`Not logged in`) | na subskrypcji bezużyteczna — potwierdza to, co `claude.rs` mówi w nagłówku od T-04 |
| `--allowedTools` to lista **auto-zatwierdzania**, nie filtr dostępności | zostaje, ale przestaje być jedynym ogranicznikiem |
| `--tools` to twarda **biała lista dostępności**; usuwa `Task`/`Agent`/`Skill`/`Workflow` z zestawu | to jest nowa flaga tego zadania |
| `claude --help` o `--tools`, dosłownie: *„Specify the list of available tools from the built-in set. Use `""` to disable all tools, `default` to use all tools, or specify tool names (e.g. `Bash,Edit,Read`)"* | forma to **jedno** wystąpienie flagi i **jeden** argument z przecinkami, jak `--allowedTools`; `""` i `default` są słowami vendora o dwóch skrajnościach i **żadna polityka nie ma prawa ich wysłać** |
| `--plugin-dir` wnosi umiejętności przy całkowicie odciętych ustawieniach projektu | **nie w tym zadaniu** — patrz „Świadomie poza zakresem" |

**Read first:**
`src-tauri/src/engine/drivers/claude.rs` — nagłówek modułu („Trzy rzeczy, które w tym pliku wychodzą
cicho źle", w tym pomiar 36 870 vs 4 725 tokenów i `--bare`), stała `TRANSPORT` (sześć flag
transportu), stała `LEAN_CONTEXT` (`--strict-mcp-config --setting-sources ""` i akapit o argumencie
o **zerowej** długości), `permission_flags` (~linia 200 — **jedyna** tabela tłumaczenia polityki na
flagi, niezmiennik 23, i akapit o tym, dlaczego `Unrestricted` nie dostaje `--allowedTools`),
`ClaudeDriver::command` (~linia 336 — tabela argv w doc-komentarzu, którą **rozszerzasz o dwa
wiersze**), `ClaudeDriver::with_transcript` (wzorzec szwu, który kopiujesz: budowniczy przez wartość,
nie pole w `RunSpec`);
`src-tauri/src/engine/drivers/mod.rs` — enum `Policy` (trzy warianty i doc, który mówi, że „żaden
adapter nie ma prawa udawać, że jakaś lista narzędzi jeszcze coś tu ogranicza [T1 §5.2]" — przeczytaj
go uważnie, poprawiasz w nim **jedno zdanie**), struktura `RunSpec` (i akapit „Mina" niżej o tym,
dlaczego jej **nie dotykasz**);
`src-tauri/tests/it/claude_argv_transport.rs` i `claude_argv_policy.rs` — kryteria AC-1 i AC-2
z T-04; **oba zostają zielone**, a ich metoda („asercja jest na SĄSIEDZTWIE, nie na obecności";
`value_after(args, flag)`) jest tą, którą powtarzasz;
`src-tauri/tests/it/main.rs` — nagłówek modułu: jeden cel testowy zamiast 122 i dlaczego plik bez
linii `mod` „wygląda dokładnie jak zestaw, który przeszedł";
`.claude/settings.json` **tego** repo — fikstura z AC-4 jest ulepiona z jego prawdziwego kształtu:
`env`, `permissions.allow`, `permissions.deny`, `hooks.Stop`, `hooks.PostToolUse`;
`docs/ARCHITECTURE.md` §8 (warstwa plików — gdzie leżą artefakty biegu);
`AGENTS.md` §2a oraz §3 (niezmienniki 1, 6, 9, 20, 21, 23, 24, 28).

## Kto to robi

- **Agent:** `rust-core`
- **Druga opinia:** `codex` (pisze `claude`; recenzent nie ma interesu w tym, żeby powierzchnia
  narzędzi `claude` była szeroka — decyzja D3)
- **Artefakty biegu:** `runs/T-53/` — nigdy `$TMPDIR`

## Co to zadanie posiada

- `src-tauri/src/engine/drivers/claude.rs` — **cztery** zmiany i ani jednej więcej: (1) tabela
  białej listy narzędzi wyprowadzona z `Policy`, stojąca **obok** `permission_flags`, nie zamiast
  niej; (2) `--tools <lista>` w `command()`; (3) zapis pliku ustawień biegu; (4) `--settings
  <ścieżka>` w `command()`. Tabela argv w doc-komentarzu `command()` dostaje dwa nowe wiersze —
  ta tabela jest czytana przez ludzi i rozjechana z kodem jest gorsza niż jej brak (niezmiennik 24).
- `src-tauri/src/engine/drivers/host.rs` — **nowy plik**: czytanie i przepisanie reguł gospodarza.
  Ścieżkę projektu bierze **argumentem**, nigdy nie zgaduje jej z `cwd` procesu i nigdy nie pyta
  o nią Tauri (niezmiennik 1 — ten plik nie zna słowa „tauri" tak samo jak reszta `engine/`).
  Sąsiad `claude.rs`, nie część rdzenia: `.claude/settings.json` to kształt jednego vendora, a
  `mod.rs` nie zna ani jednego vendora.
- `src-tauri/src/engine/drivers/mod.rs` — **dokładnie dwie rzeczy**: linia `pub mod host;` oraz
  **jedno zdanie** poprawki w doc-komentarzu `Policy::Unrestricted`. To zdanie dziś mówi, że żaden
  adapter nie ma prawa udawać, iż lista narzędzi coś tu ogranicza — i **zostaje prawdziwe
  o `--allowedTools`**, bo lista auto-zatwierdzania rzeczywiście nie wiąże `bypassPermissions`
  [T1 §5.2]. Przestaje być prawdziwe o `--tools`, która jest twardą listą dostępności [zmierzone
  2026-08-19]. Dopisz to rozróżnienie jednym zdaniem z datą. Nic więcej w tym pliku.
- `src-tauri/tests/it/main.rs` — **wyłącznie po to**, żeby dopisać cztery linie `mod` dla nowych
  plików testów: `mod driver_claude_tool_surface;`, `mod driver_claude_policy_surface;`,
  `mod driver_claude_settings_file;`, `mod host_deny_rewrite;`. Bez tych linii pliki **nie
  kompilują się do niczego**, nie uruchamiają ani jednego testu i **wyglądają dokładnie jak zestaw,
  który przeszedł** — pilnuje tego `checks/quick-tests-listed.sh`, ale zobaczysz to dopiero
  w bramce, a `./verify.sh before` powie ci wtedy nieprawdę. Żadnej innej zmiany w tym pliku.
- `src-tauri/tests/it/driver_claude_tool_surface.rs`, `driver_claude_policy_surface.rs`,
  `driver_claude_settings_file.rs`, `host_deny_rewrite.rs` — po jednym pliku na kryterium, bo
  `check:` wskazuje **jeden** plik po ścieżce. To są moduły jednego celu `it`, nie osobne cele.

**Mina, na którą to repo weszło już trzy razy: nie dokładaj pola do `RunSpec`.** Zmierzone
2026-08-19 (`grep -rln 'RunSpec {' src-tauri/`): strukturę konstruuje literałem **trzynaście plików
spoza tego bloku OWNS** — `commands/chat.rs`, `commands/run.rs`, `commands/skills.rs`,
`tests/flow_say_to_agent.rs` i dziewięć plików w `tests/it/`. Nowe pole wywraca kompilację ich
wszystkich, czyli robi czerwień **poza OWNS**, której nie masz prawa naprawić — dokładnie to zrobił
commit kontraktowy T-30 sześciu specyfikacjom T-15. Szew jest budowniczym na sterowniku
(`ClaudeDriver::with_settings(...)`, kopia `with_transcript`), a nie polem we wspólnej strukturze.
To samo dotyczy sygnatury `AgentDriver` i wariantów `Policy`: nie ruszasz ich.

**Najmniejszy kształt, który te cztery kryteria osiągają.** Wolno wybrać inny, pod jednym warunkiem:
kryterium ma go dosięgnąć **bez dotykania pliku spoza OWNS**.

```
host.rs     pub fn deny_rules(project: &Path) -> Vec<String>
claude.rs   pub fn tools_for(policy: Policy) -> &'static [&'static str]
claude.rs   pub struct RunSettings { .. }  ::write(dir, deny) -> Result<RunSettings>  ::path()
claude.rs   impl ClaudeDriver { pub fn with_settings(self, settings: RunSettings) -> Self }
```

**Czerwień „before" ma być czerwienią z asercji, nie z kompilatora.** `todo!()` jest w tym repo
zakazane (`todo = "deny"` w `[workspace.lints.clippy]`), więc zaślepki **kompilują się i zwracają
pustą wartość albo `Err`**: `tools_for` oddaje `&[]`, `deny_rules` oddaje `Vec::new()`, `write`
oddaje `Err`, `command()` nie dokłada jeszcze żadnej z dwóch flag. Uwaga na uczciwość takiego
szkieletu: **pusta biała lista przechodzi asercję o pustym przecięciu z listą zakazaną** — i to
jest dokładnie powód, dla którego AC-1 nosi kontrolę „co najmniej dwie pozycje". Sprawdź przed
implementacją, że **każde** z czterech kryteriów jest na szkielecie czerwone; kryterium zielone
w warstwie `before` nic nie sprawdza.

## Niezmienniki

- **6 — zabijamy grupę procesów i dowodzimy, że nie żyje.** To jest niezmiennik, którego to zadanie
  broni, choć nie dopisuje do supervisora ani jednej linii: dopóki bieg ładuje ustawienia
  gospodarza, cudzy hak startuje proces w **swojej** grupie, jego dziecko dostaje `ppid=1` i dowód
  śmierci **naszej** grupy jest prawdziwy i bez znaczenia. Cicho łamie się tak: ktoś dokłada
  `--settings <plik>` i „dla pewności, żeby się wczytał" dopisuje drugie `--setting-sources project`.
- **9 — prompt i sekrety wyłącznie przez stdin, `env_clear()` plus jawna lista.** Dotyczy tu dwóch
  rzeczy naraz. Pierwsza: `--settings` przyjmuje **albo ścieżkę, albo JSON w argumencie** — wersja
  z treścią w argv jest tym samym wyciekiem co prompt w argv, tylko trudniej ją zobaczyć.
  Druga: blok `env` gospodarza nadpisuje środowisko, które podał Loadout, czyli przewraca
  `env_clear()` od zewnątrz. Dlatego `env` nie przechodzi przez przepisanie **nigdy**.
- **20 — test sprawdza zachowanie, nie obecność stringa.** Żaden z czterech testów nie czyta
  `claude.rs` ani `host.rs` z dysku. Pytamy **zbudowaną komendę** i **zapisany plik**. Selftest
  w repo źródłowym asertował obecność flagi w skrypcie, przechodził **na komentarzu**, a żywa flaga
  brzmiała inaczej [raport 06 §2].
- **21 — nie pisz artefaktu, którego nikt nie czyta.** Plik ustawień biegu ma dokładnie jednego
  czytelnika: proces, który startujemy. Jeżeli powstaje, a `--settings` nie stoi w argv z **jego**
  ścieżką, to jest śmieć w katalogu biegu i jednocześnie cała izolacja, której nie ma.
- **23 — polityka mieszka w jednym rdzeniu, adaptery mają po pięć linii.** `Policy` zostaje trzema
  wariantami po ludzku w `mod.rs`. Biała lista narzędzi to **nazwy narzędzi Claude**, więc jej
  miejsce jest w `claude.rs`, obok `permission_flags`, jako **druga kolumna tej samej decyzji** —
  nie druga decyzja w drugim miejscu. Cicho łamie się tak: `if agent == "claude"` w miejscu
  wywołania, i tak właśnie po cichu umarło skanowanie sekretów w repo źródłowym.
- **24 — komentuj DLACZEGO, zwłaszcza incydent.** Dziesięć zakazanych nazw to nie jest gust.
  Przy tabeli białej listy ma stać datowany powód: osiem z nich startuje proces poza naszą grupą,
  a jeden z nich zmierzalnie spalił 38–41 tys. tokenów poza rozliczeniem [2026-08-19].
- **28 — najpierw skrypt albo hak, dopiero potem prompt.** To zadanie jest wzorcowym stopniem (3)
  z tej listy: zamiast pisać agentowi „nie wołaj podagentów", **czynimy to niemożliwym** przez
  powierzchnię narzędzi. Prompt byłby miękki, rósłby monotonicznie i kosztował tokeny w każdym
  biegu na zawsze; biała lista kosztuje raz i sama siebie testuje.
- **1 — `engine/` nie importuje `tauri::*`.** `host.rs` dostaje ścieżkę projektu argumentem.

## Kryteria akceptacji

## AC-1 `--tools` jest białą listą i nie ma na niej ani jednej ścieżki startu procesu
check: cargo test --test it driver_claude_tool_surface::
expect: (\d+) passed

Dla **każdej z trzech polityk** zbuduj komendę (`ClaudeDriver::new().command(&spec)`), weź wartość
stojącą **zaraz za** `--tools` i rozbij ją po przecinku. Każdą pozycję znormalizuj do części
**przed** nawiasem otwierającym, bo składnia zakresowa (`Bash(git *)`) należy do `--allowedTools`,
a wpis `Task(*)` w białej liście byłby tą samą ścieżką startu procesu w przebraniu.

Lista zakazana jest wypisana **w teście, dosłownie**, jako stała — dziesięć nazw:
`Task`, `Agent`, `Skill`, `Workflow`, `SendMessage`, `CronCreate`, `RemoteTrigger`,
`ScheduleWakeup`, `EnterWorktree`, `Monitor`. Nie importuj jej z `claude.rs`: test, który czyta tę
samą listę, co kod, zawsze się z nim zgadza i nie mierzy niczego (niezmiennik 20). Asercja:
**przecięcie białej listy z listą zakazaną jest puste** dla wszystkich trzech polityk, a komunikat
nazywa politykę i konkretną pozycję, która się przedostała.

Kontrola przeciw pustemu czytaniu, bez niej całe kryterium jest ozdobą: `--tools` **musi być
znalezione** (brak flagi to `Err` z testu, nie cisza), biała lista **każdej** polityki ma **co
najmniej dwie pozycje**, a jej wartość nie jest ani pusta, ani równa `default` — to są dwa słowa
vendora o dwóch skrajnościach: `""` znaczy „żadnych narzędzi", `default` znaczy „wszystkie", czyli
dokładnie stan, przed którym stoi to zadanie.

*Słaba asercja:* `assert!(forbidden.iter().all(|f| !tools.contains(f)))` przechodzi **idealnie** dla
sterownika, który `--tools` nie wysyła w ogóle — przecięcie z pustą listą jest puste, więc test
świeci na zielono dokładnie w stanie, w którym jesteśmy dziś, przed napisaniem jednej linii.
Rozróżniają to dwie rzeczy: `value_after(&args, "--tools").ok_or("--tools is missing")?` (brak flagi
jest porażką, nie milczeniem) oraz `assert!(entries.len() >= 2, ...)` osobno dla każdej z trzech
polityk. Druga słaba wersja jest subtelniejsza: `value.contains("Task")` na **sklejonym** stringu
jest jednocześnie za czułe (zapala się na hipotetycznym, legalnym `TaskOutput`) i za mało czułe
w drugą stronę, bo porównanie **surowej** pozycji przez `==` przepuszcza `Task(*)`. Dlatego
normalizacja przez ucięcie na `(` i porównanie **równością**, a nie zawieraniem.

## AC-2 Trzy polityki, trzy niepuste powierzchnie, a `--allowedTools` przestaje być jedynym ogranicznikiem
check: cargo test --test it driver_claude_policy_surface::
expect: (\d+) passed

Cztery rzeczy, wszystkie na zbudowanej komendzie.

**Pierwsza: `Unrestricted` nie dostaje pustego `--tools`.** Pusty argument to w słowniku vendora
„wyłącz wszystkie narzędzia", więc agent, któremu obiecano „No limits", nie mógłby przeczytać ani
jednego pliku. Wartość dla `Unrestricted` jest niepusta, ma co najmniej dwie pozycje i nie jest
`default`.

**Druga: trzy polityki mają trzy różne białe listy, i to w konkretnym porządku.** `Write` ani
`Edit` **nie występują** w liście `ReadOnly` — to jest asercja o zachowaniu, nie o różności napisów.
Dalej: `ReadOnly ⊊ EditInFolder ⊊ Unrestricted`, ostro na każdym kroku. Agent obiecany jako
czytający nie ma prawa mieć pod ręką pisania, a agent bez ograniczeń nie ma prawa mieć **mniej** niż
ten, który edytuje folder.

**Trzecia: `--allowedTools` zostaje wyłącznie do auto-zatwierdzania.** Wszędzie tam, gdzie flaga
występuje, jej pozycje — znormalizowane tak samo jak w AC-1 — są **podzbiorem** `--tools` tej samej
polityki. Narzędzie auto-zatwierdzone, ale niedostępne, jest obietnicą, której proces nie może
dotrzymać, a czytający argv w nią uwierzy. Kryterium T-04 (`claude_argv_policy.rs`) dalej mówi, że
`Unrestricted` **nie wysyła** `--allowedTools`, i to zadanie tego nie rusza: to są dwie różne flagi
o dwóch różnych znaczeniach i cała ta fala jest o tym rozróżnieniu.

**Czwarta: `--setting-sources` występuje dokładnie RAZ, a jego argument ma ZERO znaków.** Liczba
wystąpień, nie obecność.

*Słaba asercja:* `assert!(!tools.is_empty())` powtórzone dla trzech polityk przechodzi dla adaptera,
który wypisuje **jedną i tę samą** listę wszystkim trzem — czyli dla dokładnie tej pomyłki, którą
T-04 nazwało już raz przy `--permission-mode` („trzy polityki po ludzku muszą dojść do CLI jako trzy
różne tryby"). Rozróżnia to łańcuch ostrych zawierań plus jawna nieobecność `Write` i `Edit`
w `ReadOnly`. Druga słaba wersja siedzi w punkcie czwartym: `has_flag(&args, "--setting-sources")`
przechodzi dla argv, które niesie tę flagę **dwa razy**, drugi raz z `project` — a to jest dokładnie
ten kształt, w którym haki gospodarza wracają tego dnia, w którym ktoś doda `--settings` i „zrobi,
żeby się wczytywało". Widzi to wyłącznie
`args.iter().filter(|a| **a == OsStr::new("--setting-sources")).count() == 1` postawione razem
z asercją, że argument obok jest pusty.

## AC-3 Plik ustawień biegu piszemy my i jest w nim jeden klucz
check: cargo test --test it driver_claude_settings_file::
expect: (\d+) passed

W katalogu z `tempfile::tempdir()` każ sterownikowi zapisać plik ustawień biegu, podając mu listę
reguł `deny` z osobliwym znacznikiem (np. `Read(LOADOUT-T53-DENY-MARKER/**)`). Potem zbuduj komendę
tego samego sterownika i **czytaj plik ze ścieżki wziętej z argv**, nie ze ścieżki zwróconej przez
zapis — to jedna asercja mniej i jedno spięcie obu połówek więcej.

1. `--settings` występuje **dokładnie raz**, a wartość obok jest ścieżką **do tego pliku**
   (porównanie `Path`, nie „flaga jest"). Wartość **nie zaczyna się od `{`**: `--settings` przyjmuje
   też JSON wprost w argumencie, a treść w argv widzi `ps` każdego użytkownika maszyny — to jest
   kształt z niezmiennika 9 także wtedy, gdy nie chodzi o prompt.
2. Plik istnieje pod **podanym** katalogiem. Sterownik nie wybiera sobie miejsca sam i nie pisze
   do `$TMPDIR`: artefakty biegu leżą w katalogu biegu (`docs/ARCHITECTURE.md` §8).
3. `serde_json::from_str::<Value>` na treści pliku **się udaje**.
4. Klucze najwyższego poziomu to **dokładnie** `["permissions"]`, a klucze `permissions` to
   **dokładnie** `["deny"]`. Porównanie posortowanego wektora kluczy, nie `get(..).is_some()`.
5. Lista `deny` niesie znacznik, w tej samej kolejności, w której ją podano.
6. **Surowy tekst pliku nie zawiera** napisów `allow`, `env`, `sandbox` ani `hooks` — nigdzie,
   na żadnym poziomie zagnieżdżenia.

*Słaba asercja:* `assert!(doc.get("permissions").and_then(|p| p.get("deny")).is_some())` przechodzi
dla dokumentu, który **oprócz** `deny` niesie `env` i `hooks` przepisane hurtem z gospodarza —
czyli dla dokładnie tego pliku, który przywraca maszynerię, po której pozbycie się to zadanie
istnieje. Rozróżniają to dwie asercje o **całych zbiorach kluczy** na obu poziomach plus przemiatanie
surowego tekstu z punktu 6: zagnieżdżony przemyt przechodzi każde sprawdzenie kluczy najwyższego
poziomu. Druga słaba wersja jest po stronie argv: `has_flag(&args, "--settings")` przechodzi dla
sterownika, który flagę stawia, a pliku nie pisze — a wtedy CLI umiera na brakującym pliku dopiero
w produkcji, przy starcie prawdziwego biegu. Rozróżnia to czytanie pliku **spod ścieżki z argv**:
dopiero to wiąże „co obiecaliśmy procesowi" z „co naprawdę leży na dysku".

## AC-4 Z gospodarza bierzemy tekst `deny` i cztery pozostałe pola ODRZUCAMY
check: cargo test --test it host_deny_rewrite::
expect: (\d+) passed

Fikstura, którą test **sam zapisuje** do `<tempdir>/.claude/settings.json`, ulepiona z prawdziwego
kształtu `.claude/settings.json` tego repo i niosąca **wszystkie pięć pól naraz**, każde z własnym
znacznikiem:

```
permissions.deny   ["Read(HOST-DENY-MARKER/**)"]
permissions.allow  ["Bash(HOST-ALLOW-MARKER:*)"]
env                {"HOST_ENV_MARKER": "1"}
sandbox            {"autoAllowBashIfSandboxed": true}
hooks              {"PreToolUse": [{"hooks": [{"type": "command", "command": "HOST-HOOK-MARKER"}]}]}
```

`deny_rules(<tempdir>)` oddaje **dokładnie** `vec!["Read(HOST-DENY-MARKER/**)"]` — porównanie całego
wektora, nie `contains`.

Druga połowa, ta, która dowodzi **odrzucenia**: przepuść wynik przez dokument ustawień z AC-3
i asertuj cztery nieobecności, każdą osobną asercją z własnym komunikatem mówiącym, ile by
kosztowała — `HOST-ALLOW-MARKER` (cudza lista dozwoleń to nie jest nasza polityka),
`HOST_ENV_MARKER` (blok `env` gospodarza nadpisuje środowisko podane przez Loadouta i przewraca
`env_clear()` od zewnątrz), `autoAllowBashIfSandboxed` (przepuszcza **dowolną** komendę mimo naszej
białej listy — pole, które nas ROZSZERZA), `HOST-HOOK-MARKER` (hak gospodarza startuje proces
w swojej grupie, jego dziecko dostaje `ppid=1` i przeżywa wyjście `claude`; zmierzone 30 sierot).
Piąta asercja, w drugą stronę: `HOST-DENY-MARKER` **jest** w dokumencie — bez niej cztery
nieobecności spełnia funkcja zwracająca pustkę.

Dwie fikstury degeneracyjne w tym samym pliku: **brak** `<projekt>/.claude/settings.json` to **pusta
lista, nie błąd** (bieg w repo, które nigdy nie widziało Claude, ma prawo wystartować), i tak samo
plik, który **nie jest poprawnym JSON-em** — repo gospodarza, którego nie kontrolujemy, nie ma prawa
zatrzymać naszego biegu jednym zepsutym przecinkiem.

*Słaba asercja:* `assert_eq!(rules, vec!["Read(HOST-DENY-MARKER/**)"])` samo w sobie dowodzi
wyłącznie, że `deny` zostało **wzięte**. Przechodzi je implementacja, która obok tego przenosi
`env`, `sandbox` i `hooks` **drugą drogą** — kopiując cały obiekt `permissions` albo cały plik
i dokładając `deny` na wierzch. To jest ta implementacja, która przywraca haki gospodarza
i `autoAllowBashIfSandboxed`, a jej test świeci na zielono. Rozróżniają to wyłącznie cztery asercje
**negatywne postawione na dokumencie, który naprawdę idzie na dysk** — asercja typu
`assert!(result.env.is_none())` na jakiejś strukturze pośredniej nie liczy się, bo przechodzi
trywialnie dla struktury, która pola `env` w ogóle nie ma, podczas gdy droga zapisu kopiuje surowy
plik obok niej.

## Świadomie poza zakresem

- **Wołacz produkcyjny.** Kto woła `deny_rules` z jaką ścieżką projektu i do którego katalogu biegu
  trafia plik ustawień — to jest `commands/run.rs`, a on **nie leży w tym bloku OWNS**. Kształt jest
  ten sam co przy `ClaudeDriver::with_transcript` z T-34: mechanizm jest kompletny i nieużywany,
  dopóki człowiek go nie zepnie, a jeden wiersz poza blokiem OWNS jest **pytaniem, nie cichym
  dopiskiem** (`AGENTS.md` §7). Napisz to w uwagach wprost, razem z propozycją miejsca.
- **`--plugin-dir` i wnoszenie umiejętności przy odciętych ustawieniach projektu.** Struktura
  `.claude-plugin/plugin.json` + `skills/<nazwa>/SKILL.md` działa przy całkowicie odciętym
  `--setting-sources ""` [zmierzone 2026-08-19] — i jest osobnym zadaniem tej fali. Tutaj nie
  dokładasz ani tej flagi, ani katalogu wtyczki.
- **`--disallowedTools`.** To jest czarna lista, czyli dokładnie to, co to zadanie usuwa. Nie
  dokładaj jej „dla pewności": lista zakazów obok listy dozwoleń to dwa źródła prawdy o jednej
  rzeczy i pierwszy rozjazd między nimi będzie cichy.
- **`--bare`.** Nigdy. Gasi haki i rozbija OAuth (`Not logged in`); powód stoi w nagłówku
  `claude.rs` od T-04 i jest zmierzony dwa razy.
- **Drugi klucz w naszym pliku ustawień.** `permissions.allow`, `env`, `sandbox`, `hooks` po
  **naszej** stronie też nie wchodzą. Plik ma jeden klucz; drugi jest nowym kryterium, nie łatką.
- **`Vendor::Codex`.** Adaptera nie ma (odpowiada za niego `drivers/absent.rs`). Kiedy powstanie,
  dostanie **własną** tabelę tłumaczenia we własnym pliku — niezmiennik 23 nie pozwala mu czytać
  naszej i nie pozwala nam pisać jego.
- **Zmiana `Policy`, `RunSpec` i sygnatury `AgentDriver`.** Jeżeli któreś kryterium wygląda na
  niewykonalne bez takiej zmiany — **zatrzymaj się i powiedz to**, zamiast rozszerzać strukturę
  konstruowaną w trzynastu plikach spoza OWNS (`AGENTS.md` §7).
- **Sprawdzenie w `checks/`, które pilnowałoby białej listy w kodzie źródłowym.** Kuszące i błędne:
  `checks/` nie leży w OWNS, a sprawdzenie czytające `claude.rs` gerpem to jest ten sam kształt, co
  selftest przechodzący na komentarzu (niezmiennik 20). Wyrocznią jest zbudowana komenda.

<!-- OWNS
src-tauri/src/engine/drivers/claude.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/engine/drivers/host.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/driver_claude_tool_surface.rs
src-tauri/tests/it/driver_claude_policy_surface.rs
src-tauri/tests/it/driver_claude_settings_file.rs
src-tauri/tests/it/host_deny_rewrite.rs
-->
