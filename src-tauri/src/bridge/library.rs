//! Czym lider odpowiada, gdy pyta o bibliotekę człowieka.
//!
//! # Jeden czytelnik, nie drugi obchód katalogu
//!
//! Wiersze pochodzą z **tych samych** funkcji, z których czyta je okno
//! ([`crate::commands::workflows::list_workflow_definitions_inner`],
//! [`crate::commands::agents::list_agents_inner`]). Drugi obchód katalogu byłby drugą odpowiedzią
//! na pytanie „co ten człowiek ma" (niezmiennik 13), a rozjazd między nimi nie wygląda jak błąd:
//! lider proponuje workflow, którego nie widać na liście, albo milczy o tym, co widać.
//!
//! # Pozycji uszkodzonych nie pokazujemy, i to jest decyzja
//!
//! `Definition` niesie także wpisy, których nie dało się wczytać — okno rysuje je jako problem
//! do naprawienia (T-203). Lider dostaje wyłącznie zdrowe: nie ma jak naprawić pliku, którego
//! nie rozumie parser, a wymienienie go w odpowiedzi kończy się propozycją uruchomienia czegoś,
//! co i tak odmówi. Liczba pominiętych wchodzi do odpowiedzi jako jedno zdanie — cicho pominięta
//! pozycja jest gorsza niż wymieniona, bo człowiek pyta „czemu go nie widzisz".

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::{Value, json};

use super::host::Answers;
use super::{Answer, Call};
use crate::commands::agents::list_agents_inner;
use crate::commands::chat::LEAD;
use crate::commands::workflows::{WorkflowPlace, list_workflow_definitions_inner, typable};
use crate::engine::line::Line;
use crate::ipc::{LineSink, Sent};
use crate::library::definition::Definition;

/// Workflow tego człowieka — nazwa do wpisania, tytuł, liczba kroków i półka.
///
/// Liczba kroków jest tu jedyną wartością, która na tej liście odróżnia mały workflow od takiego,
/// który uruchomi sześciu agentów i zacznie płacić — ta sama, którą z tego samego powodu pokazuje
/// podpowiedź pod wierszem wejścia.
pub fn workflows(home: &Path, project: Option<&Path>) -> Result<Value, String> {
    let catalog = list_workflow_definitions_inner(home, project)
        .map_err(|error| format!("Loadout could not read your workflows: {error}"))?;

    let mut listed: Vec<Value> = Vec::new();
    let mut unreadable = 0_usize;
    for definition in catalog {
        match definition {
            Definition::Healthy { value, .. } => {
                listed.push(json!({
                    "name": typable(&value.workflow.name),
                    "title": value.workflow.name,
                    "steps": value.workflow.steps.len(),
                    "shelf": match value.place {
                        WorkflowPlace::Project => "this project",
                        WorkflowPlace::Library => "your library",
                    },
                }));
            }
            Definition::DefinitionProblem { .. } => unreadable += 1,
        }
    }

    Ok(said(listed, unreadable, "workflow"))
}

/// Agenci tego człowieka — nazwa do wpisania i zdanie o tym, do czego są.
pub fn agents(home: &Path) -> Result<Value, String> {
    let saved = list_agents_inner(home)
        .map_err(|error| format!("Loadout could not read your agents: {error}"))?;

    let listed: Vec<Value> = saved
        .into_iter()
        .map(|agent| {
            json!({
                "name": typable(&agent.name),
                "title": agent.name,
                /* Zdanie „co to robi" jest jedyną rzeczą, która na tej liście odróżnia jednego
                 * agenta od drugiego. Puste zostaje puste — wymyślone tutaj byłoby zdaniem
                 * o agencie, którego nikt nie napisał. */
                "does": agent.summary,
                /* CZEGO TU NIE MA: vendora. Lider nie wybiera, którą aplikacją agent pracuje —
                 * to jest pole w jego definicji, ustawione przez człowieka. Wymienione tutaj
                 * byłoby zaproszeniem do rozmowy o czymś, czego lider i tak nie zmieni. */
            })
        })
        .collect();

    Ok(said(listed, 0, "agent"))
}

