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
//! # Ile pętli naraz
//!
//! Tyle, ile ich jest, byle miały ROZŁĄCZNE ciała — od 2026-08-22, na prośbę właściciela.
//! Do tego dnia ta funkcja rozwijała dokładnie jedną, a graf z dwiema gałęziami, z których każda
//! ma własne sprawdzenie i własną poprawkę, był przez to niewyrażalny.
//!
//! Pętle rozłączne nie potrzebują żadnego nowego rozstrzygnięcia: każdy krok należy do najwyżej
//! jednej z nich, więc „która to runda" ma jedną odpowiedź, a strzałka MIĘDZY pętlami jest tym
//! samym, co strzałka z pętli na zewnątrz — wychodzi rundą ostatnią nadawcy i celuje w rundę
//! pierwszą odbiorcy. Zagnieżdżonych i przecinających się dalej nie umiemy i odmawia ich
//! walidator (`workflow::check::loops_that_cross`): dla kroku wspólnego dwóm pętlom nie wiadomo
//! ani ile razy ma się powtórzyć, ani która jego runda wychodzi na zewnątrz.
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

/// Jedna pętla pliku, policzona raz i oddana dalej.
///
/// 2026-08-22 — TA STRUKTURA JEST NOWA i istnieje po to, żeby planista nie liczył ciała pętli
/// drugi raz. Do tego dnia pętla była jedna, więc `commands::run` rozpoznawał ją po jednym
/// fakcie: „runda większa od zera". Przy dwóch pętlach ten fakt przestał wystarczać — runda 1
/// pętli frontowej i runda 1 pętli backendowej są dwiema różnymi rundami dwóch różnych pętli,
/// a werdykt jednej nie ma prawa pomijać rund drugiej.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loop {
    /// Pozycja kroku, z którego powrót WYCHODZI. To on orzeka.
    pub judge: usize,
    /// Pozycja kroku, do którego powrót wraca.
    pub entry: usize,
    /// Ile rund ma ta pętla. Ostatnia runda to `turns - 1`.
    pub turns: u8,
    /// Kroki, które ta pętla powtarza — oba końce powrotu należą do ciała.
    pub body: BTreeSet<usize>,
}

/// Graf po rozwinięciu: węzły, strzałki po ich numerach i pętle, które to rozwinięcie wypisało.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unrolled {
    pub nodes: Vec<Node>,
    pub arrows: Vec<(usize, usize)>,
    /// Pętle w kolejności powrotów z pliku. Pusty wektor znaczy „plik bez ani jednej pętli".
    pub loops: Vec<Loop>,
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

/// Pętle pliku, po jednej na powrót, z pominięciem tych, które przecinają wcześniejszą.
///
/// PĘTLE ROZŁĄCZNE, ILE ICH JEST. Do 2026-08-22 rozwinięcie brało pierwszy powrót z pliku
/// i tyle; graf z dwiema gałęziami, z których każda ma własne sprawdzenie, był przez to
/// niewyrażalny i właściciel musiał wybrać jedną gałąź.
///
/// GRANICA ZOSTAJE, tylko biegnie tam, gdzie naprawdę leży. Pętle o ROZŁĄCZNYCH ciałach
/// rozwijają się niezależnie: każdy krok należy do najwyżej jednej z nich, więc „która to runda"
/// ma jedną odpowiedź. Pętle zagnieżdżone albo przecinające się dalej nie mają rozstrzygnięcia —
/// dla kroku wspólnego dwóm pętlom nie wiadomo, ile razy ma się powtórzyć ani która runda
/// wychodzi na zewnątrz — i odmawia ich walidator (`check::loops_that_cross`).
///
/// Gdyby taki plik mimo wszystko tu dotarł, pierwszeństwo ma pętla WCZEŚNIEJSZA w pliku, a druga
/// zostaje pominięta. To nie jest domysł co do znaczenia, tylko wybór deterministyczny zamiast
/// paniki: rozwinięcie stoi na drodze KAŻDEGO biegu i nie ma prawa go wywrócić.
fn disjoint_loops(ways_back: &[(usize, usize, u8)], forward: &[(usize, usize)]) -> Vec<Loop> {
    ways_back
        .iter()
        .map(|&(judge, entry, turns)| Loop {
            judge,
            entry,
            turns,
            body: body_of(judge, entry, forward),
        })
        .fold(Vec::new(), |mut kept: Vec<Loop>, one| {
            if kept.iter().all(|other| other.body.is_disjoint(&one.body)) {
                kept.push(one);
            }
            kept
        })
}

