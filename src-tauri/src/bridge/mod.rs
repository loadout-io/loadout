//! Czasowniki Loadouta dla agenta — most między procesem agenta a aplikacją.
//!
//! # Po co to istnieje
//!
//! Zmierzone 2026-08-29 na `claude 2.1.251`, dokładnie flagami Loadouta (`-p`,
//! `--strict-mcp-config`, `--setting-sources ""`): w trybie bez terminala vendor **nie daje**
//! narzędzia `AskUserQuestion` — ani domyślnie (27 narzędzi w `system/init`), ani po wypisaniu
//! go w `--tools`. Model odpowiedział na to wprost: „I don't have an AskUserQuestion tool
//! available in this session".
//!
//! Skutek w tym drzewie: gałąź `"AskUserQuestion" => Action::Asked` w [`crate::engine::stream`]
//! **nigdy się nie odpala w produkcji**. Agent nie ma żadnej drogi, żeby zapytać człowieka,
//! zobaczyć jego bibliotekę albo uruchomić jego workflow.
//!
//! Ten moduł buduje tę drogę tą samą, którą człowiek podpina Figmę: serwerem narzędziowym.
//! Zmierzone tą samą sondą: `--tools` **nie rządzi** narzędziami z takiego serwera — wystarczy
//! `mcp__loadout` w `--allowedTools`, czyli szew, który już istnieje
//! ([`crate::engine::drivers::DriverConfiguration::servers`]). Tabela polityk zostaje nietknięta.
//!
//! # Czego tu nie ma i mieć nie może
//!
//! **Ani jednego warunku nazywającego etap biegu** (niezmiennik 27). Czasownik jest DOSTĘPNY,
//! nigdy WYMAGANY: żadne zdanie w tym drzewie nie każe agentowi po niego sięgnąć. To jest wprost
//! wymaganie właściciela z 2026-08-30 — „nie chcę też aby na sztywno było żeby agent lub
//! ktokolwiek zadawał 2-3 pytania, wszystko zależy od analiz i potrzeb" — i ta sama reguła, co
//! D7 („domyślnie: nic").

pub mod host;
pub mod library;
pub mod serve;
pub mod verbs;

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Cała praca procesu mostu, z własnym, jednowątkowym runtime.
///
/// # Dlaczego własny runtime, a nie `#[tokio::main]` na `main`
///
/// Bo aplikacja startuje swój przez Tauri i most nie ma prawa go dotknąć. Jednowątkowy, bo ten
/// proces robi dokładnie jedno: przepisuje linie między dwoma strumieniami. Pula wątków byłaby
/// pamięcią wydaną na nic w procesie, który towarzyszy każdemu agentowi.
///
/// Błąd wychodzi na `stderr` i kodem wyjścia, nigdy na `stdout`: tam płynie protokół, a jedna
/// nasza linia w nim to vendor, który przestaje rozumieć most. Zmierzona pułapka Codeksa: nigdy
/// `2>&1` na tym potoku.
pub fn run_bridge(socket: &Path) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("Loadout could not start its bridge: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = runtime.block_on(serve::serve(socket)) {
        eprintln!("Loadout's bridge stopped: {error}");
        std::process::exit(1);
    }
}

/// Pierwsza wiadomość gniazda: co ten most ma prawo oferować.
///
/// **Aplikacja odzywa się pierwsza**, i to jest własność bezpieczeństwa, nie kolejność dla
/// wygody. Most nie liczy listy sam, nie zna pojęcia roli i nie czyta biblioteki — więc nie ma
/// jak poszerzyć własnej powierzchni, nawet gdyby ktoś podmienił jego argv. Tabela ról zostaje
/// po stronie, która zna człowieka.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Greeting {
    /// Gotowa odpowiedź na `tools/list`, policzona przez [`verbs::tool_list`].
    pub tools: Value,
}

/// Wywołanie czasownika, w drodze od mostu do aplikacji.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    /// Identyfikator wiadomości vendora. Wraca w odpowiedzi, żeby dwie tury nie zamieniły się
    /// wynikami — a przy narzędziu, które blokuje turę, zamiana byłaby odpowiedzią na cudze
    /// pytanie.
    pub id: Value,
    /// Nazwa czasownika, znak w znak z [`verbs`].
    pub call: String,
    /// Argumenty od modelu. Pusty obiekt, kiedy narzędzie ich nie ma.
    #[serde(default)]
    pub input: Value,
}

