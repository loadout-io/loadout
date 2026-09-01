//! Format pliku workflow — typy z T3 §3.1 i ani jednego pola więcej.
//!
//! To jest jedyna rzecz w Loadoucie, którą użytkownik może **stracić**: plik da się zmergować
//! gitem, poprawić ręcznie w edytorze i otworzyć raz nowszym buildem, raz starszym. Stąd dwie
//! decyzje, które w tym module wyglądają na drobiazgi, a są całą jego treścią:
//!
//! 1. **Nigdzie `deny_unknown_fields`** (T3 §8.4). Plik agenta pisze człowiek i literówka ma
//!    zaboleć od razu — tam `deny_unknown_fields` jest wymagane (T-11). Plik workflow pisze
//!    maszyna i ma przeżyć wersję, której nie zna, więc tutaj ta sama flaga byłaby błędem.
//! 2. **`#[serde(flatten)] extra` na każdym kroku.** Bez tego starszy build wczytuje plik
//!    z polem, którego nie zna, zapisuje go z powrotem i **kasuje pracę nowszego builda bez
//!    jednego komunikatu**. T3 §3.2 uruchomił to na tej maszynie: wewnętrznie tagowany enum
//!    z `flatten` przepuszcza nieznane klucze bez straty, razem z typem liczbowym.
//!
//! Czego tu nie ma i nie będzie: portów, typu krawędzi, węzła-grupy. Trzeci rodzaj kafelka jest
//! tym, co zabiło poprzedniego prototypu.
//!
//! 2026-08-19 — PĘTLA WESZŁA, i to jest zmiana rozstrzygnięcia, nie przeoczenie. Stało tu
//! „strzałka znaczy »po« i nic więcej (T3 §6.2)", a właściciel poprosił o kształt, którego bez
//! powrotu nie da się wyrazić: implementer wysyła do testera, tester zdaje raport, `fail` wraca
//! do implementera, `pass` puszcza bieg dalej. Powrót jest **polem na strzałce**
//! ([`Link::max_turns`]), a nie nowym rodzajem kafelka ani węzłem-grupą — czyli dokładnie tą
//! rzeczą, przed którą broni akapit wyżej, nie została dodana. Sufit tur jest w schemacie
//! obowiązkowy, więc pętli bez końca nie da się zapisać.
//! Projekt: `docs/superpowers/specs/2026-08-19-petla-z-limitem-tur-design.md`.

use std::collections::BTreeMap;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

pub mod check;
pub mod file;
pub mod roster;
pub mod unroll;

/// Skok siatki płótna w pikselach [T3 §8.2 reguła 1].
///
/// Pozycje zapisujemy jako całkowite wielokrotności tej liczby, bo `240.00000001` brudzi diff
/// przy każdym najechaniu myszą, a `240` nie brudzi go nigdy.
pub const GRID: f64 = 24.0;

/// Cały plik: `~/.loadout/workflows/<slug>.json`.
///
/// `format` jest **pierwszym** kluczem i czyta się go przed deserializacją całej reszty —
/// odmowa-w-przód z [`file::load`] musi zadziałać także wtedy, gdy nowszy build zmienił
/// kształt kroku tak, że ten build nie umie go wczytać.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowFile {
    /// Wersja formatu. Podnoszona **wyłącznie** przy zmianie łamiącej (T3 §8.4).
    pub format: u32,
    /// Stabilny identyfikator, nigdy nie zmieniany przy zmianie nazwy.
    pub id: String,
    /// To, co użytkownik wpisał: „Ship a feature".
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Kolejność **wstawiania**, nigdy przesortowana [T3 §8.2 reguła 2]: sortowanie
    /// topologiczne czyta się ładniej i przy wstawieniu kroku u góry przepisuje cały plik.
    pub steps: Vec<Step>,
    pub links: Vec<Link>,
    /// Klucze, których ta wersja nie zna — patrz `extra` na kroku.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Dwa rodzaje kafelka **wobec vendorów**. To jest cała lista i ma taka zostać