/// Wspólny kształt odpowiedzi: wiersze plus, gdy trzeba, jedno zdanie o tym, czego tu nie ma.
///
/// Pusta lista dostaje własne zdanie, bo `[]` czyta się dla modelu jak „nie udało się sprawdzić"
/// i kończy się liderem, który zgaduje albo powtarza wywołanie. Zdanie mówi, co jest, i nazywa
/// następny ruch człowieka (DESIGN §8).
fn said(listed: Vec<Value>, unreadable: usize, kind: &str) -> Value {
    let count = listed.len();
    let note = match (count, unreadable) {
        (0, 0) => Some(format!(
            "There are no {kind}s saved yet. The person creates them in the {} screen.",
            screen_of(kind)
        )),
        (_, 0) => None,
        (0, skipped) => Some(format!(
            "There are no {kind}s Loadout could read, and {skipped} file(s) it could not. The \
             person can fix them in the {} screen.",
            screen_of(kind)
        )),
        (_, skipped) => Some(format!(
            "{skipped} more file(s) could not be read, so they are not on this list. The person \
             can fix them in the {} screen.",
            screen_of(kind)
        )),
    };

    let mut answer = serde_json::Map::new();
    answer.insert("count".to_owned(), Value::from(count));
    /* Wiersze WCHODZĄ tu przez wartość, a nie przez referencję w `json!`: inaczej `listed` jest
     * argumentem, którego ta funkcja nie konsumuje, i `clippy::needless_pass_by_value` ma rację. */
    answer.insert(kind_key(kind).to_owned(), Value::Array(listed));
    if let Some(note) = note {
        answer.insert("note".to_owned(), Value::String(note));
    }
    Value::Object(answer)
}

/// Pod jakim kluczem jadą wiersze tego rodzaju.
const fn kind_key(kind: &str) -> &'static str {
    match kind.as_bytes() {
        b"agent" => "agents",
        _ => "workflows",
    }
}

/// Który ekran otwiera człowiek, żeby to naprawić albo dodać. Słowa z interfejsu, nie z kodu.
const fn screen_of(kind: &str) -> &'static str {
    match kind.as_bytes() {
        b"agent" => "Agents",
        _ => "Workflows",
    }
}

/// Pytanie tej rozmowy, które czeka na człowieka. **Najwyżej jedno naraz.**
///
/// # Dlaczego jedno wystarczy, i to nie jest uproszczenie
///
/// Wywołanie narzędzia BLOKUJE turę agenta, a tury jednej rozmowy porządkuje actor
/// (`Conversation`). Lider stojący na pytaniu nie zadaje więc drugiego — nie dlatego, że mu
/// zabraniamy, tylko dlatego, że nie ma czym. Kolejka byłaby tu maszynerią na stan, który nie
/// może zajść (`AGENTS.md`, tabela zakazów: „nie pisz artefaktu, którego nikt nie czyta").
///
/// # Dlaczego pamiętamy, KTO pyta
///
/// Bo w jednym strumieniu stoją dwa różne pytania: to od lidera i to z kafelka kontrolnego biegu.
/// Człowiek odpowiada na jedno z nich, a okno nie ma jak rozstrzygnąć, na które — więc
/// rozstrzyga to strona, która wie: odpowiedź trafia do lidera **tylko wtedy**, gdy podpis się
/// zgadza. Bez tego odpowiedź na punkt kontrolny odblokowywałaby przy okazji cudze pytanie,
/// zdaniem, które go nie dotyczy.
#[derive(Debug, Default)]
pub struct Waiting {
    /// Podpis pytającego i kanał na odpowiedź. `None` znaczy „nikt nie czeka".
    ///
    /// `std::sync::Mutex` i nigdy trzymany przez `await` (niezmiennik 8): pod nim są wyłącznie
    /// `take` i `replace`.
    slot: Mutex<Option<(String, Arc<()>, tokio::sync::oneshot::Sender<String>)>>,
}

impl Waiting {
    /// Odkłada pytanie i oddaje kanał, którym przyjdzie odpowiedź.
    ///
    /// Nadpisuje to, co stało wcześniej: nadawca porzucony tutaj zamyka swój kanał, więc
    /// poprzednie pytanie dostaje odmowę zamiast ciszy. Cisza byłaby turą wiszącą do końca
    /// rozmowy.
    fn park(&self, asker: String) -> (Arc<()>, tokio::sync::oneshot::Receiver<String>) {
        let (say, hear) = tokio::sync::oneshot::channel();
        let ticket = Arc::new(());
        *self.slot.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((asker, Arc::clone(&ticket), say));
        (ticket, hear)
    }