impl Call {
    /// Wywołanie z wiadomości `tools/call`, albo `None`, gdy nie ma w niej nazwy narzędzia.
    ///
    /// Nazwa jedzie z `params.name` **bez prefiksu serwera**: vendor woła
    /// `mcp__loadout__list_workflows`, ale w `params.name` stoi już samo `list_workflows`.
    /// Zmierzone sondą 2026-08-29.
    #[must_use]
    pub fn from_tool_call(message: &Value) -> Option<Self> {
        let name = message.pointer("/params/name").and_then(Value::as_str)?;
        Some(Self {
            id: message.get("id").cloned()?,
            call: name.to_owned(),
            input: message
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        })
    }
}

/// Odpowiedź aplikacji na wywołanie.
///
/// **Odmowa jest wariantem wartości, nie błędem** (niezmiennik 7 w duchu): `Err` zmuszałby
/// każdego wołającego do rozpakowywania czegoś, co awarią nie jest — a „w tym zakresie już coś
/// biegnie" jest poprawną odpowiedzią, nie usterką.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Answer {
    /// Poszło. Treść jedzie do modelu jako wynik narzędzia.
    #[serde(rename = "ok")]
    Ok(Value),
    /// Nie poszło, i to jest **gotowe zdanie dla człowieka** — takie samo, jakie zobaczyłby on
    /// sam. Enum z drutu ani kod błędu nigdy nie trafia na ekran (niezmiennik 14).
    #[serde(rename = "error")]
    Refused(String),
}

/// Odpowiedź razem z identyfikatorem pytania, którego dotyczy — **jedna linia na gnieździe**.
///
/// # 2026-08-30 — TEN TYP POWSTAŁ Z WADY ZŁAPANEJ PRZEZ ŻYWEGO VENDORA
///
/// Aplikacja doklejała `id` do zserializowanego [`Answer`] w miejscu wywołania, a most czytał
/// linię wprost jako `Answer`. Serde reprezentuje ten enum **zewnętrznym tagiem**, czyli obiektem
/// o DOKŁADNIE jednym kluczu — więc doklejone `id` czyniło linię nieczytelną. Zmierzone na żywym
/// `claude 2.1.251`: serwer był `connected`, model wywołał czasownik, aplikacja go odebrała,
/// a do modelu wróciło „Loadout answered in a way this version could not read".
///
/// Żadne z ówczesnych kryteriów tego nie widziało, bo wszystkie czytały surowy JSON zamiast tego,
/// co czyta MOST. Kryterium jest teraz obrotem: aplikacja pisze, most czyta, wartość ma wrócić ta
/// sama.
///
/// `flatten` na enumie z zewnętrznym tagiem daje `{"id": …, "ok": …}` w jedną i w drugą stronę —
/// czyli dokładnie tę linię, którą obie strony już wysyłały, tylko teraz **umówioną w typie**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    /// Identyfikator wiadomości vendora, znak w znak taki, jak przyszedł w [`Call`].
    pub id: Value,
    /// Co się stało.
    #[serde(flatten)]
    pub answer: Answer,
}

/// Czyim głosem mówi ten most.
///
/// # Dlaczego rola, a nie pole w definicji agenta
///
/// Rozstrzygnięcie właściciela 2026-08-30. Wskazanie lidera **jest** zgodą człowieka, wyrażoną
/// tam, gdzie już mieszka (pasek pracy i Settings, T-163) — więc drugie pole obok byłoby drugą
/// odpowiedzią na to samo pytanie (niezmiennik 13). Wersja z przełącznikiem w formularzu została
/// przez niego odrzucona w rozwidleniu, a osobno wymagałaby siedemnastego klucza w zapisanym
/// agencie, którego broni `agents_wire_shape`.
///
/// Zysk jest przy tym większy niż oszczędność jednego pola: krok biegu, który startuje drugi
/// bieg, jest przy roli **niemożliwy strukturalnie**, a nie „domyślnie wyłączony".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Agent, którego człowiek wskazał na lidera rozmowy.
    Lead,
    /// Krok wewnątrz biegu.
    Step,
}