/// (D6, ARCHITECTURE §6b).
///
/// 2026-08-19 — DOCHODZI TRZECI WARIANT, KTÓRY VENDORA NIE ZNA, i to jest poprawka zdania,
/// nie jego skasowanie. D6 brzmi w całości: „wszystko, co **vendor** wprowadzi, konfigurujemy
/// per agent — nigdy jako nowy typ węzła", a konsekwencja z `ARCHITECTURE.md` §6b ma dopisany
/// zakres: „liczba rodzajów kafelka zostaje dwa **niezależnie od tego, ile funkcji dowiozą
/// vendorzy**". D6 broni płótna przed powtarzaniem funkcji Claude'a i Codeksa.
///
/// [`Step::Check`] nie jest funkcją żadnego vendora — jest mechanizmem Loadouta, wymienionym
/// **z nazwy** w tabeli D7 („bramka (verify.sh) → krok typu »sprawdź« — uruchamia twoje checki").
/// Obie decyzje zapisano tego samego dnia i obie są zamknięte, więc sprzeczności nie ma.
///
/// I drugie rozstrzygnięcie, ważniejsze dla silnika: ten wariant nazywa **rodzaj sterownika**,
/// dokładnie tak jak `claude` stoi obok `codex` — nie ETAP biegu. Niezmiennik 27 zakazuje
/// `if review_enabled` i każdego innego warunku nazywającego etap; test rozróżniający jest
/// jednozdaniowy: *czy da się zapisać graf, w którym ten krok stoi w innym miejscu albo nie
/// stoi wcale?* Dla kroku „sprawdź" — tak, trywialnie, bo kolejność mieszka wyłącznie w grafie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Step {
    Agent(AgentStep),
    Checkpoint(CheckpointStep),
    Check(CheckStep),
    /// Uruchom i zostaw — proces, który **przeżywa swój krok**.
    Serve(ServeStep),
}

impl Step {
    /// Nazwa kroku, tak jak stoi na kafelku — to ona jedzie do człowieka, nigdy identyfikator
    /// (niezmiennik 14).
    /// Co zrobić z robotą, kiedy ten krok nie przejdzie.
    ///
    /// Kafelek kontrolny i „uruchom i zostaw" oddają [`WhenItFails::Stop`] i to jest treść, nie
    /// zaniedbanie: pierwszy JEST pytaniem do człowieka (drugie pytanie po nim byłoby tym samym
    /// pytaniem dwa razy), a drugi nie orzeka o niczyjej robocie — odmawia przy starcie albo
    /// stawia proces i schodzi z drogi.
    #[must_use]
    pub const fn when_it_fails(&self) -> WhenItFails {
        match self {
            Self::Agent(one) => one.when_it_fails,
            Self::Check(one) => one.when_it_fails,
            Self::Checkpoint(_) | Self::Serve(_) => WhenItFails::Stop,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Agent(one) => one.name.as_str(),
            Self::Checkpoint(one) => one.name.as_str(),
            Self::Check(one) => one.name.as_str(),
            Self::Serve(one) => one.name.as_str(),
        }
    }

    /// Identyfikator kroku, niezależnie od jego rodzaju.
    ///
    /// 2026-08-22 — JEDNO MIEJSCE ZAMIAST TRZECH. Ten sam rozjazd po rodzajach był dotąd
    /// przepisany w `unroll::key_of` i wołany w kilku innych, a każda kopia jest kopią, którą
    /// czwarty wariant kroku zostawi po cichu nieaktualną.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Agent(one) => one.id.as_str(),
            Self::Checkpoint(one) => one.id.as_str(),
            Self::Check(one) => one.id.as_str(),
            Self::Serve(one) => one.id.as_str(),
        }
    }
}

