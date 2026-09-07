//! G-01: kontrakt szkicu agenta i walidacja dopasowania.
//!
//! # Co ten moduł rozstrzyga
//!
//! Model pisze **konfigurację**, nie wykonuje roli. Wszystko, co da się sprawdzić kodem —
//! tożsamość, dostępność narzędzia, zgoda, model z katalogu, przelotka vendora — jest
//! sprawdzane kodem, a nie proszone w prompcie (niezmiennik 28). Modelowi zostaje to,
//! czego stan plików nie zawiera: synteza roli i dobór instrukcji.
//!
//! # Czego szkic NIE MOŻE ustawić
//!
//! Tożsamości. `id`, `schema` i ścieżka zapisu powstają po tej stronie — model, który wybiera
//! istniejący identyfikator, nadpisuje cudzego agenta, a model, który wybiera wersję schematu,
//! decyduje o tym, jak czyta się wszystkie pliki biblioteki.
//!
//! # Jeden typ, nie drugi model danych
//!
//! Szkic jest **istniejącym** [`Agent`], nie surowym `settings.json` ani `config.toml`.
//! Drugi model danych dla generacji rozjechałby się z pierwszym w dniu, w którym ktoś dołoży
//! pole tylko do jednego z nich (niezmiennik 23) — a pola `serviceAccess` i `agentMessages`
//! istnieją właśnie od niedawna.

use std::collections::BTreeSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::agents::{Agent, Color, FileAccess, Thinking, Tools, Vendor};
use crate::workflow::check::{escalation_in, is_reserved, literal_secret_in};

/// Sufit odpowiedzi modelu. Szkic agenta jest kilkoma kilobajtami; wszystko powyżej znaczy,
/// że model odpowiedział czymś innym niż konfiguracją.
pub const DRAFT_LIMIT_BYTES: usize = 64 * 1024;

/// Sufit opisu roli od człowieka.
pub const DESCRIPTION_LIMIT_BYTES: usize = 8 * 1024;

/// Domyślny sufit czasu całej operacji, razem z jedną korektą formatu.
pub const DEFAULT_DEADLINE: Duration = Duration::from_mins(3);

/// Co wolno wygenerowanemu agentowi wskazać — migawka wzięta PRZED wysłaniem opisu.
///
/// Snapshot, nie zapytanie w trakcie: lista, która zmienia się między prośbą a walidacją,
/// daje szkic odrzucany za coś, co w chwili pisania istniało.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    /// Umiejętności widoczne w zatwierdzonym kontekście.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Połączenia zapisane w bibliotece.
    #[serde(default)]
    pub connections: Vec<String>,
    /// Usługi, o które ten agent może prosić.
    #[serde(default)]
    pub services: Vec<String>,
    /// Modele ze zweryfikowanego katalogu **albo** jawnego ustawienia człowieka.
    ///
    /// Pusta lista znaczy „nie wiadomo, jakie modele są dziś dostępne", i wtedy model
    /// wygenerowanego agenta przechodzi bez sprawdzenia — bo sprawdzenie wobec pustej wiedzy
    /// odrzucałoby każdą prawidłową odpowiedź.
    #[serde(default)]
    pub models: Vec<String>,
}

/// Żądanie wygenerowania jednego agenta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    /// Identyfikator operacji — po nim idzie anulowanie i po nim poznaje się spóźniony wynik.
    pub operation: Uuid,
    /// Opis roli, słowo w słowo od człowieka.
    pub described: String,
    /// Kto pisze konfigurację.
    pub generator: Vendor,
    /// Dla kogo ta konfiguracja powstaje.
    pub target: Vendor,
    /// Co wolno wskazać.
    pub available: Available,
    /// Sufit czasu całej operacji.
    pub deadline: Duration,
}

impl Wanted {
    /// Żądanie z jawną konwencją MVP: pisze ten vendor, dla którego to jest.
    ///
    /// Rozdział `generator`/`target` zostaje w modelu żądania, bo to są dwie różne rzeczy —
    /// ale w tym interfejsie oba są równe i nie ma selektora, którego nikt nie zamówił.
    #[must_use]
    pub fn from_one_vendor(described: String, vendor: Vendor, available: Available) -> Self {
        Self {
            operation: Uuid::now_v7(),
            described,
            generator: vendor,
            target: vendor,
            available,
            deadline: DEFAULT_DEADLINE,
        }
    }
}

