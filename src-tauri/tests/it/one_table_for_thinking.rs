//! AC-3 dla T-91: **jedna** tabela `Thinking` → poziom wysiłku vendora, dla biegu i dla rozmowy.
//!
//! # Po co to istnieje
//!
//! `Thinking` ma cztery szczeble, pole w formularzu agenta i wiersz nadpisania w panelu kroku —
//! a do 2026-08-23 nie miało w drzewie ani jednego czytelnika poza importem. Wpięcie takiego pola
//! zaprasza do dwóch tabel: jedna w adapterze Claude'a („bo `--effort` to jego flaga"), druga
//! w adapterze Codeksa („bo u niego to klucz konfiguracji"). Obie odpowiadałyby dziś tak samo,
//! więc każda asercja o wartościach przechodziłaby dla obu — i to jest dokładnie ten kształt,
//! który T-63 wyciął przy dialu plików (`one_table_for_policy.rs`).
//!
//! Rozjechać się może dokładnie jedna rzecz: **przecelowanie istniejącego ramienia** w jednym
//! z dwóch miejsc. Człowiek, którego planer ma szczebel najwyższy, dostaje wtedy rozmowę myślącą
//! najwyżej i krok myślący średnio — a z zewnątrz to wygląda na model, który „tym razem się nie
//! postarał".
//!
//! # Dlaczego ten plik CZYTA źródło, choć niezmiennik 20 mówi inaczej
//!
//! Bo pytanie brzmi „ile jest źródeł prawdy", a nie „co ta funkcja zwraca". Zachowanie sądzi
//! pierwszy test (obie drogi na WSZYSTKICH czterech szczeblach plus kontrola przeciw tabeli,
//! która oddaje jedną wartość); drugi liczy miejsca, w których to odwzorowanie jest zapisane.
//! Komentarze zdejmujemy przed liczeniem, bo prozy o `Thinking` jest w tym repo więcej niż kodu,
//! a sprawdzenie liczące zdania byłoby tym selftestem, który przechodził **na komentarzu**
//! [niezmiennik 20, raport 06 §2].
//!
//! `import/` jest z liczenia wyjęty świadomie i to nie jest furtka: importer tłumaczy w DRUGĄ
//! stronę — `model_reasoning_effort` z cudzego pliku na nasz szczebel — więc jego tabela ma
//! inne wejście, inne wyjście i własny powód istnienia (`Some("xhigh" | "max")` scala dwie
//! wartości vendora w jeden nasz szczebel, czego tabela w drugą stronę zrobić nie może).
//!
//! # Słaba wersja tego kryterium
//!
//! `assert_eq!(effort_level(Thinking::Deep), "high")` powtórzone cztery razy. Przechodzi dla
//! dwóch zgodnych kopii tabeli, czyli dla dokładnie tego stanu, który to kryterium ma skasować.
//! Rozróżnia to drugi test.

// `expect()` w teście: panika w teście JEST jego wynikiem. Ten sam idiom i ten sam powód, co
// w `one_table_for_policy` i w pozostałych plikach tego celu.
#![allow(clippy::expect_used)]
// Wynik `Result` w teście, który nigdy nie oddaje `Err`: clippy nazywa to zbędnym opakowaniem,
// a `--all-targets` w pełnej bramce podnosi to do błędu (`quick` katalogu tests/ nie widzi).
#![allow(clippy::unnecessary_wraps)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};

use loadout_lib::commands::chat::Lead;
use loadout_lib::library::agents::{Agent, Thinking, effort_level};

/// Katalog kodu produkcyjnego. Ścieżka z `CARGO_MANIFEST_DIR`, nie z katalogu roboczego procesu.
fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Każdy szczebel, wypisany.
///
/// Sama tablica niczego nie pilnuje — pilnuje tego [`on_the_form`] niżej, bo jest **wyczerpującym**
/// dopasowaniem: piąty szczebel nie skompiluje tego pliku, więc kryterium nie może po cichu
/// przestać sądzić jego istnienia.
const EVERY: [Thinking; 4] = [
    Thinking::Quick,
    Thinking::Balanced,
    Thinking::Deep,
    Thinking::Deepest,
];

/// Brzmienie szczebla w formularzu agenta — i jednocześnie dowód, że [`EVERY`] jest pełne.
///
/// Odwzorowanie na napis z ekranu, nie na poziom vendora: druga kopia TAMTEJ tabeli, napisana
/// w teście, który jej pilnuje, byłaby tym samym błędem w miniaturze.
fn on_the_form(thinking: Thinking) -> &'static str {
    match thinking {
        Thinking::Quick => "quick",
        Thinking::Balanced => "balanced",
        Thinking::Deep => "deep",
        Thinking::Deepest => "deepest",
    }
}

/// Definicja agenta z tym szczeblem i niczym więcej różnym.
fn definition(thinking: Thinking) -> Agent {
    Agent {
        thinking,
        ..Agent::example()
    }
}