/// Krok, który uruchamia agenta.
///
/// Vendora ani modelu tu nie ma: krok nazywa **agenta**, a vendor, model, narzędzia i tryb
/// uprawnień mieszkają w jego definicji (T3 §3.1). Zmiana modelu dzieje się raz, nie w sześciu
/// kafelkach.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStep {
    pub id: String,
    /// Nazwa widoczna na kafelku — i **to samo zdanie**, którym uwaga z `check()` nazywa winnego.
    pub name: String,
    /// Id zapisanego agenta (`library::agents`).
    pub agent: String,
    /// Patch RFC 7396 nad definicją agenta: brak klucza znaczy „dziedzicz" [T4 §5.1].
    /// `{}` dla kroku nietkniętego.
    ///
    /// 2026-08-16 — surowa mapa, nie typ `Overrides`: typ, `resolve()` i `capture()` należą do
    /// T-11 (`library::agents`), którego w tym drzewie jeszcze nie ma. Ten moduł patcha
    /// **nie scala** — przenosi go z pliku do T-11 i z powrotem. Przy scalaniu T-11 to pole
    /// dostaje jego typ; drugiej implementacji scalania nie piszemy (TASK.md, rozstrzygnięcie 1).
    #[serde(default)]
    pub overrides: Map<String, Value>,
    /// Przelotka na opcje vendora: `"claude" -> {flaga: wartość}` (ARCHITECTURE §6b, D6).
    /// Loadout nie interpretuje zawartości — sprawdza tylko, czy nie podnosi tego, co ustawia
    /// sam. `BTreeMap`, nie `Value`: kolejność ma być deterministyczna, żeby zapis nie
    /// produkował fałszywych różnic w gicie.
    ///
    /// Jedyne pole spoza `Option`, które przy zapisie **znika, gdy jest puste**. `overrides: {}`
    /// niesie informację („ten krok nie jest nadpisany", T4 §5.1); pusta przelotka nie niesie
    /// żadnej i byłaby wierszem szumu w każdym kroku każdego pliku.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vendor_options: BTreeMap<String, BTreeMap<String, String>>,
    /// Ile identycznych sesji naraz, 1–8 [T3 §4.4]. Osiem jednoczesnych sesji na prawdziwej
    /// maszynie to już dużo.
    #[serde(default = "one_copy")]
    pub copies: u8,
    /// Prompt, zwykły tekst. `{{copy}}` i `{{copies}}` podstawia silnik [T3 §4.3].
    #[serde(default)]
    pub instructions: String,
    #[serde(default)]
    pub skills: Skills,
    /// Co ten kafelek pożycza z repozytorium, w którym pracuje bieg — i **domyślnie nic**.
    ///
    /// WŁASNOŚĆ KROKU, NIE BIEGU, i to jest cała treść tego pola. „Pożycz rolę `backend-dev`
    /// z tego repozytorium" jest własnością kafelka dokładnie tak samo, jak wybór agenta:
    /// dwa kafelki jednego biegu mogą chcieć dwóch różnych rzeczy, a jedno pole na bieg
    /// zamieniłoby ten wybór w przełącznik „to repozytorium: tak albo nie".
    ///
    /// PRZY ZAPISIE ZNIKA, GDY JEST PUSTE — ten sam powód, co przy [`AgentStep::vendor_options`]:
    /// `"borrow": {{}}` dopisane do KAŻDEGO kroku KAŻDEGO pliku przepisałoby przy pierwszym
    /// zapisie wszystkie istniejące workflow, a nie niesie ani jednej informacji ponad swój brak.
    /// `#[serde(default)]` z drugiej strony: plik zapisany, zanim to pole istniało, ma się
    /// wczytać bez jednej zmiany.
    #[serde(default, skip_serializing_if = "Borrow::is_nothing")]
    pub borrow: Borrow,
    #[serde(default)]
    pub folder: Folder,
    /// Zostaje w schemacie bez kontrolki w UI: czyta je T-16, a edytor pól formularza jest
    /// odłożony (T3 §7.1).
    #[serde(default)]
    pub handover: Handover,
    /// Co zrobić z robotą, kiedy ten krok nie przejdzie. Brak klucza znaczy [`WhenItFails::Stop`],
    /// czyli dokładnie to, co robił każdy krok do 2026-08-23.
    ///
    /// PRZY ZAPISIE ZNIKA, GDY JEST DOMYŚLNE — ten sam powód, co przy [`AgentStep::vendor_options`]
    /// i dokładnie z tego samego pomiaru: `"whenItFails": "stop"` dopisane do KAŻDEGO kroku
    /// KAŻDEGO pliku przepisałoby przy pierwszym zapisie wszystkie istniejące workflow, a nie
    /// niesie ani jednej informacji ponad swój brak. `overrides: {}` zostaje, bo tam pusta mapa
    /// znaczy „ten kafelek nie jest nadpisany" — tu domyślna wartość nie znaczy nic.
    #[serde(default, skip_serializing_if = "WhenItFails::is_the_default")]
    pub when_it_fails: WhenItFails,
    /// Brak klucza znaczy `{"x":0,"y":0}`: plik poprawiony ręcznie ma się wczytać, a nie odmówić
    /// z powodu pozycji, którą płótno i tak umie ustawić.
    #[serde(default)]
    pub at: Point,
    /// Klucze, których **ta** wersja nie zna.
    ///
    /// 2026-08-16 — powód jest jednozdaniowy: starszy build nie kasuje pola nowszego. Bez tego
    /// jedno otwarcie w starszym Loadoucie zjada konfigurację, której nowszy build nie umie
    /// odtworzyć, i nie zostawia po tym ani jednego komunikatu [T3 §3.2, uruchomione].
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Domyślna liczba kopii. Funkcja, bo `#[serde(default)]` dla `u8` dałoby zero, a zero kopii
/// to krok, który nigdy nie biegnie.
fn one_copy() -> u8 {
    1
}