/// Co model odpowiedział, zanim cokolwiek sprawdzimy.
///
/// `Serialize` jest tu po to, żeby prośba do modelu mogła pokazać KSZTAŁT tej odpowiedzi,
/// wypisując [`Answered::example`] — patrz powód przy tamtej funkcji.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Answered {
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub color: Option<Color>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub thinking: Option<Thinking>,
    #[serde(default)]
    pub file_access: Option<FileAccess>,
    #[serde(default)]
    pub give_up_after_minutes: Option<u32>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default)]
    pub reaches_the_web: Option<bool>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub connections: Vec<String>,
    #[serde(default)]
    pub service_access: Vec<super::agents::ServiceGrant>,
    #[serde(default)]
    pub agent_messages: Option<bool>,
    #[serde(default)]
    pub vendor_options: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    /// Czego model musiał się domyślić. To nie jest tok rozumowania, tylko lista rzeczy,
    /// które człowiek ma potwierdzić albo poprawić.
    #[serde(default)]
    pub assumptions: Vec<String>,
    /// Użytkowe uzasadnienia istotnych ustawień — „QA needs app access to test the real
    /// application", nie łańcuch myśli.
    #[serde(default)]
    pub because: Vec<String>,
}

impl Answered {
    /* KSZTAŁT ODPOWIEDZI POKAZUJEMY, A NIE OPISUJEMY SŁOWAMI.
     *
     * 2026-09-07, znalezione żywą próbą: prośba wymieniała same NAZWY kluczy, więc prawdziwy
     * model zgadywał budowę tych złożonych — `agentMessages` wracało obiektem zamiast wartością
     * tak/nie, a `because` mapą zamiast listą. Każda taka pomyłka wyrzucała cały szkic, razem
     * z instrukcjami, na których człowiekowi zależy najbardziej.
     *
     * Przykład jest WYPISANY Z TEGO SAMEGO TYPU, którym potem czytamy odpowiedź, więc nie ma
     * jak się z nim rozjechać (niezmiennik 13). Że naprawdę spełnia własny kontrakt, pilnuje
     * `the_example_shape_is_a_draft_this_reader_accepts`.
     */
    /// Wypełniony przykład o dokładnie tym kształcie, którego oczekuje [`read_draft`].
    #[must_use]
    pub fn example() -> Self {
        Self {
            name: "diff-reviewer".to_owned(),
            summary: "Reads a diff and names the risk it carries.".to_owned(),
            instructions: "You review changes and name the risk they carry.".to_owned(),
            color: Some(Color::Slate),
            model: Some("opus".to_owned()),
            thinking: Some(Thinking::Deep),
            file_access: Some(FileAccess::LookOnly),
            give_up_after_minutes: Some(20),
            tools: Some(vec!["Read".to_owned(), "Grep".to_owned()]),
            reaches_the_web: Some(false),
            skills: Vec::new(),
            connections: Vec::new(),
            service_access: Vec::new(),
            agent_messages: Some(false),
            vendor_options: std::collections::BTreeMap::new(),
            assumptions: vec!["The diff comes from the working tree.".to_owned()],
            because: vec![
                "fileAccess is read-only because the person asked for a reviewer that changes \
                 nothing."
                    .to_owned(),
            ],
        }
    }
}

/// Gotowy szkic razem z tym, czego nie udało się dopasować.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// Szkic **istniejącego** typu, gotowy do edycji w istniejącym formularzu.
    pub agent: Agent,
    /// Czego model musiał się domyślić.
    pub assumptions: Vec<String>,
    /// Krótkie uzasadnienia istotnych ustawień.
    pub because: Vec<String>,
    /// Możliwości, o które model poprosił, a których w zatwierdzonym kontekście nie ma.
    ///
    /// Nie usuwamy ich po cichu i nie instalujemy: brak zamienia się w czytelny problem,
    /// który człowiek widzi PRZED zapisem.
    pub missing: Vec<String>,
    /// Rzeczy odrzucone przy dopasowaniu — po jednym zdaniu, gotowym na ekran.
    pub refused: Vec<String>,
}

