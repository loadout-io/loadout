//! Powłoka aplikacji po stronie Rusta: dziennik, hak paniki, okno.
//!
//! Logowanie jest modułem *wewnątrz* tego pliku, bo `src-tauri/src/logging.rs` nie należy do
//! T-01. Nie zakładamy tu też `engine/` ani helperów, po które sięgnie T-02: niezmiennik 1
//! czyta się w tym zadaniu odwrotnie — silnik nie ma prawa zależeć od pliku, który zna Tauri.
//!
//! Kod platformowy też tu nie mieszka (niezmiennik 3). Przepis na czyste chrome jest
//! macOS-owy, ale jest zapisany jako DANE w `tauri.conf.json`, nie jako `cfg` w tym pliku.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::Manager;

use crate::commands::Drivers;
use crate::engine::drivers::AgentDriver;
use crate::engine::drivers::claude::ClaudeDriver;
use crate::engine::drivers::codex::CodexDriver;
use crate::library::agents::Vendor;
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use tracing_subscriber::fmt::writer::{MakeWriter, MakeWriterExt};

/// Warstwa komend: funkcje `*_inner`, ktore nie znaja slowa „Tauri". Wypelnia T-15.
pub mod commands;

/// Silnik: graf, planista, nadzor procesow. Wypelnia T-02 i dalej.
pub mod engine;

/// Dziedziczenie wiedzy repo gospodarza: tekst, nigdy maszyneria. Wypelnia T-54.
pub mod inherit;

/// Import setupow repo do natywnych agentow, skilli, polaczen i workflow. Wypelnia T-75.
pub mod import;

/// Polaczenia narzedziowe zarzadzane przez Loadout. Wypelnia T-75.
pub mod connections;

/// Granica z oknem: pompa sklejajaca i kanal do webviewa. Wypelnia T-07.
pub mod ipc;

/// Biblioteka uzytkownika: agenci, umiejetnosci, pamiec. Wypelnia T-11 i dalej.
pub mod library;
/// Magazyn: schemat `SQLite`, jeden pisarz, migracje. Wypelnia T-06.
pub mod store;

/// Pamiec: pliki przekazan miedzy krokami (T-16) i notatki (T-17).
pub mod memory;

/// Odzyskiwanie po awarii: wykryj, sprzatnij po `pgid`, zapytaj. Wypelnia T-20.
pub mod recovery;

/// Umiejetnosci: jeden folder, dwa katalogi, szesciu vendorow. Wypelnia T-18 i T-19.
pub mod skills;

/// Format pliku workflow i walidacja przy zapisie. Wypelnia T-12.
pub mod workflow;

/// Rejestr workspace'ow: jeden folder — jedna karta, jedna wspolna pula miejsc. Wypelnia T-24.
pub mod workspace;

// 2026-08-15 — WARUNEK USUNIECIA DEKLARACJI TYMCZASOWEJ ZASZEDL, wiec jej tu nie ma.
//
// Stala tu para linii `#[path = "engine/supervisor.rs"] pub mod supervisor;`, bo `engine/mod.rs`
// nie mialo wtedy `pub mod supervisor;` — a jeden wiersz poza blokiem OWNS to pytanie do
// czlowieka (AGENTS.md §7), nie cichy dopisek. Czlowiek odpowiedzial commitem 687712a: linia
// stoi w `engine/mod.rs`, wiec jedyny poprawny adres modulu to `engine::supervisor`.
//
// Zostawienie obu naraz zbudowaloby ten sam plik dwa razy, jako dwa rozne moduly. To nie jest
// blad kompilacji — to dwa niezalezne typy `GroupProof`, ktorych kompilator nie zamieni jeden
// w drugi, wiec `stop()` z jednego modulu nie da sie porownac z dowodem z drugiego.

/// Nazwa pliku dziennika wewnątrz katalogu podanego do [`install_logging`].
const LOG_FILE: &str = "loadout.log";

/// Etykieta jedynego okna. Ta sama wartość stoi w `app.windows[0].label` w `tauri.conf.json`
/// i w polu `windows` każdego pliku w `src-tauri/capabilities/`. Uprawnienia celujące w okno,
/// którego nie ma, nie dotyczą niczego i odmawiają KAŻDEGO wywołania z webviewa — a webview
/// dowiedziałby się o tym dopiero w T-07 i przeczytał to jako zepsute wywołanie.
const MAIN_WINDOW: &str = "main";

