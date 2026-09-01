//! Definicja agenta: kształt na drucie, dwuwarstwowe dziedziczenie, plik na dysku
//! i przelotka na opcje vendora.
//!
//! Trzy reguły trzymają ten plik w kupie i żadna nie jest kosmetyczna:
//!
//! 1. **Każde pole [`Agent`] jest wymagane** — ani jednego `Option<T>`. Szablon jest zawsze
//!    kompletny, więc wynik złożenia zawsze się deserializuje [T4 §4.3, reguła 2].
//! 2. **[`Overrides`] jest w całości `Option`.** „Czy to nadpisane?" ma być pytaniem
//!    o typ, nie o wartość.
//! 3. **Nigdzie nie ma `null`.** 2026-08-15, sprawdzone lokalnie na `json-patch` 4.2.0:
//!    w RFC 7396 `null` w patchu **kasuje klucz**, a skasowany klucz to plik ustawień,
//!    który się nie wczyta [T4 §4.3, reguła 1]. Dlatego „brak limitu" to
//!    `giveUpAfterMinutes: 0`, „brak umiejętności" to `[]`, a „wszystkie narzędzia" to
//!    wariant [`Tools::Everything`]. Kodujemy brak wartością, nigdy pustką.
//!
//! Dwie rzeczy, które T4 bierze z zewnątrz, a ten plik robi sam — obie z tego samego powodu:
//! `Cargo.toml` nie należy do T-11 (`checks/quick-scope.sh`, lista `DENIED`), więc dołożenie
//! zależności byłoby pytaniem do człowieka (`AGENTS.md` §7), a nie decyzją do podjęcia w biegu.
//!
//! - **Złożenie RFC 7396** (`merge` niżej) zamiast `json_patch::merge` z §7.1. Dwanaście
//!   linii, ten sam algorytm; `resolve` niżej jest kopią §7.1 co do znaku.
//! - **Front-matter** (`front_matter`, `scalar`) zamiast `serde_yaml`. Czytamy i piszemy
//!   podzbiór, który sami produkujemy: `nazwa: wartość` po jednej w wierszu, a struktury
//!   (`tools`, `vendorOptions`) w stylu przepływowym, czyli w JSON-ie — ten jest legalnym
//!   YAML-em 1.2 i wraca z `serde_json` bajt w bajt takim, jakim poszedł. Czego ten podzbiór
//!   NIE zna: bloków wcięciowych, kotwic, wielolinijkowych `|` i `>`. Ręcznie dopisany blok
//!   wcięciowy jest więc odmową z nazwą pliku, a nie cichym zgubieniem ustawienia.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::durable_file::{DEFINITION_FILE_MODE, DurableFilePublisher, ModePolicy, PublishError};
use crate::engine::supervisor::{PublicationEntryKind, PublicationRoot};

use crate::engine::drivers::Policy;
use crate::workflow::check::{escalation_in, is_reserved};

/// Wersja formatu, w którym zapisujemy agenta. Jedna liczba i na razie jedna wartość —
/// migracja „na przyszłość" jest w `AGENTS.md` na liście zakazanych, a przy `schema < CURRENT`
/// wchodzi wtedy, kiedy naprawdę powstanie druga wersja [T4 §5.2].
pub const SCHEMA: u8 = 1;

/// Program, który uruchamia agenta.
///
/// **Nie da się tego nadpisać na kroku** [T4 §6.4]. Przełączenie vendora unieważniłoby
/// połowę pozostałych pól (lista `tools`, której Codex nie umie uszanować), a odmowa
/// na poziomie typu kasuje całą klasę walidacji. Ta sama rola u drugiego vendora to
/// duplikat agenta — jedno kliknięcie i plik, który da się przeczytać.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Vendor {
    ClaudeCode,
    Codex,
}

/// Ile agent ma myśleć. Cztery szczeble, tłumaczone przez [`effort_level`] na poziom, którym
/// mówią obaj vendorzy — nazwy vendorów nigdy nie docierają na ekran (niezmiennik 14).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Thinking {
    Quick,
    #[default]
    Balanced,
    Deep,
    Deepest,
}

/// Cztery szczeble po ludzku → poziom wysiłku, którym mówią OBAJ vendorzy.
///
/// # Ta tabela jest jedna i stoi tutaj, przy szczeblu (2026-08-23, T-91)
///
/// Do tego dnia `Thinking` nie miało w drzewie ani jednego czytelnika poza importem: doc przy
/// enumie obiecywał tłumaczenie „niżej na `--effort` i `model_reasoning_effort`", a `grep` po
/// całym drzewie znajdował te dwa napisy WYŁĄCZNIE w `import/adapters.rs`, przy **czytaniu**
/// cudzej konfiguracji Codeksa. Planer właściciela, zapisany na `deepest`, biegał na domyślnym
/// wysiłku od pierwszego dnia — i nie dało się tego zobaczyć, klikając (niezmiennik 16).
///
/// JEDNA tabela dla obu vendorów, bo poziomy nazywają się u nich tak samo; różni się wyłącznie
/// SPOSÓB podania, a ten jest wiedzą adaptera (`AgentDriver::effort_argv`): Claude Code bierze
/// `--effort <poziom>`, Codex `-c model_reasoning_effort=<poziom>`. Rozpisanie tego odwzorowania
/// osobno w każdym adapterze dałoby dwie kopie, które dziś odpowiadają tak samo i rozjeżdżają
/// się w dniu, w którym ktoś przeceluje jedno ramię — a wtedy krok i rozmowa tego samego agenta
/// myślą inaczej i nic tego nie mówi (niezmiennik 23; mierzy to `one_table_for_thinking.rs`).
///
/// Stoi przy szczeblu, a nie w module biegu, z tego samego powodu, co [`policy_of`]: rozmowa
/// (`commands::chat`) **nie ma prawa** zależeć od `commands::run`, bo brak tej zależności jest
/// jedynym mechanizmem, którym lider nie może zacząć biegu.
///
/// Czego tu NIE MA: `max` Claude'a. U nas są cztery szczeble, u vendora pięć poziomów, a piąty
/// jest poza tabelą do decyzji człowieka — przelotka `vendor_options` pozwala go wpisać ręcznie.
#[must_use]
pub const fn effort_level(thinking: Thinking) -> &'static str {
    match thinking {
        Thinking::Quick => "low",
        Thinking::Balanced => "medium",
        Thinking::Deep => "high",
        Thinking::Deepest => "xhigh",
    }
}

/// Co agent może zrobić z plikami. Trzypozycyjny dial bezpieczeństwa, jedyny, jaki widzi
/// użytkownik; siedem trybów Claude'a i trzy piaskownice Codeksa są tłumaczone pod spodem
/// [T4 §3.3, §6.3].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FileAccess {
    #[default]
    LookOnly,
    AskFirst,
    WorkFreely,
}