    /// Wycofuje pytanie, którego nie udało się pokazać.
    ///
    /// Ticket wskazuje dokładnie TEN `park`, nie tylko tego samego pytającego. Dwa połączenia
    /// jednego Leada mogą pytać równolegle; starsze `Dropped` nie ma prawa wycofać nowszego,
    /// widocznego pytania tylko dlatego, że oba mają podpis `Lead`.
    fn withdraw(&self, ticket: &Arc<()>) -> bool {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        match slot.take() {
            Some((_waiting, parked, _say)) if Arc::ptr_eq(&parked, ticket) => true,
            other => {
                *slot = other;
                false
            }
        }
    }

    /// Odkłada pytanie tak, jak zrobiłby to czasownik — **wyłącznie dla kryteriów**.
    ///
    /// Istnieje, bo gałąź „nikt nie odpowiedział" w [`Desk::ask`] jest osiągalna dokładnie jedną
    /// drogą: kolejnym `park`, które zastępuje poprzednie. Bez tej metody nie da się jej osądzić
    /// spoza modułu, a gałąź bez sędziego jest gałęzią, o której wiadomo tylko tyle, że się
    /// kompiluje.
    ///
    /// Nazwa mówi wprost, do czego to służy, bo `quick-wired.sh` sądzi każde nowe `pub fn` — i ma
    /// prawo zapytać, kto to woła.
    pub fn park_for_test(&self, asker: &str) -> tokio::sync::oneshot::Receiver<String> {
        self.park(asker.to_owned()).1
    }

    /// Czy podpisane pytanie stoi w slocie — wyłącznie do synchronizacji kryterium mostu.
    #[must_use]
    pub fn is_waiting_for_test(&self, asker: &str) -> bool {
        self.slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|(waiting, _ticket, _say)| waiting == asker)
    }

    /// Człowiek odpowiedział. `false`, kiedy ten podpis na nic nie czekał.
    ///
    /// Wynik jest tu treścią, nie uprzejmością: okno woła to przy KAŻDEJ odpowiedzi, także tej
    /// na punkt kontrolny biegu, i musi wiedzieć, czy poszła gdzie indziej.
    pub fn answer(&self, asker: &str, said: String) -> bool {
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        match slot.take() {
            Some((waiting, _ticket, say)) if waiting == asker => say.send(said).is_ok(),
            /* CUDZE PYTANIE WRACA NA MIEJSCE. Zabrane stąd zostawiłoby lidera czekającego bez
             * końca na odpowiedź, którą człowiek dał komu innemu. */
            other => {
                *slot = other;
                false
            }
        }
    }
}

/// Biurko lidera: kto odpowiada na czasowniki biblioteczne.
///
/// # Dlaczego to jest typ, a nie zamknięcie
///
/// Bo most trzyma go przez całą sesję (`Arc<dyn Answers>`), a odpowiedzi muszą pochodzić z tych
/// samych dwóch ścieżek przez cały czas jej trwania. Zamknięcie łapiące ścieżki byłoby tym samym
/// z mniejszą liczbą miejsc, w których widać, CO ono właściwie wie o człowieku.
///
/// # Czego to biurko NIE MA
///
/// Dostępu do biegów. Nie zna `RunDeps`, nie widzi `RunControl` i nie ma jak niczego uruchomić —
/// tak samo jak `commands::chat`. Czasownik, który startuje bieg, dojdzie tu dopiero razem
/// z drogą przez okno, bo tylko okno umie zbudować kanał wierszy.
#[derive(Clone)]
pub struct Desk {
    /// Strumień tej rozmowy — tędy wiersz startu trafia na ekran człowieka.
    ///
    /// `None` znaczy „ta rozmowa nie ma jeszcze kanału do okna", i wtedy `start_workflow`
    /// **odmawia**. Bieg zaczęty bez śladu na ekranie jest dokładnie tą awarią, przed którą stoi
    /// całe „rusza samo": jedyną ochroną człowieka jest to, że widzi, co ruszyło.
    lines: Option<Arc<Mutex<LineSink>>>,
    /// Pytanie tej rozmowy, które czeka na człowieka. Współdzielone z rejestrem wątków, bo to
    /// on dostaje odpowiedź z okna.
    waiting: Arc<Waiting>,
    /// `~/.loadout` — biblioteka człowieka. `None` znaczy „okno jeszcze nie powiedziało, gdzie
    /// ona jest", i wtedy odpowiedzią jest zdanie, nie zgadnięta ścieżka z `HOME`.
    home: Option<PathBuf>,
    /// Folder zakresu, w którym stoi ta rozmowa. Workflow tego projektu przesłania biblioteczne —
    /// tą samą regułą, którą widzi okno.
    project: PathBuf,
}

