//! AC-4 dla T-63: **jedna** tabela `FileAccess` → `Policy`, dla biegu i dla rozmowy.
//!
//! # Po co to istnieje
//!
//! To znalezisko drugiej opinii przy T-60, które tamto zadanie mogło tylko UDOKUMENTOWAĆ:
//! `commands::run::policy_of` było wtedy prywatne, a T-60 nie posiadało `run.rs`, więc lider
//! dostał **drugą, ręcznie napisaną** tabelę w `chat.rs`. Autor napisał to w komentarzu wprost —
//! „ZDANIE WYŻEJ JEST DZIŚ NIEPRAWDĄ" — i wskazał jedyne wyjście: jedno słowo w pliku spoza jego
//! bloku OWNS. To zadanie posiada oba pliki, więc zamyka to jednym ruchem.
//!
//! Dlaczego to jest wada, choć dziś nic nie jest zepsute: obie tabele **się zgadzają**, więc każdy
//! test porównujący wartości przechodzi. Oba dopasowania są też wyczerpujące po `FileAccess`, więc
//! czwarta pozycja dialu nie skompiluje się bez ruszenia obu. Rozjechać się może dokładnie jedna
//! rzecz — **przecelowanie istniejącego ramienia** w jednym z dwóch miejsc — i tego nie widzi dziś
//! ani jedno sprawdzenie w tym repo. Lider, któremu wolno pisać, choć człowiek ustawił „look only",
//! nie wygląda na awarię: wygląda na lidera, który zapisał plik.
//!
//! # Dlaczego ten jeden plik CZYTA źródło, choć niezmiennik 20 mówi inaczej
//!
//! Bo pytanie brzmi „ile jest źródeł prawdy", a nie „co ta funkcja zwraca". Dwie kopie tabeli,
//! które dziś oddają to samo, są nieodróżnialne dla każdej asercji o wartościach — i to jest cała
//! treść tego kryterium. Zachowanie sądzi pierwszy test w tym pliku (obie drogi na WSZYSTKICH
//! pozycjach dialu); drugi liczy miejsca, w których to odwzorowanie jest zapisane. Komentarze
//! zdejmujemy przed liczeniem, bo prozy o `FileAccess::LookOnly` w tym repo jest więcej niż kodu,
//! a sprawdzenie liczące zdania byłoby dokładnie tym selftestem, który przechodził **na
//! komentarzu** [niezmiennik 20, raport 06 §2].
//!
//! # Słaba wersja tego kryterium
//!
//! `assert_eq!(lead_policy(FileAccess::LookOnly), Policy::ReadOnly)` powtórzone trzy razy.
//! Przechodzi dla dwóch kopii tabeli, czyli dla dokładnie tego stanu, który to kryterium ma
//! skasować: asercja o wartościach udaje kryterium o źródle prawdy. Rozróżnia to drugi test.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `lead_comes_from_the_agent` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]

use std::error::Error;
use std::path::{Path, PathBuf};

use loadout_lib::commands::chat::Lead;
use loadout_lib::commands::run::policy_of;
use loadout_lib::engine::drivers::Policy;
use loadout_lib::library::agents::{Agent, FileAccess};

/// Katalog kodu produkcyjnego. Ścieżka z `CARGO_MANIFEST_DIR`, nie z katalogu roboczego procesu:
/// `cargo test` uruchamia binarium z katalogu paczki, ale to nie jest obietnica, na której warto
/// stawiać kryterium.
fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Każda pozycja dialu, wypisana.
///
/// Sama tablica niczego nie pilnuje — pilnuje tego [`on_the_dial`] niżej, bo jest **wyczerpującym**
/// dopasowaniem: czwarta pozycja nie skompiluje tego pliku, więc kryterium nie może po cichu
/// przestać sądzić jej istnienia.
const EVERY: [FileAccess; 3] = [
    FileAccess::LookOnly,
    FileAccess::AskFirst,
    FileAccess::WorkFreely,
];

/// Brzmienie pozycji dialu w formularzu agenta — i jednocześnie dowód, że [`EVERY`] jest pełne.
///
/// Odwzorowanie na napis, nie na [`Policy`]: druga kopia TAMTEJ tabeli, napisana w teście, który
/// jej pilnuje, byłaby tym samym błędem w miniaturze.
fn on_the_dial(access: FileAccess) -> &'static str {
    match access {
        FileAccess::LookOnly => "look only",
        FileAccess::AskFirst => "ask first",
        FileAccess::WorkFreely => "work freely",
    }
}

/// Definicja agenta z tą pozycją dialu i niczym więcej różnym.
fn definition(access: FileAccess) -> Agent {
    Agent {
        file_access: access,
        ..Agent::example()
    }
}

/// Źródło bez komentarzy liniowych.
///
/// Cięcie jest naiwne i takie ma zostać: `//` wewnątrz literału tekstowego obcięłoby za dużo. To ta
/// sama technika, którą stosuje `checks/quick-boundary.sh` (`sed 's://.*::'`) i `ipc_commands_registered`.
fn without_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for line in source.lines() {
        out.push_str(line.split_once("//").map_or(line, |(before, _)| before));
        out.push('\n');
    }
    out
}