/// Źródło bez komentarzy liniowych.
///
/// Cięcie jest naiwne i takie ma zostać: `//` wewnątrz literału tekstowego obcięłoby za dużo. To ta
/// sama technika, którą stosuje `checks/quick-boundary.sh` (`sed 's://.*::'`) i `one_table_for_policy`.
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

/// Linie kodu, które zapisują odwzorowanie szczebla na poziom vendora — czyli ramiona
/// `Thinking::X => "…"`.
///
/// Trzy rzeczy w jednej linii kodu, nie sama nazwa typu: `thinking: Thinking::Balanced`
/// w `Agent::example()` jest **użyciem** wartości, nie tabelą, i policzone jako tabela
/// zamieniłoby to kryterium w sprawdzenie, którego nie da się spełnić.
fn mapping_lines(code: &str) -> Vec<String> {
    without_comments(code)
        .lines()
        .filter(|line| line.contains("Thinking::") && line.contains("=>") && line.contains('"'))
        .map(|line| line.trim().to_owned())
        .collect()
}

#[test]
fn both_roads_read_the_same_rung() -> Result<(), Box<dyn Error>> {
    assert!(
        EVERY.len() >= 4,
        "the form offers {} rung(s) here. A control that walks fewer than four cannot tell 'both \
         roads agree' from 'both roads answer one value'",
        EVERY.len()
    );

    let mut walked = 0_usize;
    for thinking in EVERY {
        let label = on_the_form(thinking);
        let lead = Lead {
            agent: definition(thinking),
        };

        assert_eq!(
            lead.effort(),
            effort_level(thinking),
            "a lead saved as '{label}' and a run step saved as '{label}' have to compose the \
             effort the same way. The conversation said {:?} and the run said {:?}",
            lead.effort(),
            effort_level(thinking)
        );
        walked += 1;
    }
    assert_eq!(
        walked,
        EVERY.len(),
        "the loop above has to visit every rung of the form; it visited {walked} of {}",
        EVERY.len()
    );

    // Kontrola przeciw pustemu przejściu: cztery szczeble muszą dać CZTERY różne poziomy. Bez
    // tego wszystko wyżej przechodzi dla tabeli oddającej jedną wartość na wszystko — czyli dla
    // kontrolki bez skutku (niezmiennik 16), przy której porównanie wyżej porównuje jedną wartość
    // samą ze sobą.
    let answers: BTreeSet<&'static str> = EVERY.into_iter().map(effort_level).collect();
    assert_eq!(
        answers.len(),
        EVERY.len(),
        "the four rungs composed only {} distinct vendor level(s): {answers:?}. Two rungs that \
         mean the same thing are a dial with fewer positions than the form shows",
        answers.len()
    );

    // I ani jeden poziom pusty: `--effort ""` jest u vendora argumentem o zerowej długości,
    // a `model_reasoning_effort=` kluczem bez wartości — jedno i drugie startuje i znaczy co
    // innego, niż wygląda.
    for thinking in EVERY {
        assert!(
            !effort_level(thinking).trim().is_empty(),
            "the rung '{}' composed an empty vendor level, which reads to the CLI as a flag with \
             no value rather than as a choice",
            on_the_form(thinking)
        );
    }
    Ok(())
}

#[test]
fn the_mapping_is_written_in_exactly_one_place() -> Result<(), Box<dyn Error>> {
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

    // Importer czyta cudzy plik i tłumaczy W DRUGĄ STRONĘ, więc jego tabela ma inne wejście
    // i inne wyjście. To jedyne wyłączenie i jest wypisane tutaj, a nie schowane w regule.
    let importer = root.join("import");

    let mut tables: Vec<String> = Vec::new();
    let mut arms = 0_usize;
    for path in &files {
        if path.starts_with(&importer) {
            continue;
        }
        let code = std::fs::read_to_string(path)?;
        let found = mapping_lines(&code);
        if !found.is_empty() {
            arms += found.len();
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
        "the rung-to-vendor-level mapping is written in {} place(s) outside import/: {tables:?}. \
         Two tables that simply agree drift apart on the day one of them re-aims a single arm - \
         and nothing in this repo sees that, because every assertion about values passes for both \
         copies. Changing one arm has to change both roads at once, and that is only true when \
         there is one table",
        tables.len()
    );

    // A druga połowa tego samego pytania: jedno MIEJSCE, nie jeden plik z dwiema tabelami
    // w środku. Cztery szczeble to cztery ramiona; osiem znaczy, że kopia stoi linijkę niżej.
    assert_eq!(
        arms,
        EVERY.len(),
        "the one file that carries the mapping writes {arms} arm(s) for {} rung(s). A second copy \
         living a few lines below the first is the same drift, only harder to see than a second \
         file",
        EVERY.len()
    );
    Ok(())
}
