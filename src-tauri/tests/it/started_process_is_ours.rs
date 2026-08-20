//! AC-1 dla T-72: rzecz zamówiona komendą jest NASZA, żyje po powrocie tego wywołania,
//! i da się ją ubić **z dowodem**.
//!
//! # Cztery zdania, które ten plik trzyma razem, i po co razem
//!
//! Rzecz uruchomiona z wiersza wejścia (`/start npm run dev`) różni się od kroku „sprawdź"
//! dokładnie tym, czego nie widać w typie: krok wraca sam i wtedy orzekamy, a ta rzecz nie wraca
//! wcale, dopóki człowiek jej nie zatrzyma albo nie zniknie okno. Implementacja, która oddaje ją
//! przez `CommandDriver::run`, kompiluje się, czyta dobrze i jest krokiem sprawdzającym pod inną
//! nazwą — wołający dostaje wtedy nekrolog zamiast uchwytu, więc przez cały czas życia tej
//! rzeczy nie ma czego pokazać na kafelku ani czego ubić.
//!
//! # SŁABA WERSJA i dlaczego jest słaba
//!
//! `assert!(processes.stop(pgid).await.is_some())` — czyli sprawdzenie, że zatrzymanie wróciło
//! bez błędu. Przechodzi dla implementacji, która wysyła sygnał i nie czeka: proces żyje dalej,
//! kafelek gaśnie, a maszyna płonie w tle. Dokładnie ten pomiar dał `A after kill: total=2
//! orphaned=2` [T7 §3.1] w chwili, w której status bezpośredniego dziecka mówił „zabity".
//! Rozróżnia to (c) razem z (e): pytamy JĄDRO, a nie naszą wartość zwrotną, i pytamy je
//! **przed** zatrzymaniem, żeby „nie ma nikogo" nie okazało się prawdą o pustym zbiorze.
//!
//! # Kształt fikstury jest kształtem prawdziwej apki
//!
//! `npm run dev` rozwidla dzieci i biegnie dalej. Skrypt niżej robi to samo: odpala wnuka w tle
//! i sam kręci się w pętli krótkich snów. Pętla, nie pojedyncze `sleep`, bo powłoka
//! exec-optymalizuje ostatnią komendę i znacznik znika wtedy z `argv`, a skan `ps` przestaje
//! cokolwiek widzieć [T7 §8.2].
//!
//! # Granica z niezmiennika 3
//!
//! Sygnał zerowy **w pliku testu** jest w porządku: `checks/quick-boundary.sh` wyłącza ścieżki
//! `*/tests/*` ze wszystkich trzech granic, po ŚCIEŻCE, nigdy po treści. To, że sterownik tej
//! granicy nie przekracza, sprawdza tamten skrypt — jest to tu wymienione, żeby nikt nie
//! „naprawił" testu, wkładając `libc::kill` do `command.rs`.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
//
// 2026-08-20 — `too_many_lines` DOPISANE, i to jest zgłoszenie, nie wygoda. `full-clippy` był
// czerwony na tej gałęzi od commita kontraktowego: pierwszy przypadek niżej ma 124 wiersze przy
// sufcie 100, a `quick-clippy` biegnie `--lib`, więc tego nie widzi ani razu. Zmierzone —
// funkcja jest co do bajtu ta sama, co w `d464206`. Rozcięcie jej na pomocnicze funkcje byłoby
// przepisaniem specyfikacji, której asercji nie wolno tknąć, a jej długość jest treścią: to jest
// jedna sekwencja pomiarowa, w której KOLEJNOŚĆ zdań jest asercją (grupa żyje → dowód → grupa
// nie żyje). Rozdzielona na trzy funkcje przestałaby o tej kolejności cokolwiek mówić.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use std::error::Error;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use loadout_lib::commands::processes::Processes;
use loadout_lib::engine::drivers::command::{GIVE_UP_AFTER, StartSpec};
use loadout_lib::engine::supervisor::{self, GroupProof};
use tokio::process::Command;