/// Dial „co agent może zrobić z plikami" → polityka, którą rozumie sterownik.
///
/// Trzy pozycje na trzy warianty, po kolei. Środkowa jest przybliżeniem i tak jest opisana
/// w macierzy T4 §6.3 (`fileAccess` jest `Approximate` u obu vendorów): [`Policy`] nie ma dziś
/// wariantu „pytaj", więc `ask-first` ląduje na „edytuje w swoim folderze". Sklejenie dwóch pozycji
/// dialu w jedną politykę byłoby gorsze — dial miałby wtedy pozycję, która nic nie robi, czyli
/// kontrolkę bez handlera (niezmiennik 16).
///
/// # To jest JEDYNA kopia tego odwzorowania i jest to mierzone (T-63 AC-4)
///
/// `one_table_for_policy.rs` liczy pliki pod `src-tauri/src/`, w których stoi ramię
/// `FileAccess::… => Policy::…`, i wymaga dokładnie jednego. Dwie wyczerpujące kopie, które po
/// prostu się zgadzają, są nieodróżnialne dla każdej asercji o wartościach — a rozjechać się może
/// **przecelowanie jednego ramienia** w jednej z nich. Lider, któremu wolno pisać, choć człowiek
/// ustawił „look only", nie wygląda na awarię: wygląda na lidera, który zapisał plik.
///
/// # Dlaczego ta tabela mieszka TUTAJ, a nie w `commands::run` [2026-08-20, T-63]
///
/// Bo ma dwóch czytelników w dwóch **równorzędnych** modułach komend — krok biegu
/// (`commands::run::plan_agent`) i rozmowę z liderem (`commands::chat::Lead::policy`) — a jeden
/// z nich nie ma prawa zależeć od drugiego. `chat_never_starts_a_run.rs` (T-60) asertuje, że
/// w kodzie `commands/chat.rs` nie ma napisu `super::run`, i to **nie jest kosmetyka**: brak tej
/// zależności JEST mechanizmem, którym rozmowa nie może zacząć biegu, a zdanie w promptcie
/// systemowym to tylko prośba, którą model może zignorować.
///
/// Więc wspólny fakt idzie w dół, do modułu, od którego oba te moduły już zależą — i idzie
/// dokładnie tam, gdzie stoi [`FileAccess`], bo to jest jedno zdanie o jednym dialu. Strzałka się
/// przy tym nie odwraca: `library/` zależy od `workflow/`, a to od `engine::dag`, więc
/// `library/` → `engine::drivers` jest kierunkiem, który już istnieje. Odwrotność —
/// `engine/` zależne od `library/` — jest tym, przed czym ostrzega [`RunSpec::tools`], i tego tu
/// nie ma.
///
/// `commands::run` re-eksportuje tę nazwę, bo pod tamtym adresem woła ją T-62
/// (`ask_one_agent.rs`) i T-63 (`one_table_for_policy.rs`): jedna funkcja, dwie drogi do niej,
/// zero drugich tabel.
#[must_use]
pub const fn policy_of(access: FileAccess) -> Policy {
    match access {
        FileAccess::LookOnly => Policy::ReadOnly,
        FileAccess::AskFirst => Policy::EditInFolder,
        FileAccess::WorkFreely => Policy::Unrestricted,
    }
}

/// Narzędzia: wszystkie albo wymieniona lista.
///
/// `rename_all` **i** `rename_all_fields` stoją tu razem świadomie (04 §2.5): pierwsze
/// nazywa warianty, drugie pola wariantu strukturalnego. Dzisiaj `Only` jest wariantem
/// krotkowym, więc drugi atrybut nic nie robi — i po to tu jest. Dzień, w którym ktoś
/// zamieni go na `Only { names: Vec<String> }`, jest dniem, w którym bez niego
/// `started_at` poleciałoby do frontendu, który czyta wyłącznie `camelCase`, i położyło
/// ekran.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Tools {
    #[default]
    Everything,
    Only(Vec<String>),
}

/// Kolor tożsamości agenta. Pięć wartości, bo tyle jest tokenów `--id-1`…`--id-5`
/// (`docs/design/DESIGN.md` §3).
///
/// Enum, nie `String` — i to jest odpowiedź na otwarte pytanie O6 z T4, które brzmiało
/// „ośmiu Claude'owych czy własnych". DESIGN §3 rozstrzygnął je na pięć przygaszonych
/// tokenów, bo kolor tożsamości **nigdy** nie może być pomylony z kolorem stanu; dla
/// ośmiu Claude'owych nie mamy tokenów, a kolor bez tokenu to hex w komponencie.
/// Przy enumie „`color: neon` jest odmową" wynika z typu, a nie z walidatora, który
/// ktoś kiedyś zapomni zawołać.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Color {
    Slate,
    Plum,
    Clay,
    Moss,
    Rose,
}

/// Surowe opcje vendora: `{ "claude": { "--jakas-nowa-flaga": "wartosc" } }`.
///
/// `BTreeMap`, nie `HashMap`: zapis ma być deterministyczny, żeby dwa zapisy tej samej
/// definicji dały bajt w bajt ten sam plik (`DECISIONS-LOCKED.md` §D6). `HashMap` daje
/// przy każdym uruchomieniu inną kolejność kluczy, czyli plik, który „zmienia się sam"
/// w każdym `git diff`.
pub type VendorOptions = BTreeMap<String, BTreeMap<String, String>>;

/// Domyślna wartość [`Agent::reaches_the_web`] — powód stoi przy tym polu.
///
/// Funkcja, a nie `#[serde(default)]`, bo `bool` domyśla się `false`. Bez niej ta domyślna
/// obowiązywałaby wyłącznie nowych agentów, a każdy plik zapisany wcześniej czytałby się
/// z siecią wyłączoną.
pub(crate) const fn reaching_the_web() -> bool {
    true
}

