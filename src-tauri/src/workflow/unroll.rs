//! Rozwinięcie pętli na literalne rundy — czysta funkcja nad plikiem, przed planowaniem biegu.
//!
//! # Dlaczego rozwinięcie, a nie cykl w planiście
//!
//! Planista dostaje `Dag` (`commands::run`), a wraz z nim pulę miejsc, dowód śmierci grupy,
//! anulowanie i świeże kopie plików. To jest najgłębsza i najbardziej krucha warstwa tej
//! aplikacji. Rozwinięcie zostawia ją **nietkniętą**: graf, który do niej trafia, jest dalej bez
//! cykli, tylko ma więcej węzłów. Cała pętla mieszka w tym pliku i sądzi się bez uruchamiania
//! czegokolwiek. Projekt: `docs/superpowers/specs/2026-08-19-petla-z-limitem-tur-design.md` §4.
//!
//! # Co to znaczy „ciało pętli"
//!
//! Powrót `J → E` (`max_turns: n`) zamyka ciało: to są kroki, do których da się dojść **w przód**
//! z `E` i z których da się dojść do `J`. `E` i `J` należą do ciała. Węzły spoza ciała nie są
//! kopiowane ani razu — kopiowanie czegokolwiek poza ciałem uruchamiałoby cudzą pracę n razy
//! i było najdroższym możliwym błędem tej funkcji.
//!
//! # Gdzie wchodzą i wychodzą strzałki
//!
//! Strzałka Z ZEWNĄTRZ w ciało celuje w rundę **pierwszą**: pętla zaczyna się raz.
//! Strzałka Z ciała na zewnątrz wychodzi z rundy **ostatniej**: to jedyny węzeł, po którym
//! wiadomo, że pętla się skończyła — czy to werdyktem `pass`, czy wyczerpaniem tur. Rundy
//! pośrednie nie mają wyjścia na zewnątrz, bo krok za pętlą, który czeka na WSZYSTKIE rundy,
//! czekałby także na te, których bieg nigdy nie potrzebował.
//!
//! # Czego ta funkcja NIE robi
//!
//! Nie wie nic o werdyktach i o pomijaniu rund. Rozwinięcie jest kształtem grafu; „runda 3 nie
//! była potrzebna, bo runda 2 przeszła" jest faktem z biegu i mieszka po stronie planisty razem
//! z [`crate::memory::handoff::verdict_in`]. Ten podział jest celowy: kształt da się osądzić
//! na sucho, a bieg nie.

use std::collections::BTreeSet;

use super::WorkflowFile;

/// Jeden węzeł rozwiniętego grafu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    /// Pozycja kroku w pliku workflow. Rundy tego samego kroku mają tę samą wartość — i to jest
    /// warunek właściciela „nie ma być widać, że spawnujemy nowych agentów" zapisany w danych:
    /// nazwa, agent i wszystko inne bierze się z JEDNEGO kroku pliku.
    pub step: usize,
    /// Która runda pętli, licząc od zera. `0` dla kroku spoza jakiejkolwiek pętli.
    pub turn: u8,
}

/// Graf po rozwinięciu: węzły i strzałki po ich numerach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unrolled {
    pub nodes: Vec<Node>,
    pub arrows: Vec<(usize, usize)>,
}

/// Kroki po numerach, po których da się dojść w przód z `start`, wliczając `start`.
fn forward_from(start: usize, arrows: &[(usize, usize)]) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(at) = stack.pop() {
        if !seen.insert(at) {
            continue;
        }
        for &(from, to) in arrows {
            if from == at {
                stack.push(to);
            }
        }
    }
    seen
}

/// Kroki, z których da się dojść do `goal`, wliczając `goal`.
fn back_from(goal: usize, arrows: &[(usize, usize)]) -> BTreeSet<usize> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![goal];
    while let Some(at) = stack.pop() {
        if !seen.insert(at) {
            continue;
        }
        for &(from, to) in arrows {
            if to == at {
                stack.push(from);
            }
        }
    }
    seen
}

