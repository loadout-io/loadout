//! Poprawka po przebiegu: o co pytamy agenta i jak czytamy jego odpowiedź.
//!
//! # Co ten moduł proponuje, a czego nie
//!
//! Proponuje **nowy tekst instrukcji agenta** i nic poza tym. Nie dotyka przypadków, nie
//! dotyka kolumn i nie dotyka komendy sprawdzającej — bo to są rzeczy, którymi się MIERZY,
//! a mechanizm poprawiający własną miarę zawsze dochodzi do stu procent. To jest ta sama
//! choroba, przed którą stoi niezmiennik 22, przeniesiona o warstwę wyżej: ewaluacja, która
//! wolno jej przepisać wyrocznię, mierzy sama siebie.
//!
//! # Dlaczego poprawka nie jest stosowana sama
//!
//! Bo instrukcja agenta jest tym, co ten agent robi w KAŻDYM biegu, także poza Labem. Pętla
//! „zaproponuj → zmierz → zostaw, jeśli lepiej" umiałaby to przepisywać w nocy i po tygodniu
//! nikt nie wiedziałby, dlaczego agent zachowuje się inaczej. Człowiek czyta cały nowy tekst
//! i klika Apply — ten sam wzorzec, co przy notatce `suggested` i przy kandydatce na przypadek.
//!
//! # Czego tu świadomie NIE MA: poprawki dla umiejętności
//!
//! Zmiana `SKILL.md` musi przejść tę samą drogę, co umiejętność wciągnięta z linku:
//! `place::emit` odbiera jej front-matter, a skaner z `skills::ingest` szuka pięciu klas
//! wstrzyknięcia. Napisanie tego pliku stąd byłoby **obejściem tamtego skanera** — a tekst
//! napisany przez model jest dokładnie tak samo nieufny jak wklejony z internetu. Poprawka
//! umiejętności ma więc jedną drogę i jest nią `author_skill`, w sekcji Skills; ekran Labu
//! mówi to jednym zdaniem zamiast rysować przycisk, który tamtędy nie idzie.

/// Nagłówek, po którym poznajemy nowy tekst instrukcji.
const INSTRUCTIONS: &str = "## Instructions";

/// Nagłówek uzasadnienia.
const WHY: &str = "## Why";

/// Co agent proponuje zmienić.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// Dlaczego — jedno albo kilka zdań, które człowiek czyta PRZED tekstem.
    pub because: String,
    /// Cały nowy tekst instrukcji. Całość, nie różnica.
    pub instructions: String,
}

/// O co prosimy agenta, kiedy człowiek chce poprawki.
///
/// # Trzy rzeczy w tym pytaniu i każda ma powód
///
/// **Pełny obecny tekst**, bo poprawka jest jego przepisaniem, a model, który go nie widzi,
/// napisze go od zera i skasuje wszystko, co człowiek tam kiedyś wpisał.
///
/// **Wyłącznie zdania z komórek, które nie przeszły** — nie cały przebieg. Sto komórek, z
/// których trzy są czerwone, to prompt, w którym trzy istotne zdania giną w dziewięćdziesięciu
/// siedmiu nieistotnych; a długość kosztuje w każdej turze.
///
/// **Zakaz ruszania przypadków**, powiedziany wprost. Bez niego najkrótszą drogą do zielonej
/// tabeli jest zmiana tego, co ta tabela mierzy.
#[must_use]
pub fn ask_for_a_fix(name: &str, instructions: &str, failures: &[String]) -> String {
    let seen = failures
        .iter()
        .map(|one| format!("- {one}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "An agent called {name} was given the same work under a few different settings, and \
these are the places where the work came back wrong:

{seen}

Here is everything that agent is told before it starts, word for word:

---
{instructions}
---

Rewrite that text so the work above comes back right, and change as little as you can. Do not \
propose changes to the work itself, to what counts as right, or to the command that judges it: \
the shortest way to a clean table is to move the thing being measured, and a set that measures \
its own answer is worth nothing.

Answer in this exact shape and write nothing else:

{WHY}
one or two sentences on what was going wrong and what your change does about it.

{INSTRUCTIONS}
the whole new text, ready to replace what is between the dashes above."
    )
}

/// Czyta odpowiedź agenta. `None`, kiedy nie ma w niej nowego tekstu.
///
/// **`None` zamiast pustej poprawki**, i to jest cała treść tego typu zwrotnego: karta
/// z pustym tekstem i przyciskiem Apply skasowałaby instrukcję agenta jednym kliknięciem,
/// a wyglądałaby dokładnie jak karta z poprawką.
///
/// Uzasadnienie jest wymagane z tego samego powodu, co przy kandydatce na przypadek: człowiek
/// ocenia poprawkę po tym, co ona ma naprawić. Nowy tekst bez zdania „dlaczego" to ściana
/// znaków, którą akceptuje się albo odrzuca bez czytania.
#[must_use]
pub fn read_fix(said: &str) -> Option<Fix> {
    let because = section(said, WHY)?;
    let instructions = section(said, INSTRUCTIONS)?;
    if because.is_empty() || instructions.is_empty() {
        return None;
    }
    Some(Fix {
        because,
        instructions,
    })
}

/// Treść sekcji o tym nagłówku, do następnego nagłówka albo do końca.
///
/// Dopasowanie po **całym wierszu po obcięciu spacji**, nie po podciągu: zdanie „see the
/// ## Instructions below" w środku prozy nie jest nagłówkiem, a parser szukający podciągu
/// zacząłby od niego sekcję, której nikt nie napisał.
fn section(said: &str, heading: &str) -> Option<String> {
    let mut lines = said.lines().skip_while(|line| line.trim() != heading);
    lines.next()?;
    let body: Vec<&str> = lines
        .take_while(|line| !line.trim_start().starts_with("## "))
        .collect();
    // Otaczające puste wiersze schodzą, a te w środku zostają: instrukcja agenta bywa
    // akapitami i sklejenie ich w jeden byłoby zmianą treści, o którą nikt nie prosił.
    Some(body.join("\n").trim().to_owned())
}
