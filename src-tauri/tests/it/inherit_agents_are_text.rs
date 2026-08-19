//! AC-5 dla T-54: podagent gospodarza jedzie jako **tekst**; front-matter zostaje po jego
//! stronie granicy.
//!
//! **Słabą wersją tego kryterium jest `assert!(!wynik.contains("mcpServers"))` jako jedyna
//! asercja negatywna.** Przechodzi dla implementacji, która front-mattera **nie wycina**, tylko
//! **filtruje z niego znane pola**: `mcpServers` znika, a `tools`, `memory`, `model` i pierwsze
//! pole dołożone przez vendora jadą dalej. Czarna lista jest z definicji niekompletna i cicho
//! pęknie przy następnym wydaniu CLI. Przechodzi też dla implementacji, która zdejmuje sam
//! **wiersz** `mcpServers:` i zostawia jego wcięte dzieci — czyli dokładnie te dwie wartości,
//! które uruchamiają proces.
//!
//! Rozróżniają to trzy asercje naraz: pięć **osobnych** sprawdzeń, po jednym na pole, żeby
//! komunikat porażki nazywał to, które przeszło; sprawdzenie **wartości**, nie tylko nazw
//! kluczy, bo `args:` w cudzym pliku może stać pod inną nazwą klucza; oraz brak separatora
//! `---` w wyniku, bo tylko wycięcie **całego** bloku zdejmuje jednocześnie klucze, wartości
//! i kreskę. Filtr pól zostawia przynajmniej kreskę.
//!
//! DLACZEGO to jest granica maszynerii, a nie sprzątanie: `mcpServers` z tego pliku uruchamia
//! proces (`npx -y @playwright/mcp@0.0.75`) **poza grupą procesów Loadouta**. Taki proces nie
//! wchodzi ani do dowodu śmierci grupy (niezmiennik 6), ani do żadnego licznika kosztu —
//! a zmierzone 2026-08-19 osierocenia (14 w jednym biegu, 30 łącznie) i 38–41 tys. tokenów
//! spalonych poza rozliczeniem to dokładnie ten sam wypadek, tylko wywołany z innej strony.
//! `tools` i `permissionMode` przepisują politykę biegu z miejsca, którego nasze UI nie
//! pokazuje; `memory` wskazuje cudzy katalog pamięci; `model` cicho zmienia rachunek
//! (niezmiennik 9).
//!
//! JEDEN `#[test]`: zaślepka zwracająca pusty napis przechodzi wszystkie asercje negatywne —
//! rozbite na osobne zestawy dałyby w warstwie `before` obraz „w połowie zielony". Przypadek
//! pozytywny stoi więc pierwszy.

// `unwrap()` i `expect()` w teście: panika w teście JEST jego wynikiem, a `?` na tej samej
// linii zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh`
// biegnie `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use loadout_lib::inherit::scan;

const BODY_MARKER: &str = "BODY-ONLY-7d10";

/// Odwzorowanie `.claude/agents/e2e-author.md` gospodarza: pięć pól, które przenoszą
/// maszynerię, w tym `mcpServers` z zagnieżdżonym `command` i `args` [zmierzone 2026-08-19,
/// trzy pliki na trzynaście].
const AGENT_MD: &str = "\
---
name: e2e-author
description: Writes end-to-end specs for the checkout flow.
tools: Read, Write, Edit, Bash
model: opus
permissionMode: acceptEdits
memory: ../../.claude/memory/e2e
mcpServers:
  playwright:
    command: npx
    args: [\"-y\", \"@playwright/mcp@0.0.75\"]
---

# End-to-end author

BODY-ONLY-7d10 — every spec starts from the page it is about.

Ask for the shortest path that still proves the click reached the server.
";

/// Pięć nazw pól, z których każda ma własną asercję. Wypisane literalnie, nie wzięte ze stałej
/// implementacji: kryterium sprawdzające implementację jej własną tablicą przechodzi po każdej
/// zmianie tej tablicy, łącznie z literówką.
const MACHINERY_FIELDS: [&str; 5] = ["tools", "model", "permissionMode", "memory", "mcpServers"];

/// Plik **bez** front-mattera. Całe jego ciało ma wrócić nietknięte.
const NO_FRONT_MATTER: &str = "# Reviewer\n\nRead the diff twice before you write a word.\n";

/// `---` w pierwszej linii pliku, który **nigdy się nie domyka**, jest poziomą kreską, a nie
/// nagłówkiem. To jest lustro reguły `skills::ingest::parse_doc`, przepisane tu świadomie, bo
/// tamten parser jest prywatny.
const UNCLOSED_FRONT_MATTER: &str =
    "---\nname: half-written\n\n# Reviewer\n\nRead the diff twice before you write a word.\n";

#[test]
fn the_body_crosses_the_boundary_and_the_whole_front_matter_stays_behind() {
    let body = scan::agent_body(AGENT_MD);

    // (a) Ciało naprawdę przyjechało.
    assert!(
        body.contains(BODY_MARKER),
        "the agent's body did not come through: {body:?}"
    );

    // (f) Kontrola przeciw pustemu czytaniu: pusty wynik przechodzi każdą asercję negatywną
    // niżej, więc bez tych dwóch linii cały ten test świeciłby na zielono dla funkcji, która
    // nie zwraca nic.
    assert!(
        body.lines().count() >= 2,
        "the body is {} line(s) long, so the negative assertions below prove nothing",
        body.lines().count()
    );
    assert!(
        body.len() < AGENT_MD.len(),
        "the body is not shorter than the file, so nothing was taken off the front"
    );

    // (b) Pięć OSOBNYCH asercji, po jednej na pole: komunikat porażki ma nazwać to, które
    // przeszło. Jedna wspólna asercja na pięć pól mówi tylko, że coś jest nie tak.
    for field in MACHINERY_FIELDS {
        assert!(
            !body.contains(field),
            "`{field}` is in the text we would send to the model. Front matter is the boundary \
             of machinery: mcpServers starts a process outside Loadout's process group, tools \
             and permissionMode rewrite the run's policy from a place our UI never shows, \
             memory points at someone else's memory directory and model quietly changes the bill"
        );
    }

    // (c) WARTOŚCI, nie tylko nazwy kluczy: `args:` w cudzym pliku może stać pod inną nazwą
    // klucza, a to te dwa napisy uruchamiają proces.
    for value in ["npx", "@playwright/mcp"] {
        assert!(
            !body.contains(value),
            "`{value}` survived. That is the command line which starts a process outside our \
             process group — 14 orphans in one measured run, 30 across the experiments"
        );
    }

    // (d) Kreska. Tylko wycięcie CAŁEGO bloku zdejmuje jednocześnie klucze, wartości i
    // separator; filtr pól zostawia przynajmniej kreskę.
    assert!(
        !body.contains("---"),
        "the front-matter separator is still in the body, so this is a filtered header and not \
         a removed one: {body:?}"
    );

    // (e) Front-matter bez domknięcia NIE JEST front-matterem, a plik bez nagłówka jest samym
    // ciałem. Obie odpowiedzi są dosłownie całą treścią pliku.
    assert_eq!(
        scan::agent_body(NO_FRONT_MATTER),
        NO_FRONT_MATTER,
        "a file with no front matter lost part of its body"
    );
    assert_eq!(
        scan::agent_body(UNCLOSED_FRONT_MATTER),
        UNCLOSED_FRONT_MATTER,
        "a `---` on the first line of a file that never closes is a horizontal rule, not a \
         header — and cutting at it silently eats the first paragraph of the agent"
    );
}