/// Co ten kafelek bierze z repozytorium, w którym pracuje bieg.
///
/// Trzy pola, bo to są trzy różne rzeczy o dwóch różnych drogach do procesu: umiejętności jadą
/// katalogiem pluginu w argv, a plik roli i opis podagenta — tekstem w prompcie. Jedna wspólna
/// lista nazw zlepiłaby je w jedno i pierwsza pomyłka wsadziłaby treść do argv, którą widzi `ps`
/// każdego użytkownika maszyny (niezmiennik 9).
///
/// TE SAME TRZY POLA CO `inherit::wire::Chosen`, i to nie jest duplikat przez nieuwagę: tamten
/// typ jest pytaniem zadawanym cudzemu repozytorium, a ten jest **kształtem pliku na dysku**,
/// który ma się otwierać także za rok. Ten sam podział stoi w tym module przy [`Skills`] wobec
/// `crate::skills`, i z tego samego powodu — jedno przełożenie mieszka w `commands::run`.
///
/// Klucz podagenta nazywa się `agent`, a nie `subagent`, bo tak nazywa się półka, z której
/// pochodzi (`<projekt>/.claude/agents/`). Nazwa w pliku ma odpowiadać temu, co człowiek widzi
/// w cudzym repozytorium, a nie temu, jak my o tym mówimy w środku.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Borrow {
    /// Nazwy katalogów spod `<projekt>/.claude/skills/`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    /// Nazwa pliku roli spod `<projekt>/.claude/learnings/`, bez rozszerzenia. Do promptu
    /// wchodzi z niego **wyłącznie** sekcja `## Recurring patterns`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub learnings: Option<String>,
    /// Nazwa podagenta spod `<projekt>/.claude/agents/`, bez rozszerzenia. Do promptu wchodzi
    /// z niego **wyłącznie ciało**; front-matter jest granicą maszynerii.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

impl Borrow {
    /// Czy ten kafelek nie pożycza niczego.
    ///
    /// Bierze referencję, bo tego żąda `skip_serializing_if` — ta sama linia, z tego samego
    /// powodu, stoi przy [`WhenItFails::is_the_default`].
    #[must_use]
    pub fn is_nothing(&self) -> bool {
        self.skills.is_empty() && self.learnings.is_none() && self.agent.is_none()
    }
}

/// Krok, który zatrzymuje bieg i pyta człowieka [T3 §6.1 punkt 5].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointStep {
    pub id: String,
    pub name: String,
    /// „Does the plan look right?"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    /// Jak `AgentStep::at`.
    #[serde(default)]
    pub at: Point,
    /// Jak `AgentStep::extra`.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Krok, który uruchamia **naszą** komendę i sam orzeka, czy przeszła.
