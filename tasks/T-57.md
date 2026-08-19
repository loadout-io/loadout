# T-57 — Wiedza gospodarza dojezdza do agenta, a nie tylko daje sie odczytac

T-54 zbudowalo **mechanizm**: skan `.claude/skills/**` gospodarza, przepisanie do wlasnego
katalogu pluginu, wyciecie sekcji `## Recurring patterns` z jego learnings i odjecie
front-mattera od definicji podagenta. Zaden z tych czterech krokow nie ma dzis konsumenta
produkcyjnego -- `plugin_dir`, `plugin_argv`, `recurring_patterns` i `agent_body` sa wolane
wylacznie z `tests/`, czyli z osobnych skrzyn, w ktorych `dead_code` milczy.

To jest **dokladnie ten podpis**, ktory `checks/quick-wired.sh` opisuje w swoim naglowku:
*„a function used only from tests/ is NOT dead code to clippy -- integration tests are separate
crates, so dead_code stays silent and the thing rots green. That is how `engine::limits::Limiter`
landed with a passing test and zero production callers."* Rozdzielenie mechanizmu od podpiecia
bylo swiadome (T-54 nie mialo prawa tknac `claude.rs`, bo ten plik nalezal wtedy do
niewyladowanego T-53) -- ale rozdzielenie, o ktorym nikt nie napisal zadania, jest zgnilizna.
To zadanie jest tym zadaniem.

Cicha porazka, przed ktora stoi caly ten kontrakt: przepisanie **dziala**, katalog pluginu
**powstaje na dysku**, a `--plugin-dir` nie wchodzi do argv -- wiec vendor nigdy nie widzi ani
jednej odziedziczonej umiejetnosci. Nic nie pada, nic nie krzyczy, a czlowiek dowiaduje sie
o tym nigdy, bo „agent nie zna umiejetnosci" jest nieodroznialne od „model nie uznal, ze warto
jej uzyc" (`skills/place.rs` opisuje te sama wade przy instalacji).