/// Zapisany agent. Piętnaście kluczy na drucie i ani jednego z podkreśleniem.
///
/// `deny_unknown_fields` jest tu jedyną obroną przed defektem zmierzonym w T4 §9:
/// `claude --agents '{"broken":{"model":"sonnet"}}' -p "hi"` kończy się **kodem 0, bez
/// słowa na stderr**. Źle zbudowana definicja wygląda dokładnie tak samo jak zła instrukcja
/// w promptcie i kosztuje godziny diagnozy — więc walidacja jest nasza i dzieje się, zanim
/// cokolwiek odpalimy.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Agent {
    /// Wersja schematu. Jedna liczba, wprowadzona teraz, bo dopisanie jej później znaczy
    /// zgadywanie, co znaczą pliki bez niej [T4 §5.2].
    pub schema: u8,
    /// Stabilny przez zmianę nazwy. Ukryty przed użytkownikiem.
    pub id: Uuid,
    pub name: String,
    /// Jedno zdanie „co to robi" — etykieta `What it does`.
    pub summary: String,
    pub color: Color,
    /// Prompt systemowy. Na dysku jest treścią pliku, nie kluczem front-mattera.
    pub instructions: String,
    pub runs_with: Vendor,
    pub model: String,
    /// Który szczebel „ile myśleć" ma ten agent.
    ///
    /// 2026-08-23 — DO TEGO DNIA TO POLE NIE MIAŁO ANI JEDNEGO CZYTELNIKA i doc przy nim mówił
    /// nieprawdę. Tłumaczenie leży w [`effort_level`] (jedna tabela dla obu vendorów), a nazwę
    /// flagi dokłada adapter (`engine::drivers::AgentDriver::effort_argv`). Bieg czyta to pole
    /// w `commands::run`, rozmowa przez `commands::chat::Lead::effort`.
    pub thinking: Thinking,
    pub file_access: FileAccess,
    /// `0` znaczy „bez limitu". Nigdy `None` — patrz reguła 3 w nagłówku modułu.
    pub give_up_after_minutes: u32,
    pub tools: Tools,
    /// Czy ten agent może sięgnąć do internetu.
    ///
    /// # Po co to jest OSOBNYM polem, a nie pozycją na liście narzędzi
    ///
    /// Bo to jest jedyny kształt, którym umieją mówić OBAJ vendorzy. U Claude'a sieć to dwa
    /// czasowniki (`WebFetch`, `WebSearch`) i lista narzędzi jest ich naturalnym miejscem;
    /// Codex nie ma listy narzędzi wcale — u niego sieć jest ustawieniem PIASKOWNICY
    /// (`sandbox_workspace_write.network_access`). Nazwa narzędzia wpisana w formularz byłaby
    /// więc kontrolką działającą u jednego z dwóch, a u drugiego wygaszoną (`capabilities.ts`:
    /// `tools` jest przy Codeksie `unavailable`).
    ///
    /// 2026-08-23 — POWSTAŁO Z PYTANIA WŁAŚCICIELA: „czemu dostępu do neta nie mają?". Zmierzone
    /// w jego bibliotece: 18 agentów, ani jeden z siecią. Agenci claude'owi mieli `everything`,
    /// co znaczy „to, co daje dial", a sieć jest w tabeli dopiero przy `Unrestricted`; agenci
    /// codexowi — `codex-reaserch`, `planner`, `riczi`, czyli dokładnie ci od researchu — nie
    /// mieli jak jej dostać w ogóle, bo `network_access` nie było wysyłane nigdy.
    ///
    /// # Dlaczego to nie jest czwarty stopień dialu
    ///
    /// Bo dial mówi o PLIKACH („look only" znaczy „nie zmienia plików"), a nie o tym, czy agent
    /// widzi świat. Cała treść T-63: „lider do researchu, który nie może zepsuć repo". Sieć
    /// wpuszczona w dial dawałaby wybór między „widzi świat i może zepsuć pliki" a „nie zepsuje
    /// niczego i nie widzi nic".
    ///
    /// # Domyślnie WŁĄCZONE — rozstrzygnięcie właściciela z 2026-08-23
    ///
    /// Pole weszło z domyślną `false` i z powodem: sieć włączona bez pytania jest poszerzeniem
    /// uprawnień, o które nikt nie prosił. Właściciel to rozstrzygnął w drugą stronę tego samego
    /// dnia — „niech to będzie true by default" — i rozstrzygnięcie stoi na jego liczbach:
    /// w bibliotece 18 agentów, ani jeden z siecią, bo do wyłączonej domyślnej trzeba było
    /// TRAFIĆ, a nikt nie trafiał. Kontrolka, której nikt nie znajduje, jest kontrolką, której
    /// nie ma — a agent do researchu bez internetu jest droższą pomyłką niż weryfikator, który
    /// przy okazji może coś doczytać.
    ///
    /// **Dial to zostaje nietknięty.** Sieć nie daje ani jednego czasownika plikowego: „look
    /// only" dalej znaczy „nie zmienia plików", u obu vendorów, i tego pilnują kryteria.
    ///
    /// `default = "reaching_the_web"`, a nie `#[serde(default)]`: `bool` domyśla się `false`,
    /// więc bez tej funkcji każdy plik zapisany przed tą zmianą czytałby się z siecią wyłączoną
    /// — czyli domyślna obowiązywałaby wyłącznie nowych agentów, a stara biblioteka zostałaby
    /// tam, gdzie była. To byłyby dwie różne odpowiedzi na jedno pytanie (niezmiennik 13).
    #[serde(default = "reaching_the_web")]
    pub reaches_the_web: bool,
    pub skills: Vec<String>,
    /// Nazwy serwerów narzędziowych. W interfejsie: `Connections`.
    pub connections: Vec<String>,
    /// Ścieżka pliku pamięci; `""` znaczy „nigdzie". Ustawiane **na kroku**, nie w tym
    /// formularzu — ścieżka wyniku należy do kroku, nie do roli (`docs/mockup/index.html`,
    /// panel kroku). W typie zostaje, bo krok nadpisuje pole, którego szablon musi mieć.
    pub write_results_to: String,
    /// Przelotka `DECISIONS-LOCKED.md` §D6: Loadout tego **nie interpretuje**. Bez niej
    /// każda nowa flaga vendora wymaga wydania Loadouta.
    ///
    /// `skip_serializing_if` nie jest tu wygodą. Pusta przelotka nie ma prawa dołożyć
    /// szesnastego klucza do zapisanego agenta — to jest dokładnie ten „jeden klucz
    /// więcej", przed którym broni kryterium 1.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vendor_options: VendorOptions,
}

impl Agent {
    /// Agent, na którym opisujemy kształt na drucie. Jeden, żeby kryterium miało co
    /// serializować, i żeby „jak wygląda zapisany agent" miało jedną odpowiedź w repo.
    #[must_use]
    pub fn example() -> Self {
        Self {
            schema: SCHEMA,
            // `from_u128`, nie `parse_str`: identyfikator z §5.1 jest tu stałą, a stała nie ma
            // prawa zwracać `Result`, którego wołający musiałby obsłużyć w kodzie ekranu.
            id: Uuid::from_u128(0x0198_97b4_8f3a_7c21_9d44_0b6a_1e2c_5f77),
            name: "Forge".to_string(),
            summary: "Writes code".to_string(),
            color: Color::Clay,
            instructions: "Write the smallest change that makes the checks pass.".to_string(),
            runs_with: Vendor::ClaudeCode,
            model: "opus".to_string(),
            thinking: Thinking::Balanced,
            file_access: FileAccess::WorkFreely,
            give_up_after_minutes: 20,
            tools: Tools::Everything,
            reaches_the_web: reaching_the_web(),
            skills: Vec::new(),
            connections: Vec::new(),
            write_results_to: "handoffs/build.md".to_string(),
            // Pusta i taka ma zostać: niepusta przelotka dokłada szesnasty klucz, a piętnaście
            // jest tu liczbą, nie zaokrągleniem (kryterium 1).
            vendor_options: VendorOptions::new(),
        }
    }
}

/// Co pamięta jeden krok workflow: **wyłącznie różnicę** wobec agenta.
///
/// Serializuje się do patcha RFC 7396 — brak klucza znaczy „idź za agentem". Nigdy nie
/// emituje `null`, bo `skip_serializing_if` nie ma jak go wyprodukować.
///
/// Czego tu nie ma i mieć nie będzie: `id`, `name`, `runsWith`. Krok, który przestawia
/// vendora, unieważnia połowę reszty [T4 §6.4].
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Overrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_access: Option<FileAccess>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub give_up_after_minutes: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Tools>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connections: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_results_to: Option<String>,
}

/// Wynik złożenia agenta z nadpisaniem: co naprawdę pobiegnie plus lista nazw do znacznika
/// „N changed".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub agent: Agent,
    /// Nazwy nadpisanych pól, posortowane. Puste, kiedy krok niczego nie zmienił.
    pub changed: Vec<String>,
}