///
/// Ani modelu, ani promptu, ani agenta: to jest cała różnica między tym kafelkiem a krokiem
/// agenta o instrukcji „uruchom `./verify.sh full` i powiedz, czy przeszło". Tamten waliduje
/// się, biegnie, mówi `checks passed` — i sprzedaje jedyne rozróżnienie, dla którego ten produkt
/// powstał: co agent POWIEDZIAŁ kontra co się STAŁO
/// (`docs/research/projects/00-SYNTHESIS.md` §2.1, `docs/harness-as-workflow.md` ustalenie U-1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckStep {
    pub id: String,
    /// Nazwa widoczna na kafelku — „Run the checks". To ona pada w uwagach z [`check`].
    pub name: String,
    /// Wiersz powłoki, dosłownie jak wpisał go człowiek: `./verify.sh full && npm test`.
    ///
    /// `#[serde(default)]` jest ROZSTRZYGNIĘCIEM, nie niedbałością, i ten sam powód stoi przy
    /// [`AgentStep::at`]: plik poprawiony ręcznie ma się **wczytać** i dostać zdanie z `check()`
    /// przy kafelku, a nie odbić się o błąd serde, którego użytkownik nie umie umiejscowić
    /// (T3 §8.4). Odmowa pada przy **zapisie**, a nie przy odczycie.
    #[serde(default)]
    pub command: String,
    /// Wzorzec dowodu: zwykły tekst z jednym metaznakiem, `(\d+)` znaczy „co najmniej jedna
    /// cyfra". Ta sama notacja, którą człowiek pisze w linii `expect:` naszej własnej bramki
    /// (`AGENTS.md` §2a punkt 4) — jedna notacja, jedno znaczenie.
    ///
    /// Pole jest OBOWIĄZKOWE w sensie walidatora, a nie w sensie serde, i to jest niezmiennik 19:
    /// bez dowodu werdykt liczyłby się z samego kodu wyjścia, a suita, która nie uruchomiła ani
    /// jednego testu, wychodzi zerem. Dlatego `check()` odmawia zapisu kroku bez tego wzorca —
    /// patrz [`check`] — a `#[serde(default)]` służy wyłącznie temu, żeby taki plik dał się
    /// OTWORZYĆ i naprawić.
    #[serde(default)]
    pub proof: String,
    /// Gdzie ta komenda biegnie. `cargo test` pisze po `target/`, więc to **nie** jest krok
    /// tylko do odczytu i reguła kolizji z niezmiennika 12 obowiązuje go tak samo jak agenta.
    #[serde(default)]
    pub folder: Folder,
    /// Jak [`AgentStep::when_it_fails`], razem z tym, że przy zapisie znika, gdy jest domyślne.
    /// Komenda, która nie przeszła, jest ślepym punktem dokładnie tak samo jak agent.
    #[serde(default, skip_serializing_if = "WhenItFails::is_the_default")]
    pub when_it_fails: WhenItFails,
    /// Jak [`AgentStep::at`].
    #[serde(default)]
    pub at: Point,
    /// Jak [`AgentStep::extra`].
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Krok, który **uruchamia coś i zostawia to żywe** — dev server, watcher, cokolwiek, co reszta
/// biegu ma zastać działające.
///
/// 2026-08-23 — POWSTAŁ ZE ZMIERZONEJ PORAŻKI. Bieg właściciela: `design-qa` mierzy computed
/// style na ŻYWEJ aplikacji, a `Front` uczciwie odpalił jej serwer i napisał następnemu krokowi
/// adres — `http://127.0.0.1:4202`. Sekundę później krok się skończył, Loadout zabił jego grupę
/// procesów razem z dowodem śmierci (niezmiennik 6), a sprawdzający zastał ciszę. Trzy rundy
/// pętli, `qualityScore 0`, i zdanie, które sam napisał: *„this is an orchestration-level
/// problem, not something design-qa can fix by retrying"*. Miał rację.
///
/// Zderzyły się wtedy dwie POPRAWNE reguły: proces poboczny nie ma prawa przeżyć kroku, a
/// weryfikacja przez pomiar wymaga, żeby przeżył. Ten kafelek jest rozstrzygnięciem: proces
/// należy do **rejestru rzeczy uruchomionych** (`commands::processes`), nie do kroku — więc żyje
/// dalej, stoi w liście po prawej ze swoim „Stop", i dalej ma dowód śmierci, tylko żądany przez
/// człowieka albo przez zamknięcie okna, a nie przez koniec jednego kafelka.
///
/// **Krok kończy się, gdy proces WSTANIE**, nie gdy zejdzie — inaczej graf zatrzymałby się na
/// nim na zawsze, a to jest dokładnie ta wada, którą niezmiennik 6 miał wykluczyć.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServeStep {
    pub id: String,
    /// Nazwa widoczna na kafelku — „Start the app".
    pub name: String,
    /// Wiersz powłoki, dosłownie jak wpisał go człowiek: `npx nx serve urc-portal --port 4202`.
    ///
    /// `#[serde(default)]` z tego samego powodu, co przy [`CheckStep::command`]: plik poprawiony
    /// ręcznie ma się WCZYTAĆ i dostać uwagę przy kafelku, a nie odbić się o serde.
    #[serde(default)]
    pub command: String,
    /// Gdzie ta komenda biegnie. Serwer dev podaje kod ze SWOJEGO drzewa, więc dla weryfikacji
    /// w kopii kroku ten wybór jest treścią, nie szczegółem.
    #[serde(default)]
    pub folder: Folder,
    /// Jak [`AgentStep::at`].
    #[serde(default)]
    pub at: Point,
    /// Jak [`AgentStep::extra`].
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Gdzie krok pracuje.
///
/// `fresh-copy` to obietnica izolacji z ARCHITECTURE §2 punkt 4 („każdy krok dostaje własną
/// kopię twoich plików"). Dlatego dwa kroki, które **mogą biec równocześnie** i celują w ten
/// sam folder, są odmową przy zapisie, a nie podpowiedzią (niezmiennik 12).
///
/// 2026-08-20 (T-56) — DOCHODZI CZWARTY WARIANT, a trzy dotychczasowe zostają z tymi samymi
/// nazwami i tym samym zachowaniem (niezmiennik 25). Powód jest zmierzony na harnessie, który
/// budujemy z Loadouta: łańcuch „implementacja → sprawdzenie → druga opinia → poprawka" ma JEDNO
/// drzewo robocze repo, a tych trzech wariantów nie da się tak ustawić. Zostawał wybór między
/// dwoma kłamstwami: `project` (poprawka pisze po plikach człowieka) albo `fresh-copy` (każdy krok
/// dostaje własne drzewo, więc poprawka nie widzi kodu, który ma poprawić). Oba **kończą się
/// sukcesem**, więc nikt nie zgłasza biegu, w którym recenzent czytał nie ten kod
/// (`docs/harness-as-workflow.md`, ustalenie U-2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "use", rename_all = "kebab-case")]
pub enum Folder {
    /// Folder projektu, w którym biegnie workflow.
    #[default]
    Project,
    /// Własna kopia tylko dla tego kroku.
    FreshCopy,
    /// To samo drzewo robocze, w którym pracował krok przede mną.
    ///
    /// **WSKAZUJE, kiedy przed krokiem jest JEDNO drzewo; SKŁADA, kiedy jest ich więcej.**
    /// Czym jest własne drzewo — drzewem gita na własnej gałęzi dla repozytorium, klonem
    /// systemowym dla folderu, który repem nie jest — rozstrzyga T-52. Przy jednym poprzedniku
    /// ten wariant mówi wyłącznie „ten sam katalog roboczy, co najbliższy poprzednik po
    /// strzałkach", jakiegokolwiek rodzaju ten poprzednik jest.
    ///
    /// 2026-08-29 — „WSKAZUJE, nie zakłada" przestaje być prawdą przy więcej niż jednym drzewie
    /// przed krokiem. Do tego dnia taki kształt był odmową przy Starcie, więc dwie równoległe
    /// gałęzie dało się narysować i nie dało się na nich pracować. Teraz krok dostaje **własną,
    /// nową kopię** i bieg znosi do niej zmiany plikowe wszystkich poprzedników
    /// (`commands::fan_in`); dwoje poprzedników, którzy napisali w jednym pliku różne rzeczy,
    /// zatrzymuje ten krok przed sterownikiem, zamiast po cichu wybierać jednego z nich.
    SameCopy,
    /// Wskazany ręcznie.
    Pick { path: String },
}