/* RĘCZNIE, bo `LineSink` nie jest `Debug` i nie ma być. Pokazujemy dwa fakty, które cokolwiek
 * znaczą w dzienniku: gdzie to biurko patrzy i czy ma jak cokolwiek pokazać. */
impl std::fmt::Debug for Desk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Desk")
            .field("project", &self.project)
            .field("can_show", &self.lines.is_some())
            .finish_non_exhaustive()
    }
}

impl Desk {
    /// Biurko dla rozmowy stojącej w tym folderze, bez drogi na ekran.
    ///
    /// Czasowniki czytające działają; `start_workflow` odmawia, bo nie miałby jak pokazać, co
    /// ruszyło.
    #[must_use]
    pub fn at(home: Option<PathBuf>, project: PathBuf) -> Self {
        Self {
            home,
            project,
            lines: None,
            waiting: Arc::new(Waiting::default()),
        }
    }

    /// To samo biurko, ale z kanałem, którym przyjdzie odpowiedź człowieka.
    #[must_use]
    pub fn hearing(mut self, waiting: Arc<Waiting>) -> Self {
        self.waiting = waiting;
        self
    }

    /// To samo biurko, ale z drogą na ekran tej rozmowy.
    #[must_use]
    pub fn showing(mut self, lines: Arc<Mutex<LineSink>>) -> Self {
        self.lines = Some(lines);
        self
    }

    /// Zaczyna bieg: sprawdza, co da się sprawdzić TUTAJ, i kładzie wiersz startu na ekran.
    ///
    /// # Dlaczego rozstrzygnięcie jest tu, a nie w oknie
    ///
    /// Bo `launchRun` po tamtej stronie rozwiązuje się dopiero **z końcem biegu** (`launch.ts`:
    /// „`start` rozwiązuje się dopiero z końcem biegu"), więc okno nie ma czym zameldować SAMEGO
    /// startu. Czekanie na nie byłoby wyścigiem z zegarem wpisanym w kontrakt narzędzia.
    ///
    /// Tu odpowiadamy na jedyne pytanie, które trzeba rozstrzygnąć natychmiast: **czy ten
    /// workflow w ogóle istnieje**. Odmowa nazywa wtedy nazwy, które istnieją — bo lista, której
    /// lider nie widzi, jest zagadką, a zagadka kończy się zgadywaniem nazw.
    ///
    /// # Czego ta odpowiedź NIE OBIECUJE, i to jest zapisane w jej treści
    ///
    /// Że bieg ruszył. Okno może jeszcze odmówić — najczęściej dlatego, że w tym zakresie coś już
    /// biegnie — a to zdanie ląduje **w strumieniu**, czyli tam, gdzie człowiek patrzy. Lider
    /// dowie się o nim przy następnej turze, jeśli człowiek zapyta. To jest znany dług tej wersji
    /// i ma go zamknąć osobna droga meldunku, nie zegar w tym miejscu.
    /// Pyta człowieka i **czeka na odpowiedź**, blokując turę agenta.
    ///
    /// # Po co to istnieje
    ///
    /// Zgłoszenie właściciela 2026-08-29: „w trakcie planowania nie mamy wgl pytan od agentow".
    /// Zmierzone tego samego dnia: vendor w trybie bez terminala **nie daje** narzędzia
    /// `AskUserQuestion` — ani domyślnie, ani po wypisaniu w `--tools`. Agent nie miał więc żadnej
    /// drogi, żeby zapytać, i jedynym, co mu zostawało, było zgadnąć.
    ///
    /// # Dlaczego to musi BLOKOWAĆ, a nie tylko zapytać
    ///
    /// Bo pytanie, po którym agent pracuje dalej, nie jest pytaniem. Rozważona i odrzucona wersja
    /// (umowa w prozie: agent pisze pytanie, dostaje odpowiedź następną turą) kończy turę PRZED
    /// odpowiedzią — więc agent zdąży zrobić robotę, zanim człowiek kliknie. Zmierzone
    /// 2026-08-29 na żywym vendorze: wywołanie narzędzia stoi tyle, ile trzeba, a odpowiedź
    /// wraca **do kontekstu tej samej tury**.
    ///
    /// # Bez sufitu czasu, świadomie
    ///
    /// Zablokowane wywołanie nie pali tokenów — zmierzone: sześć sekund snu nie kosztowało nic
    /// poza czasem. Kafelek kontrolny parkuje bieg bezterminowo już dziś, więc trzeci sposób
    /// kończenia pytania obok odpowiedzi i zamknięcia rozmowy byłby trzecim zdaniem na ekranie
    /// bez trzeciego powodu. Zamknięcie terminalu porzuca nadawcę, więc czekanie kończy się
    /// odmową — nigdy ciszą.
    ///
    /// # Czego tu NIE MA
    ///
    /// Ani jednego zdania każącego zapytać. Narzędzie jest DOSTĘPNE, nigdy WYMAGANE — wymaganie
    /// właściciela z 2026-08-30, dosłownie: „nie chcę też aby na sztywno było żeby agent lub
    /// ktokolwiek zadawał 2-3 pytania, wszystko zależy od analiz i potrzeb".
    async fn ask(&self, input: &Value) -> Answer {
        let Some(lines) = self.lines.as_ref() else {
            return Answer::Refused(
                "Loadout has no stream open for this conversation, so a question would appear \
                 nowhere. Answer it yourself this time."
                    .to_owned(),
            );
        };
        let Some(question) = input
            .get("question")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|question| !question.is_empty())
        else {
            return Answer::Refused("Say what you want to ask.".to_owned());
        };