/// Co poszło nie tak z definicją agenta.
///
/// Komunikat zawsze nazywa **plik**, a przy pliku także klucz, przez który się wywrócił.
/// T4 §10: „pokaż nazwę pliku i «Open in editor», nie połykaj" — połknięty błąd wygląda
/// jak zła instrukcja w promptcie i kosztuje godziny.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// Pliku nie da się przeczytać albo mówi coś, czego definicja agenta nie zna.
    #[error("{file} — {detail}")]
    Unreadable { file: String, detail: String },
    /// Na drucie stanęła pustka tam, gdzie ma stać wartość. W RFC 7396 `null` kasuje
    /// klucz, więc przepuszczony `null` produkuje plik ustawień, który się nie wczyta.
    #[error("{field} has no value. Remove the line to go back to the agent's setting")]
    EmptySetting { field: String },
}

/// Agent + nadpisania -> co naprawdę pobiegnie, plus lista nazw dla znacznika „N changed".
///
/// Cała algebra dziedziczenia to te kilkanaście linii: złożenie RFC 7396 i policzenie
/// kluczy patcha. Wariant „pełna kopia agenta na kroku" (T4 §4.1 A) byłby prostszy
/// i **fałszywy**: edycja szablonu nigdy nie dotarłaby do workflow.
pub fn resolve(base: &Agent, overrides: &Overrides) -> Result<Resolved, serde_json::Error> {
    let patch = serde_json::to_value(overrides)?;
    let mut doc = serde_json::to_value(base)?;
    merge(&mut doc, &patch);

    // Nazwy kluczy patcha, nie różnica dwóch pełnych obiektów. To jest ta jedna linia,
    // dla której znacznik „N changed" jest darmowy [T4 §4.2].
    let mut changed: Vec<String> = Vec::new();
    if let Some(fields) = patch.as_object() {
        changed.extend(fields.keys().cloned());
    }
    changed.sort();

    Ok(Resolved {
        agent: serde_json::from_value(doc)?,
        changed,
    })
}