impl Folder {
    /// Czy ten krok dostaje **własną** kopię plików.
    ///
    /// Jedyny folder, który nie koliduje z niczym — także sam ze sobą, kiedy krok biegnie
    /// w kilku kopiach. Reszta reguły o kolizjach mieszka w [`check`], bo to ona zna drugi krok.
    #[must_use]
    pub fn is_own_copy(&self) -> bool {
        matches!(self, Self::FreshCopy)
    }
}

/// `"all"` albo lista nazw [T3 §3.1].
///
/// Znacznik jest osobnym typem, bo w enumie `untagged` wariant jednostkowy serializuje się jako
/// `null`, a format wymaga w tym miejscu stringa `"all"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EverySkill {
    #[default]
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Skills {
    Every(EverySkill),
    Only(Vec<String>),
}

impl Default for Skills {
    fn default() -> Self {
        Self::Every(EverySkill::All)
    }
}

/// `"notes"` — zwykła proza. Osobny typ z tego samego powodu, co [`EverySkill`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlainNotes {
    #[default]
    Notes,
}

/// Co ma się stać z robotą, kiedy **ten krok nie przejdzie**.
///
/// 2026-08-23 — ZAMÓWIENIE WŁAŚCICIELA, dosłownie: „workflows zawsze ma mieć opcje kontynuacji
/// a nie ślepe punkty".
///
/// CO BYŁO. Krok, który padł, zabierał ze sobą CAŁY stożek potomków
/// (`engine::scheduler`, `mark_cone`) — bezwarunkowo i bez zdania. Bieg właściciela
/// `20260823-092142`: sędzia `Verification 1` trzy razy odesłał research do poprawki, trzy razy
/// go nie przepuścił, i tym samym skasował `Syntezę`, `Design` i `Implementation` — mimo że
/// dwie pozostałe weryfikacje przeszły. Nie było jak powiedzieć, co ma się stać zamiast tego.
///
/// NA KROKU, NIE NA STRZAŁCE POWROTU. Pierwsza wersja tego projektu kładła to pole na powrocie
/// pętli, bo tam mieszka `max_turns`. To byłoby węższe niż zamówienie: pokrywałoby wyłącznie
/// sędziego, który wyczerpał próby, a nie krok, który padł zwyczajnie. Ślepy punkt jest tym
/// samym ślepym punktem niezależnie od tego, dlaczego krok nie przeszedł.
///
/// `CarryOn` JEST DOMYŚLNE — DECYZJA WŁAŚCICIELA 2026-08-23, tego samego dnia i o jeden krok
/// dalej niż samo pole: „wiesz co to w sumie carry on powinno być domyślnie".
///
/// Pierwsza wersja miała tu `Stop`, z uzasadnieniem „każdy plik zapisany przed tą zmianą biegnie
/// po niej co do kroku tak samo". To uzasadnienie było prawdziwe i **nie o to chodziło**:
/// zgodność wsteczna zachowywała dokładnie ten stan, który był awarią. Żaden z zapisanych plików
/// właściciela nie ma tego pola, więc domyślna `Stop` znaczyła „nic się dla ciebie nie zmienia" —
/// czyli ślepy punkt zostaje wszędzie tam, gdzie już był.
///
/// Kontynuacja jest bezpieczna do postawienia domyślną WYŁĄCZNIE dlatego, że nie jest cicha:
/// krok zostaje czerwony, a następny dostaje zdanie o tym, że materiał nie przeszedł
/// (`Live::when_this_one_fails`). Domyślne przepuszczanie, które by o tym milczało, byłoby
/// gorsze od domyślnego zatrzymania — synteza budowałaby na odrzuconej robocie, a bieg
/// meldowałby sukces.
///
/// NIE `AskMe`, choć to ona zachowuje oba dobra naraz. Biegi startują też wtedy, kiedy nikt nie
/// patrzy — z wyzwalaczy i z rytmu (`commands::triggers`) — a domyślne pytanie parkowałoby je
/// do rana. Domyślna wartość musi być bezpieczna dla biegu bez człowieka przy ekranie.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WhenItFails {
    /// Nic po tym kroku się nie wydarzy. Tak zachowywał się każdy krok do 2026-08-23.
    Stop,
    /// Robota jedzie dalej mimo wszystko — a krok i tak zostaje czerwony.
    #[default]
    ///
    /// Następny krok **musi się dowiedzieć**, że dostaje materiał, który nie przeszedł: bez tego
    /// synteza buduje na odrzuconej robocie i nikt tego nie widzi.
    CarryOn,
    /// Bieg staje i pyta człowieka, co dalej.
    ///
    /// Tą samą drogą, którą pyta kafelek kontrolny (`Live::wait_for_a_person`) — mechanizm
    /// parkowania biegu bierze `StepId`, a nie rodzaj kroku, więc nie trzeba go pisać drugi raz.
    AskMe,
}