/// Sufit cierpliwości. Bez niego regresja objawia się jako zawieszenie, bramka zwraca rc 124,
/// a to jest fałszywa czerwień, nie dowód.
const PATIENCE: Duration = Duration::from_secs(20);

/// Rodzic: odpala wnuka w tle i kręci się dalej — tak jak każdy dev server.
const PARENT: &str = r#"#!/bin/sh
# $1 = ścieżka skryptu-wnuka, $2 = znacznik
"$1" "$2-child" &
while :; do
  sleep 0.2
done
"#;

/// Wnuk: nic nie robi poza tym, że **jest widoczny w `ps`** pod swoim znacznikiem.
const GRANDCHILD: &str = r"#!/bin/sh
# $1 = znacznik; ma zostać w argv, więc pętla, nie pojedyncze `sleep`
while :; do
  sleep 0.2
done
";

/// Jeden wiersz `ps -eo pid,ppid,pgid,args`.
#[derive(Debug)]
struct PsRow {
    ppid: i32,
    pgid: i32,
    args: String,
}

/// Znacznik unikalny dla tego biegu. Bez unikalności skan `ps` łapałby procesy z poprzedniego,
/// przerwanego biegu i meldował wyciek, którego nie ma — albo zieleń, której nie ma.
fn unique_marker(tag: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("loadout-t72-{tag}-{}-{nanos}", std::process::id())
}