/// Rozwija każdą pętlę pliku na jej rundy.
///
/// Plik bez ani jednego powrotu wychodzi stąd **niezmieniony co do kształtu**: jeden węzeł na
/// krok, `turn: 0`, strzałki jak w pliku. To jest warunek, na którym stoi cała wstecznina —
/// dołożenie tej funkcji do planisty nie ma prawa zmienić ani jednego istniejącego biegu.
#[must_use]
pub fn unroll(file: &WorkflowFile) -> Unrolled {
    let at: std::collections::BTreeMap<&str, usize> = file
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| (key_of(step), index))
        .fold(
            std::collections::BTreeMap::new(),
            |mut acc, (key, index)| {
                // Przy powtórzonym identyfikatorze wygrywa PIERWSZY — ta sama reguła, co w `check`
                // i w `commands::run::arrows`, żeby strzałka nie celowała raz w jeden krok, raz
                // w drugi, zależnie od tego, kto ją liczy.
                acc.entry(key).or_insert(index);
                acc
            },
        );

    let ends = |from: &str, to: &str| Some((*at.get(from)?, *at.get(to)?));
    let forward: Vec<(usize, usize)> = file
        .links
        .iter()
        .filter(|link| !link.is_a_way_back())
        .filter_map(|link| ends(link.from.as_str(), link.to.as_str()))
        .collect();
    let ways_back: Vec<(usize, usize, u8)> = file
        .links
        .iter()
        .filter_map(|link| {
            let turns = link.max_turns?;
            let (from, to) = ends(link.from.as_str(), link.to.as_str())?;
            Some((from, to, turns))
        })
        .collect();

    if ways_back.is_empty() {
        return Unrolled {
            nodes: (0..file.steps.len())
                .map(|step| Node { step, turn: 0 })
                .collect(),
            arrows: forward,
        };
    }

    // JEDNA PĘTLA W TEJ WERSJI. Dwie pętle w jednym pliku wymagają rozstrzygnięcia, co znaczy ich
    // zagnieżdżenie albo przecięcie — a to jest pytanie do człowieka, nie domysł do zakodowania.
    // Walidator odmawia takiego pliku (`check::two_ways_back`), więc tutaj bierzemy pierwszy
    // i nie udajemy, że rozumiemy resztę.
    let (judge, entry, turns) = ways_back[0];
    let body: BTreeSet<usize> = forward_from(entry, &forward)
        .intersection(&back_from(judge, &forward))
        .copied()
        .collect();

    let mut nodes: Vec<Node> = Vec::new();
    // Numer węzła dla (krok, runda). Kroki spoza ciała mają jeden węzeł i rundę zero.
    let mut number = std::collections::BTreeMap::<(usize, u8), usize>::new();

    // KOLEJNOŚĆ WĘZŁÓW JEST KOLEJNOŚCIĄ Z PLIKU, a rundy tego samego kroku idą jedna za drugą.
    //
    // 2026-08-19 — TO JEST NAPRAWA, nie kosmetyka, i została znaleziona analizą kontraktu raportu.
    // Pierwsza wersja tej funkcji emitowała najpierw WSZYSTKIE kroki spoza ciała, a dopiero potem
    // rundy. Numer węzła jest jednak prefiksem nazwy pliku przekazania (`<NN>__<from>__<kind>.md`,
    // `memory::handoff`) i wierszem w `run.json`, więc `ship` — chronologicznie ostatni — dostawał
    // numer `01` i `ls handoffs/` pokazywał go DRUGIM, przed pracą, którą syntetyzuje.
    //
    // Łamało to trzy zapisane obietnice naraz: „kolejność wynikowa jest kolejnością nazw plików,
    // bo prefiks NN jest numerem kroku" (`memory::handoff`), „pozycja w pliku jest tą samą liczbą,
    // którą niesie prefiks nazwy pliku przekazania" (`commands::run`) i „rosnąco, czyli
    // w kolejności z pliku workflow" przy liczeniu indeksu przekazań dla kroku syntezy. Ostatnie
    // jest najgorsze: krok z trzema wejściami dostawałby je w innej kolejności, niż mówi graf,
    // i nikt by tego nie zauważył, bo prompt dalej wygląda poprawnie.
    for step in 0..file.steps.len() {
        if body.contains(&step) {
            for turn in 0..turns {
                number.insert((step, turn), nodes.len());
                nodes.push(Node { step, turn });
            }
        } else {
            number.insert((step, 0), nodes.len());
            nodes.push(Node { step, turn: 0 });
        }
    }

    let last = turns.saturating_sub(1);
    let mut arrows: Vec<(usize, usize)> = Vec::new();
    let mut push = |from: usize, to: usize| {
        if !arrows.contains(&(from, to)) {
            arrows.push((from, to));
        }
    };
    for &(from, to) in &forward {
        let inside_from = body.contains(&from);
        let inside_to = body.contains(&to);
        match (inside_from, inside_to) {
            // Wewnątrz ciała: ta sama strzałka w każdej rundzie.
            (true, true) => {
                for turn in 0..turns {
                    if let (Some(&a), Some(&b)) =
                        (number.get(&(from, turn)), number.get(&(to, turn)))
                    {
                        push(a, b);
                    }
                }
            }
            // Z ciała na zewnątrz: wychodzi RUNDA OSTATNIA. Krok za pętlą, który czekałby na
            // wszystkie rundy, czekałby także na te, których bieg nigdy nie potrzebował.
            (true, false) => {
                if let (Some(&a), Some(&b)) = (number.get(&(from, last)), number.get(&(to, 0))) {
                    push(a, b);
                }
            }
            // Z ZEWNĄTRZ DO CIAŁA i CAŁKIEM POZA CIAŁEM sklejone jednym ramieniem, bo mają
            // identyczne ciało — `clippy::match_same_arms` (pedantic, a bramka biegnie
            // `-D warnings`) nie przepuszcza dwóch ramion o tym samym wnętrzu. NIE znaczą tego
            // samego i to jest cała treść tego komentarza: pierwsze mówi „pętla zaczyna się RAZ,
            // więc celuj w rundę pierwszą", drugie „ten krok nie ma z pętlą nic wspólnego".
            // Zbiegają się dlatego, że runda pierwsza i runda kroku spoza ciała to obie zero.
            (false, true | false) => {
                if let (Some(&a), Some(&b)) = (number.get(&(from, 0)), number.get(&(to, 0))) {
                    push(a, b);
                }
            }
        }
    }
    // Powrót: sędzia rundy k prowadzi do wejścia rundy k+1. To jest cała pętla, wypisana wprost.
    for turn in 0..last {
        if let (Some(&a), Some(&b)) = (number.get(&(judge, turn)), number.get(&(entry, turn + 1))) {
            push(a, b);
        }
    }

    Unrolled { nodes, arrows }
}