/// Jeden uchwyt pliku na cały bieg, współdzielony przez wszystkie wątki.
///
/// `Arc<File>`, nigdy `try_clone()` na linijkę: `try_clone` to `dup(2)` przy każdym zdarzeniu,
/// a w Murmurze skończyło się to paniką z wyczerpania deskryptorów **wewnątrz samego
/// logowania**, czyli w jedynym kodzie, który mógł o tym opowiedzieć [T8 §9, 2026-08-15].
/// `&File` implementuje `Write`, więc pisanie nie potrzebuje ani zamka, ani kopii uchwytu.
#[derive(Debug)]
struct SharedFile(Arc<File>);

impl<'a> MakeWriter<'a> for SharedFile {
    type Writer = &'a File;

    fn make_writer(&'a self) -> Self::Writer {
        &self.0
    }
}

/// Wpina `tracing` w plik pod `dir` i zwraca ścieżkę tego pliku. Zdarzenia lecą jednocześnie
/// na wyjście diagnostyczne i do pliku, bo uruchomiona dwuklikiem aplikacja nie ma tego
/// pierwszego: `LaunchServices` je wyrzuca, więc release bez pliku jest niediagnozowalny.
///
/// Uchwyt pliku jest JEDEN na cały bieg (`Arc<File>` + `MakeWriterExt::and`), nigdy
/// `try_clone()` na linijkę: w Murmurze to był `dup(2)` na linijkę i panika z wyczerpania
/// deskryptorów wewnątrz samego logowania [T8 §9, 2026-08-15].
pub fn install_logging(dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(LOG_FILE);

    // O_APPEND, nie „otwórz i pisz od pozycji": przy ośmiu wątkach dopisujących do jednego
    // deskryptora tylko append daje zapis na koniec pliku jednym wywołaniem jądra. Bez tego
    // dwa zapisy potrafią wylądować na tym samym offsecie i zostaje jedna linia sklejona
    // z dwóch — awaria, która wygląda jak zgubione zdarzenie, a nie jak wyścig.
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let to_file = SharedFile(Arc::new(file));

    // Bez RUST_LOG i tak chcemy dziennik: aplikacja odpalona dwuklikiem nie ma jak go dostać.
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .parse_lossy(std::env::var("RUST_LOG").unwrap_or_default());

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        // Kody sterujące terminala w pliku, który czyta się rok później, to szum.
        .with_ansi(false)
        .with_writer(to_file.and(io::stderr))
        .finish();

    tracing::subscriber::set_global_default(subscriber).map_err(io::Error::other)?;

    Ok(path)
}

/// Wpina hak paniki, który najpierw loguje przez `tracing`, a potem **woła poprzedni hak**.
///
/// Łańcuchowanie, nie zastąpienie: tokio połyka paniki na granicy zadania, a domyślny hak pisze
/// wyłącznie na wyjście diagnostyczne, które `LaunchServices` wyrzuca — hak, który zastępuje
/// poprzedni, kasuje jedyny ślad po pierwszej panice w release [T8 §9, 2026-08-15].
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("{info}");
        previous(info);
    }));
}

/// Katalog użytkownika Loadouta. Pliki są prawdą, a dziennik leży obok nich
/// (`docs/ARCHITECTURE.md` §8).
fn loadout_dir() -> PathBuf {
    // HOME zamiast osobnej zależności na katalogi: to jedyne miejsce w repo, które o to pyta.
    std::env::var_os("HOME").map_or_else(
        || PathBuf::from(".loadout"),
        |home| PathBuf::from(home).join(".loadout"),
    )
}

