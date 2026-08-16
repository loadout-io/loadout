//! AC-3 dla T-20: tabela statusów po odzyskaniu, i drugi start, który nie ma już nic do roboty.
//!
//! `docs/ARCHITECTURE.md` §5, wiersz „`running` + crash aplikacji": krok idzie do `failed`,
//! a `interrupted` jest **powodem**. `interrupted` jest przy tym statusem **biegu** — stoi
//! w szóstce z `CHECK`a przy tabeli `runs` i nie ma go w siódemce przy `steps`. Wpisanie go
//! w kolumnę statusu kroku to wiersz, którego `SQLite` nie przyjmie, w środku startu po awarii.
//!
//! Drugi przebieg jest tu drugą połową kryterium, nie ozdobą. Odzyskiwanie biegnie przy
//! **każdym** starcie, więc uruchomi się także na wierszach, które samo poprawiło godzinę
//! wcześniej. Gdyby przy trzecim starcie znowu wysłało `SIGTERM` do `pgid` z tamtych wierszy,
//! trafiłoby w numer, który dawno należy do kogoś innego [T7 §6.3, ryzyko 2].
//!
//! **Słaba wersja tego kryterium to sprawdzenie samego drugiego przebiegu** („plan jest pusty").
//! Spełnia je funkcja zwracająca pusty plan zawsze — czyli odzyskiwanie, które nie odzyskuje
//! niczego. Dlatego pierwszy przebieg stoi w tej samej funkcji testowej i musi dać niepuste
//! `reap`, `ask` i `step_status` z wypisanymi wprost identyfikatorami kroków. Puste i niepuste
//! stoją obok siebie, więc implementacja zwracająca zawsze to samo pada na jednym z nich.
//!
//! Kolejność `plan.reap` jest kolejnością wierszy i jest tu sprawdzana wprost. Kolejność
//! `run_status` i `step_status` **nie jest** częścią kontraktu — te dwie listy porównujemy po
//! posortowaniu, żeby implementacja grupująca wiersze mapą nie padała na czymś, czego żadne
//! kryterium nie obiecuje.

use loadout_lib::recovery::{self, Machine, RecoveryPlan, RecoveryRow};

/// Czas startu systemu — zgodny, więc strażnik z AC-1 przepuszcza sprzątanie.
const BOOT: &str = "1786900000";
/// Własna grupa Loadouta, z dala od `pgid`-ów z fikstury.
const OWN_PGID: i32 = 501;

/// Bieg zastany w stanie `running`.
const RUN_RUNNING: &str = "0199ab00-0000-7000-8000-000000000301";
/// Bieg zastany w stanie `paused`. `paused` jest stanem biegu i nigdy stanem kroku [T7 §9.3].
const RUN_PAUSED: &str = "0199ab00-0000-7000-8000-000000000302";

/// Grupa procesów kroku, który miał permit, ale nie zdążył wystartować.
const PGID_READY: i32 = 4401;
/// Grupa procesów pierwszego biegnącego kroku.
const PGID_RUNNING_ONE: i32 = 4402;
/// Grupa procesów kroku biegnącego w drugim biegu.
const PGID_RUNNING_TWO: i32 = 4403;

/// Wiersz fikstury: krok, jego bieg, jego status i `pgid`, który po nim został.
fn row(
    step_id: &str,
    run_id: &str,
    run_status: &str,
    step_status: &str,
    pgid: Option<i32>,
) -> RecoveryRow {
    RecoveryRow {
        step_id: step_id.to_owned(),
        run_id: run_id.to_owned(),
        run_status: run_status.to_owned(),
        step_status: step_status.to_owned(),
        run_boot_id: Some(BOOT.to_owned()),
        pid: pgid,
        pgid,
        session_id: Some(format!("0199ab00-0000-7000-8000-{:012x}", step_id.len())),
        attempt: 0,
    }
}

/// Osiem wierszy: dwa biegi i wszystkie siedem stanów kroku z `docs/ARCHITECTURE.md` §5.
///
/// Kroki `succeeded`, `failed` i `cancelled` niosą `pgid`, który po nich został w kolumnie.
/// Bez tych trzech liczb kryterium przepuściłoby filtr zbierający „każdy wiersz z `pgid`"
/// zamiast „każdy wiersz, który nie zdążył się skończyć".
fn rows() -> Vec<RecoveryRow> {
    vec![
        row("ready-1", RUN_RUNNING, "running", "ready", Some(PGID_READY)),
        row("pending-1", RUN_RUNNING, "running", "pending", None),
        row(
            "running-1",
            RUN_RUNNING,
            "running",
            "running",
            Some(PGID_RUNNING_ONE),
        ),
        row(
            "succeeded-1",
            RUN_RUNNING,
            "running",
            "succeeded",
            Some(4390),
        ),
        row("failed-1", RUN_RUNNING, "running", "failed", Some(4391)),
        row(
            "running-2",
            RUN_PAUSED,
            "paused",
            "running",
            Some(PGID_RUNNING_TWO),
        ),
        row("cancelled-2", RUN_PAUSED, "paused", "cancelled", Some(4392)),
        row("skipped-2", RUN_PAUSED, "paused", "skipped", None),
    ]
}

