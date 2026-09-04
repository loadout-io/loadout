//! Generowanie własnej konfiguracji vendora z zatwierdzonych Connections.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::drivers::DriverConfiguration;

use super::{Connection, Transport};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Connection {0} is not enabled in Loadout.")]
    NotEnabled(String),
    #[error("Connection {0} does not exist in the Loadout library.")]
    NotFound(String),
    /// 2026-09 (Z-23) — ZDANIE MÓWI, GDZIE LOADOUT SZUKAŁ, i to jest cała różnica między odmową,
    /// po której człowiek wie, co zrobić, a taką, po której zgaduje. „Nie jest ustawiona"
    /// zostawia pytanie „ustawiona GDZIE?", a odpowiedź zależy od tego, jak wstała aplikacja:
    /// uruchomiona z Docka nie widzi ani jednej zmiennej z powłoki tego człowieka.
    #[error(
        "Connection {connection} needs environment variable {name}, but it is not set. Loadout \
         reads it from {read_from}."
    )]
    MissingEnvironment {
        connection: String,
        name: String,
        read_from: String,
    },
    #[error("This agent app cannot receive Loadout Connections.")]
    UnsupportedVendor,
    #[error("Loadout could not prepare its Connection configuration: {0}")]
    Io(#[from] std::io::Error),
    #[error("Loadout could not encode its Connection configuration: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorConfigurations {
    pub claude: Value,
    pub codex: String,
}

#[must_use]
pub fn for_connections(connections: &[Connection]) -> VendorConfigurations {
    VendorConfigurations {
        claude: claude_config(connections),
        codex: codex_config(connections),
    }
}

fn claude_config(connections: &[Connection]) -> Value {
    let servers: BTreeMap<&str, Value> = connections
        .iter()
        .filter(|one| one.enabled)
        .map(|one| {
            let value = match &one.transport {
                Transport::Stdio {
                    command,
                    args,
                    environment: _,
                } => json!({
                    "type": "stdio",
                    "command": command,
                    "args": args,
                }),
                Transport::Http {
                    url,
                    token_environment,
                } => {
                    let mut value = json!({ "type": "http", "url": url });
                    if let Some(name) = token_environment {
                        value["headers"] = json!({
                            "Authorization": format!("Bearer ${{{name}}}")
                        });
                    }
                    value
                }
            };
            (one.name.as_str(), value)
        })
        .collect();
    json!({ "mcpServers": servers })
}

/// Wpis Codeksa dla jednego Połączenia — **jedna odpowiedź dla obu dróg** (niezmiennik 23).
///
/// 2026-09 (Z-23) — DO TEGO DNIA BYŁY DWIE i różniły się w rzeczy, która decyduje o działaniu:
/// dokument dla człowieka pisał `required_env`, a argv jednego biegu o środowisku nie mówiło ani
/// słowa. Dwie odpowiedzi na jedno pytanie rozjeżdżają się przy pierwszej zmianie jednej z nich,
/// a rozjechały się od początku.
///
/// `values` to nazwy JUŻ ROZWIĄZANE dla tego biegu. Pusto znaczy „to jest podgląd, nie
/// uruchomienie": dokument dla człowieka pokazuje wtedy same nazwy w `required_env`, dokładnie
/// tak, jak `codex mcp get` maskuje wartości.
fn codex_entry(connection: &Connection, values: &[(String, OsString)]) -> Vec<(String, String)> {
    match &connection.transport {
        Transport::Stdio {
            command,
            args,
            environment,
        } => {
            let mut pairs = vec![
                ("command".to_owned(), quoted(command)),
                ("args".to_owned(), array(args)),
                ("required_env".to_owned(), array(environment)),
            ];
            /* TU, I WYŁĄCZNIE TU, WARTOŚĆ SEKRETU WOLNO POSTAWIĆ W ARGV.
             *
             * 2026-09-04 (Z-23) — wyjątek otwarty decyzją właściciela i zapisany w niezmienniku 9:
             * „Wartość sekretu wolno podać w argv **wyłącznie** w nadpisaniu
             * `-c mcp_servers.<nazwa>.env.<ZMIENNA>` przekazywanym Codeksowi, i wyłącznie po to,
             * żeby doszła do procesu serwera MCP."
             *
             * Powód jest zmierzony, nie wyprowadzony (2026-09-04, `codex-cli 0.153.0`): serwer
             * stdio pod Codeksem **nie dziedziczy środowiska rodzica** — dostaje zamkniętą listę
             * kilkunastu zmiennych, w której nie ma ani jednego klucza API — więc wartość podana
             * procesowi Codeksa nie dociera do serwera, którego dotyczy. Ten klucz jest jedyną
             * drogą, jaką ten vendor daje.
             *
             * Cena, którą właściciel przyjął: przez czas życia procesu Codeksa wartość widzi każdy
             * `ps` na tej maszynie. Wyjątek kończy się na tym kluczu — prompt i klucze vendora
             * jadą dalej wyłącznie stdinem i jawną listą przepuszczaną.
             *
             * Nazwa idzie przez `toml_key`, ta sama funkcja co nazwa serwera wyżej: druga ścieżka
             * kodowania rozjechałaby się przy pierwszym identyfikatorze z kropką (niezmiennik 23). */
            for name in environment {
                if let Some((_, value)) = values.iter().find(|(known, _)| known == name) {
                    pairs.push((
                        format!("env.{}", toml_key(name)),
                        quoted(&value.to_string_lossy()),
                    ));
                }
            }
            pairs
        }
        Transport::Http {
            url,
            token_environment,
        } => {
            let mut pairs = vec![("url".to_owned(), quoted(url))];
            if let Some(name) = token_environment {
                pairs.push(("bearer_token_env_var".to_owned(), quoted(name)));
            }
            pairs
        }
    }
}

