//! JEDYNA tabela czasowników (niezmiennik 23).
//!
//! Czyta ją odpowiedź `tools/list` **i** rozdzielnik wywołań. Druga kopia — choćby dziś
//! identyczna — rozjeżdża się w dniu, w którym ktoś dopisze czasownik do jednej z nich. Skutek
//! rozjazdu nie wygląda jak błąd: albo model widzi narzędzie, którego nikt nie obsługuje (i
//! obiecuje człowiekowi coś, czego nie zrobi), albo aplikacja obsługuje czasownik, o którym
//! model nie wie.

use serde_json::{Value, json};

use super::Role;

/// Jeden czasownik: to, co jedzie do modelu, i nic poza tym.
#[derive(Debug, Clone)]
pub struct Verb {
    /// Nazwa, po której model go woła.
    ///
    /// Część kontraktu z modelem, nie szczegół: zmiana nazwy jest zmianą zachowania i sądzi ją
    /// kryterium, bo model nauczony jednej nazwy nie trafi w drugą.
    pub name: &'static str,
    /// Zdanie dla modelu — po co ten czasownik istnieje i kiedy po niego sięgnąć.
    ///
    /// Czasownik bez opisu jest czasownikiem, po który model nie sięgnie. To nie jest tekst
    /// widoczny dla użytkownika, ale jest po angielsku z tego samego powodu, co reszta drutu.
    pub describe: &'static str,
    /// Schemat wejścia, w kształcie, którego chce MCP.
    pub schema: Value,
}

/// Czasowniki tej roli.
///
/// # Na tej liście stoi WYŁĄCZNIE to, na co aplikacja umie odpowiedzieć
///
/// Czasownik wpisany tutaj przed swoją drogą byłby narzędziem, które model widzi, obiecuje
/// człowiekowi i za każdym razem oddaje błąd — niezmiennik 16 w najgorszym możliwym miejscu,
/// bo obietnicę składa wtedy nie przycisk, tylko zdanie agenta.
///
/// Kolejność jest treścią, nie gustem. `ask_the_person` stoi pierwszy, bo to jest ruch, który
/// model ma rozważyć **zanim zgadnie**; `list_workflows` przed `start_workflow`, bo nazwa dla
/// startu pochodzi właśnie stamtąd.
#[must_use]
pub fn for_role(role: Role) -> Vec<Verb> {
    match role {
        /* KROK BIEGU NIE DOSTAJE NIC, i to jest zdanie o bezpieczeństwie, nie o zakresie.
         * Krok, który umie wystartować bieg, startuje go w środku cudzej pracy — a silnik
         * prowadzi jeden bieg na zakres, więc drugi start jest w najlepszym razie odmową,
         * a w najgorszym cudzą pracą wyrzuconą do kosza. Pusty wektor znaczy przy tym, że
         * `tools/list` nie wymieni ani jednej nazwy: model nie dowie się, że taki czasownik
         * w ogóle istnieje, więc nie obieca człowiekowi, że go użyje. */
        Role::Step => Vec::new(),
        Role::Lead => vec![
            Verb {
                name: "ask_the_person",
                describe: "Ask this person a question and wait for their answer. Use it when you \
                           genuinely do not know something only they can decide — not as a habit, \
                           and not to confirm what they already told you. Their answer comes back \
                           to you here.",
                schema: json!({
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The question, in their language, in one sentence.",
                        },
                        "options": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Answers they can pick with one click. Leave it out \
                                            when the answer is not a choice; they can always \
                                            type their own words instead.",
                        },
                    },
                    "required": ["question"],
                }),
            },
            Verb {
                name: "list_workflows",
                describe: "List the workflows this person has built, each with the name to use \
                           when starting it. Look here before starting anything, so you start \
                           something they actually have.",
                schema: json!({ "type": "object", "properties": {} }),
            },
            Verb {
                name: "list_agents",
                describe: "List the agents this person has saved, each with what it is for.",
                schema: json!({ "type": "object", "properties": {} }),
            },
            Verb {
                name: "start_workflow",
                describe: "Start one of this person's workflows. Use the name exactly as \
                           list_workflows gave it. The run appears in the stream they are \
                           watching; if it cannot start, the reason appears there too.",
                schema: json!({
                    "type": "object",
                    "properties": {
                        "workflow": {
                            "type": "string",
                            "description": "The name, exactly as list_workflows gave it.",
                        },
                        "task": {
                            "type": "string",
                            "description": "What this run should build, in your own words. \
                                            Leave it out and every step does what it already says.",
                        },
                    },
                    /* SAMA NAZWA JEST WYMAGANA. Zadanie wymagane zmuszałoby lidera do wymyślania
                     * pracy workflow, którego kroki już mówią, co robią — a wymyślone zdanie
                     * jedzie wtedy do sześciu agentów jako polecenie. */
                    "required": ["workflow"],
                }),
            },
        ],
    }
}

/// Definicje narzędzi tej roli — **tablica**, w kształcie, którego chce MCP.
///
/// # Dlaczego tablica, a nie gotowa odpowiedź `{"tools": […]}`
///
/// Zmierzone 2026-08-30 na żywym `claude 2.1.251`: most oddający tu gotową odpowiedź zawijał ją
/// drugi raz w warstwie protokołu i wysyłał gołą tablicę jako `result`. Serwer został wtedy
/// w stanie `pending`, a lider napisał człowiekowi „I don't have a loadout tool available".
/// Kryterium tego nie widziało, bo porównywało odpowiedź z tą samą wartością, którą samo podało
/// na wejściu — zgadzało się samo ze sobą (niezmiennik 20).
///
/// Opakowanie należy więc do JEDNEJ warstwy: [`super::serve::local_answer`], bo to jest kształt
/// protokołu, a nie kształt naszej listy.
///
/// `inputSchema`, nie `schema`: tak nazywa ten klucz MCP. Zła nazwa znaczy narzędzie porzucone
/// przez vendora **w ciszy** — a z zewnątrz wygląda to dokładnie jak lider, który nie chciał go
/// użyć. Dlatego kryterium pyta o tę nazwę wprost.
#[must_use]
pub fn tool_list(role: Role) -> Value {
    let tools: Vec<Value> = for_role(role)
        .into_iter()
        .map(|verb| {
            json!({
                "name": verb.name,
                "description": verb.describe,
                "inputSchema": verb.schema,
            })
        })
        .collect();
    Value::Array(tools)
}
