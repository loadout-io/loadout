# T-16 — Pliki przekazań: front-matter pisze Loadout, agent daje tylko treść

Agent, który wymyśla własne metadane, zmyśli je [T6 §4]. Trudność tego zadania nie polega na
zapisaniu markdownu z nagłówkiem YAML — polega na tym, że **treść ciała jest tekstem od agenta,
czyli danymi niezaufanymi**, a metadane muszą być nadal prawdą Loadouta. Cicha porażka wygląda tak:
agent kończy turę, jego ciało zaczyna się od własnego bloku `---`, implementacja „naprawia"
front-matter przez **scalenie** zamiast przez **nadpisanie**, i od tej chwili `status: superseded`,
`reads: []` albo cudze `id` pochodzą od modelu. Nikt tego nie zauważy, bo plik wygląda idealnie,
UI pokazuje ładny wiersz, a bieg dalej działa. Druga cicha porażka to limit rozmiaru: ciało ucięte
w połowie zdania na 8192 bajcie przechodzi każdy test na „≤ 8 KB" i gubi dokładnie to jedno
zdanie, dla którego przekazanie powstało.

**Read first:**
`docs/research/topics/T6-memory.md` §10.2 (kontrakt przekazania — 13 pól front-mattera, trzy sekcje
ciała, twardy limit 8 KB i przelew do `attachments/`; §4 tabela „failure mode → mitigation" mówi,
ile linii każda z tych rzeczy ma kosztować), §9 (niezmienność i `supersedes`),
`docs/ARCHITECTURE.md` §8 (układ katalogu biegu i zdanie „front-matter pisze Loadout, nie agent"),
§2 pyt. 2 (pliki są prawdą, SQLite jest indeksem — to decyduje, że `status` mieszka w pliku),
`AGENTS.md` §3 (niezmienniki 2, 4, 5, 9, 21, 24),
`docs/research/projects/00-SYNTHESIS.md` §2.2 (słownik: `handoff` nie ma nazwy w UI — to jest
„co przekazał"; nie wprowadzaj nowej).

## Kto to robi

- **Agent:** `rust-core` (Claude Code)
- **Druga opinia:** `./review.sh codex` — nigdy ten sam vendor, co pisał
- **Artefakty biegu:** `runs/T-16/` (zapis, plik wyników, plan) — nigdy `$TMPDIR`

## Co to zadanie posiada

- `src-tauri/src/memory/handoff.rs` — zapis, odczyt i skanowanie plików przekazań; jedyne miejsce,
  które składa front-matter
- `src-tauri/src/memory/mod.rs` — wspólne dla całej pamięci: płaski czytnik/pisarz front-mattera,
  `est_tokens()`, slugifikacja nazw plików. T-17 z tego korzysta i **nie pisze drugiej kopii**
- `src-tauri/tests/memory_handoff_frontmatter.rs`, `…_cap.rs`, `…_sections.rs`, `…_supersede.rs`,
  `…_scan.rs`, `…_paths.rs` — pliki testów kryteriów; nazwy są globalnie unikalne i należą do tego
  zadania

`src-tauri/src/lib.rs` masz w OWNS **wyłącznie** po to, żebyś dopisał `pub mod memory;`, jeśli
tego wiersza jeszcze tam nie ma. Żadnej innej zmiany w tym pliku.

Czego **nie** posiadasz, a będzie kusiło: `src-tauri/Cargo.toml` (dopisanie zależności to też
AGENTS.md §7), `src-tauri/src/store/**` (T-06).

**Konsekwencja braku `Cargo.toml`:** nie ma `gray_matter`. Nasz front-matter jest płaską mapą
`klucz: wartość` z dwoma polami listowymi (`to`, `reads`) — ręczny czytnik/pisarz to ~60 linii
w `mod.rs` i **jest lepszy** dla własności integralności z AC-1, bo dokładnie wiadomo, co jest
parsowane. Nie sięgaj po `serde_yaml` (ostatnie wydanie to `0.9.34+deprecated`) [T6 §7.3].

## Niezmienniki

- **4 — pliki są prawdą, `loadout.db` jest indeksem.** Każde z 13 pól front-mattera musi dać się
  odczytać z samego pliku. *Jak się łamie po cichu:* `status` albo `reads` trafia tylko do wiersza
  w SQLite, bo „i tak zawsze czytamy z bazy". Kasujesz `loadout.db` i przekazanie oznaczone jako
  zastąpione wraca do obiegu.
- **2 — do SQLite pisze wyłącznie `store::writer`.** `handoff.rs` **nie otwiera połączenia**;
  zwraca strukturę, którą ktoś inny podaje pisarzowi. *Jak się łamie po cichu:* „szybki
  `rusqlite::Connection::open` tylko do zapisania wiersza po write" — drugie połączenie
  zapisujące to zakleszczenie, nie „czasem wolniej".
- **5 — nieznane zdarzenie nie wywala biegu.** Dotyczy tu *odczytu*: plik z nieznanym kluczem
  front-mattera albo nieznanym `kind` (starszy/nowszy Loadout, ręczna edycja) ląduje w `extra`
  i jedzie dalej. *Jak się łamie po cichu:* `serde(deny_unknown_fields)` na strukturze meta —
  skan całego katalogu biegu przewraca się na jednym pliku i UI pokazuje pustą listę zamiast błędu.
- **9 — prompt wyłącznie przez stdin.** Ciało przekazania jest wstrzykiwane do promptu następnego
  kroku; nigdy nie ląduje w argv ani w pliku tymczasowym. *Jak się łamie po cichu:* „przekażmy
  ścieżkę i treść w argumencie, tak łatwiej debugować" — treść przekazania trafia do `ps`.
- **21 — nie pisz artefaktu, którego nikt nie czyta.** `attachments/<plik>` powstaje tylko wtedy,
  gdy w ciele jest wskaźnik, który do niego prowadzi. *Jak się łamie po cichu:* zapisujemy pełny
  tekst „na wszelki wypadek" także wtedy, gdy nic nie zostało ucięte.
- **24 — komentuj DLACZEGO.** Przy limicie 8192 i przy regule „normalizuj, potem waliduj" ma stać
  datowany powód z numerem sekcji raportu, nie sama liczba.
- **3 — kod platformowy tylko w `engine/supervisor.rs`.** W `handoff.rs` nie ma `#[cfg(unix)]`:
  ścieżki składamy `PathBuf`em, uprawnień nie tykamy. *Jak się łamie po cichu:* `symlink` albo
  `PermissionsExt` „tylko na macOS" — `checks/quick-boundary.sh` to wywraca.

## Kryteria akceptacji

Kolejność pracy wymuszona przez bramkę: **najpierw publiczne sygnatury z `todo!()`, potem plik
testu, dopiero potem `./verify.sh before`.** Test, który się nie kompiluje, niczego nie uruchomił
i bramka policzy to jako nieprawdziwe czerwone (`AGENTS.md` §2a, `NOT_A_REAL_RED`).
Limit sprawdzenia w `before` to **20 s**, więc rozgrzej build przed pierwszym uruchomieniem:
`cargo test --no-run --test memory_handoff_frontmatter`. Pierwsza kompilacja drzewa Tauri trwa
minuty i bez rozgrzania każde kryterium „nie skończy się" zamiast paść.
W każdym pliku testu na górze `#![allow(clippy::unwrap_used, clippy::expect_used)]` z jednym
zdaniem powodu — `checks/full-clippy.sh` biegnie `--all-targets -- -D warnings`.

Kształt, który te kryteria zakładają:

```rust
pub struct MetaDraft { run, step, from, to, kind, title, reads }   // podaje wołający
pub struct Meta { id, run, step, from, to, kind, title, status, supersedes, reads,
                  created, bytes, est_tokens, extra }              // 13 pól + extra
pub struct Written { path, attachment: Option<PathBuf>, repaired: Vec<Section>, truncated: bool }

pub fn write_handoff(run_dir: &Path, draft: MetaDraft, agent_body: &str) -> Result<Written>;
pub fn read_handoff(path: &Path) -> Result<Handoff>;
pub fn scan_run_dir(run_dir: &Path) -> Result<Vec<Handoff>>;
pub fn supersede(run_dir: &Path, old_id: &str, draft: MetaDraft, body: &str) -> Result<Written>;
```

## AC-1 Ciało agenta nie może podmienić ani jednego pola front-mattera
check: cargo test --test memory_handoff_frontmatter

Ciało wejściowe zaczyna się od kompletnego, sfałszowanego bloku: `id: h_FORGED`, `run: run_evil`,
`step: 99`, `from: someone-else`, `to: []`, `kind: review`, `title: Forged`, `status: superseded`,
`supersedes: h_REAL`, `reads: []`, `created: 1970-01-01T00:00:00Z`, `bytes: 10`, `est_tokens: 1`
oraz klucz spoza kontraktu `admin: true`. Dalej normalne `## Answer` / `## Evidence` / `## Open`,
łącznie poniżej 8192 B.
Po `write_handoff`: `read_handoff` zwraca **wszystkie 13 pól równe wartościom Loadouta**,
`meta.extra` nie zawiera `admin`, a plik ma dokładnie jeden blok front-mattera — otwarcie `---`
na bajcie 0 i zamknięcie przed ciałem.
Sfałszowany tekst **zostaje w pliku**: `file.find("h_FORGED")` jest większe niż offset zamknięcia
front-mattera, a bajty ciała są identyczne z wejściem agenta (jeden pusty wiersz separatora).

*Słaba asercja:* test sprawdzający tylko `meta.id == loadout_id` przechodzi na implementacji, która
scala mapy i po prostu wygrywa na kluczu `id`, a przegrywa na `status` i `reads`; przechodzi też
implementacja, która **kasuje** z ciała każdy blok `---` (czyli ukrywa atak przed człowiekiem).
Rozróżnia dopiero para: równość na wszystkich 13 polach **plus** asercja na offsecie bajtowym
`h_FORGED` w ciele i na bajtowej identyczności ciała z wejściem.

## AC-2 Limit 8 KB tnie na granicy sekcji, a pełny tekst ląduje w attachments
check: cargo test --test memory_handoff_cap

`BODY_CAP = 8192` bajtów ciała po normalizacji [T6 §10.2]. Przypadek pierwszy: `## Answer` 3 000 B,
`## Evidence` 6 000 B, `## Open` 200 B. Zapisane ciało zawiera całą sekcję `Answer`, a pod
nagłówkami `Evidence` i `Open` **jedną linię wskaźnika** `Moved to attachments/<plik>` — sentinel
ze środka `Evidence` nie występuje w pliku ani razu. `attachments/<stem>__full.md` zawiera
oryginalne ciało bajt w bajt (równe sha256). `bytes` w front-matterze równa się faktycznej długości
**zapisanego** ciała i jest ≤ 8192; `est_tokens == bytes.div_ceil(4)`.
Przypadek drugi: samo `## Answer` ma 9 000 B — cięcie na ostatnim pełnym wierszu, wskaźnik na końcu,
attachment nadal z pełnym tekstem. Przypadek trzeci: ciało 8 192 B równo — `truncated == false`
i `attachment == None` (niezmiennik 21).

*Słaba asercja:* `assert!(body.len() <= 8192 && attachment.exists())` przechodzi na cięciu w połowie
słowa i na attachmencie zawierającym ucięty tekst. Rozróżnia: nieobecność sentinela ze środka
`Evidence`, obecność nagłówka `## Evidence` ze wskaźnikiem, równość sha256 attachmentu z **pełnym
oryginałem**, oraz przypadek trzeci — bo implementacja „zawsze pisz attachment" właśnie na nim pada.

## AC-3 Trzy sekcje o stałych nazwach, w stałej kolejności, nic z treści nie ginie
check: cargo test --test memory_handoff_sections

Ciało A: sama proza, zero nagłówków. Wynik: wstawione wszystkie trzy nagłówki, cała proza agenta
pod `## Answer`, `Written.repaired == [Answer, Evidence, Open]`, `Evidence` i `Open` puste.
Ciało B: jest `## Evidence` i `## Answer`, w tej kolejności, brak `## Open`. Wynik: kolejność
w pliku to Answer < Evidence < Open po offsetach bajtowych, treść każdej sekcji zostaje przy swoim
nagłówku, `repaired == [Open]`.
Ciało C: komplet trzech sekcji we właściwej kolejności — `repaired` jest puste, plik zawiera ciało
bajt w bajt.

*Słaba asercja:* `assert!(file.contains("## Open"))` przechodzi na implementacji doklejającej
brakujące nagłówki na końcu, w dowolnej kolejności, i na takiej, która przy przestawianiu sekcji
gubi treść. Rozróżniają: porównanie **offsetów** trzech nagłówków oraz asercja, że offset prozy
agenta z ciała A leży między `## Answer` a `## Evidence`.

## AC-4 Korekta to nowy plik; stary nie jest edytowany poza jedną linią statusu
check: cargo test --test memory_handoff_supersede

`supersede(run_dir, old_id, draft, body)` tworzy **nowy** plik z `supersedes: <old_id>` i
`status: current`. W starym pliku zmienia się wyłącznie linia `status:` na `superseded`:
sha256 ciała starego pliku jest równe wartości sprzed wywołania, pozostałe 12 pól front-mattera są
niezmienione, a ścieżka dalej istnieje.
`scan_run_dir(..)` filtrowane po `status == current` zwraca tylko nowy plik.
Drugie wywołanie `supersede` na już zastąpionym `id` → `Err(AlreadySuperseded)` i **żaden** plik
w katalogu nie zmienia sha256 (porównanie map ścieżka→hash przed i po).

*Słaba asercja:* sprawdzenie samego nowego pliku przechodzi na implementacji, która nadpisuje stary
plik w miejscu (historia biegu przestaje być prawdziwa). Rozróżnia: hash ciała starego pliku
i mapa hashy całego katalogu przy odrzuconym drugim wywołaniu.

## AC-5 Katalog biegu daje się odczytać bez bazy, także gdy pliki pisał ktoś inny
check: cargo test --test memory_handoff_scan

Test **wypisuje pliki jako literalne stringi** (to nie jest odczyt tego, co sam zapisał):
`01__orchestrator__brief.md` (`status: current`), `02__research-auth__findings.md`
(`status: superseded`, nieznany klucz `x-loadout-future: 1`, nieznany `kind: telepathy`
i `bytes:` celowo niezgodne z faktyczną długością), `03__planner__plan.md` (brak opcjonalnego
klucza `supersedes` w ogóle), plus śmieć `.DS_Store` i katalog `attachments/`.
`scan_run_dir` zwraca 3 rekordy w kolejności 01, 02, 03; nieznany klucz jest w `extra`, nieznany
`kind` nie jest błędem (niezmiennik 5), brakujący klucz opcjonalny to `None`, `.DS_Store` jest
pominięty bez błędu, a rekord 02 raportuje rozjazd `declared_bytes != actual_bytes`.

*Słaba asercja:* skan zwracający nazwy plików i przeliczający wszystko z zawartości przechodzi
każdą asercję „pole ma sensowną wartość", bo sam je wylicza. Rozróżnia rekord 02: wartość `bytes`
musi pochodzić **z pliku**, a rozjazd z faktyczną długością ma być zaraportowany, nie wygładzony.

## AC-6 Nazwa pliku jest funkcją Loadouta i nie da się nią wyjść z katalogu
check: cargo test --test memory_handoff_paths

Wzór `handoffs/<NN>__<from>__<kind>.md`, `NN` z dwucyfrowym zerem wiodącym z numeru kroku
[ARCHITECTURE §8]. Wejścia wrogie w polu `from`: `../../etc/passwd`, `....//x`, `/absolute/x`,
`Ünïcode Agent`, `"   "` (same białe znaki), oraz dwa razy ta sama trójka (krok, slug, kind).
Dla każdego: `fs::canonicalize(zapisana_ścieżka.parent())` równa się `fs::canonicalize(handoffs/)`,
slug pasuje do `^[a-z0-9]+(-[a-z0-9]+)*$`, pusty slug degraduje się do `agent`, a kolizja daje
sufiks `-2` — pierwszy plik zostaje nietknięty (sha256 równe sprzed).

*Słaba asercja:* `from.replace("../", "")` przechodzi na `../../etc/passwd` i **pada** na `....//x`,
a `path.starts_with(handoffs)` przechodzi na dowiązaniu i na ścieżce z `..` w środku.
Rozróżnia: `canonicalize` obu stron dla wszystkich pięciu wejść plus asercja na haszu pierwszego
pliku przy kolizji.

## Świadomie poza zakresem

- **Notatki, ich stany i sekcja Pamięć** — T-17. Tutaj powstaje tylko `est_tokens()` i płaski
  front-matter w `mod.rs`, z których T-17 korzysta.
- **Wiersze `handoff` / `step_input` w SQLite** — T-06 (schemat) i T-15 (składanie promptu).
  To zadanie zwraca strukturę; nie zna słowa `Connection`.
- **Wstrzykiwanie ciała do promptu następnego kroku** — T-15. Tutaj tylko gwarancja, że ciało
  jest kompletne, ograniczone i wskazuje na attachment.
- **Panel „Given / Produced"** — T-09.
- **Reflection na koniec biegu (max 3 kandydatki)** [T6 §5.3] — nie w v1 tego zadania; kandydatki
  wpisuje człowiek albo T-17.
- **FTS5 i wyszukiwanie po przekazaniach** — T-06.
- **Częściowe:** `kind` ma zamknięty zbiór siedmiu wartości [T6 §10.2] i wariant „inne" przy
  odczycie. Nie ma UI do wyboru `kind` — ustawia go krawędź workflow (T-13).

<!-- OWNS
src-tauri/src/lib.rs
src-tauri/src/memory/handoff.rs
src-tauri/src/memory/mod.rs
src-tauri/tests/memory_handoff_frontmatter.rs
src-tauri/tests/memory_handoff_cap.rs
src-tauri/tests/memory_handoff_sections.rs
src-tauri/tests/memory_handoff_supersede.rs
src-tauri/tests/memory_handoff_scan.rs
src-tauri/tests/memory_handoff_paths.rs
-->