/// Do której pętli należy ten krok. `None` dla kroku spoza wszystkich.
///
/// Pętle mają rozłączne ciała (`check::loops_that_cross`), więc odpowiedź jest jedna. Gdyby plik
/// z przecinającymi się pętlami mimo wszystko tu dotarł, wygrywa pętla WCZEŚNIEJSZA — wybór
/// deterministyczny zamiast paniki, bo ta funkcja stoi na drodze każdego biegu.
fn loop_of(step: usize, loops: &[Loop]) -> Option<usize> {
    loops.iter().position(|one| one.body.contains(&step))
}

/// Runda, z której krok WYCHODZI do czegokolwiek poza swoją pętlą.
///
/// Dla kroku w pętli to runda OSTATNIA: to jedyny węzeł, po którym wiadomo, że pętla się
/// skończyła — czy to werdyktem `pass`, czy wyczerpaniem tur. Krok za pętlą, który czekałby na
/// wszystkie rundy, czekałby także na te, których bieg nigdy nie potrzebował. Dla kroku spoza
/// wszystkich pętli to zero, bo ma dokładnie jedną rundę.
fn leaves_at(step: usize, loops: &[Loop]) -> u8 {
    loop_of(step, loops).map_or(0, |which| loops[which].turns.saturating_sub(1))
}

/// Ciało pętli domkniętej powrotem `judge → entry`: kroki, które ta pętla powtarza.
///
/// Krok należy do ciała, jeżeli da się do niego dojść w przód z `entry` I da się z niego dojść
/// do `judge`. Oba końce powrotu należą do ciała.
///
/// `pub`, bo tej definicji potrzebuje też walidator (`workflow::check::loops_that_cross`) —
/// odmawia pętli o wspólnym kroku, a „wspólny" znaczy dokładnie tyle, co tutaj. Dwie kopie tego
/// obchodu rozjechałyby się przy pierwszej poprawce, a rozjazd znaczyłby, że walidator wpuszcza
/// plik, którego rozwinięcie nie rozumie.
#[must_use]
pub fn body_of(judge: usize, entry: usize, forward: &[(usize, usize)]) -> BTreeSet<usize> {
    forward_from(entry, forward)
        .intersection(&back_from(judge, forward))
        .copied()
        .collect()
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
            loops: Vec::new(),
        };
    }

    let loops = disjoint_loops(&ways_back, &forward);

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
        if let Some(which) = loop_of(step, &loops) {
            for turn in 0..loops[which].turns {
                number.insert((step, turn), nodes.len());
                nodes.push(Node { step, turn });
            }
        } else {
            number.insert((step, 0), nodes.len());
            nodes.push(Node { step, turn: 0 });
        }
    }

    let mut arrows: Vec<(usize, usize)> = Vec::new();
    let mut push = |from: usize, to: usize| {
        if !arrows.contains(&(from, to)) {
            arrows.push((from, to));
        }
    };
    for &(from, to) in &forward {
        let same_loop = loop_of(from, &loops)
            .zip(loop_of(to, &loops))
            .is_some_and(|(one, other)| one == other);
        if same_loop {
            // Wewnątrz JEDNEJ pętli: ta sama strzałka w każdej jej rundzie.
            let turns = loop_of(from, &loops).map_or(1, |which| loops[which].turns);
            for turn in 0..turns {
                if let (Some(&a), Some(&b)) = (number.get(&(from, turn)), number.get(&(to, turn))) {
                    push(a, b);
                }
            }
            continue;
        }
        /* Wszystko inne jednym wyrażeniem, bo wszystko inne znaczy to samo dla KSZTAŁTU:
         * wychodzimy rundą, po której nadawca jest skończony, i celujemy w rundę PIERWSZĄ
         * odbiorcy, bo każda pętla zaczyna się raz. Cztery przypadki, które się tu zbiegają:
         * z pętli na zewnątrz, z zewnątrz do pętli, z pętli do INNEJ pętli i całkiem poza
         * pętlami. Rozpisane na osobne ramiona miałyby identyczne wnętrza, a `match_same_arms`
         * (pedantic, bramka biegnie `-D warnings`) tego nie przepuszcza. */
        if let (Some(&a), Some(&b)) = (
            number.get(&(from, leaves_at(from, &loops))),
            number.get(&(to, 0)),
        ) {
            push(a, b);
        }
    }
    // Powrót: sędzia rundy k prowadzi do wejścia rundy k+1. To jest cała pętla, wypisana wprost,
    // i każda z pętli pliku dostaje własny komplet tych strzałek.
    for one in &loops {
        for turn in 0..one.turns.saturating_sub(1) {
            if let (Some(&a), Some(&b)) = (
                number.get(&(one.judge, turn)),
                number.get(&(one.entry, turn + 1)),
            ) {
                push(a, b);
            }
        }
    }

    Unrolled {
        nodes,
        arrows,
        loops,
    }
}