/// Wszystkie pliki `.rs` pod tym katalogiem.
fn every_rust_file(dir: &Path, into: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            every_rust_file(&path, into)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            into.push(path);
        }
    }
    Ok(())
}

/// Czy ten kod zapisuje odwzorowanie dialu na politykę — czyli ma ramię `FileAccess::X => Policy::Y`.
///
/// Trzy tokeny w jednej linii kodu, nie sama nazwa typu: `file_access: FileAccess::WorkFreely`
/// w `Agent::example()` jest **użyciem** wartości, nie tabelą, i policzone jako tabela zamieniłoby
/// to kryterium w sprawdzenie, którego nie da się spełnić.
fn writes_the_mapping(code: &str) -> bool {
    without_comments(code).lines().any(|line| {
        line.contains("FileAccess::") && line.contains("=>") && line.contains("Policy::")
    })
}

#[test]
fn both_roads_read_the_same_dial() -> Result<(), Box<dyn Error>> {
    // ── (a) + (d) OBIE DROGI, WSZYSTKIE POZYCJE DIALU, LICZONE ──────────────────────────────
    assert!(
        EVERY.len() >= 3,
        "the dial has {} position(s) here. A control that walks fewer than three cannot tell 'both \
         roads agree' from 'both roads answer one value'",
        EVERY.len()
    );

    let mut walked = 0_usize;
    for access in EVERY {
        let label = on_the_dial(access);
        let lead = Lead {
            agent: definition(access),
        };

        assert_eq!(
            lead.policy(),
            policy_of(access),
            "a lead saved as '{label}' and a run step saved as '{label}' have to compose the policy \
             the same way. The conversation said {:?} and the run said {:?}",
            lead.policy(),
            policy_of(access)
        );
        walked += 1;
    }
    assert_eq!(
        walked,
        EVERY.len(),
        "the loop above has to visit every position of the dial; it visited {walked} of {}",
        EVERY.len()
    );

    // Kontrola przeciw pustemu przejściu: trzy pozycje muszą dać co najmniej dwie różne polityki.
    // Bez tego wszystko wyżej przechodzi dla tabeli, która oddaje jedną wartość na wszystko —
    // czyli dla dialu, który nic nie robi (niezmiennik 16).
    let answers: Vec<Policy> = EVERY.into_iter().map(policy_of).collect();
    assert!(
        answers.iter().any(|policy| *policy != answers[0]),
        "every position of the dial composed the same policy ({:?}), so the dial is a control \
         without an effect and the comparison above compares one value with itself",
        answers[0]
    );
    Ok(())
}

#[test]
fn the_mapping_is_written_in_exactly_one_place() -> Result<(), Box<dyn Error>> {
    // ── (b) + (c) JEDNO ŹRÓDŁO PRAWDY, NIE DWIE ZGODNE KOPIE ────────────────────────────────
    let root = source_root();
    let mut files = Vec::new();
    every_rust_file(&root, &mut files)?;
    assert!(
        files.len() > 10,
        "only {} Rust file(s) were found under {}, so this criterion is measuring an empty tree \
         instead of the code",
        files.len(),
        root.display()
    );

    let mut tables: Vec<String> = Vec::new();
    for path in &files {
        let code = std::fs::read_to_string(path)?;
        if writes_the_mapping(&code) {
            tables.push(
                path.strip_prefix(&root)
                    .unwrap_or(path)
                    .display()
                    .to_string(),
            );
        }
    }
    tables.sort();

    assert_eq!(
        tables.len(),
        1,
        "the dial-to-policy mapping is written in {} place(s): {tables:?}. Two exhaustive tables \
         that simply agree drift apart on the day one of them re-aims a single arm - and nothing in \
         this repo sees that, because every assertion about values passes for both copies. Changing \
         one arm has to change both roads at once, and that is only true when there is one table",
        tables.len()
    );

    // A drugie zdanie tego samego punktu, wprost o `chat.rs`: rozmowa ma politykę SKŁADAĆ tamtą
    // tabelą, nie dopasowywać dialu sama. Dowolne ramię po `FileAccess` w tym pliku jest tą drugą
    // kopią, choćby dziś odpowiadało tak samo.
    let chat = std::fs::read_to_string(root.join("commands").join("chat.rs"))?;
    let matched: Vec<String> = without_comments(&chat)
        .lines()
        .filter(|line| line.contains("FileAccess::"))
        .map(|line| line.trim().to_owned())
        .collect();
    assert!(
        matched.is_empty(),
        "commands/chat.rs still matches on FileAccess itself: {matched:?}. The lead has to take the \
         policy from the run's table (`commands::run::policy_of`) - a second match here is the copy \
         whose one re-aimed arm nobody would notice, and the person would see a lead that writes \
         files after they set it to look only"
    );
    Ok(())
}