/// Złożenie RFC 7396: klucz z patcha wygrywa, `null` **kasuje**, obiekty schodzą w głąb.
///
/// Dwanaście linii zamiast zależności `json-patch` — `Cargo.toml` nie należy do tego zadania.
/// Gałąź kasująca zostaje mimo reguły 3 z nagłówka: reguła mówi, czego sami nie produkujemy,
/// a ta funkcja dostaje też patche z zewnątrz. Bez tej gałęzi byłaby to inna algebra niż ta,
/// którą nazywa dokumentacja, i rozjazd wyszedłby na pierwszym zaimportowanym pliku.
fn merge(target: &mut Value, patch: &Value) {
    let Value::Object(fields) = patch else {
        // `clone_from`, nie `*target = patch.clone()`: to samo znaczenie, a clippy odmawia
        // drugiej formy (`assigning_clones`), bo ta pierwsza umie użyć pamięci, która już jest.
        target.clone_from(patch);
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let Some(into) = target.as_object_mut() else {
        return;
    };
    for (key, value) in fields {
        if value.is_null() {
            into.remove(key);
        } else {
            merge(into.entry(key.as_str()).or_insert(Value::Null), value);
        }
    }
}

/// Dziewięć pól, które krok może zmienić [T4 §7.1].
///
/// Czego tu nie ma: `id` i `name` (tożsamość agenta, nie kroku) oraz `runsWith` — krok, który
/// przestawia vendora, unieważnia połowę reszty, `tools` na czele [T4 §6.4]. Ta lista jest
/// filtrem wykonywanym na **wyprodukowanym patchu**, a nie komentarzem obok pętli: sama
/// długość niczego nie pilnuje, bo `retain`, którego nikt nie zawołał, też ma dziewięć pozycji.
const OVERRIDABLE: [&str; 9] = [
    "instructions",
    "model",
    "thinking",
    "fileAccess",
    "giveUpAfterMinutes",
    "tools",
    "skills",
    "connections",
    "writeResultsTo",
];

/// Formularz pokazuje wartości efektywne; przy zapisie zostaje z nich **sama różnica**.
///
/// Pola, których krok nie może ruszyć (`id`, `name`, `runsWith`), nie mają prawa wypłynąć
/// do patcha, choćby się różniły.
pub fn capture(base: &Agent, edited: &Agent) -> Result<Overrides, serde_json::Error> {
    let before = serde_json::to_value(base)?;
    let after = serde_json::to_value(edited)?;

    let mut patch = Map::new();
    if let (Value::Object(before), Value::Object(after)) = (&before, &after) {
        for (key, value) in after {
            if before.get(key) != Some(value) {
                patch.insert(key.clone(), value.clone());
            }
        }
    }
    patch.retain(|key, _| OVERRIDABLE.contains(&key.as_str()));

    serde_json::from_value(Value::Object(patch))
}

/// Odmawia `null`-a na surowym JSON-ie, zanim stanie się on [`Overrides`] albo [`Agent`].
///
/// Woła się to na tym, co przyszło z zewnątrz — z formularza, z pliku workflow, z importu.
/// Po tym sprawdzeniu złożenie merge patchem jest funkcją totalną, a jedyna słynna
/// pułapka RFC 7396 znika [T4 §4.3].
pub fn validate_no_nulls(raw: &Value) -> Result<(), AgentError> {
    look_for_a_null(raw, "")
}

/// Ścieżka do pustki, nie sam fakt pustki: „coś jest puste" nie da się otworzyć i poprawić.
fn look_for_a_null(value: &Value, path: &str) -> Result<(), AgentError> {
    match value {
        Value::Null => Err(AgentError::EmptySetting {
            field: if path.is_empty() {
                "This setting".to_string()
            } else {
                path.to_string()
            },
        }),
        Value::Object(fields) => {
            for (key, child) in fields {
                let below = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                look_for_a_null(child, &below)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                look_for_a_null(child, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Czyta `agents/<slug>.md`: front-matter YAML + treść jako instrukcje.
///
/// Treść jest instrukcjami i **tylko** treść nimi jest. Klucz `instructions` we
/// front-matterze dawałby dwa źródła prawdy dla najdłuższego pola definicji [T4 §5.1].
pub fn read_agent_file(path: &Path) -> Result<Agent, AgentError> {
    let bytes = std::fs::read(path).map_err(|error| refused(path, &error.to_string()))?;
    read_agent_snapshot(path, &bytes)
}

/// Jedno descriptor-bound źródło biblioteki dla ekranu i Startu. Wynik zachowuje błąd per plik,
/// bo ekran dziś odmawia całej listy, a Run może nadal znaleźć zdrowego, wskazanego agenta obok.
pub type AgentLibraryEntry = (PathBuf, Result<Agent, AgentError>);

pub fn read_agent_directory(dir: &Path) -> Result<Vec<AgentLibraryEntry>, AgentError> {
    let publisher = DurableFilePublisher::new(dir);
    let mut listed = None;
    match publisher.recover_with(|root| {
        listed = Some(read_agent_directory_from_root(root, dir));
        Ok(())
    }) {
        Ok(()) => {
            listed.ok_or_else(|| refused(dir, "the recovered agent library was not listed"))?
        }
        Err(PublishError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(Vec::new())
        }
        Err(error) => Err(refused(dir, &error.to_string())),
    }
}

fn read_agent_directory_from_root(
    root: &PublicationRoot,
    dir: &Path,
) -> Result<Vec<AgentLibraryEntry>, AgentError> {
    let mut names = root
        .list_directory(Path::new(""))
        .map_err(|error| refused(dir, &error.to_string()))?
        .into_iter()
        .filter(|entry| {
            entry.kind == PublicationEntryKind::Regular
                && Path::new(&entry.name)
                    .extension()
                    .is_some_and(|ext| ext == "md")
        })
        .map(|entry| entry.name)
        .collect::<Vec<_>>();
    names.sort();

    let mut out = Vec::with_capacity(names.len());
    for file_name in names {
        let path = dir.join(&file_name);
        let agent = root
            .read_regular(Path::new(&file_name), false)
            .map_err(|error| refused(&path, &error.to_string()))
            .and_then(|bytes| read_agent_snapshot(&path, &bytes));
        out.push((path, agent));
    }
    Ok(out)
}

/// Wariant dla produkcyjnego listowania, które trzyma deskryptor katalogu od recovery aż do
/// parsera. `path` służy wyłącznie bezpiecznej nazwie w odmowie; bajty nie są otwierane drugi raz.
pub(crate) fn read_agent_snapshot(path: &Path, bytes: &[u8]) -> Result<Agent, AgentError> {
    let text = std::str::from_utf8(bytes).map_err(|error| refused(path, &error.to_string()))?;

    let (front, body) = front_and_body(text).ok_or_else(|| {
        refused(
            path,
            "an agent file is three dashes, then its settings, then three dashes, then what \
             the agent should do",
        )
    })?;

    let mut fields = front_matter(front).map_err(|detail| refused(path, &detail))?;

    // Dwa źródła prawdy dla najdłuższego pola definicji rozjeżdżają się przy pierwszej ręcznej
    // edycji i nikt tego nie zauważa, bo oba wyglądają poprawnie [T4 §5.1]. Odmawiamy.
    if fields.contains_key("instructions") {
        return Err(refused(
            path,
            "what the agent should do belongs under the second row of dashes, not in a line \
             of its own up top. Two places to write it is two answers",
        ));
    }
    fields.insert("instructions".to_string(), Value::String(body.to_string()));

    serde_json::from_value(Value::Object(fields)).map_err(|error| refused(path, &error.to_string()))
}

/// Odmowa, która nazywa plik.
///
/// T4 §10: „pokaż nazwę pliku i «Open in editor», nie połykaj". Zmierzone w §9, i to jest cały
/// powód, dla którego walidacja jest nasza: `claude --agents '{"broken":{"model":"sonnet"}}'
/// -p "hi"` kończy się **kodem 0, bez słowa na stderr**, więc zepsuta definicja wygląda
/// dokładnie tak samo jak zła instrukcja w promptcie i kosztuje godziny.
fn refused(path: &Path, detail: &str) -> AgentError {
    AgentError::Unreadable {
        file: path.display().to_string(),
        detail: detail.to_string(),
    }
}

/// Front-matter i treść. Pierwsze `---` otwiera, pierwsze samotne `---` zamyka, **cała** reszta
/// jest treścią — znak w znak, bez obcinania białych znaków na końcu. Pusta linia w środku to
/// akapit, który napisał człowiek, a pusta linia na końcu to dowód, że nikt nic nie obciął.
fn front_and_body(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    Some((&rest[..end], &rest[end + "\n---\n".len()..]))
}

/// Front-matter na mapę JSON-a. Merge patch i `serde` pracują na JSON-ie, więc konwersja jest
/// jedna i jest tutaj [T4 §5.1].
///
/// Wiersz, którego ten podzbiór nie rozumie, jest błędem z treścią wiersza — nie ciszą.
/// Dopisany ręcznie blok wcięciowy wyląduje tu jako klucz, którego definicja agenta nie zna,
/// i wróci jako odmowa z nazwą pliku.
fn front_matter(text: &str) -> Result<Map<String, Value>, String> {
    let mut fields = Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, raw)) = line.split_once(':') else {
            return Err(format!("this line is not a setting: {line}"));
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(format!(
                "this line has a value but no setting to put it in: {line}"
            ));
        }
        fields.insert(name.to_string(), scalar(raw.trim()));
    }
    Ok(fields)
}

/// Jedna wartość front-mattera.
///
/// Skalary po YAML-owemu (liczba, `true`, `false`, pustka), struktury w stylu przepływowym —
/// czyli w JSON-ie, bo ten jest legalnym YAML-em 1.2 i wraca z `serde_json` bajt w bajt takim,
/// jakim poszedł.
fn scalar(text: &str) -> Value {
    if text.is_empty() || text == "null" || text == "~" {
        return Value::Null;
    }
    if text == "true" {
        return Value::Bool(true);
    }
    if text == "false" {
        return Value::Bool(false);
    }
    if text.starts_with(['{', '[', '"']) {
        if let Ok(value) = serde_json::from_str::<Value>(text) {
            return value;
        }
        if let Some(items) = flow_list(text) {
            return Value::Array(items);
        }
        return Value::String(text.to_string());
    }
    if let Ok(number) = text.parse::<u64>() {
        return Value::Number(number.into());
    }
    if let Ok(number) = text.parse::<i64>() {
        return Value::Number(number.into());
    }
    if let Ok(number) = text.parse::<f64>() {
        // `from_f64` odmawia NaN-owi i nieskończoności, więc `model: nan` zostaje napisem —
        // i dobrze, bo to jest nazwa modelu, a nie liczba.
        if let Some(number) = serde_json::Number::from_f64(number) {
            return Value::Number(number);
        }
    }
    Value::String(text.to_string())
}

/// `[rust-tauri, pdf]` — lista przepływowa bez cudzysłowów. JSON tego nie przyjmie, a ręcznie
/// dopisana lista umiejętności wygląda właśnie tak, więc czytamy oba zapisy.
fn flow_list(text: &str) -> Option<Vec<Value>> {
    let inner = text.strip_prefix('[')?.strip_suffix(']')?;
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    Some(inner.split(',').map(|item| scalar(item.trim())).collect())
}

/// Zapisuje agenta do `dir/<slug>.md` i zwraca ścieżkę, pod którą wylądował.
///
/// Slug wyprowadzamy z nazwy tutaj, w jednym miejscu, żeby wołający nie musiał znać
/// reguły — a przy okazji żeby była JEDNA reguła.
pub fn write_agent_file(dir: &Path, agent: &Agent) -> Result<PathBuf, AgentError> {
    let path = dir.join(format!("{}.md", slug(agent)));

    let wire = serde_json::to_value(agent).map_err(|error| refused(&path, &error.to_string()))?;
    let Value::Object(mut fields) = wire else {
        return Err(refused(&path, "an agent has to be a list of settings"));
    };

    let instructions = fields.remove("instructions");
    let body = instructions.as_ref().and_then(Value::as_str).unwrap_or("");

    let mut text = String::from("---\n");
    for name in FRONT_MATTER {
        if let Some(value) = fields.remove(name) {
            text.push_str(&setting(name, &value));
        }
    }
    // Cokolwiek zostało po tej pętli — czyli pole dopisane do `Agent` i niedopisane do listy
    // wyżej — ląduje na końcu, posortowane. Ma się zapisać brzydko, a nie zniknąć: cicha
    // utrata ustawienia jest awarią, kolejność wierszy nie jest.
    for (name, value) in &fields {
        text.push_str(&setting(name, value));
    }
    text.push_str("---\n");
    text.push_str(body);

    std::fs::create_dir_all(dir).map_err(|error| refused(&path, &error.to_string()))?;
    DurableFilePublisher::new(dir)
        .atomic_replace(
            &path,
            text.as_bytes(),
            ModePolicy::PreserveExistingOr(DEFINITION_FILE_MODE),
        )
        .map_err(|error| refused(&path, &error.to_string()))?;
    Ok(path)
}

/// Kolejność wierszy front-mattera — ta z T4 §5.1, wypisana, a nie wzięta z kolejności pól
/// struktury. Zapis ma być deterministyczny co do bajtu, żeby `git diff` na katalogu agentów
/// odpowiadał na pytanie „czy ktoś tego agenta ruszał", a nie na pytanie „czy zapisał go dwa
/// razy" (`DECISIONS-LOCKED.md` §D6).
const FRONT_MATTER: [&str; 15] = [
    "schema",
    "id",
    "name",
    "summary",
    "color",
    "runsWith",
    "model",
    "thinking",
    "fileAccess",
    "giveUpAfterMinutes",
    "writeResultsTo",
    "tools",
    "skills",
    "connections",
    "vendorOptions",
];

/// Jeden wiersz front-mattera, zakończony znakiem końca linii.
fn setting(name: &str, value: &Value) -> String {
    match value {
        Value::String(text) => format!("{name}: {}\n", quoted_when_needed(text)),
        // Liczby, wartości logiczne i struktury zapisuje `serde_json`: styl przepływowy JSON-a
        // jest legalnym YAML-em, a mapa `BTreeMap` daje ten sam porządek kluczy przy każdym
        // zapisie.
        other => format!("{name}: {other}\n"),
    }
}

/// Napis w postaci, w jakiej wróci z odczytu bez zmiany.
///
/// Cudzysłów dokładamy dokładnie wtedy, kiedy goła postać przeczytałaby się jako coś innego:
/// pusty napis, liczba, `true`, wiersz z dwukropkiem albo z kratką. Sprawdzamy to wołając
/// własny czytnik — reguła i jej dowód są wtedy jedną rzeczą, a nie dwiema, które mogą się
/// rozjechać.
fn quoted_when_needed(text: &str) -> String {
    if plain_reads_back(text) {
        text.to_string()
    } else {
        Value::String(text.to_string()).to_string()
    }
}

fn plain_reads_back(text: &str) -> bool {
    if text.is_empty() || text.trim() != text {
        return false;
    }
    if text.contains(['\n', '\r', '\t', '#', '"', '\'', '\\'])
        || text.contains(": ")
        || text.ends_with(':')
    {
        return false;
    }
    if text.starts_with([
        '-', '?', ':', ',', '[', ']', '{', '}', '&', '*', '!', '|', '>', '%', '@', '`',
    ]) {
        return false;
    }
    scalar(text) == Value::String(text.to_string())
}

/// Nazwa pliku z nazwy agenta. Jedna reguła, w jednym miejscu — żeby wołający nie musiał jej
/// znać, a przy okazji żeby była jedna.
fn slug(agent: &Agent) -> String {
    let mut out = String::new();
    for character in agent.name.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() {
        // Nazwa, z której nie zostaje ani jeden znak nadający się na nazwę pliku (same znaki
        // spoza ASCII, sama interpunkcja). Identyfikator jest wtedy jedyną rzeczą, która na
        // pewno jest unikalna — dwa pliki `.md` o tej samej nazwie nadpisałyby się nawzajem.
        return agent.id.to_string();
    }
    trimmed.to_string()
}

/// Pole definicji agenta, które w ogóle dociera do vendora.
///
/// Nazwy na drucie są dokładnie tymi, które zna `CapabilityField` w
/// `src/sections/agents/capabilities.ts` — dopóki nie ma generatora (`ts-rs` albo `specta`,
/// T4 §7.2), obie kopie stoją obok siebie i muszą to mówić wprost.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Field {
    Instructions,
    Model,
    Thinking,
    FileAccess,
    GiveUpAfterMinutes,
    Tools,
    Skills,
    Connections,
}

/// Co druga aplikacja robi z tym polem. Trzy stany, bo dwa kłamią [T4 §6.1]: „jest" i „nie ma"
/// nie mają gdzie zapisać ustawienia, które istnieje, ale jest przybliżeniem — a takich jest
/// najwięcej.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Native,
    Approximate,
    Unavailable,
}

/// Macierz z T4 §6.3, zweryfikowana 2026-08-15 przez `--help` obu aplikacji.
///
/// **Dane, nie kod** [T4 §6.1]. Szesnaście wierszy stoi tutaj po to, żeby `if vendor == Codex`
/// nie stało nigdzie indziej: polityka przepisana per adapter jest tym, w jaki sposób
/// w repo źródłowym po cichu umarło skanowanie sekretów (niezmiennik 23).
///
/// Pole odpowiada **najsłabszym** ze swoich tłumaczeń. Stąd `fileAccess` przybliżone u obu:
/// u Claude'a `look-only` to tryb planowania, u Codeksa `ask-first` i `work-freely` to ta sama
/// piaskownica. „Native" znaczyłoby wtedy „część działa dokładnie", a to jest zdanie, którego
/// nie chcemy mówić o dialu bezpieczeństwa.
const CAPABILITIES: [(Field, Vendor, Capability); 16] = [
    (Field::Instructions, Vendor::ClaudeCode, Capability::Native),
    (Field::Instructions, Vendor::Codex, Capability::Native),
    (Field::Model, Vendor::ClaudeCode, Capability::Native),
    (Field::Model, Vendor::Codex, Capability::Native),
    (Field::Thinking, Vendor::ClaudeCode, Capability::Native),
    (Field::Thinking, Vendor::Codex, Capability::Native),
    (
        Field::FileAccess,
        Vendor::ClaudeCode,
        Capability::Approximate,
    ),
    (Field::FileAccess, Vendor::Codex, Capability::Approximate),
    (
        Field::GiveUpAfterMinutes,
        Vendor::ClaudeCode,
        Capability::Native,
    ),
    (Field::GiveUpAfterMinutes, Vendor::Codex, Capability::Native),
    (Field::Tools, Vendor::ClaudeCode, Capability::Native),
    (Field::Tools, Vendor::Codex, Capability::Unavailable),
    (Field::Skills, Vendor::ClaudeCode, Capability::Native),
    (Field::Skills, Vendor::Codex, Capability::Approximate),
    (Field::Connections, Vendor::ClaudeCode, Capability::Native),
    (Field::Connections, Vendor::Codex, Capability::Native),
];

/// Co ta aplikacja robi z tym polem — albo `None`, kiedy tabela nie ma na to odpowiedzi.
///
/// `Option`, a nie wartość domyślna: para bez wiersza to kontrolka, której formularz nie umie
/// narysować, i sterownik, który nie wie, czy ma co tłumaczyć. Domyślne „unavailable" byłoby
/// odpowiedzią wyglądającą na sprawdzoną.
#[must_use]
pub fn capability(field: Field, vendor: Vendor) -> Option<Capability> {
    CAPABILITIES
        .iter()
        .find(|(row_field, row_vendor, _)| *row_field == field && *row_vendor == vendor)
        .map(|(_, _, answer)| *answer)
}

/// Wpis przelotki, który nie dojechał do argv, i powód, dla którego nie dojechał.
///
/// Dwa pola, bo użytkownik potrzebuje dwóch rzeczy naraz: **który wiersz skasować** (`flag`)
/// i **dlaczego** (`escalation`). Odmowa bez nazwy uczy go, że przelotka nie działa — zamiast
/// tego, że została zablokowana; po stronie kroku workflow tę samą pomyłkę naprawia zdanie
/// z `workflow::check`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    /// Klucz z `vendorOptions`, znak w znak taki, jak stoi w pliku agenta.
    pub flag: String,
    /// Podniesienie, przez które ten wpis odpadł. Słowo wzięte z JEDNEJ listy polityki
    /// (`workflow::check::FORBIDDEN_ESCALATIONS`), nigdy z drugiej kopii — druga kopia listy
    /// to sposób, w jaki w repo źródłowym po cichu umarło skanowanie sekretów (niezmiennik 23).
    pub escalation: String,
}

