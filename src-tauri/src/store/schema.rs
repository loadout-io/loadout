//! Schemat: cztery żywe tabele, `STRICT`, i odmowy, które `SQLite` wypowiada **sam**.
//!
//! Ten plik jest jednym z trzech, którym `checks/quick-boundary.sh` pozwala nieść DDL —
//! obok `store::migrate` i `store::writer`. Nazwa nie jest dowolna: sprawdzenie zna ją
//! z nazwy, a `CREATE TABLE` w czwartym pliku przewraca bramkę i ma rację.
//!
//! Co tu **musi** stanąć i dlaczego każdy z tych punktów jest osobnym kryterium:
//!
//! - **Cztery żywe tabele** — `runs`, `steps`, `events`, `artifacts` — wszystkie `STRICT`.
//!   Bez `STRICT` `SQLite` przyjmie `attempt = 'dwa'` i policzy to jako tekst, a wtedy pierwszy
//!   odczyt dostaje wartość, której nie umie dodać (AC-7 b). Notatki nie mają tabeli: ich pliki
//!   są jedynym miejscem zapisu i źródłem prawdy (T-140).
//! - **`CHECK` na `steps.status`** z siedmioma stanami z `docs/ARCHITECTURE.md` §5 i **`CHECK`
//!   na `events.level`** z trzema poziomami. Odmowa napisana w naszej funkcji `insert_step`
//!   przechodzi na schemacie bez `CHECK` — a wtedy pierwszy zapis spoza naszego API (migracja,
//!   odbudowa z AC-4, skrypt) wpuszcza stan, którego UI nie umie narysować (AC-7 a).
//! - **`REFERENCES … ON DELETE CASCADE`** na `steps.run_id`, `events.run_id`, `events.step_id`
//!   i `artifacts.*` — kasowanie biegu ma zabrać jego kroki i zdarzenia (AC-7 c). Działa
//!   wyłącznie przy `foreign_keys` ON, czyli razem z [`super::apply_pragmas`].
//! - **`UNIQUE (run_id, node_key)`** na `steps` (AC-7 d).
//! - **Wyzwalacze reject-update i reject-delete na `events`**, każdy z `RAISE(ABORT, …)`
//!   niosącym [`APPEND_ONLY_REFUSAL`]. To one, a nie Rust, czynią „append-only" prawdą dla
//!   połączenia, które omija nasze API: migracji, skryptu naprawczego, przyszłego daemona,
//!   `sqlite3` z terminala (AC-2). Wzorzec przychodzi z poprzedniego prototypu,
//!   `the earlier prototype's store/src/schema.rs:163-190` — trzy linie na wyzwalacz.
//! - **Indeksy z [T7 §5.4]**, w tym **częściowy** `WHERE level = 'headline'`: szyna czyta
//!   wyłącznie nagłówki, więc indeks ma zawierać wyłącznie nagłówki.
//!
//! Czego tu **nie** ma i nie będzie: tabeli wersji schematu i drugiej wersji schematu. Jedna
//! wersja, aż zajdzie potrzeba drugiej (`AGENTS.md`, tabela zakazów). Migracja jest addytywna
//! i idempotentna (niezmiennik 25), więc wersji nie ma po czym numerować.
//!
//! # Dlaczego reject-delete ma warunek, a reject-update nie ma
//!
//! Zmierzone 2026-08-16 na `SQLite` 3.51/3.53: **kaskada `ON DELETE` odpala wyzwalacze
//! `BEFORE DELETE` tabeli potomnej**, i to niezależnie od `recursive_triggers`. Bezwarunkowy
//! reject-delete na `events` nie jest więc „ostrożniejszą" wersją tego samego — on **blokuje
//! kasowanie biegu**, czyli wywraca AC-7 c i zostawia użytkownika z historią, której nie da się
//! wyczyścić.
//!
//! Warunek `WHEN` rozdziela dwie operacje, które bez niego wyglądają identycznie:
//!
//! - **przepisanie historii** — skasowanie linii transkryptu biegu, który dalej istnieje;
//!   to jest ta operacja, przed którą ten wyzwalacz chroni, i ona zostaje odmówiona zawsze,
//!   także dla kaskady z pojedynczego kroku (`DELETE FROM steps` przy żywym biegu);
//! - **wyrzucenie biegu w całości** — `DELETE FROM runs`, po którym nie zostaje ani wiersz
//!   `runs`, ani jego kroki, ani jego zdarzenia. To nie jest przepisanie historii, tylko
//!   wypisanie jej z indeksu, i jest jawnie przewidziane przez `ON DELETE CASCADE` z T7 §5.4.
//!
//! Kiedy kaskada dochodzi do `events`, wiersz `runs` **już nie istnieje** (sprawdzone tą samą
//! próbą), więc podzapytanie w `WHEN` daje zero i wyzwalacz milczy. Przy próbie prosto na
//! `events` bieg stoi na miejscu, podzapytanie daje jeden i `RAISE(ABORT, …)` odmawia.