/// Klucze, które są NASZE: zna je dokument dla człowieka i nasz własny import
/// (`import::adapters`), a `-c` Codeksa nie. Nieznany klucz w `-c` jest odmową startu, więc lista
/// stoi tu jako kod, a nie jako zdanie w komentarzu (2026-09, Z-23).
const OURS_ALONE: &[&str] = &["required_env"];

/// Czy brak wartości wolno przemilczeć, zamiast zatrzymać Start.
///
/// 2026-09-04 (Z-23) — WYŁĄCZNIE serwer stdio pod Codeksem, bo wyłącznie ten dostaje wartość
/// nadpisaniem w argv: nierozwiązana nazwa nie tworzy wtedy nadpisania i nic poza tym się nie
/// zmienia. Każda inna droga bierze wartość ze środowiska procesu, więc jej brak jest odmową
/// najpóźniej przy Starcie (niezmiennik 12), a nie awarią w połowie biegu.
fn the_server_can_say_it_itself(vendor: &str, transport: &Transport) -> bool {
    vendor == "codex" && matches!(transport, Transport::Stdio { .. })
}

fn codex_config(connections: &[Connection]) -> String {
    let mut out = String::new();
    for connection in connections.iter().filter(|one| one.enabled) {
        let _ = writeln!(out, "[mcp_servers.{}]", toml_key(&connection.name));
        // Bez wartości: ten dokument czyta CZŁOWIEK i idzie do wyniku importu, a nazwy wolno
        // pokazać, wartości nigdy (2026-09, Z-23 — ten sam wzorzec ma `codex mcp get`).
        for (key, value) in codex_entry(connection, &[]) {
            let _ = writeln!(out, "{key} = {value}");
        }
        out.push('\n');
    }
    out
}

/// Rozwiązuje nazwy z definicji agenta do zatwierdzonych plików biblioteki. Biegnie podczas
/// planowania, więc brak albo wyłączone połączenie zatrzymuje cały bieg przed pierwszym procesem.
/// Wszystkie zatwierdzone połączenia z biblioteki, w kolejności katalogu.
///
/// `pub`, bo pyta o to także walidator workflow (`workflow::roster`) — chce powiedzieć przy
/// BUDOWANIU, że krok nazywa połączenie, którego nie ma albo które jest wyłączone, zamiast
/// zostawiać to odmowie Startu. Jedna funkcja czytająca ten katalog, nie dwie: druga rozjechałaby
/// się przy pierwszej zmianie kształtu pliku.
pub fn all(root: &Path) -> Result<Vec<Connection>, RuntimeError> {
    let mut out = Vec::new();
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if entry.file_type()?.is_file() && entry.path().extension() == Some(OsStr::new("json"))
            {
                let bytes = fs::read(entry.path())?;
                out.push(serde_json::from_slice::<Connection>(&bytes)?);
            }
        }
    }
    Ok(out)
}

pub fn selected(root: &Path, names: &[String]) -> Result<Vec<Connection>, RuntimeError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let available = all(root)?;
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for wanted in names {
        if !seen.insert(wanted) {
            continue;
        }
        let connection = available
            .iter()
            .find(|one| one.id == *wanted || one.name == *wanted)
            .ok_or_else(|| RuntimeError::NotFound(wanted.clone()))?;
        if !connection.enabled {
            return Err(RuntimeError::NotEnabled(connection.name.clone()));
        }
        out.push(connection.clone());
    }
    out.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(out)
}

/// Buduje konfigurację konkretnego vendora w katalogu biegu i rozwiązuje wyłącznie nazwy
/// środowiska zapisane w zatwierdzonych Connections. Resolver pozostaje parametrem, żeby test
/// nie mutował globalnego środowiska równoległego procesu testowego.
///
/// Produkcja woła [`for_driver_with_secrets`], nigdy tę: tamta pyta także nośnik Loadouta,
/// a bez niego bieg z Docka nie ma skąd wziąć ani jednej wartości (2026-09, Z-23).
pub fn for_driver<F>(
    run_dir: &Path,
    vendor: &str,
    connections: &[Connection],
    resolve: F,
) -> Result<DriverConfiguration, RuntimeError>
where
    F: FnMut(&str) -> Option<OsString>,
{
    built(
        run_dir,
        vendor,
        connections,
        resolve,
        &super::secrets::Carrier::in_library(None),
    )
}