/// Co przelotka oddaje vendorowi — i co jej po drodze odebrano.
///
/// Jedna wartość, dwie odpowiedzi, bo pytanie jest jedno: „co z tego pojedzie do argv".
/// Rozbicie na dwie funkcje dałoby dwa przebiegi tej samej pętli po tej samej mapie i dwa
/// miejsca, w których filtr może się rozjechać sam ze sobą.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Passthrough {
    /// Argumenty w kolejności `klucz, wartość, klucz, wartość` — to, co dostaje vendor.
    pub args: Vec<String>,
    /// Odrzucone wpisy, po jednym na wpis. Puste, kiedy przelotka niczego nie podnosiła.
    pub refused: Vec<Refusal>,
}

/// Jak [`vendor_args`], tylko mówi też, **czego nie przepuściła**.
///
/// `DECISIONS-LOCKED.md` §D6 stawia na przelotce dwa ograniczenia; drugie brzmi dosłownie
/// „przelotka nie omija diala bezpieczeństwa". Filtr stoi tutaj, **zanim** ktokolwiek podepnie
/// tę funkcję do biegu: podpięcie będzie jednolinijkowe i nikt przy nim nie przeczyta D6.
///
/// Lista podniesień jest ta sama, którą przy zapisie workflow czyta `workflow::check` — jedna
/// lista, jedno miejsce (niezmiennik 23). Druga kopia rozjechałaby się w dniu, w którym ktoś
/// dopisze flagę tylko do jednej z nich, i dokładnie tak powstała ta dziura.
///
/// # Czego ta funkcja świadomie NIE filtruje (2026-08-24, T-98)
///
/// Kolizji z listą zarezerwowaną. Ta odmowa jest **twardsza** i pada wyżej: bieg pyta najpierw
/// [`passthrough_refused`] i przy niepustej odpowiedzi w ogóle nie startuje, więc wpis z tamtej
/// listy nie ma jak dojść do argv tą drogą. Ciche wycięcie go tutaj dałoby drugą odpowiedź na
/// to samo pytanie — bieg, który rusza z połową przelotki i nie mówi o tym ani słowa.
#[must_use]
pub fn vendor_args_filtered(agent: &Agent, vendor: &str) -> Passthrough {
    let Some(options) = agent.vendor_options.get(vendor) else {
        // Vendor, o którym ta definicja nic nie mówi, nie ma przelotki — a nie pustą przelotkę
        // z odmowami. Nazwy vendora, której nie znamy, nie tykamy (§D6).
        return Passthrough::default();
    };

    let mut handed = Passthrough {
        args: Vec::with_capacity(options.len() * 2),
        refused: Vec::new(),
    };
    for (flag, value) in options {
        // JEDNA reguła, nie druga kopia tej samej pętli — ta sama funkcja, którą przy zapisie
        // kroku workflow woła `workflow::check::the_passthrough`. Podniesienie liczy się tak
        // samo, kiedy stoi w nazwie flagi, jak wtedy, kiedy stoi w jej wartości, i obie połówki
        // są konieczne: sama nazwa przepuszcza `--sandbox danger-full-access` (`--sandbox` nie
        // jest zarezerwowane, a dial omija dokładnie tak samo skutecznie jak `-s`), a sama
        // wartość przepuszcza wiersz, którym otwiera się TASK.md T-36 — flagę, która JEST
        // podniesieniem i stoi z pustą wartością.
        if let Some(raise) = escalation_in(flag, value) {
            // Odmowa z nazwą, nie cisza. Cicha odmowa uczy użytkownika, że przelotka nie
            // działa — zamiast tego, że została zablokowana — więc wpisuje to samo jeszcze raz,
            // innym zapisem. Stąd dwa pola: `flag` to wiersz do skasowania, `escalation` to
            // powód, bez którego `--verbose-tool-output` (samo w sobie legalne) czyta się jak
            // awaria Loadouta.
            handed.refused.push(Refusal {
                flag: flag.clone(),
                escalation: raise.to_string(),
            });
            continue;
        }
        // Klucz i wartość obok siebie, w tej kolejności i po jednym razie. Flaga wklejona bez
        // wartości to albo błąd składni przy starcie, albo — gorzej — flaga, która znaczy
        // wtedy co innego.
        handed.args.push(flag.clone());
        handed.args.push(value.clone());
    }
    handed
}