**Read first:**
`src-tauri/src/inherit/` (caly modul z T-54 -- `scan::skills`, `scan::recurring_patterns`,
`scan::agent_body`, `rewrite::plugin_dir`, `rewrite::plugin_argv`),
`src-tauri/src/engine/drivers/claude.rs` (`command()` sklada argv; `LEAN_CONTEXT` niesie
`--setting-sources ""`, a T-53 dolozylo tam `--tools` i `--settings`),
`src-tauri/src/engine/drivers/host.rs` (T-53: przepisane `permissions.deny` gospodarza --
ten sam wzorzec „czytamy cudze, piszemy swoje"),
`src-tauri/src/commands/run.rs` (`lay_out_the_run_dir` zaklada katalog biegu; tu powstaje
katalog pluginu),
`docs/STATUS.md` wpis z 2026-08-19 22:20 (pomiary, na ktorych to stoi),
`AGENTS.md` niezmienniki 6, 9, 21, 23.

## Niezmienniki, ktorych to dotyka

- **21 — nie pisz artefaktu, ktorego zaden skrypt nie czyta.** To zadanie istnieje wylacznie po
  to, zeby cztery funkcje T-54 przestaly byc takim artefaktem.
- **9 — prompt i sekrety wylacznie przez stdin.** Odziedziczone `## Recurring patterns` ida do
  **promptu**, nigdy do argv. Sciezka katalogu pluginu w argv jest w porzadku; jego TRESC nie.
- **23 — polityka w jednym rdzeniu.** Decyzja „co dziedziczymy" mieszka w `inherit`, a `claude.rs`
  dostaje gotowa liste flag. Drugi zestaw regul w adapterze jest tym, jak po cichu umarlo
  skanowanie sekretow w repo zrodlowym.

## Kryteria akceptacji

## AC-1 Katalog pluginu powstaje w katalogu biegu i `--plugin-dir` wchodzi do argv
check: cargo test --test it inherit_reaches_the_argv::
expect: (\d+) passed

Bieg z folderem projektu, w ktorym stoi `.claude/skills/<nazwa>/SKILL.md`, sklada argv
sterownika przez `rewrite::plugin_dir` i `rewrite::plugin_argv`. Asercje: (a) katalog pluginu
lezy **wewnatrz katalogu biegu**, nie w projekcie gospodarza i nie w `/tmp`; (b) argv zawiera
`--plugin-dir` z ta wlasnie sciezka, dokladnie raz; (c) `SKILL.md` w katalogu pluginu ma tresc
**bajt w bajt** ta sama, co u gospodarza; (d) kontrola przeciw pustemu przejsciu: projekt bez
`.claude/skills/` daje argv **bez** `--plugin-dir` -- flaga bez wartosci albo ze sciezka do
pustego katalogu jest bledem, nie neutralnym zachowaniem.

*Slaba asercja:* `assert!(argv.contains(&"--plugin-dir".into()))`. Przechodzi dla implementacji,
ktora doklada flage ZAWSZE, takze gdy nie ma czego dziedziczyc -- a wtedy vendor dostaje sciezke
do katalogu, ktorego nie ma, i to jest awaria startu procesu, nie brak funkcji. Rozroznia to
wylacznie (d), i dlatego stoi w tym samym tescie, a nie „gdzies obok".

## AC-2 `## Recurring patterns` dojezdza do promptu, i nic poza ta sekcja
check: cargo test --test it inherit_reaches_the_prompt::
expect: (\d+) passed

Fikstura: plik learnings gospodarza z obiema sekcjami, przy czym `## Run journal` jest
**wielokrotnie dluzsza** od `## Recurring patterns` (tak wygladaja prawdziwe pliki: zmierzone
1701 z 32922 bajtow i 2016 z 73258). Asercje: (a) `RunSpec::prompt` zawiera zdanie z sekcji
patterns; (b) **nie zawiera** ani jednego zdania z journalu; (c) doklejony fragment jest krotszy
niz 20% pliku zrodlowego; (d) tresc leci **wylacznie** przez `prompt`, a `system_append` i argv
sa od niej wolne (niezmiennik 9: `--append-system-prompt` jedzie do argv, ktore widzi `ps`);
(e) brak pliku learnings u gospodarza to prompt bez doklejki, nie blad biegu.

*Slaba asercja:* sprawdzenie, ze prompt jest dluzszy niz bez dziedziczenia. Przechodzi dla
implementacji, ktora dokleja CALY plik -- czyli dla tej, ktorej caly ten mechanizm ma zapobiec:
73 KB journalu w kazdej turze, w kazdym biegu, na zawsze. Rozroznia to (b) razem z (c).

## AC-3 Podagent gospodarza wchodzi jako tresc, a jego front-matter nie wchodzi nigdy
check: cargo test --test it inherit_subagent_is_text_only::
expect: (\d+) passed

Fikstura: `.claude/agents/<rola>.md` z front-matterem zawierajacym `tools`, `model`,
`permissionMode`, `memory` **i `mcpServers`**. Asercje: (a) tresc doklejona do promptu pochodzi
z `scan::agent_body`, czyli z ciala po drugim `---`; (b) **zadne** z piaciu pol front-mattera nie
wystepuje w prompcie, w argv ani w pliku ustawien biegu; (c) w szczegolnosci komenda z
`mcpServers` (`npx`) nie pojawia sie nigdzie; (d) kontrola przeciw pustemu czytaniu: test sam
sprawdza, ze fikstura naprawde niesie wszystkie piec pol -- inaczej mierzy plik bez
front-mattera i przechodzi na niczym.

Powod (c) napisz w prozie: `mcpServers` uruchamia proces **poza grupa procesow Loadouta**,
a niezmiennik 6 wymaga dowodu smierci grupy, ktorej nie zalozylismy. Zmierzone 2026-08-19:
hak gospodarza zostawil 14 sierot z `ppid=1`, ktore przezyly wyjscie `claude`.

*Slaba asercja:* `assert!(!prompt.contains("mcpServers"))`. Przechodzi dla implementacji, ktora
zjada nazwe pola, a przepuszcza jego zawartosc -- a to zawartosc odpala proces. Rozroznia to
asercja o KOMENDZIE (`npx`), nie o nazwie pola.

## AC-4 Wybor jest czlowieka, a domyslnie nie dziedziczymy niczego
check: cargo test --test it inherit_is_opt_in::
expect: (\d+) passed

Asercje: (a) bieg bez jawnej listy odziedziczonych umiejetnosci sklada argv **bez**
`--plugin-dir` i prompt **bez** doklejki, nawet gdy projekt gospodarza ma pelne `.claude/`;
(b) lista wybranych pozycji jest respektowana co do sztuki -- pozycja spoza niej nie trafia do
katalogu pluginu; (c) nazwa spoza tego, co skan naprawde znalazl, jest **odmowa z nazwaniem
pozycji**, nie cichym pominieciem; (d) kontrola: fikstura ma co najmniej trzy umiejetnosci,
z czego wybrane sa dwie.

*Slaba asercja:* test na samym (a). Przechodzi dla implementacji, ktora dziedziczy WSZYSTKO,
gdy tylko lista jest niepusta -- czyli zamienia wybor czlowieka w przelacznik. Rozroznia to (b).

<!-- OWNS
src-tauri/src/inherit/mod.rs
src-tauri/src/inherit/wire.rs
src-tauri/src/engine/drivers/claude.rs
src-tauri/src/engine/drivers/mod.rs
src-tauri/src/commands/run.rs
src-tauri/tests/it/main.rs
src-tauri/tests/it/inherit_reaches_the_argv.rs
src-tauri/tests/it/inherit_reaches_the_prompt.rs
src-tauri/tests/it/inherit_subagent_is_text_only.rs
src-tauri/tests/it/inherit_is_opt_in.rs
-->