/// Identyfikator kroku, niezależnie od rodzaju. Jedno miejsce z odpowiedzią: `Step::id`.
fn key_of(step: &super::Step) -> &str {
    step.id()
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

    /// Kształt z ekranu właściciela: jeden plan, dwie gałęzie, każda ze swoim sprawdzeniem
    /// i swoim powrotem. Ani jeden krok nie należy do obu pętli.
    fn with_two_loops() -> Result<WorkflowFile, serde_json::Error> {
        file(
            &[
                step("s_plan"),
                step("s_front"),
                step("s_design"),
                step("s_back"),
                step("s_checked"),
            ],
            &[
                arrow("s_plan", "s_front"),
                arrow("s_front", "s_design"),
                arrow("s_plan", "s_back"),
                arrow("s_back", "s_checked"),
                back("s_design", "s_front", 3),
                back("s_checked", "s_back", 2),
            ],
        )
    }

    #[test]
    fn two_loops_side_by_side_each_get_their_own_turns() -> Result<(), serde_json::Error> {
        let unrolled = unroll(&with_two_loops()?);

        assert_eq!(
            unrolled.loops.len(),
            2,
            "two ways back that share no step are two loops, and until 2026-08-22 this function              took the first one and silently dropped the second — a run that looked fine while              doing something else than the person drew"
        );
        assert_eq!(
            nodes_of(&unrolled, 1)
                .iter()
                .map(|one| one.1)
                .collect::<Vec<u8>>(),
            vec![0, 1, 2],
            "the front branch repeats three times, because its own way back says three"
        );
        assert_eq!(
            nodes_of(&unrolled, 3)
                .iter()
                .map(|one| one.1)
                .collect::<Vec<u8>>(),
            vec![0, 1],
            "and the backend branch repeats twice, because ITS way back says two. One shared              count here would mean one branch runs a round nobody asked for"
        );
        assert_eq!(
            nodes_of(&unrolled, 0).len(),
            1,
            "the step before both branches belongs to neither loop, so it is not copied at all.              Copying it would run the planning again for every round of either branch"
        );
        Ok(())
    }

    #[test]
    fn a_step_in_one_branch_never_waits_for_a_round_of_the_other() -> Result<(), serde_json::Error>
    {
        let unrolled = unroll(&with_two_loops()?);
        let front = nodes_of(&unrolled, 1);
        let back = nodes_of(&unrolled, 3);

        let crossing: Vec<(usize, usize)> = unrolled
            .arrows
            .iter()
            .copied()
            .filter(|&(from, to)| {
                let ends = |at: usize| front.iter().any(|one| one.0 == at);
                let others = |at: usize| back.iter().any(|one| one.0 == at);
                (ends(from) && others(to)) || (others(from) && ends(to))
            })
            .collect();

        assert!(
            crossing.is_empty(),
            "the two branches are independent: neither one waits for a round of the other. An              arrow across them would make the front branch sit idle until the backend one is              done, and the person drew no such arrow. Got: {crossing:?}"
        );
        Ok(())
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