/// Tłumaczy przelotkę [`VendorOptions`] na dodatkowe argumenty **jednego** vendora.
///
/// Czysta funkcja i nic więcej. Komendę buduje sterownik — `claude.rs` (T-04) i `codex.rs`
/// (T-10) — bo polityka mieszka w jednym rdzeniu, a adaptery mają po pięć linii
/// (niezmiennik 23). Nazwy vendora, której nie znamy, nie tykamy: przelotka ma przetrwać
/// vendora, którego jeszcze nie wspieramy (`DECISIONS-LOCKED.md` §D6).
///
/// 2026-08-16 — **to są te same drzwi, tylko bez raportu**: ciało jest jedno i mieszka
/// w [`vendor_args_filtered`]. To ta funkcja tłumaczy przelotkę prosto do argv i to ją podepnie
/// sterownik — jednolinijkowo i bez czytania §D6 przy tym. Filtr, który mieszkałby wyłącznie
/// w drugiej funkcji, byłby filtrem, którego bieg nigdy nie zawoła (niezmiennik 16), a dwie
/// kopie filtra to dwie odpowiedzi na jedno pytanie, z których podpięta jest zawsze starsza.
#[must_use]
pub fn vendor_args(agent: &Agent, vendor: &str) -> Vec<String> {
    vendor_args_filtered(agent, vendor).args
}