/// Katalog, w którym pracują agenci tego okna.
///
/// ZMIERZONE 2026-08-17, przy pierwszym prawdziwym uruchomieniu. Pierwsza wersja brała po prostu
/// `current_dir()` i to jest ZŁE w obu przypadkach, w jakich ta aplikacja startuje: `npm run
/// tauri dev` uruchamia cargo z `src-tauri/`, więc bieg zakładałby `src-tauri/.loadout/runs/`
/// — w środku drzewa źródeł, gdzie `.gitignore` tego NIE łapie (ignoruje `/.loadout/runs/*`,
/// czyli tylko w korzeniu). Artefakty biegu wjechałyby do repozytorium i zapaliłyby
/// `quick-scope` przy pierwszym zadaniu. Kliknięcie w ikonę daje z kolei `/`.
///
/// Katalog projektu ma WYBIERAĆ CZŁOWIEK — to są karty workspace'ów z T-08 i `ARCHITECTURE.md`
/// §6a („karty odpowiadają na »w którym folderze«"). Ta droga nie dochodzi jeszcze do stanu,
/// więc do tego czasu: `LOADOUT_PROJECT`, jeśli ktoś go poda, a w przeciwnym razie DEDYKOWANY
/// katalog w bibliotece. Dedykowany, a nie `current_dir()`, bo cicha praca w nieoczekiwanym
/// miejscu jest gorsza niż praca w miejscu nudnym, ale nazwanym w dzienniku.
fn project_dir(home: &Path) -> PathBuf {
    std::env::var_os("LOADOUT_PROJECT").map_or_else(|| home.join("workspace"), PathBuf::from)
}

/// Domyka biegi, które zginęły razem z poprzednim uruchomieniem aplikacji.
///
/// Trzy kroki, w tej kolejności i nie w innej: przeczytaj, ROZSTRZYGNIJ, dopiero potem działaj.
/// `recovery::decide` jest czystą funkcją i to ona trzyma wszystkie zasady — łącznie ze
/// strażnikiem czasu startu maszyny, bez którego `killpg` po zapisanym `pgid` trafiałby po
/// restarcie w niewinny proces (`kern.maxproc` = 16 000, PID-y przewijają się w godzinach).
///
/// Domykacz wstrzykujemy jako domknięcie, a nie wołamy w środku `apply`: dzięki temu kryterium
/// akceptacji może podstawić własny i sprawdzić, że NIC nie zostało zabite, bez zabijania
/// czegokolwiek na prawdziwej maszynie.
///
/// PUBLICZNA, i to nie jest ustępstwo na rzecz testu. To jest NAZWANA FAZA startu aplikacji,
/// a kryterium AC-2 z `tasks/T-35.md` żąda dowodu, że biegnie ona przez ścieżkę startową, a nie
/// że `decide()` da się zawołać wprost — bo `decide()` dawało się wołać od T-20 i przez cały ten
/// czas nie wołał go nikt. Prywatna funkcja byłaby niesprawdzalna dokładnie w tym jednym
/// wymiarze, o który tu chodzi.
pub async fn recover_from_last_time(
    store: &store::Store,
) -> Result<(usize, usize, recovery::RecoveryReport), Box<dyn std::error::Error>> {
    let rows = recovery::rows_to_judge(&store.reader()?)?;
    if rows.is_empty() {
        return Ok((0, 0, recovery::RecoveryReport::default()));
    }

    let machine = recovery::Machine {
        // Brak odpowiedzi z systemu zapisujemy jako pusty napis, a nie jako zgadniętą wartość:
        // pusty nie zrówna się z żadnym zapisanym znacznikiem, więc strażnik wstrzyma strzał.
        // Zgadnięta wartość mogłaby przypadkiem trafić i wtedy strażnik byłby ozdobą.
        boot_id: engine::supervisor::machine_booted_at().unwrap_or_default(),
        own_pgid: engine::supervisor::own_process_group(),
    };

    let plan = recovery::decide(&rows, &machine);
    let report = recovery::apply(
        &plan,
        &mut |pgid| match engine::supervisor::reap_group(pgid) {
            engine::supervisor::GroupProof::Dead { .. } => recovery::ReapOutcome::ProvenDead,
            engine::supervisor::GroupProof::Alive => recovery::ReapOutcome::StillAlive,
        },
    );

    // Zapis idzie JEDYNYM pisarzem (niezmiennik 2), a nie własnym połączeniem: drugie
    // połączenie zapisujące do tej bazy jest zakleszczeniem, nie „czasem wolniej", i
    // `checks/quick-boundary.sh` czyta konstruktory połączeń gerpem właśnie po to.
    let runs: Vec<(String, String)> = plan
        .run_status
        .iter()
        .map(|c| (c.run_id.clone(), c.status.clone()))
        .collect();
    let steps: Vec<(String, String, String)> = plan
        .step_status
        .iter()
        .map(|c| (c.step_id.clone(), c.status.clone(), c.reason.clone()))
        .collect();
    let counts = (runs.len(), steps.len());

    // `await`, nie `block_on`: TA funkcja nie ma prawa decydować, jak jej wołający mostkuje
    // sync z async. Zmierzone 2026-08-17 — `block_on` w środku panikuje zdaniem „Cannot start
    // a runtime from within a runtime", kiedy woła ją ktoś, kto już jest w runtime (a kryterium
    // akceptacji jest właśnie takim wołającym). Most stoi więc w `setup`, czyli w jedynym
    // miejscu, które naprawdę nie jest asynchroniczne.
    store.writer().recovered(runs, steps).await?;
    Ok((counts.0, counts.1, report))
}