impl WhenItFails {
    /// Czy to jest wartość domyślna — czyli czy przy zapisie ma zniknąć z pliku.
    ///
    /// Bierze referencję, bo tego żąda `skip_serializing_if`, choć typ jest `Copy`.
    #[must_use]
    pub const fn is_the_default(&self) -> bool {
        matches!(self, Self::CarryOn)
    }
}

/// Co krok przekazuje dalej [T3 §3.1].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Handover {
    Plain(PlainNotes),
    Form { fields: Vec<HandoverField> },
}

impl Default for Handover {
    fn default() -> Self {
        Self::Plain(PlainNotes::Notes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoverField {
    pub name: String,
    pub describe: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// Strzałka. Bez portów i bez danych — znaczy „po" (T3 §3.1).
///
/// JEDEN WYJĄTEK OD „BEZ WARUNKU": [`Link::max_turns`]. Strzałka, która niesie tę liczbę, jest
/// **powrotem** — wraca do kroku, który już był, i zamyka pętlę o zapisanym z góry suficie.
/// Projekt stoi w `docs/superpowers/specs/2026-08-19-petla-z-limitem-tur-design.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub from: String,
    pub to: String,
    /// Ile razy ta strzałka może zawrócić bieg, 1–[`MOST_TURNS`]. Brak pola znaczy „zwykłe po".
    ///
    /// BRAK POLA TO NIE JEST `Some(1)`. Rozróżnienie jest treścią: strzałka bez tej liczby nie
    /// ma prawa domykać koła i walidator ją za to odrzuca, a strzałka z `1` domyka koło, które
    /// przejdzie dokładnie jedną rundę. Dlatego `Option`, a nie liczba z domyślną wartością —
    /// domyślna wartość zamieniłaby KAŻDĄ strzałkę w potencjalny powrót i skasowała regułę,
    /// która broni przed strzałką pociągniętą w złą stronę.
    ///
    /// `skip_serializing_if`: plik bez pętli ma wyglądać dokładnie tak, jak wyglądał, żeby
    /// dołożenie tej funkcji nie przepisało każdego workflow na dysku (T3 §8.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u8>,
}

impl Link {
    /// Czy ta strzałka jest powrotem, czyli czy wolno jej domknąć koło.
    #[must_use]
    pub fn is_a_way_back(&self) -> bool {
        self.max_turns.is_some()
    }
}

/// Zamknięty język decyzji na strzałce. Nie przyjmuje skryptu ani dowolnego wyrażenia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "source",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Condition {
    Check { outcome: CheckOutcome },
    Checkpoint { choice: String },
    Handoff { field: String, equals: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckOutcome {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalLink {
    pub from: String,
    pub to: String,
    pub when: Condition,
}

/// Wartość, która naprawdę powstała podczas biegu i może wybrać strzałkę.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", content = "value", rename_all = "kebab-case")]
pub enum RouteEvidence {
    Check(CheckOutcome),
    Checkpoint(String),
    Handoff(BTreeMap<String, String>),
}

#[must_use]
pub fn condition_matches(condition: &Condition, evidence: &RouteEvidence) -> bool {
    match (condition, evidence) {
        (Condition::Check { outcome: left }, RouteEvidence::Check(right)) => left == right,
        (Condition::Checkpoint { choice: left }, RouteEvidence::Checkpoint(right)) => left == right,
        (Condition::Handoff { field, equals }, RouteEvidence::Handoff(fields)) => {
            fields.get(field) == Some(equals)
        }
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("This step did not produce the value needed to choose what runs next.")]
    MissingEvidence,
    #[error("This result does not match any next step in the workflow.")]
    NoMatch,
    #[error("This result matches more than one next step in the workflow.")]
    Ambiguous,
}

/// Wybiera dokładnie jedną zapisaną strzałkę. Brak warunków zachowuje zwykłą semantykę grafu.
pub fn select_branch<'a>(
    links: &'a [ConditionalLink],
    from: &str,
    evidence: Option<&RouteEvidence>,
) -> Result<Option<&'a ConditionalLink>, RouteError> {
    let relevant: Vec<&ConditionalLink> = links.iter().filter(|link| link.from == from).collect();
    if relevant.is_empty() {
        return Ok(None);
    }
    let evidence = evidence.ok_or(RouteError::MissingEvidence)?;
    let selected: Vec<&ConditionalLink> = relevant
        .into_iter()
        .filter(|link| condition_matches(&link.when, evidence))
        .collect();
    match selected.as_slice() {
        [] => Err(RouteError::NoMatch),
        [only] => Ok(Some(*only)),
        [_, _, ..] => Err(RouteError::Ambiguous),
    }
}

/// Pozycja kafelka na płótnie.
///
/// Pole jest `f64`, bo plik można poprawić ręcznie i przyjdzie stamtąd `241.4`. Zapisany tekst
/// niesie jednak zawsze **całkowitą wielokrotność [`GRID`]** [T3 §8.2].
///
/// 2026-08-16 — przyciąganie siedzi w samej serializacji, a nie w [`file::save`], i to jest
/// mocniejsza wersja tej obietnicy: pozycji nieprzyciągniętej **nie da się** zapisać, także
/// z pliku poprawionego ręcznie, w którym żadnego frontendu nie było. Odwrotnie —
/// przyciąganie w jednej funkcji zapisującej trzeba pamiętać w każdej następnej, która też
/// zapisze plik, a takich będzie więcej niż jedna.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Serialize for Point {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut point = serializer.serialize_struct("Point", 2)?;
        point.serialize_field("x", &Coordinate(snapped(self.x)))?;
        point.serialize_field("y", &Coordinate(snapped(self.y)))?;
        point.end()
    }
}

/// Najbliższa całkowita wielokrotność [`GRID`].
///
/// `240.00000001` brudzi diff przy każdym najechaniu myszą, a `240` nie brudzi go nigdy.
/// Wartość nieskończona albo NaN wraca bez zmiany: przyciąganie i tak dałoby z niej NaN,
/// a plik ma się zapisać z tym, co w nim jest, zamiast po cichu przesuwać kafelek do zera.
fn snapped(value: f64) -> f64 {
    if value.is_finite() {
        (value / GRID).round() * GRID
    } else {
        value
    }
}

/// Współrzędna w tekście pliku — liczba całkowita, nie `240.0`.
///
/// 2026-08-16 — `f64` serializuje się przez `ryu`, które do liczby całkowitej zawsze dopisuje
/// `.0`. Wtedy plik zapisany przez Loadouta różni się od tego samego pliku poprawionego ręcznie
/// o kropkę przy każdej pozycji, czyli o wiersz diffa na każdym kroku — przy pierwszym zapisie
/// po ręcznej poprawce przepisany zostaje cały plik.
#[derive(Debug)]
struct Coordinate(f64);

impl Serialize for Coordinate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Przez tekst, nie przez `as i64`: rzutowanie z `f64` jest w tym repo stratne i pełna
        // bramka je odrzuca (`engine/line.rs` nosi ten sam powód przy liczeniu czasu). Dla
        // liczby już przyciągniętej do siatki `{:.0}` jest odwzorowaniem dokładnym.
        match format!("{:.0}", self.0).parse::<i64>() {
            Ok(whole) => serializer.serialize_i64(whole),
            // Pozycja spoza zakresu `i64` — albo NaN z ręcznej edycji — nie może zniknąć po
            // cichu. Zapisujemy ją taką, jaka przyszła; płótno poprawi ją przy pierwszym ruchu.
            Err(_) => serializer.serialize_f64(self.0),
        }
    }
}