/// Dlaczego nie da się z tej odpowiedzi zrobić szkicu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotADraft {
    /// Odpowiedź jest dłuższa, niż konfiguracja agenta może być.
    TooLong,
    /// Odpowiedź nie jest tym kontraktem.
    NotTheContract {
        /// Zdanie dla człowieka, po angielsku.
        said: String,
    },
}

impl NotADraft {
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            Self::TooLong => format!(
                "The answer was longer than {DRAFT_LIMIT_BYTES} bytes, which is more than an \
                 agent's settings can be. Nothing was saved."
            ),
            Self::NotTheContract { said } => said.clone(),
        }
    }
}

/// Czyta odpowiedź modelu i dopasowuje ją do tego, co naprawdę istnieje.
///
/// Zwraca szkic **także wtedy**, gdy coś odrzucono: człowiek ma zobaczyć propozycję razem
/// z listą tego, czego w niej nie będzie, a nie pustą stronę z komunikatem.
pub fn read_draft(wanted: &Wanted, answered: &[u8]) -> Result<Draft, NotADraft> {
    if answered.len() > DRAFT_LIMIT_BYTES {
        return Err(NotADraft::TooLong);
    }
    let said: Answered = serde_json::from_slice(answered).map_err(|error| {
        NotADraft::NotTheContract {
            said: format!(
                "The answer was not an agent's settings in the shape this asks for ({}). Nothing \
                 was saved.",
                one_line(&error.to_string())
            ),
        }
    })?;
    if said.name.trim().is_empty() {
        return Err(NotADraft::NotTheContract {
            said: "The answer did not name the agent, so there is nothing to save it as."
                .to_owned(),
        });
    }

    let mut refused = Vec::new();
    let mut missing = Vec::new();

    let skills = kept(&said.skills, &wanted.available.skills, "skill", &mut missing);
    let connections = kept(
        &said.connections,
        &wanted.available.connections,
        "connection",
        &mut missing,
    );
    let service_access: Vec<_> = said
        .service_access
        .iter()
        .filter(|grant| {
            let known = wanted.available.services.contains(&grant.service);
            if !known {
                missing.push(format!(
                    "the service \"{}\", which this project does not have",
                    grant.service
                ));
            }
            known
        })
        .cloned()
        .collect();

    let model = model_from_the_catalogue(&said, wanted, &mut refused);
    let tools = tools_for(&said, wanted, &mut refused);
    let vendor_options = passthrough_that_holds(&said, wanted, &mut refused);

    /* TOŻSAMOŚĆ WYBIJA BACKEND. Model nie widzi tych pól w kontrakcie i nie ma jak ich podać:
     * `deny_unknown_fields` odrzuciłby próbę, a `Uuid::now_v7()` powstaje tutaj. */
    let agent = Agent {
        schema: Agent::example().schema,
        id: Uuid::now_v7(),
        name: said.name.trim().to_owned(),
        summary: said.summary.trim().to_owned(),
        color: said.color.unwrap_or(Color::Slate),
        instructions: said.instructions.trim().to_owned(),
        // Vendor bierze się z przycisku, nie z odpowiedzi: to człowiek wybrał, dla kogo
        // ten agent powstaje.
        runs_with: wanted.target,
        model,
        thinking: said.thinking.unwrap_or_default(),
        file_access: said.file_access.unwrap_or_default(),
        give_up_after_minutes: said
            .give_up_after_minutes
            .unwrap_or(Agent::example().give_up_after_minutes),
        tools,
        reaches_the_web: said.reaches_the_web.unwrap_or(true),
        skills,
        connections,
        service_access,
        agent_messages: said.agent_messages.unwrap_or(false),
        write_results_to: String::new(),
        vendor_options,
        extra: serde_json::Map::new(),
    };

    Ok(Draft {
        agent,
        assumptions: shortened(&said.assumptions),
        because: shortened(&said.because),
        missing,
        refused,
    })
}

/// Nazwy, które naprawdę istnieją w zatwierdzonym kontekście.
fn kept(asked: &[String], available: &[String], what: &str, missing: &mut Vec<String>) -> Vec<String> {
    let known: BTreeSet<&String> = available.iter().collect();
    asked
        .iter()
        .filter(|one| {
            if known.contains(one) {
                return true;
            }
            missing.push(format!("the {what} \"{one}\", which is not available here"));
            false
        })
        .cloned()
        .collect()
}