/// Identyfikator kroku, niezależnie od rodzaju.
fn key_of(step: &super::Step) -> &str {
    match step {
        super::Step::Agent(one) => one.id.as_str(),
        super::Step::Checkpoint(one) => one.id.as_str(),
    }
}

#[cfg(test)]
mod tests {
    //! Kształt rozwiniętego grafu — sądzony w całości, bo połowa tej funkcji to KTÓRA runda.
    //!
    //! # Słabą wersją każdego kryterium niżej jest liczenie węzłów
    //!
    //! Liczba węzłów przechodzi dla implementacji, która kopiuje też kroki SPOZA ciała pętli
    //! (czyli uruchamia cudzą pracę n razy — najdroższy możliwy błąd tej funkcji), i dla tej,
    //! która wiąże krok za pętlą ze WSZYSTKIMI rundami (czyli każe mu czekać także na rundy,
    //! których bieg nigdy nie potrzebował). Dlatego każde kryterium sądzi konkretne strzałki,
    //! a nie ich liczbę.
    //!
    //! # Dlaczego pierwsze kryterium dotyczy pliku BEZ pętli
    //!
    //! Ta funkcja wchodzi na drogę KAŻDEGO biegu. Plik bez ani jednego powrotu musi wyjść z niej
    //! nietknięty co do kształtu — inaczej dołożenie pętli zmienia bieg każdemu, kto o niej nie
    //! słyszał. To jest kryterium wsteczniny i stoi pierwsze z rozmysłu.

