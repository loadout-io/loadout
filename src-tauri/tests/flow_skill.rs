//! TWORZENIE SKILLA, całą drogą: link → przegląd → instalacja → usunięcie.
//!
//! # Po co to istnieje
//!
//! Druga połowa flow, o które poprosił właściciel 2026-08-18: „przejść całe flow od tworzenia
//! skilla/agenta/workflow". Agenta i workflow przechodzi `flow_todo_app.rs`; skill przechodzi ten
//! plik. Do dziś ta droga nie miała ani jednego kryterium sądzącego ją end-to-end: `review_skill`
//! i `install_skill` były sprawdzane osobno, na fiksturach z dysku, a **usunięcia nie było wcale**
//! — ani komendy, ani kontrolki. Dodawanie bez zabierania nie jest połową mechanizmu, jest
//! pułapką: sekcja Skills pisze do `~/.claude/skills` i `~/.agents/skills`
//! (`skills::DESTINATION_DIRS`), czyli do konfiguracji NARZĘDZI człowieka, a nie do jego projektu.
//! Jedno błędne kliknięcie „Add" wchodziło do każdej następnej sesji Claude Code i nie miało drogi
//! powrotnej.
//!
//! # Dlaczego `#[ignore]`, i dlaczego katalogi człowieka są bezpieczne
//!
//! Sięga do **sieci** (`raw.githubusercontent.com`), więc w bramce byłby kryterium, które pada,
//! kiedy pada `Wi-Fi` — a `harness/gate.py` słusznie nie uznaje takiej czerwieni za czerwień kodu.
//!
//! `HOME` NIE jest tu podstawiany i nie musi być — sprawdzone w kodzie, nie założone.
//! `commands::skills::global_roots` liczy katalog domowy jako `library.parent()`, a nie z `$HOME`,
//! i jego własny komentarz podaje powód: „drugi odczyt `HOME` tutaj znaczyłby też, że każdy test
//! pisze do prawdziwych katalogów vendorów". Biblioteka tego testu leży więc w katalogu
//! tymczasowym, a wszystko, co instalacja napisze, ląduje obok niej — **ani jeden bajt nie idzie
//! do `~/.claude/skills` człowieka, który to uruchomił**.
//!
//! # Skąd ten konkretny link
//!
//! Prawdziwy, publiczny skill Anthropica, znaleziony przez API `GitHuba` i sprawdzony przed
//! napisaniem tego pliku (`HTTP 200`, poprawny front-matter, 73 linie). Adres zmyślony albo
//! wskazujący na plik, którego nie ma, dałby kryterium padające na 404 — czyli mierzące sieć,
//! a nie import.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::commands::skills::{
    Landing, install_skill_into, list_skills_in, review_skill_inner,
};
use loadout_lib::skills::place::remove;
use loadout_lib::skills::{DESTINATION_DIRS, Roots, Scope};

/// Prawdziwy skill Anthropica. Link **wprost do `SKILL.md`**, bo tylko taki kształt
/// `ingest::resolve_url` bierze z `raw.githubusercontent.com`.
const LINK: &str =
    "https://raw.githubusercontent.com/anthropics/skills/HEAD/skills/brand-guidelines/SKILL.md";

/// Nazwa, którą ten skill nadaje sobie sam we front-matterze. Czytamy ją z przeglądu, a nie
/// przepisujemy — ta stała jest tylko po to, żeby zdanie asercji mogło ją nazwać.
const NAME: &str = "brand-guidelines";