/// Ta sama droga, tyle że pytająca **nośnik Loadouta**, a nie samo środowisko okna.
///
/// 2026-09 (Z-23) — JEDYNE PRODUKCYJNE WEJŚCIE, i to jest cały powód, dla którego stoi obok,
/// a nie zamiast: [`for_driver`] jest przypięte kryteriami, które podają własne domknięcie
/// i nie mają nośnika ani po co, ani gdzie. Obie drogi mają jedno ciało ([`built`] niżej), więc
/// polityka („co wolno, w jakiej kolejności, i co powiedzieć, gdy brakuje") mieszka w jednym
/// miejscu (niezmiennik 23), a różni je wyłącznie to, kogo pytają o wartość.
pub fn for_driver_with_secrets(
    run_dir: &Path,
    vendor: &str,
    connections: &[Connection],
    secrets: &super::secrets::Carrier,
) -> Result<DriverConfiguration, RuntimeError> {
    built(
        run_dir,
        vendor,
        connections,
        |name| secrets.value_of(name),
        secrets,
    )
}

fn built<F>(
    run_dir: &Path,
    vendor: &str,
    connections: &[Connection],
    mut resolve: F,
    secrets: &super::secrets::Carrier,
) -> Result<DriverConfiguration, RuntimeError>
where
    F: FnMut(&str) -> Option<OsString>,
{
    if connections.is_empty() {
        return Ok(DriverConfiguration::default());
    }
    let mut environment = Vec::new();
    let mut names = BTreeSet::new();
    for connection in connections {
        let required: Vec<&String> = match &connection.transport {
            Transport::Stdio { environment, .. } => environment.iter().collect(),
            Transport::Http {
                token_environment, ..
            } => token_environment.iter().collect(),
        };
        for name in required {
            // Rozwiązana przy poprzednim Połączeniu jedzie raz. Sprawdzamy OBECNOŚĆ, nie wstawiamy:
            // nazwa przemilczana niżej ma zostać zapytana ponownie przy Połączeniu, dla którego
            // jej brak JEST odmową (2026-09, Z-23).
            if names.contains(name) {
                continue;
            }
            match resolve(name) {
                Some(value) => {
                    names.insert(name.clone());
                    environment.push((name.clone(), value));
                }
                /* BRAK WARTOŚCI NIE ZATRZYMUJE SERWERA STDIO POD CODEKSEM (2026-09-04, Z-23,
                 * decyzja właściciela). Ta jedna droga podaje wartość nadpisaniem `env` w argv,
                 * więc jej brak znaczy tu dokładnie „nie ma czego nadpisać" — serwer wstaje
                 * i sam mówi, czego mu brak, a to jest zdanie o JEGO wymaganiach, dokładniejsze
                 * niż nasze. Do tego dnia była tu odmowa i zostaje wszędzie tam, gdzie brak
                 * wartości naprawdę przesądza sprawę: u Claude'a, który czyta ją ze środowiska
                 * procesu, i przy `bearer_token_env_var`, gdzie nazwa bez wartości jest nagłówkiem
                 * bez tokena. */
                None if the_server_can_say_it_itself(vendor, &connection.transport) => {}
                None => {
                    return Err(RuntimeError::MissingEnvironment {
                        connection: connection.name.clone(),
                        name: name.clone(),
                        read_from: secrets.where_loadout_reads_it_from(),
                    });
                }
            }
        }
    }

    let arguments = match vendor {
        "claude" => {
            fs::create_dir_all(run_dir)?;
            let path = run_dir.join("claude-mcp.json");
            let mut document = serde_json::to_vec_pretty(&claude_config(connections))?;
            document.push(b'\n');
            fs::write(&path, document)?;
            vec!["--mcp-config".to_owned(), path.display().to_string()]
        }
        "codex" => codex_overrides(connections, &environment),
        _ => return Err(RuntimeError::UnsupportedVendor),
    };
    Ok(DriverConfiguration {
        arguments,
        environment,
        // Kolejność z `selected()`, czyli po nazwie — argv ma być tym samym napisem przy tym
        // samym zestawie połączeń, żeby dwa identyczne biegi dały się porównać.
        servers: connections.iter().map(|one| one.name.clone()).collect(),
    })
}

fn codex_overrides(connections: &[Connection], values: &[(String, OsString)]) -> Vec<String> {
    let mut arguments = Vec::new();
    for connection in connections {
        let prefix = format!("mcp_servers.{}", toml_key(&connection.name));
        for (key, value) in codex_entry(connection, values) {
            if OURS_ALONE.contains(&key.as_str()) {
                continue;
            }
            arguments.push("-c".to_owned());
            arguments.push(format!("{prefix}.{key}={value}"));
        }
    }
    arguments
}

fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len().saturating_add(2));
    out.push('"');
    for character in value.chars() {
        match character {
            '\u{0008}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{000c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            // TOML basic strings reject raw control bytes. Keeping every key/value and escaping
            // the uncommon controls prevents a private server from staying enabled merely
            // because its identifier could not be represented. (2026-08-24, T-111)
            other if other.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(other));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| quoted(value))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub(crate) fn toml_key(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        value.to_owned()
    } else {
        quoted(value)
    }
}