/// Model ze zweryfikowanego katalogu — albo domyślny vendora.
///
/// Nie odpalamy promptu, żeby zgadywał najnowsze nazwy, i nie ufamy starej statycznej liście
/// z formularza: jedno i drugie odpowiada na pytanie „co model pamięta", a nie „co dziś działa".
fn model_from_the_catalogue(said: &Answered, wanted: &Wanted, refused: &mut Vec<String>) -> String {
    let Some(asked) = said.model.as_ref().map(|one| one.trim()).filter(|one| !one.is_empty())
    else {
        return String::new();
    };
    if wanted.available.models.is_empty() || wanted.available.models.iter().any(|one| one == asked) {
        return asked.to_owned();
    }
    refused.push(format!(
        "The model \"{asked}\" is not one this computer offers, so the agent was left on the \
         default one. Pick a model in the form if you meant a different one."
    ));
    String::new()
}

/// Lista narzędzi — tylko tam, gdzie vendor w ogóle ma listę narzędzi.
///
/// Ustawienie niedostępne u vendora nie może udawać działającego: Codex nie ma listy narzędzi,
/// więc nazwy wpisane dla niego byłyby kontrolką bez skutku (niezmiennik 16).
fn tools_for(said: &Answered, wanted: &Wanted, refused: &mut Vec<String>) -> Tools {
    let Some(asked) = said.tools.as_ref() else {
        return Tools::Everything;
    };
    let named: Vec<String> = asked
        .iter()
        .map(|one| one.trim().to_owned())
        .filter(|one| !one.is_empty())
        .collect();
    if named.is_empty() {
        return Tools::Everything;
    }
    if wanted.target == Vendor::Codex {
        refused.push(
            "This agent's app does not take a list of tools, so the named ones were dropped. \
             What it may touch is set by what it can do with files and whether it reaches the web."
                .to_owned(),
        );
        return Tools::Everything;
    }
    Tools::Only(named)
}

/// Przelotka, która przeszła istniejącą walidację — i tylko ona.
///
/// Trzy powody odmowy, wszystkie z jednego rdzenia (niezmiennik 23): flaga, którą Loadout
/// ustawia sam; podniesienie uprawnień; sekret wpisany literalnie.
fn passthrough_that_holds(
    said: &Answered,
    wanted: &Wanted,
    refused: &mut Vec<String>,
) -> super::agents::VendorOptions {
    let mut kept = super::agents::VendorOptions::default();
    for (vendor, options) in &said.vendor_options {
        let for_this_agent = matches!(
            (vendor.as_str(), wanted.target),
            ("claude", Vendor::ClaudeCode) | ("codex", Vendor::Codex)
        );
        if !for_this_agent {
            refused.push(format!(
                "Raw {vendor} settings were left out: this agent does not run on that app."
            ));
            continue;
        }
        let mut room = std::collections::BTreeMap::new();
        for (flag, value) in options {
            if let Some(what) = literal_secret_in(flag, value) {
                refused.push(format!(
                    "The raw setting {flag} looks like it carries {what}. Loadout does not write \
                     that into a saved agent — hand it over as an environment variable instead."
                ));
                continue;
            }
            if let Some(raise) = escalation_in(flag, value) {
                refused.push(format!(
                    "The raw setting {flag} widens what this agent may do ({raise}). A generated \
                     agent does not grant itself more than the form does."
                ));
                continue;
            }
            if is_reserved(vendor, flag) {
                refused.push(format!(
                    "Loadout sets {flag} itself, so a generated agent cannot set it too."
                ));
                continue;
            }
            room.insert(flag.clone(), value.clone());
        }
        if !room.is_empty() {
            kept.insert(vendor.clone(), room);
        }
    }
    kept
}

/// Ile pozycji listy uzasadnień przeżywa i jak długa może być każda.
const MOST_LINES: usize = 12;
const LONGEST_LINE: usize = 400;

fn shortened(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .take(MOST_LINES)
        .map(|one| one_line(one))
        .filter(|one| !one.is_empty())
        .collect()
}

fn one_line(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.len() <= LONGEST_LINE {
        return flat;
    }
    let mut end = LONGEST_LINE;
    while !flat.is_char_boundary(end) {
        end -= 1;
    }
    flat[..end].to_owned()
}