#[test]
#[ignore = "siega do sieci; wolaj z --ignored"]
fn a_skill_can_be_added_and_taken_away() -> Result<(), Box<dyn Error>> {
    let bench = Bench::new()?;

    // ── (a) PRZEGLĄD: link → plik w bibliotece, zanim ktokolwiek go zatwierdzi ───────────────
    //
    // To jest cała teza tego ekranu: skilla z sieci NAJPIERW się czyta, a dopiero potem wpuszcza
    // do narzędzi. Przegląd, który od razu instaluje, jest przyciskiem „Add" udającym „Show me
    // the file first".
    let review = review_skill_inner(&bench.library, LINK)?;
    assert_eq!(
        review.name, NAME,
        "the review has to read the skill's own name out of its front matter; it said {:?}",
        review.name
    );
    assert!(
        !review.summary.trim().is_empty(),
        "a review with no description is a review of nothing: `description` is the ONE field a \
         model reads to decide whether to reach for this skill, so an empty one means the person \
         would be approving something that can never fire"
    );
    let placed = installed_dirs(&bench.home);
    assert!(
        placed.is_empty(),
        "reviewing must NOT install. The skill is already in {placed:?}, so the button that says \
         \"Show me the file first\" would be lying about what it did."
    );

    // ── (b) INSTALACJA: skill trafia do KAŻDEGO katalogu narzędzi ───────────────────────────
    let wrote = install_skill_into(&bench.library, NAME, Landing::Everywhere, None)?;
    assert!(
        !wrote.is_empty(),
        "installing has to write something; it reported no files at all"
    );

    let placed = installed_dirs(&bench.home);
    assert_eq!(
        placed.len(),
        DESTINATION_DIRS.len(),
        "the skill has to land in every tool directory Loadout knows ({DESTINATION_DIRS:?}), \
         otherwise \"Ready for Claude and Codex\" is true in one place and false in another. \
         It landed in {placed:?}"
    );

    // ── (c) LISTA WIDZI TO, CO LEŻY NA DYSKU ────────────────────────────────────────────────
    let listed = list_skills_in(&bench.library, None)?;
    assert!(
        listed.iter().any(|one| one.name == NAME),
        "the section reads its list from disk, so a skill that was just installed has to be in \
         it. It listed: {:?}",
        listed.iter().map(|one| &one.name).collect::<Vec<_>>()
    );

    // ── (d) USUNIĘCIE ZABIERA GO ZE WSZYSTKICH KATALOGÓW ────────────────────────────────────
    //
    // Najważniejsza asercja tego pliku, bo tej drogi do 2026-08-18 NIE BYŁO. Skill zostawiony
    // w połowie — zdjęty z jednego katalogu, obecny w drugim — jest gorszy niż nieusunięty:
    // sekcja pokazuje go jako nieobecnego, a Claude Code dalej go czyta.
    /* Te same korzenie, które liczy `commands::skills::global_roots`: dom to RODZIC biblioteki,
     * projekt jest `None` (okno nie przysyła zakresu), a dane to sama biblioteka. Policzone tu
     * tak samo, bo `remove` jest warstwą niżej i nie ma skorupy komendy, która by je podała. */
    let roots = Roots {
        home: bench.home.clone(),
        project: None,
        data: bench.library.clone(),
    };
    let taken = remove(NAME, Scope::Global, &roots)?;
    let left = installed_dirs(&bench.home);
    assert!(
        left.is_empty(),
        "taking a skill away has to take it out of EVERY tool directory; it is still in {left:?} \
         (the remover reported {taken:?})"
    );
    Ok(())
}

/// Katalogi narzędzi, w których ten skill NAPRAWDĘ leży.
///
/// Pytamy dysk, nie odpowiedź komendy: komenda mówi, co zamierzała, a to pyta, co się stało.
fn installed_dirs(home: &Path) -> Vec<String> {
    DESTINATION_DIRS
        .iter()
        .filter(|dir| home.join(dir).join(NAME).exists())
        .map(|dir| (*dir).to_owned())
        .collect()
}

/// Biblioteka Loadouta i udawany katalog domowy na czas jednego kryterium.
///
/// `home` jest RODZICEM `library`, bo dokładnie tak liczy go produkcja
/// (`commands::skills::global_roots`) — gdyby ten test ułożył je inaczej, mierzyłby inny układ
/// katalogów niż ten, który powstaje po kliknięciu „Add".
struct Bench {
    home: PathBuf,
    library: PathBuf,
}

impl Bench {
    fn new() -> Result<Self, Box<dyn Error>> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let home = std::env::temp_dir().join(format!("loadout-skill-{stamp}"));
        let library = home.join(".loadout");
        fs::create_dir_all(&library)?;
        Ok(Self { home, library })
    }
}