        /* WARIANTY SĄ PODPOWIEDZIĄ, NIE PRZYMUSEM. Pusta lista znaczy „odpowiedz własnymi
         * słowami" i tak rysuje ją okno od pierwszego dnia — pole tekstowe stoi tam obok
         * przycisków, nie zamiast nich. */
        let options: Vec<String> = input
            .get("options")
            .and_then(Value::as_array)
            .map(|listed| {
                listed
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .filter(|option| !option.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();

        /* NASŁUCH PRZED PYTANIEM. Odwrotna kolejność ma okno, w którym człowiek odpowiada
         * szybciej, niż zdążymy odłożyć kanał — a wtedy odpowiedź trafia w nikogo i tura stoi
         * już na zawsze. Ten sam powód i ta sama kolejność, co przy `wait_for_a_person`. */
        let (ticket, hear) = self.waiting.park(LEAD.to_owned());

        let shown = lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(Line::Asked {
                agent: LEAD.to_owned(),
                text: question.to_owned(),
                options,
            });

        /* PYTANIE, KTÓRE NIE DOTARŁO NA EKRAN, NIE MOŻE BLOKOWAĆ TURY. Zmierzone 2026-09-01:
         * pełna kolejka zwraca `Dropped`, lecz wcześniejsza wersja ignorowała ten wynik i czekała
         * na odpowiedź bezterminowo. Człowiek nie miał czego kliknąć, więc wyglądało to jak Lead,
         * który po prostu przestał odpowiadać. */
        if shown == Sent::Dropped {
            let _withdrew = self.waiting.withdraw(&ticket);
            return Answer::Refused(
                "Loadout could not show that question on the screen. Continue without waiting for \
                 an answer, or ask it in your reply."
                    .to_owned(),
            );
        }

        match hear.await {
            Ok(said) => Answer::Ok(Value::String(said)),
            /* Kanał zamknięty znaczy „rozmowa zeszła, zanim ktoś odpowiedział". Zdanie jedzie do
             * modelu jako wynik narzędzia, żeby lider wiedział, że nie dostał odpowiedzi —
             * cisza wyglądałaby dla niego jak odpowiedź pusta. */
            Err(_) => Answer::Refused(
                "Nobody answered — this conversation closed before they could.".to_owned(),
            ),
        }
    }

    fn start(&self, home: &Path, input: &Value) -> Result<Value, String> {
        let Some(lines) = self.lines.as_ref() else {
            return Err(
                "Loadout has no stream open for this conversation, so a run would start \
                        with nothing on screen. Reopen the work screen and ask again."
                    .to_owned(),
            );
        };

        let Some(wanted) = input.get("workflow").and_then(Value::as_str) else {
            return Err(
                "Say which workflow to start. Use the name exactly as list_workflows \
                        gave it."
                    .to_owned(),
            );
        };
        let wanted = typable(wanted);

        let listed = workflows(home, Some(&self.project))?;
        let rows = listed
            .get("workflows")
            .and_then(Value::as_array)
            .map_or_else(Vec::new, Clone::clone);
        let names: Vec<&str> = rows
            .iter()
            .filter_map(|row| row.get("name").and_then(Value::as_str))
            .collect();

        let Some(found) = rows
            .iter()
            .find(|row| row.get("name").and_then(Value::as_str) == Some(wanted.as_str()))
        else {
            /* WYMIENIA NAZWY, i to jest cała treść tej odmowy. „Unknown workflow" zostawia lidera
             * dokładnie tam, gdzie był, a nazw, których nie widzi, nie ma jak zgadnąć — więc
             * następnym ruchem jest zgadywanie kolejnej. To samo zdanie i ten sam powód, co
             * `noSuchWorkflow` po stronie okna. */
            return Err(format!(
                "There is no workflow called \"{wanted}\". These are the ones this person has: {}.",
                names.join(", ")
            ));
        };
        let title = found
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(wanted.as_str());

        let task = input
            .get("task")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|task| !task.is_empty());

        /* KOMENDA ZNAK W ZNAK TAKA, JAKĄ WPISAŁBY CZŁOWIEK. Okno rozbiera ją tą samą funkcją,
         * którą rozbiera Enter (`startFromLine`), więc „który workflow, ile naraz, w którym
         * folderze" ma dalej JEDNĄ odpowiedź (niezmiennik 23). Druga droga startu rozjechałaby
         * się po cichu: liczba „ile naraz" byłaby wczytywana, logowana i inna. */
        let command = match task {
            Some(task) => format!("/run {wanted} {task}"),
            None => format!("/run {wanted}"),
        };
        let text = match task {
            Some(task) => format!("Starting {title} — {task}"),
            None => format!("Starting {title}"),
        };

        let line = Line::Suggested {
            agent: LEAD.to_owned(),
            text,
            auto: true,
            command,
        };
        let _ = lines
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .send(line);

        Ok(json!({
            "asked": true,
            "workflow": wanted,
            "note": "Loadout is starting it. What happens next — including a refusal, if it \
                     cannot start right now — appears in the stream this person is watching.",
        }))
    }
}

#[async_trait]
impl Answers for Desk {
    async fn answer(&self, call: Call) -> Answer {
        let Some(home) = self.home.as_deref() else {
            /* ZDANIE, NIE ZGADNIĘTA ŚCIEŻKA. Biblioteka odgadnięta tutaj z `HOME` znaczyłaby, że
             * każde kryterium rozmawia z prawdziwą biblioteką człowieka — ten sam wybór i ten sam
             * powód, co przy `ThreadRegistry::library`. */
            return Answer::Refused(
                "Loadout does not know where this person's library is yet, so there is nothing to \
                 list. Reopen the work screen and try again."
                    .to_owned(),
            );
        };

        let said = match call.call.as_str() {
            "list_workflows" => workflows(home, Some(&self.project)),
            "list_agents" => agents(home),
            "start_workflow" => self.start(home, &call.input),
            /* CZEKANIE JEST TU, A NIE W TABELI: to jedyny czasownik, który nie odpowiada od razu,
             * i dlatego jako jedyny ma własną gałąź poza `match`em wartości. */
            "ask_the_person" => return self.ask(&call.input).await,
            /* NAZYWA CZASOWNIK, którego nie zna. Odmowa bez nazwy zostawia model przy powtarzaniu
             * tego samego wywołania, bo nie ma z czego wywnioskować, że pomylił nazwę. */
            other => Err(format!(
                "Loadout has no verb called {other}. Ask for tools/list again to see what it has."
            )),
        };

        match said {
            Ok(value) => Answer::Ok(value),
            Err(sentence) => Answer::Refused(sentence),
        }
    }
}