/// Otwiera okno. Cała powłoka po stronie Rusta zaczyna się tutaj i tutaj kończy.
pub fn run() {
    match install_logging(&loadout_dir()) {
        Ok(path) => tracing::info!("this run writes to {}", path.display()),
        Err(error) => eprintln!("Loadout could not open its log file: {error}"),
    }
    install_panic_hook();

    /* STAN APLIKACJI — podłączony 2026-08-17, po tym jak okno stanęło i nie umiało nic zapisać.
     *
     * `ipc.rs` sam to zgłosił w komentarzu: „nikt jej jeszcze nie oddaje builderowi… trzy komendy
     * biegu są zarejestrowane i odmawiają wywołania zdaniem »state not managed«". Pisarz T-30 nie
     * mógł tego dopisać — `lib.rs` nie był w jego OWNS — i słusznie zostawił to człowiekowi
     * (AGENTS.md §7). To jest ta decyzja.
     *
     * Bez `.manage(…)` KAŻDA komenda biorąca `State<'_, AppState>` pada pod palcem, a
     * `generate_handler!` tego nie widzi: rejestracja i stan to dwie różne rzeczy, więc lista
     * komend jest kompletna i aplikacja i tak nie działa.
     *
     * PROJEKT to katalog, w którym stoi proces. Karty workspace'ów (T-08) wybiorą go per karta,
     * ale ta droga jeszcze nie dochodzi do stanu; do tego czasu jedno okno pracuje nad jednym
     * katalogiem i mówi o tym wprost w dzienniku, zamiast po cichu pisać nie tam, gdzie myślisz. */
    let home = loadout_dir();
    let project = project_dir(&home);
    tracing::info!(
        "library at {}, project at {}",
        home.display(),
        project.display()
    );

    let outcome = tauri::Builder::default()
        .setup(move |app| {
            /* Baza otwiera się WEWNĄTRZ runtime'u Tauri, i to nie jest ozdoba składniowa.
             *
             * ZMIERZONE 2026-08-17: `setup` biegnie na wątku głównym i **nie jest** kontekstem
             * tokio, więc `Store::open` — który pyta `Handle::try_current()` — nie znajduje
             * runtime'u i cała aplikacja pada zdaniem „a store can only be opened from inside
             * a tokio runtime". Panika w `setup` jest nieodwracalna (`panic in a function that
             * cannot unwind`), więc okno nie zdąża się nawet pokazać.
             *
             * Poprzednia wersja tego komentarza twierdziła, że `setup` runtime MA. Twierdzenie
             * było nieprawdziwe i kosztowało jedno uruchomienie; zostaje zapisane, bo następny
             * czytelnik zada dokładnie to samo pytanie.
             *
             * `block_on` z runtime'u Tauri, a nie własny `Runtime::new()`: druga pętla zdarzeń
             * w tym procesie to drugi zestaw wątków i drugie miejsce, w którym żyją zadania
             * biegu. */
            let store = tauri::async_runtime::block_on(async {
                store::Store::open(&home.join("loadout.db"))
            })?;

            /* Fabryka sterowników. Funkcja, nie mapa — trzeci vendor ma wejść bez wydania
             * Loadouta (`commands/mod.rs`). Oba istniejące adaptery muszą być żywe także dla
             * analizy importu; atrapą Codeksa aplikacja pokazywała wybór, który zawsze odmawiał. */
            let claude: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::new());
            let codex: Arc<dyn AgentDriver> = Arc::new(CodexDriver::new());
            let drivers: Drivers = Arc::new(move |vendor| match vendor {
                Vendor::ClaudeCode => Arc::clone(&claude),
                Vendor::Codex => Arc::clone(&codex),
            });

            /* ODZYSKIWANIE PO AWARII — wpięte 2026-08-17 (T-35 AC-2), i do tego dnia było
             * STRUKTURALNIE MARTWE. `recovery::decide()` i `apply()` istniały od T-20, miały
             * własne kryteria i **nikt ich nie wołał**; do tego nikt nie zapisywał czasu startu
             * maszyny, więc gdyby je wtedy wpiąć, każdy wiersz padłby na `NO_BOOT_TIME` i nic
             * by nie posprzątało. Mechanizm był zielony w testach i nie mógł zadziałać.
             *
             * Biegnie TUTAJ, przed oddaniem stanu oknu: bieg, który zginął razem z aplikacją,
             * ma być oznaczony, ZANIM człowiek zobaczy listę. Ekran pokazujący `running` dla
             * czegoś, czego nikt już nie prowadzi, jest gorszy niż pusta lista — bo wygląda
             * na pracę w toku.
             *
             * PORAŻKA ODZYSKIWANIA NIE ZABIERA OKNA. Aplikacja, która nie wstaje, bo nie udało
             * się posprzątać po poprzednim uruchomieniu, zamyka człowieka poza jego własnymi
             * plikami. Zdanie idzie do dziennika i idziemy dalej. */
            // `block_on` TUTAJ, bo `setup` Tauri nie jest kontekstem async, a pisarz magazynu
            // jest zadaniem tokio. To jest jedyne miejsce w tym łańcuchu, które musi mostkować.
            match tauri::async_runtime::block_on(recover_from_last_time(&store)) {
                Ok((runs, steps, report)) if runs + steps > 0 => tracing::info!(
                    "recovery: {runs} run(s) and {steps} step(s) marked interrupted; \
                     {} group(s) proven dead, {} still alive, {} belong to someone else",
                    report.reaped.len(),
                    report.unproven.len(),
                    report.foreign.len()
                ),
                Ok(_) => tracing::debug!("recovery: nothing was left running"),
                Err(error) => {
                    tracing::error!(
                        "recovery could not finish, opening the window anyway: {error}"
                    );
                }
            }

            app.manage(ipc::AppState::new(
                home.clone(),
                project.clone(),
                store,
                drivers,
            ));
            Ok(())
        })
        // Pierwsza w kolejności i tak ma zostać: druga kopia Loadouta to drugi zestaw agentów
        // pod tymi samymi plikami. Zamiast otwierać kolejne okno, podnosimy to, które jest.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                let _ = window.set_focus();
            }
        }))
        /* ZAMKNIĘCIE OKNA ZATRZYMUJE BIEG, i to jest transport, nie polityka: całą decyzję
         * podejmuje `commands::run::stop_before_closing` (niezmiennik 1 i 23), a tutaj zostaje
         * wyłącznie „wstrzymaj zamknięcie, zawołaj, potem zamknij".
         *
         * `prevent_close` PRZED czymkolwiek innym: bez tego okno znika w tej samej chwili, proces
         * kończy się razem z nim, a zadanie zatrzymujące bieg nie ma już gdzie działać — czyli
         * agenci zostają żywi, dokładnie tak, jak było do 2026-08-19.
         *
         * `destroy()` na końcu, także po błędzie: okno, którego nie da się zamknąć, bo
         * zatrzymywanie biegu się nie udało, zamykałoby człowieka wewnątrz aplikacji. Zdanie
         * o niepowodzeniu idzie do dziennika — a odzyskiwanie przy następnym starcie jest siecią
         * pod tym przypadkiem i tam już jest. */
        .on_window_event(|window, event| {
            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            api.prevent_close();
            let window = window.clone();
            tauri::async_runtime::spawn(async move {
                let state = window.state::<ipc::AppState>();
                if let Err(error) = commands::run::stop_before_closing(&state.deps()).await {
                    tracing::error!("closing anyway: the run could not be stopped: {error}");
                }
                /* Rozmowa z orchestratorem też jest procesem — po zamknięciu okna przeszłaby pod
                 * PID 1 i pracowała dalej, a odzyskiwanie po niej nie posprząta, bo rozmowa nie ma
                 * wpisu w indeksie biegów. */
                state.close_chat().await;
                let _ = window.destroy();
            });
        })
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(ipc::command_handler())
        .run(tauri::generate_context!());

    if let Err(error) = outcome {
        tracing::error!("Loadout could not open its window: {error}");
        std::process::exit(1);
    }
}