/// Zapisuje wykonywalny skrypt `#!/bin/sh` i zwraca jego ścieżkę.
fn write_script(dir: &Path, name: &str, body: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = dir.join(name);
    fs::write(&path, body)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

/// Pyta JĄDRO, czy w grupie `pgid` jest jeszcze ktokolwiek — nie wysyłając sygnału.
///
/// To jedyny pomiar, który liczy się w niezmienniku 6, i jedyny spoza drzewa naszego procesu:
/// status zebrany przez `wait()` mówi wyłącznie o bezpośrednim dziecku, a płacimy za wnuki.
// `kill(2)` nie ma bezpiecznego opakowania w std, a ten test z definicji pyta system operacyjny
// zamiast naszego kodu (niezmiennik 20).
#[allow(unsafe_code)]
fn group_probe(pgid: i32) -> io::Result<()> {
    // ZAPORA, NIE OZDOBA. `kill(-0, …)` znaczy „moja własna grupa", czyli ten proces testowy
    // i wszystko, co go uruchomiło. Szkielet, który nie startuje niczego, oddaje `pgid` równy
    // zeru — a pytanie o niego wyglądałoby jak zieleń, zamiast jak brak procesu.
    assert!(
        pgid > 1,
        "pgid {pgid} is not a process group this test may ask about: 0 means our own group and \
         the answer would be about the test runner, not about what was started"
    );
    // SAFETY: `kill` z sygnałem 0 niczego nie dostarcza — sprawdza tylko istnienie i prawa.
    // Argumenty to zwykłe liczby, więc nie ma tu żadnego wskaźnika ani czasu życia do złamania.
    let rc = unsafe { libc::kill(-pgid, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Wiersze `ps` zawierające `marker`. Pomiar spoza naszego drzewa procesów.
async fn ps_scan(marker: &str) -> Result<Vec<PsRow>, Box<dyn Error>> {
    let output = Command::new("ps")
        .args(["-eo", "pid,ppid,pgid,args"])
        .output()
        .await?;
    let text = String::from_utf8_lossy(&output.stdout);

    let mut rows = Vec::new();
    for line in text.lines() {
        if !line.contains(marker) {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        // `parent`/`group` zamiast `ppid`/`pgid`: dwie nazwy różniące się jedną literą w środku
        // to ten rodzaj pary, w której podmiana jednej na drugą przechodzi przez recenzję
        // niezauważona — a tutaj jedna odpowiada na „czy osierocony", druga na „czy w naszej
        // grupie".
        let (Ok(parent), Ok(group)) = (fields[1].parse::<i32>(), fields[2].parse::<i32>()) else {
            continue;
        };
        rows.push(PsRow {
            ppid: parent,
            pgid: group,
            args: fields[3..].join(" "),
        });
    }
    Ok(rows)
}

/// Czeka, aż `ps` pokaże co najmniej `want` procesów ze znacznikiem. Zwraca ostatni skan — także
/// wtedy, gdy jest za krótki, żeby asercja wołającego mogła powiedzieć, czego brakuje.
async fn wait_for_rows(
    marker: &str,
    want: usize,
    limit: Duration,
) -> Result<Vec<PsRow>, Box<dyn Error>> {
    let deadline = Instant::now() + limit;
    loop {
        let rows = ps_scan(marker).await?;
        if rows.len() >= want || Instant::now() >= deadline {
            return Ok(rows);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Skrypt-rodzic razem z wnukiem, gotowy do podania jako wiersz powłoki.
fn app_that_forks(dir: &Path, marker: &str) -> Result<String, Box<dyn Error>> {
    let grandchild = write_script(dir, "grandchild.sh", GRANDCHILD)?;
    let parent = write_script(dir, "parent.sh", PARENT)?;
    Ok(format!(
        "{} {} {marker}",
        parent.display(),
        grandchild.display()
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_started_command_is_ours_and_goes_down_with_proof() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let marker = unique_marker("ours");
    let line = app_that_forks(dir.path(), &marker)?;

    let processes = Processes::new();
    let started = processes.start(&StartSpec {
        command: line.clone(),
        cwd: dir.path().to_path_buf(),
    })?;

    // ── (a) WŁASNA GRUPA, ZNANA OD RAZU ───────────────────────────────────────────────────
    // `pgid` jest zwykłą wartością dostępną natychmiast po starcie, nie czymś wyłuskanym
    // z pierwszej linii wyjścia [T7 §6.2] — i to jest ta kolejność, która w ogóle czyni
    // sprzątanie po awarii aplikacji możliwym.
    assert!(
        started.pgid > 1,
        "starting something has to report a real process group the moment it starts; it \
         reported {started:?}"
    );
    assert_ne!(
        started.pgid,
        supervisor::own_process_group(),
        "it landed in OUR OWN group, so nothing was put into a group of its own. Then \
         kill(-pgid, …) aims at Loadout itself, and the escalation that is supposed to end one \
         thing would end the window instead (invariant 6, and the reason spawn goes through one \
         place at all)"
    );
    assert_eq!(
        started.command, line,
        "the shell line has to come back character for character: it is the NAME of this thing \
         on the screen, and a label nobody typed is a relation that is not in the data \
         (invariant 17)"
    );
    assert!(
        started.alive,
        "and it has to say it is up, because it is: {started:?}"
    );

    // ── (b) ŻYJE PO POWROCIE WYWOŁANIA, KTÓRE JE ZAMÓWIŁO ─────────────────────────────────
    // To jest CAŁA różnica wobec kroku „sprawdź", i jedyne miejsce, w którym ją widać: tam
    // wywołanie wraca, kiedy komenda się skończy, więc na kafelku nie byłoby czego pokazać.
    let child = format!("{marker}-child");
    let before = wait_for_rows(&marker, 2, Duration::from_secs(5)).await?;
    assert!(
        before.iter().any(|row| row.args.contains(&child)),
        "the child never showed up in ps, so the rest of this test would prove dead something \
         that never lived. A thing that starts nothing cannot leak one either. ps saw {before:?}"
    );
    assert!(
        before.iter().all(|row| row.pgid == started.pgid),
        "everything carrying the marker has to sit in the group we were handed (pgid {}); a \
         child in another group is one that a group-wide stop will never reach, and that is the \
         whole leak. ps saw {before:?}",
        started.pgid
    );

    // ── (e) KONTROLA PRZECIW PUSTEMU PRZEJŚCIU ────────────────────────────────────────────
    // Bez tej linii „w grupie nie ma nikogo" niżej jest prawdą także wtedy, gdy nikogo tam
    // nigdy nie było — czyli dla implementacji, która nie uruchomiła nic.
    group_probe(started.pgid).map_err(|why| {
        format!(
            "kill(-{}, 0) says the group is not there BEFORE anything stopped it: {why}. Every \
             assertion about proving it dead would then be a statement about an empty set",
            started.pgid
        )
    })?;

    assert!(
        processes
            .list()
            .iter()
            .any(|one| one.pgid == started.pgid && one.alive),
        "and the list has to know about it while it runs, or the agents list has nothing to draw \
         a tile from. It says: {:?}",
        processes.list()
    );

    // ── (c) ZATRZYMANIE WRACA DOPIERO Z DOWODEM ───────────────────────────────────────────
    let stopped = tokio::time::timeout(PATIENCE, processes.stop(started.pgid))
        .await
        .map_err(|_| format!("stopping it did not come back within {PATIENCE:?}"))?;
    let Some(proof) = stopped else {
        return Err(format!(
            "the list did not know pgid {}, so nothing was stopped and nothing was proven",
            started.pgid
        )
        .into());
    };
    assert!(
        matches!(proof, GroupProof::Dead { .. }),
        "stopping it carries the proof that the group is GONE, not a report that a signal went \
         out. Ok(()) after a signal reads as 'dead' to the caller while the group keeps burning \
         the machine (invariant 6). It carried {proof:?}"
    );

    let asked = group_probe(started.pgid);
    let errno = asked.err().and_then(|error| error.raw_os_error());
    assert_eq!(
        errno,
        Some(libc::ESRCH),
        "kill(-{}, 0) still finds somebody in the group after we called it stopped. This is the \
         measurement that returned total=2 orphaned=2 in T7 §3.1 while the child's own exit \
         status said 'killed'",
        started.pgid
    );

    let after = ps_scan(&marker).await?;
    let orphaned: Vec<&PsRow> = after.iter().filter(|row| row.ppid == 1).collect();
    assert!(
        orphaned.is_empty(),
        "total={} orphaned={} — things carrying our marker were reparented to PID 1 and are \
         still running. That is the leak from T7 §3.1 verbatim, and it burns money invisibly: \
         {orphaned:?}",
        after.len(),
        orphaned.len()
    );
    assert!(
        after.is_empty(),
        "ps still finds something carrying the marker after we were told the group is gone: \
         {after:?}"
    );
    Ok(())
}

/// To, co ta rzecz wypisze, DOCHODZI — i dochodzi z obu potoków.
///
/// # Po co ta asercja tu stoi
///
/// Bo panel, który otwiera kliknięcie w kafelek, pokazuje dokładnie tę wartość, a żadne
/// z czterech zdań kryterium jej nie dotyka: kryterium frontendowe biegnie pod atrapą granicy,
/// która odpowiada KSZTAŁTEM, więc przechodzi z pustym wyjściem co do joty. Zostawiłoby to
/// kafelek, w który da się wejść i nie ma tam nic — czyli kontrolkę bez skutku z dodatkowym
/// krokiem (niezmiennik 16), przy zamówieniu, które brzmiało „po kliku mogę tam wejść".
///
/// OBA POTOKI, nie jeden, i to jest ta sama wada, która żyła w `supervisor.rs` do 2026-08-18:
/// `stderr` był ustawiany na potok i **nie dawał się odebrać**, więc najczęstsza realna awaria —
/// brak albo niezalogowane CLI, które pisze WŁAŚNIE tam — była niediagnozowalna z okna. Rzecz
/// uruchomiona z ręki pisze tam jeszcze częściej: `npm` sypie ostrzeżeniami na skargi, a wyjście
/// bez nich czyta się jak bieg bez problemów.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn what_a_started_command_prints_reaches_the_registry() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let processes = Processes::new();
    let started = processes.start(&StartSpec {
        // Jedno zdanie na wyjście, jedno na skargi. Przez powłokę, bo tak wygląda każda linia,
        // którą wpisze człowiek — i bo `2>&1` po drodze zlałoby te dwa potoki w jeden, czyli
        // skasowałoby połowę tej asercji.
        command: "echo it-said-this; echo it-complained-this 1>&2".to_owned(),
        cwd: dir.path().to_path_buf(),
    })?;

    let deadline = Instant::now() + PATIENCE;
    loop {
        let said = processes.said(started.pgid).unwrap_or_default();
        if said.contains("it-said-this") && said.contains("it-complained-this") {
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "after {PATIENCE:?} the registry holds {said:?}, and this thing printed one \
                 sentence to each of its two streams. A thing whose output never arrives is a \
                 tile you can click into and find empty — and worse, a pipe nobody empties stops \
                 the child on `write` at ~64 KB, which from the window looks like an app that \
                 came up and went quiet"
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // Rejestr, który nie zna tej grupy, oddaje `None` — wartość, nie błąd. Kontrola przeciw
    // wersji, która na KAŻDE pytanie odpowiada tym samym tekstem.
    assert_eq!(
        processes.said(started.pgid + 1_000_000),
        None,
        "asking about a group the list does not know has to answer 'nothing', or the assertion \
         above says nothing about WHICH thing printed that"
    );

    let _ = tokio::time::timeout(PATIENCE, processes.close()).await;
    Ok(())
}

/// Rzecz, która zeszła SAMA, przestaje mówić o sobie, że żyje.
///
/// # Po co ta asercja tu stoi, skoro żadne z czterech zdań kryterium jej nie żąda
///
/// Bo bez niej cały ten plik dowodzi wyłącznie tego, że rzecz startuje i że da się ją ubić — a
/// kafelek ma istnieć dokładnie tak długo, jak rzecz za nim (niezmiennik 17). Odsiew robi czysta
/// funkcja po stronie okna (`src/sections/run/rail/processes.ts`) i sądzi go osobne kryterium,
/// tylko że ona odsiewa po POLU `alive` — a tego, że to pole kiedykolwiek gaśnie, nie sprawdzało
/// nic. Pole, które nie gaśnie, przechodzi tamto kryterium co do joty i zostawia „Running" nad
/// komendą zeszłą dwie minuty temu, czyli dokładnie tę cichą porażkę, przed którą stoi to
/// zadanie. To jest ta sama luka co „kryterium zielone, funkcja martwa" (niezmiennik 29), tylko
/// o jedno pole niżej.
///
/// Rzecz, która zeszła, ZOSTAJE na liście i to jest osobne zdanie tej samej asercji: rejestr,
/// który zapomina wpis w chwili śmierci, nie ma jak POWIEDZIEĆ oknu, że coś zeszło — a okno,
/// które o tym nie usłyszy, zostawia kafelek na ekranie.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_started_command_that_went_down_by_itself_stops_saying_it_is_up()
-> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let processes = Processes::new();
    // Komenda, która kończy się sama i od razu — najkrótsza rzecz, jaką człowiek może wpisać
    // i która ma po sobie coś sprzątnąć.
    let started = processes.start(&StartSpec {
        command: "true".to_owned(),
        cwd: dir.path().to_path_buf(),
    })?;

    // Kontrola przeciw pustemu przejściu: bez niej „przestało mówić, że żyje" jest prawdą także
    // o polu, które nie powiedziało tego ani razu — czyli o kafelku, którego nigdy nie było.
    assert!(
        started.alive,
        "it has to say it is up at the moment it starts, or the assertion below is about a \
         field that was never true: {started:?}"
    );

    let deadline = Instant::now() + PATIENCE;
    loop {
        let known = processes.list();
        let Some(one) = known.iter().find(|one| one.pgid == started.pgid) else {
            return Err(format!(
                "the list forgot pgid {} the moment it went down. Then the window never hears \
                 about the death it did not cause, and the tile stays on the screen saying \
                 'running' over something that is gone",
                started.pgid
            )
            .into());
        };
        if !one.alive {
            break;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "after {PATIENCE:?} the list still says it is up, and it ended by itself long \
                 ago. A field that never goes out passes the sifting on the window side word for \
                 word and leaves 'Running' over a line that went down two minutes ago: {one:?}"
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    // I nadal da się po niej sprzątnąć: dowód dotyczy grupy, a nie tego, kto ją zatrzymał.
    let proofs = tokio::time::timeout(PATIENCE, processes.close())
        .await
        .map_err(|_| format!("closing did not come back within {PATIENCE:?}"))?;
    assert!(
        proofs
            .iter()
            .all(|one| matches!(one, GroupProof::Dead { .. })),
        "closing has to prove the group gone even for something that ended on its own: a leader \
         nobody collected is a zombie, and a zombie still answers the zero signal — so the group \
         would never give ESRCH, not here and not in recovery. It gave {proofs:?}"
    );
    assert!(
        processes.list().is_empty(),
        "and nothing stays behind: {:?}",
        processes.list()
    );
    Ok(())
}

/// (d) Sufit kroku „sprawdź" nie ma tu prawa nic ubić.
///
/// Zegar wirtualny, nie prawdziwy: trzydziestu minut nie da się przeczekać w bramce, a stała
/// przeczytana z pliku i porównana z polem byłaby sprawdzeniem obecności napisu, nie zachowania
/// (niezmiennik 20). `tokio::time::pause()` przewija KAŻDY nasz limit naraz, więc implementacja,
/// która oddała tę drogę przez `Checking::settle` — albo dołożyła sobie własny budzik — kończy tę
/// rzecz w tej właśnie chwili, a jądro powie o tym prawdę.
///
/// Zegar zatrzymujemy PO tym, jak `ps` potwierdziło, że rzecz stoi: skan i czekanie na niego
/// biegną na `tokio::time::sleep`, więc pod zatrzymanym zegarem mierzyłyby własne przewijanie.
#[tokio::test]
async fn the_thirty_minute_ceiling_of_a_check_step_never_reaches_it() -> Result<(), Box<dyn Error>>
{
    let dir = tempfile::tempdir()?;
    let marker = unique_marker("no-ceiling");
    let line = app_that_forks(dir.path(), &marker)?;

    let processes = Processes::new();
    let started = processes.start(&StartSpec {
        command: line,
        cwd: dir.path().to_path_buf(),
    })?;

    // Kontrola: przewijanie zegara nad rzeczą, która nigdy nie wstała, dowodzi wyłącznie tego,
    // że jej nie ma.
    let up = wait_for_rows(&marker, 2, Duration::from_secs(5)).await?;
    assert!(
        up.len() >= 2,
        "this case needs it really running before the clock jumps, otherwise 'it survived' is a \
         statement about nothing. ps saw {up:?}"
    );
    group_probe(started.pgid)
        .map_err(|why| format!("the group was already gone before the clock moved: {why}"))?;

    tokio::time::pause();
    // `from_mins(1)`, nie `from_secs(60)`: ta sama wartość, a `clippy::duration_suboptimal_units`
    // jest w pełnej bramce odmową (`--all-targets -- -D warnings`, czego `--lib` nie widzi).
    // Zmierzone 2026-08-20 — czerwień stała tu od commita kontraktowego.
    tokio::time::advance(GIVE_UP_AFTER + Duration::from_mins(1)).await;
    tokio::time::resume();

    group_probe(started.pgid).map_err(|why| {
        format!(
            "kill(-{}, 0) answers {why} after the clock jumped past the ceiling a CHECK step \
             lives under. Something ordered from the command line has no such ceiling: it ends \
             when the person ends it, or together with the window. A thirty-minute limit here is \
             a dev server that dies at lunch, and nothing on the screen says why",
            started.pgid
        )
    })?;

    // Sprzątanie jest częścią tego testu, nie uprzejmością: rzecz zostawiona żywa przechodzi pod
    // PID 1 i pracuje dalej, a następny bieg bramki zobaczyłby ją jako cudzy wyciek.
    let proofs = tokio::time::timeout(PATIENCE, processes.close())
        .await
        .map_err(|_| format!("closing did not come back within {PATIENCE:?}"))?;
    assert!(
        proofs
            .iter()
            .all(|one| matches!(one, GroupProof::Dead { .. })),
        "and it still has to be possible to end it after the clock moved: {proofs:?}"
    );
    Ok(())
}