    use serde_json::{Value, json};

    use super::{Node, unroll};
    use crate::workflow::WorkflowFile;

    fn step(id: &str) -> Value {
        json!({ "kind": "agent", "id": id, "name": id, "agent": "a", "instructions": "Do it." })
    }

    fn arrow(from: &str, to: &str) -> Value {
        json!({ "from": from, "to": to })
    }

    fn back(from: &str, to: &str, turns: u32) -> Value {
        json!({ "from": from, "to": to, "max_turns": turns })
    }

    /// `Result`, nie `expect`: pełne clippy biegnie `-D warnings` z `expect_used` i `panic`
    /// w restrykcjach, a testy w tym repo propagują `?` — tak samo robią zestawy w `tests/it/`.
    fn file(steps: &[Value], links: &[Value]) -> Result<WorkflowFile, serde_json::Error> {
        serde_json::from_value(json!({
            "format": 1,
            "id": "wf",
            "name": "Test",
            "steps": steps,
            "links": links
        }))
    }

    /// `plan → implement → tester → ship`, z powrotem `tester → implement`.
    fn with_a_loop(turns: u32) -> Result<WorkflowFile, serde_json::Error> {
        file(
            &[
                step("s_plan"),
                step("s_impl"),
                step("s_test"),
                step("s_ship"),
            ],
            &[
                arrow("s_plan", "s_impl"),
                arrow("s_impl", "s_test"),
                back("s_test", "s_impl", turns),
                arrow("s_test", "s_ship"),
            ],
        )
    }