/// Przelotka agenta **scalona z przelotką kafelka** — to, z czym ten krok naprawdę pojedzie.
///
/// # 2026-08-24 (T-90) — drugi nośnik tej samej przelotki nie miał czytelnika
///
/// `AgentStep::vendor_options` jest w schemacie kroku od T3 §6b, `workflow::check` sprawdza go
/// przy zapisie i przy Starcie, a `commands::run::plan_agent` czytał wyłącznie przelotkę
/// DEFINICJI AGENTA. Człowiek dopisywał flagę na kafelku, plik ją zapisywał, walidator ją
/// przepuszczał — i proces jej nie widział. To jest ta sama martwa kontrolka, którą całe to
/// zadanie zdejmuje (niezmiennik 16), o jeden nośnik dalej, i widać ją równie źle: „vendor
/// zignorował flagę" jest z zewnątrz nieodróżnialne od „Loadout jej nie wysłał".
///
/// # Scalanie po WPISIE, nie po vendorze
///
/// To jest cała treść tej funkcji. Podmiana całej mapy jednego vendora znaczyłaby, że kafelek
/// dopisujący jedną flagę kasuje wszystkie pozostałe flagi swojego agenta — czyli że „ten jeden
/// krok chce dodatkowo X" po cichu znaczy „i zapomnij, co agent miał ustawione". To ta sama
/// algebra, którą nad resztą pól robi [`resolve`] (RFC 7396): brak klucza znaczy „dziedzicz",
/// klucz obecny znaczy „u mnie tak".
///
/// Filtr polityki stoi **za** tym scaleniem, nie przed: pytanie „czy ta flaga podnosi dial"
/// dotyczy wartości, z którą krok naprawdę pojedzie, a nie tej, którą nadpisał.
#[must_use]
pub fn passthrough_of_the_step(agent: &VendorOptions, step: &VendorOptions) -> VendorOptions {
    let mut merged = agent.clone();
    for (vendor, options) in step {
        let mine = merged.entry(vendor.clone()).or_default();
        for (flag, value) in options {
            mine.insert(flag.clone(), value.clone());
        }
    }
    merged
}

/// Przelotka tego agenta jako **gotowy fragment argv**, w kształcie, którym mówi TEN vendor.
///
/// 2026-08-23 (T-90) — DO TEGO DNIA [`vendor_args_filtered`] NIE MIAŁO W ŚCIEŻCE BIEGU ANI
/// JEDNEGO WOŁAJĄCEGO. Człowiek dopisywał flagę, plik ją zapisywał, walidator ją przepuszczał,
/// a proces jej nie widział — a „vendor zignorował flagę" jest z zewnątrz nieodróżnialne od
/// „Loadout jej nie wysłał", więc nikt się o tym nie dowiadywał (niezmiennik 16).
///
/// # Dwa kształty, bo dwie aplikacje przyjmują to inaczej
///
/// Claude Code bierze parę `--flaga wartość`; Codex bierze `-c klucz=wartość` i bierze to jako
/// opcję **globalną**, czyli przed podkomendą (`engine::drivers::codex::exec_argv`). Jeden
/// kształt dla obu przechodzi każde sprawdzenie zadane jednemu z nich i wywala drugiego przy
/// pierwszym prawdziwym biegu.
///
/// Ta funkcja stoi obok filtra, a nie w adapterze, z tego samego powodu, co cała reszta
/// polityki przelotki: adapter dostaje gotową listę i dalej nie wie, skąd przyszła
/// (niezmiennik 23). Fragment jedzie do sterownika przez `DriverConfiguration::arguments` —
/// tym samym szwem, którym jadą zatwierdzone Connections.
///
/// Wpisy odrzucone przez politykę **nie wypadają tu po cichu**: bieg pyta najpierw
/// [`passthrough_refused`] i przy niepustej odpowiedzi w ogóle nie startuje.
#[must_use]
pub fn vendor_argv(agent: &Agent, vendor: &str) -> Vec<String> {
    let handed = vendor_args_filtered(agent, vendor).args;
    if vendor != Vendor::Codex.key() {
        return handed;
    }
    // Pary `klucz, wartość` z filtra → `-c klucz=wartość`. `chunks(2)` po liście, którą filtr
    // buduje parami, więc niepełnej pary tu nie ma — a gdyby jakimś cudem była, wpis bez
    // wartości odpada, bo `-c klucz=` znaczy u tego vendora „ustaw to na pustkę".
    handed
        .chunks(2)
        .filter_map(|pair| Some((pair.first()?, pair.get(1)?)))
        .flat_map(|(key, value)| ["-c".to_owned(), format!("{key}={value}")])
        .collect()
}

/// Wpisy przelotki tego agenta, których Loadout nie poda **żadnemu** vendorowi — po jednym
/// gotowym zdaniu na wpis. Pusty wektor znaczy „cała przelotka dojedzie tam, gdzie ma".
///
/// # Odmowa startu, nigdy ciche pominięcie (D6)
///
/// Ciche pominięcie uczy człowieka, że przelotka nie działa — więc wpisuje to samo jeszcze raz,
/// innym zapisem — zamiast tego, że została zablokowana. Zdanie nazywa **wiersz do skasowania**,
/// bo bez tego odmowa jest samym niepokojem.
///
/// # Dlaczego czytamy WSZYSTKIE nazwy vendorów, a nie tylko tę, którą agent biegnie
///
/// Bo „czym ten agent biegnie" zmienia się jednym kliknięciem w formularzu, a przelotka drugiej
/// aplikacji zostaje w pliku (tak ją trzyma sekcja Agents, z rozmysłem). Wpis podnoszący dial,
/// schowany pod nazwą aplikacji, którą akurat nie biegniemy, byłby więc odmową odroczoną do dnia,
/// w którym ktoś przestawi jeden wiersz — i wtedy nikt już nie pamięta, skąd się wzięła.
///
/// Obie reguły są te same, które przy zapisie kroku workflow czyta `workflow::check`, i czytają
/// tę samą listę (niezmiennik 23). Różni się wyłącznie PODMIOT: tam pyta się o kafelek, tu
/// o definicję agenta.
#[must_use]
pub fn passthrough_refused(agent: &Agent) -> Vec<String> {
    let mut said = Vec::new();
    for (vendor, options) in &agent.vendor_options {
        for (flag, value) in options {
            let app = crate::workflow::check::vendor_name(vendor);
            if let Some(raise) = escalation_in(flag, value) {
                said.push(format!(
                    "\"{}\" has {flag} in its {app} options, and {raise} raises what an agent may \
                     do with your files. That is set on one dial and nowhere else, so Loadout \
                     stopped the run instead of starting it. Delete that line.",
                    agent.name
                ));
            } else if is_reserved(vendor, flag) {
                said.push(format!(
                    "Loadout sets {flag} itself, so \"{}\" cannot set it too. Delete it from this \
                     agent's {app} options.",
                    agent.name
                ));
            }
        }
    }
    said
}

impl Vendor {
    /// Nazwa, którą ten vendor nazywa się w przelotce `vendorOptions` i w kluczu konfiguracji.
    ///
    /// `claude`, nie `claude-code`: tak nazywa go plik agenta w przelotce, tak pyta o niego
    /// `workflow::check::reserved` i tak przedstawia się sterownik (`AgentDriver::id`). Druga
    /// pisownia dałaby przelotkę, która zapisuje się na ekranie i nie dojeżdża do nikogo.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }
}