/// Tekst odmowy jako **literał**, żeby stała i oba wyzwalacze miały jedno źródło.
///
/// `macro_rules!`, a nie `const`, z jednego powodu: [`STATEMENTS`] jest `const`, więc SQL musi
/// powstać z literałów w czasie kompilacji (`concat!`). Wklejenie tych słów trzeci raz do SQL-a
/// zrobiłoby z [`APPEND_ONLY_REFUSAL`] kopię, która może się rozjechać z wyzwalaczem i o której
/// rozjeździe dowiedziałby się dopiero człowiek czytający komunikat błędu.
macro_rules! append_only_refusal {
    () => {
        "events is append-only"
    };
}

/// Tekst, którym wyzwalacze `events` odmawiają.
///
/// Stoi w stałej, a nie wklejony dwa razy w SQL, bo pojawia się w komunikacie błędu, który
/// czyta człowiek — i dlatego, że test AC-2 sprawdza, czy odmowa przyszła **z wyzwalacza**,
/// a nie z tabeli otwartej tylko do odczytu. Te dwa błędy wyglądają identycznie, jeśli patrzeć
/// wyłącznie na to, że `UPDATE` się nie udał.
pub const APPEND_ONLY_REFUSAL: &str = append_only_refusal!();

/// Zdania SQL schematu, w kolejności wykonania.
///
/// Każde musi być **idempotentne** (`IF NOT EXISTS`), bo `store::migrate` biegnie przy każdym
/// otwarciu bazy i wolno mu wykonać się dwa razy pod rząd bez żadnej różnicy (niezmiennik 25,
/// AC-1). Kolumnę dokłada się przez `add_column_if_missing`, nigdy gołym `ALTER TABLE … ADD
/// COLUMN`: drugi start aplikacji rzuca wtedy `duplicate column name`, a użytkownik widzi
/// „nie udało się otworzyć projektu".
///
/// Jedno zdanie tej listy to **jedna tabela razem ze wszystkim, co do niej należy** — jej
/// indeksami i jej wyzwalaczami. Nie jest to podział kosmetyczny: trzymanie ich osobno jest
/// jedynym sposobem, żeby dało się dodać tabelę i zapomnieć jej wyzwalacza, a tabela `events`
/// bez wyzwalaczy wygląda dokładnie tak samo jak z nimi, dopóki ktoś nie napisze `UPDATE`.
pub const STATEMENTS: &[&str] = &[
    // ── runs ───────────────────────────────────────────────────────────────────────────────
    // `workflow_snapshot` to kopia grafu JAK BIEGŁ. Bez niej użytkownik, który poprawi
    // workflow, po cichu zmienia opowieść starych biegów stojących w historii [T7 §5.4].
    // Sześć stanów biegu; `paused` jest stanem biegu i NIGDY stanem kroku.
    "
    CREATE TABLE IF NOT EXISTS runs (
      id                TEXT    NOT NULL PRIMARY KEY,
      workflow_id       TEXT    NOT NULL,
      workflow_snapshot TEXT    NOT NULL,
      title             TEXT    NOT NULL,
      status            TEXT    NOT NULL CHECK (status IN
                          ('running', 'paused', 'succeeded', 'failed', 'cancelled', 'interrupted')),
      concurrency       INTEGER NOT NULL DEFAULT 3,
      created_at        INTEGER NOT NULL,
      started_at        INTEGER,
      ended_at          INTEGER,
      error             TEXT,
      -- Kiedy wstala maszyna, na ktorej ten bieg ruszyl (`sysctl kern.boottime`, sekundy).
      --
      -- STRAZNIK, nie diagnostyka. PID-y na macOS przewijaja sie w godzinach (`kern.maxproc`
      -- = 16 000), wiec po restarcie zapisany `pgid` z duzym prawdopodobienstwem nalezy do
      -- czegos niewinnego, a `killpg` po nim jest bledem POPRAWNOSCI [T7 ryzyko 2].
      -- Odzyskiwanie po awarii porownuje te wartosc z tym, co mowi maszyna TERAZ, i sprzata
      -- tylko wtedy, gdy oba napisy mowia o tym samym uruchomieniu systemu.
      --
      -- NULL znaczy `wiersz sprzed wprowadzenia pola` i jest brakiem straznika, a nie zgoda
      -- na strzal: `recovery::decide` odpowiada wtedy NO_BOOT_TIME i NIC nie zabija.
      boot_id           TEXT
    ) STRICT;
    ",
    // ── steps ──────────────────────────────────────────────────────────────────────────────
    // Tabela mała (~20 wierszy) i MUTOWALNA. To nie jest sprzeczne z append-only `events`,
    // tylko drugą połową tego samego podziału [T7 §5.1]: `steps` jest widokiem materializowanym
    // utrzymywanym na bieżąco, żeby GUI nie musiało odgrywać dziennika, by odpowiedzieć
    // „czy krok 4 biegnie".
    //
    // Siedem stanów kroku z `docs/ARCHITECTURE.md` §5. Ta sama siódemka stoi w enumie
    // `engine::step::StepState` — rozjazd między nim a tym `CHECK`iem skończyłby się wierszem,
    // którego SQLite nie przyjmie, w środku biegu.
    //
    // `pgid` jest tu, a nie „na przyszłość": po nim sprząta odzyskiwanie po awarii (T-20).
    "
    CREATE TABLE IF NOT EXISTS steps (
      id               TEXT    NOT NULL PRIMARY KEY,
      run_id           TEXT    NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
      node_key         TEXT    NOT NULL,
      name             TEXT    NOT NULL,
      agent            TEXT    NOT NULL,
      depends_on       TEXT    NOT NULL DEFAULT '[]',
      status           TEXT    NOT NULL CHECK (status IN
                         ('pending', 'ready', 'running', 'succeeded', 'failed', 'cancelled', 'skipped')),
      attempt          INTEGER NOT NULL DEFAULT 0,
      agent_session_id TEXT,
      pid              INTEGER,
      pgid             INTEGER,
      exit_code        INTEGER,
      started_at       INTEGER,
      ended_at         INTEGER,
      cost_usd         REAL,
      summary          TEXT,
      error            TEXT,
      UNIQUE (run_id, node_key)
    ) STRICT;

    CREATE INDEX IF NOT EXISTS idx_steps_run ON steps(run_id, status);
    ",
    // ── events ─────────────────────────────────────────────────────────────────────────────
    // Transkrypt. `seq` nadaje SQLite, bo kolejność jest globalna i monotoniczna — nie ma jej
    // kto nadać po stronie ośmiu równoległych producentów.
    //
    // Indeks CZĘŚCIOWY na `level = 'headline'` nie jest mikrooptymalizacją: szyna czyta
    // wyłącznie nagłówki, więc indeks, który zawiera także `raw`, każe jej przeglądać cały
    // surowy strumień, żeby znaleźć kilkanaście linii [T7 §5.4].
    concat!(
        "
    CREATE TABLE IF NOT EXISTS events (
      seq     INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
      run_id  TEXT    NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
      step_id TEXT             REFERENCES steps(id) ON DELETE CASCADE,
      ts      INTEGER NOT NULL,
      kind    TEXT    NOT NULL,
      level   TEXT    NOT NULL DEFAULT 'detail' CHECK (level IN ('headline', 'detail', 'raw')),
      body    TEXT
    ) STRICT;

    CREATE INDEX IF NOT EXISTS idx_events_run_seq  ON events(run_id, seq);
    CREATE INDEX IF NOT EXISTS idx_events_headline ON events(run_id, seq) WHERE level = 'headline';

    CREATE TRIGGER IF NOT EXISTS events_reject_update
    BEFORE UPDATE ON events
    BEGIN
      SELECT RAISE(ABORT, '",
        append_only_refusal!(),
        ": a line of the transcript cannot be changed');
    END;

    CREATE TRIGGER IF NOT EXISTS events_reject_delete
    BEFORE DELETE ON events
    WHEN (SELECT count(*) FROM runs WHERE id = old.run_id) > 0
    BEGIN
      SELECT RAISE(ABORT, '",
        append_only_refusal!(),
        ": a line of the transcript cannot be deleted');
    END;
    "
    ),
    // ── artifacts ──────────────────────────────────────────────────────────────────────────
    // ŚCIEŻKI, nie bloby [T7 §5.4]. Plik jest prawdą, ten wiersz jest tylko wskazaniem palcem —
    // odwrotnie byłoby drugie źródło prawdy o treści, która i tak leży na dysku.
    "
    CREATE TABLE IF NOT EXISTS artifacts (
      id         TEXT    NOT NULL PRIMARY KEY,
      run_id     TEXT    NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
      step_id    TEXT             REFERENCES steps(id) ON DELETE CASCADE,
      kind       TEXT    NOT NULL,
      name       TEXT    NOT NULL,
      path       TEXT    NOT NULL,
      bytes      INTEGER,
      created_at INTEGER NOT NULL
    ) STRICT;

    CREATE INDEX IF NOT EXISTS idx_artifacts_run ON artifacts(run_id);
    ",
];