    /// Numery węzłów danego kroku, w kolejności rund.
    fn nodes_of(unrolled: &super::Unrolled, step: usize) -> Vec<(usize, u8)> {
        unrolled
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.step == step)
            .map(|(number, node)| (number, node.turn))
            .collect()
    }

    #[test]
    fn a_file_with_no_way_back_comes_out_unchanged() -> Result<(), serde_json::Error> {
        let plain = file(&[step("s_a"), step("s_b")], &[arrow("s_a", "s_b")])?;

        let unrolled = unroll(&plain);

        assert_eq!(
            unrolled.nodes,
            vec![Node { step: 0, turn: 0 }, Node { step: 1, turn: 0 }],
            "this function is on the path of EVERY run, so a file without a loop has to come out \
             shaped exactly as it went in — otherwise adding loops changes the run for everybody \
             who never asked for one"
        );
        assert_eq!(unrolled.arrows, vec![(0, 1)]);
        Ok(())
    }

    #[test]
    fn the_body_of_the_loop_is_copied_once_per_turn() -> Result<(), serde_json::Error> {
        let unrolled = unroll(&with_a_loop(3)?);

        assert_eq!(
            nodes_of(&unrolled, 1).len(),
            3,
            "three turns means three attempts at the implementer"
        );
        assert_eq!(nodes_of(&unrolled, 2).len(), 3, "and three at the tester");
        assert_eq!(
            nodes_of(&unrolled, 1)
                .iter()
                .map(|&(_, turn)| turn)
                .collect::<Vec<_>>(),
            vec![0, 1, 2],
            "the turns are numbered from zero and in order, because the run report and the file \
             names are read by people"
        );
        Ok(())
    }

    #[test]
    fn nodes_come_out_in_the_order_of_the_file() -> Result<(), serde_json::Error> {
        /* NUMER WĘZŁA JEST PREFIKSEM NAZWY PLIKU PRZEKAZANIA (`<NN>__<from>__<kind>.md`) i wierszem
         * w `run.json`. Pierwsza wersja tej funkcji emitowała kroki spoza ciała przed rundami, więc
         * `ship` — chronologicznie ostatni — dostawał numer `01` i `ls handoffs/` pokazywał go
         * drugim. Najgorszy skutek nie był kosmetyczny: krok z kilkoma wejściami dostawał indeks
         * przekazań w innej kolejności, niż mówi graf, a prompt dalej wyglądał poprawnie.
         *
         * Kryterium sądzi CAŁĄ listę, nie samą pozycję `ship`: asercja na jednym węźle przechodzi
         * dla implementacji, która przestawia dwa inne. */
        let unrolled = unroll(&with_a_loop(3)?);

        assert_eq!(
            unrolled.nodes,
            vec![
                Node { step: 0, turn: 0 },
                Node { step: 1, turn: 0 },
                Node { step: 1, turn: 1 },
                Node { step: 1, turn: 2 },
                Node { step: 2, turn: 0 },
                Node { step: 2, turn: 1 },
                Node { step: 2, turn: 2 },
                Node { step: 3, turn: 0 },
            ],
            "steps in file order, and the turns of one step next to each other. The node number is \
             the prefix of the handoff file name, so this order IS what `ls handoffs/` shows and \
             what a merging step reads its inputs in."
        );
        Ok(())
    }

    #[test]
    fn steps_outside_the_loop_are_not_copied_at_all() -> Result<(), serde_json::Error> {
        let unrolled = unroll(&with_a_loop(3)?);

        assert_eq!(
            nodes_of(&unrolled, 0).len(),
            1,
            "`plan` runs before the loop and has nothing to do with it; copying it would run \
             somebody else's work three times, which is the most expensive mistake this function \
             can make"
        );
        assert_eq!(
            nodes_of(&unrolled, 3).len(),
            1,
            "and so would copying `ship`"
        );
        Ok(())
    }

    #[test]
    fn the_way_back_chains_turn_k_to_turn_k_plus_one() -> Result<(), serde_json::Error> {
        let unrolled = unroll(&with_a_loop(3)?);
        let judges = nodes_of(&unrolled, 2);
        let entries = nodes_of(&unrolled, 1);

        for turn in 0..2usize {
            assert!(
                unrolled
                    .arrows
                    .contains(&(judges[turn].0, entries[turn + 1].0)),
                "the tester of turn {turn} has to lead back into the implementer of the next \
                 turn — that chain IS the loop. Arrows: {:?}",
                unrolled.arrows
            );
        }
        let back_out_of_the_last: Vec<(usize, usize)> = unrolled
            .arrows
            .iter()
            .copied()
            .filter(|&(from, to)| from == judges[2].0 && entries.iter().any(|&(e, _)| e == to))
            .collect();

        assert!(
            back_out_of_the_last.is_empty(),
            "the last turn must not lead back anywhere: a way back out of the last turn is a run \
             that never ends, which is the one thing the limit exists to prevent. Found: \
             {back_out_of_the_last:?}"
        );
        Ok(())
    }

    #[test]
    fn the_step_after_the_loop_waits_for_the_last_turn_only() -> Result<(), serde_json::Error> {
        let unrolled = unroll(&with_a_loop(3)?);
        let ship = nodes_of(&unrolled, 3)[0].0;
        let judges = nodes_of(&unrolled, 2);

        let into_ship: Vec<usize> = unrolled
            .arrows
            .iter()
            .filter(|&&(_, to)| to == ship)
            .map(|&(from, _)| from)
            .collect();

        assert_eq!(
            into_ship,
            vec![judges[2].0],
            "exactly one arrow, out of the LAST turn. Wiring `ship` to every turn makes it wait \
             for turns the run never needed; wiring it to the first makes it start while the loop \
             is still going. Arrows: {:?}",
            unrolled.arrows
        );
        Ok(())
    }

    #[test]
    fn the_step_before_the_loop_leads_into_the_first_turn_only() -> Result<(), serde_json::Error> {
        let unrolled = unroll(&with_a_loop(3)?);
        let plan = nodes_of(&unrolled, 0)[0].0;
        let entries = nodes_of(&unrolled, 1);

        let out_of_plan: Vec<usize> = unrolled
            .arrows
            .iter()
            .filter(|&&(from, _)| from == plan)
            .map(|&(_, to)| to)
            .collect();

        assert_eq!(
            out_of_plan,
            vec![entries[0].0],
            "the loop starts once. An arrow into every turn would start all three attempts at the \
             same moment, in the same folder, which is the collision the folder rule exists for."
        );
        Ok(())
    }
}