/// `plan.run_status` jako posortowane wiersze.
fn run_lines(plan: &RecoveryPlan) -> Vec<String> {
    let mut lines: Vec<String> = plan
        .run_status
        .iter()
        .map(|change| format!("{} -> {}", change.run_id, change.status))
        .collect();
    lines.sort();
    lines
}

/// `plan.step_status` jako posortowane wiersze — status i powód razem, bo cała pomyłka polega
/// na wpisaniu właściwego słowa w niewłaściwą kolumnę.
fn step_lines(plan: &RecoveryPlan) -> Vec<String> {
    let mut lines: Vec<String> = plan
        .step_status
        .iter()
        .map(|change| {
            format!(
                "{} -> {} / {}",
                change.step_id, change.status, change.reason
            )
        })
        .collect();
    lines.sort();
    lines
}

/// Kroki, o które plan pyta, posortowane.
fn asked_steps(plan: &RecoveryPlan) -> Vec<String> {
    let mut ids: Vec<String> = plan
        .ask
        .iter()
        .map(|question| question.step_id.clone())
        .collect();
    ids.sort();
    ids
}

/// Nakłada plan na wiersze: to robi z nimi `store::writer` między jednym startem a drugim.
///
/// Ręcznie, a nie naszym kodem zapisu — fikstura zbudowana tym samym kodem, który sprawdzamy,
/// definiowałaby kształt zamiast go sprawdzać.
fn settle(rows: &[RecoveryRow], plan: &RecoveryPlan) -> Vec<RecoveryRow> {
    rows.iter()
        .map(|row| {
            let mut next = row.clone();
            if let Some(change) = plan
                .run_status
                .iter()
                .find(|change| change.run_id == row.run_id)
            {
                next.run_status.clone_from(&change.status);
            }
            if let Some(change) = plan
                .step_status
                .iter()
                .find(|change| change.step_id == row.step_id)
            {
                next.step_status.clone_from(&change.status);
            }
            next
        })
        .collect()
}

#[test]
fn recovery_writes_one_status_table_and_the_next_start_finds_nothing_to_do() {
    let machine = Machine {
        boot_id: BOOT.to_owned(),
        own_pgid: OWN_PGID,
    };
    let first_rows = rows();

    // ── Pierwszy start po awarii ───────────────────────────────────────────────────────────
    let first = recovery::decide(&first_rows, &machine);

    assert_eq!(
        run_lines(&first),
        vec![
            format!("{RUN_RUNNING} -> interrupted"),
            format!("{RUN_PAUSED} -> interrupted"),
        ],
        "both the running run and the paused one were cut off mid-flight, so both go to \
         interrupted. paused is a status of the RUN and never of a step [T7 §9.3], which is why \
         a paused run is here at all"
    );

    assert_eq!(
        step_lines(&first),
        vec![
            "ready-1 -> failed / interrupted".to_owned(),
            "running-1 -> failed / interrupted".to_owned(),
            "running-2 -> failed / interrupted".to_owned(),
        ],
        "only the ready and running steps were interrupted, and each goes to failed with \
         interrupted as a SEPARATE reason (ARCHITECTURE §5). The pending, succeeded, failed, \
         cancelled and skipped steps are untouched — not one entry between them, because \
         rewriting a finished step's status is how a run silently re-describes itself"
    );

    assert_eq!(
        first.reap,
        vec![PGID_READY, PGID_RUNNING_ONE, PGID_RUNNING_TWO],
        "the three unfinished steps have orphans to clean up, in row order. The finished steps \
         carry leftover pgids too (4390, 4391, 4392) and none of them may be signalled"
    );

    assert_eq!(
        asked_steps(&first),
        vec![
            "ready-1".to_owned(),
            "running-1".to_owned(),
            "running-2".to_owned()
        ],
        "one question per interrupted step, and only for the interrupted ones"
    );

    // ── Zapis planu, czyli to, co dzieje się między jednym startem a drugim ────────────────
    let settled = settle(&first_rows, &first);

    // ── Drugi start: nie ma już nic do zrobienia ───────────────────────────────────────────
    let second = recovery::decide(&settled, &machine);

    assert!(
        second.reap.is_empty(),
        "the first start already reaped these groups and marked the steps failed. Signalling \
         them again means signalling a pgid that has since been recycled and now belongs to a \
         stranger — kern.maxproc is 16000 on macOS [T7 §6.3, V]. The second plan wants to reap \
         {:?}",
        second.reap
    );
    assert!(
        second.ask.is_empty(),
        "the human was already asked about these steps. Asking again on every restart is how a \
         crash turns into a queue of identical questions: {:?}",
        asked_steps(&second)
    );
    assert!(
        second.step_status.is_empty(),
        "nothing is left in ready or running, so there is no status to write: {:?}",
        step_lines(&second)
    );
    assert!(
        second.unreadable.is_empty(),
        "interrupted and failed are values THIS code just produced, and both stand in the CHECK \
         constraints in store::schema. A recovery that cannot read back what it wrote one start \
         earlier declares its own output unreadable forever: {:?}",
        second.unreadable
    );
}
