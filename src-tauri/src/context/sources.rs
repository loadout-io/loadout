//! Rejestr źródeł plikowych: import, przygotowanie dokumentu, podgląd i usunięcie.
//!
//! # Wszystkie drogi importu kończą się TUTAJ
//!
//! Schowek przywozi bajty (okno je ma, bo dostało je od przeglądarki), a plik z dysku przywozi
//! ŚCIEŻKĘ (okno bajtów nie widziało i widzieć nie musi — 50 MiB przepchnięte base64 przez IPC
//! byłoby kopią pliku w pamięci webviewa). Dwie drogi wejścia, jeden rejestr: limity, zamknięta
//! lista rodzajów, odcisk i publikacja są w jednym miejscu, więc nie da się dodać materiału
//! drogą, która czegoś nie sprawdza (niezmiennik 23).
//!
//! # Jeden wynik na każdą pozycję żądania
//!
//! [`import`] nigdy nie przerywa na pierwszym złym pliku. Pięć wybranych plików daje pięć
//! wierszy w kolejności wyboru, bo człowiek ma zobaczyć, KTÓRY z nich nie wszedł i dlaczego —
//! import, który odmawia w całości przez jeden plik, każe zgadywać.
//!
//! # Rozliczenie po identyfikatorze operacji
//!
//! Każda rewizja źródła zapisuje obok siebie `prepared.json` z numerem importu, który ją
//! przyniósł, i odciskiem bajtów, o których mówi. Wynik przygotowania podaje oba, więc
//! spóźniona odpowiedź trafia do SWOJEJ operacji, a nie do tej, która akurat jest ostatnia
//! (PLAN §5). Bez tego wygrywa ten, kto odpowie później.
//!
//! # Co jest niezmienne
//!
//! Wszystko pod `sources/<id>/<rewizja>/`. Podmiana materiału tworzy rewizję OBOK, nigdy nie
//! nadpisuje tej, na którą ktoś już się powołał — dlatego pochodne wolno publikować przez
//! [`super::files::publish_source_file`], które nie porównuje bajtów celu.

use std::fs;
use std::io::Cursor;
use std::path::Path;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::files::{self, DraftEdit};
use super::limits::{self, Accepted, Refusal};
use super::{
    ContextDraft, ContextSetRead, ContextSource, Error, Preparation, SCHEMA, SourceKind, StoredFile,
};

/// Katalog źródeł wewnątrz katalogu zestawu.
const SOURCES: &str = "sources";

/// Pierwsza rewizja źródła. Kolejne dokłada podmiana materiału, której ten etap jeszcze nie ma —
/// i dlatego jest to STAŁA, a nie zaszyta w formatowaniu ścieżki jedynka.
const FIRST_REVISION: &str = "r1";

/// Zapis operacji, która przyniosła tę rewizję.
const PREPARED: &str = "prepared.json";

/// Katalog stron przygotowanego dokumentu.
const PAGES: &str = "pages";

/// Miniatura do podglądu.
const THUMBNAIL: &str = "thumbnail.png";

/// Wariant, który dostaje agent. Oryginał zostaje obok, nietknięty.
const FOR_THE_AGENT: &str = "for-the-agent.png";

/// Nazwa wiersza wyniku dla wklejenia, którego nikt nie nazwał.
const PASTED: &str = "Pasted material";

/// Uwaga przy obrazie, którego wariant dla agenta jest dużo mniejszy od oryginału.
///
/// „Dużo" znaczy tu: dłuższy bok oryginału przekracza czterokrotność
/// [`limits::AGENT_EDGE`]. Przy takim zmniejszeniu tekst na screenshocie i cienka linia na
/// diagramie przestają być czytelne, a źródło oznaczone jako rozpoznane bez uwag byłoby
/// obietnicą bez pokrycia (PLAN §5).
const SHRUNK: &str = "This picture is much bigger than what fits in one message, so fine detail \
                      may not be legible in the copy an agent reads. Your file is untouched.";

/// Uwaga przy pliku, który już w tym zestawie leży.
const ALREADY_HERE: &str =
    "The same file under the same name is already in this set, so it was not added twice.";

/// Bajty przywiezione ze schowka. Ten sam kształt, co `ipc::PastedImage` — nazwy pliku w nim
/// nie ma, więc nie ma jak opuścić okna przez przypadek.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PastedBytes {
    pub mime: String,
    pub base64: String,
}

impl std::fmt::Debug for PastedBytes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PastedBytes")
            .field("mime", &self.mime)
            .field(
                "base64",
                &format_args!("<private; {} bytes>", self.base64.len()),
            )
            .finish()
    }
}

/// Jedna pozycja żądania importu.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    /// Nazwa, pod którą to źródło ma stanąć na liście. Pusta znaczy „weź ją z nazwy pliku".
    #[serde(default)]
    pub name: String,
    /// Ścieżka pliku z dysku. Rust sam go otwiera i KOPIUJE — okno nie przepycha bajtów.
    #[serde(default)]
    pub path: Option<String>,
    /// Tekst wklejony ze schowka.
    #[serde(default)]
    pub text: Option<String>,
    /// Obraz wklejony ze schowka.
    #[serde(default)]
    pub image: Option<PastedBytes>,
}

/// Całe żądanie importu — jedna operacja, jeden identyfikator.
#[derive(Clone, Debug)]
pub struct ImportRequest {
    pub set_id: String,
    /// Identyfikator TEJ operacji. Po nim rozlicza się wynik, także spóźniony (PLAN §5).
    pub operation_id: String,
    /// Rewizja `draft.json`, którą okno przeczytało.
    pub expected_revision: Option<String>,
    pub items: Vec<ImportItem>,
    /// Chwila, w której człowiek kliknął.
    pub at: String,
}

/// Wynik JEDNEJ pozycji żądania. Nazwany, bo pięć plików ma dać pięć wierszy na ekranie.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    /// Nazwa, którą człowiek widzi przy tym wierszu.
    pub name: String,
    /// Identyfikatory źródeł, które naprawdę weszły do zestawu. Mieszany paste daje dwa.
    pub added: Vec<String>,
    /// Zdanie odmowy — `None`, kiedy pozycja weszła.
    pub refused: Option<String>,
    /// Uwagi o tym IMPORCIE — na przykład o pliku, który już w tym zestawie leżał.
    ///
    /// Trwałe ograniczenia źródła stoją gdzie indziej, w [`super::ContextSource::notes`],
    /// bo są własnością materiału, a nie tego kliknięcia.
    pub notes: Vec<String>,
}

/// Odpowiedź na całe żądanie: wynik każdej pozycji i zestaw po imporcie.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    /// Ta sama operacja, o którą pytało okno. Wraca, żeby okno umiało odróżnić odpowiedź na
    /// SWOJE żądanie od odpowiedzi na poprzednie, kiedy człowiek zdążył kliknąć drugi raz.
    pub operation_id: String,
    /// Jeden wynik na każdą pozycję żądania, W KOLEJNOŚCI ŻĄDANIA.
    pub results: Vec<ImportResult>,
    pub read: ContextSetRead,
}

/// Jedna strona dokumentu, przygotowana przez lokalny worker i przywieziona do zatwierdzenia.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedPage {
    /// Import, do którego ta strona należy.
    pub operation_id: String,
    /// Odcisk bajtów, z których ta strona powstała.
    pub fingerprint: String,
    /// Ile stron ma cały plik. Liczy je lokalny worker, bo to on ten plik otwiera — Rust
    /// zapisuje ją przy pierwszej stronie i sprawdza wobec [`limits::MAX_PDF_PAGES`].
    pub pages_total: u32,
    /// Numer strony, licząc od 1.
    pub number: u32,
    /// Tekst tej strony, słowo w słowo.
    #[serde(default)]
    pub text: String,
    /// Obraz tej strony. Skan nie ma tekstu, a plik mieszany ma OBA — diagramu nie widać
    /// w ekstrakcji tekstowej.
    #[serde(default)]
    pub image: Option<PastedBytes>,
    /// Czym plik zawinił, kiedy nie da się go otworzyć W OGÓLE. `None` znaczy „to jest strona",
    /// a wtedy pola wyżej opisują ją jak zwykle.
    ///
    /// Tą samą drogą, co strona, bo rozlicza się tak samo: porażka przywieziona przez okno,
    /// które pracowało nad POPRZEDNIM importem, nie ma prawa oznaczyć pliku wybranego przed
    /// chwilą. Osobna komenda musiałaby powtórzyć całe to sprawdzenie u siebie.
    #[serde(default)]
    pub failed: Option<limits::Unopenable>,
}

/// Obraz oddany do podglądu.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewImage {
    pub mime: String,
    pub base64: String,
}

/// Kawałek źródła, którego chce podgląd. Zawsze po ZATWIERDZONYM identyfikatorze źródła,
/// nigdy po ścieżce od okna (PLAN §12).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SourcePart {
    /// Miniatura obrazu.
    Image { image: PreviewImage },
    /// Jedna strona dokumentu: jej tekst i jej wygląd.
    ///
    /// `rename_all` NA WARIANCIE, nie tylko na enumie: atrybut kontenera przepisuje nazwy
    /// wariantów, a pola wariantu zostawia w `snake_case`. Bez tej linii okno dostaje
    /// `pages_total` i czyta `pagesTotal`, czyli nic — a widać to dopiero na prawdziwej granicy.
    #[serde(rename_all = "camelCase")]
    Page {
        number: u32,
        pages_total: u32,
        text: String,
        image: Option<PreviewImage>,
    },
    /// Początek materiału tekstowego.
    Text { text: String, more: bool },
    /// CAŁY plik, tak jak leży w bibliotece — dla lokalnego workera, który ma go przygotować.
    ///
    /// Idzie razem ze stemplami, którymi worker ma podpisać każdą stronę: numerem importu
    /// i odciskiem bajtów. Dzięki temu ten, kto czyta plik, dostaje w tej samej odpowiedzi
    /// wszystko, czego potrzebuje, żeby jego wynik dało się rozliczyć (PLAN §5) — a nie musi
    /// szukać drugiej drogi do faktu, który i tak leży obok tych bajtów.
    ///
    /// Bajty jadą base64 przez drut i to jest tu jedyna droga: webview nie ma dostępu do dysku
    /// (`tauri-plugin-fs` świadomie nie jest w drzewie), a przygotowanie dzieje się w oknie.
    /// Sufit 50 MiB na plik jest tym, co trzyma tę odpowiedź przy rozmiarze, który da się
    /// przewieźć.
    #[serde(rename_all = "camelCase")]
    Whole {
        mime: String,
        base64: String,
        operation_id: String,
        fingerprint: String,
    },
}

/// Zapis operacji, która przyniosła tę rewizję źródła.
///
/// Leży OBOK bajtów, a nie w szkicu, i to jest cała jego treść: szkic mówi, co człowiek ma
/// w zestawie, a ten plik mówi, czyj wynik wolno jeszcze przyjąć.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Prepared {
    schema: u32,
    operation_id: String,
    fingerprint: String,
}

/// Co udało się zapisać dla jednej pozycji żądania.
#[derive(Clone, Debug, Default)]
struct Stored {
    ids: Vec<String>,
    notes: Vec<String>,
}

/// Pochodne obrazu — miniatura i wariant dla agenta. Oryginał zostaje niezmienny obok.
#[derive(Debug)]
struct Derived {
    thumbnail: Vec<u8>,
    for_the_agent: Vec<u8>,
    notes: Vec<String>,
}

/// Importuje wszystkie pozycje żądania i oddaje wynik KAŻDEJ z nich.
pub fn import(library: &Path, request: &ImportRequest) -> Result<ImportReport, Error> {
    let folder = files::folder_of(library, &request.set_id)?;
    let read = files::read_set(library, &request.set_id)?;
    // Spóźniony zapis odbija się PRZED skopiowaniem czegokolwiek. Bajty skopiowane pod odmowę
    // są bajtami, do których nikt nie ma drogi: szkic ich nie wymienia, więc nikt ich nie widzi.
    // Wyścig i tak zamyka warunkowy zapis niżej — to jest tylko tańsza droga do tej samej odmowy.
    if request
        .expected_revision
        .as_deref()
        .is_some_and(|expected| expected != read.revision)
    {
        return Err(Error::Changed);
    }

    let mut draft = read.draft.clone();
    let mut results = Vec::with_capacity(request.items.len());
    for item in &request.items {
        results.push(one_item(&folder, &mut draft, request, item));
    }

    let saved = files::save_draft(
        library,
        &DraftEdit {
            id: request.set_id.clone(),
            title: read.set.title.clone(),
            description: read.set.description.clone(),
            draft,
            expected_revision: Some(read.revision.clone()),
            at: request.at.clone(),
        },
    )?;
    Ok(ImportReport {
        operation_id: request.operation_id.clone(),
        results,
        read: saved,
    })
}

/// Zatwierdza JEDNĄ przygotowaną stronę i oddaje zestaw po jej dołożeniu.
pub fn complete_preparation(
    library: &Path,
    set_id: &str,
    source_id: &str,
    page: &PreparedPage,
    expected_revision: Option<&str>,
    at: &str,
) -> Result<ContextSetRead, Error> {
    let folder = files::folder_of(library, set_id)?;
    let read = files::read_set(library, set_id)?;
    let mut draft = read.draft.clone();
    let seat = draft
        .sources
        .iter()
        .position(|source| source.id == source_id)
        .ok_or(Error::NoSuchSource)?;
    let mut file = draft
        .sources
        .get(seat)
        .and_then(|source| source.file.clone())
        .ok_or(Refusal::NothingToPrepare)?;
    let Some(&Preparation::Needs { pages_done: done }) =
        draft.sources.get(seat).map(|source| &source.preparation)
    else {
        return Err(Refusal::NothingToPrepare.into());
    };

    let record = read_prepared(&folder, source_id, &file.revision)?;
    // Kolejność ma znaczenie: NAJPIERW czyja to odpowiedź, dopiero potem o czym mówi. Wynik
    // spóźnionej operacji ma dostać swoje zdanie, nie zdanie o niezgodnych bajtach.
    if record.operation_id != page.operation_id {
        return Err(Refusal::LateResult.into());
    }
    if record.fingerprint != page.fingerprint {
        return Err(Refusal::DifferentFile.into());
    }

    // PORAŻKA JEST STANEM NA DYSKU, nie zdaniem, które znika przy wyjściu z ekranu. Plik
    // zaszyfrowany albo uszkodzony ma po powrocie dalej mówić, co mu jest — inaczej człowiek
    // wraca do źródła, które „czeka na przygotowanie", i klika Prepare bez końca (PLAN §5).
    // Stąd, a nie z osobnej komendy: numer operacji i odcisk są już sprawdzone wyżej, więc
    // spóźniona porażka trafia do swojej operacji tak samo jak spóźniona strona.
    if let Some(failure) = page.failed {
        if let Some(source) = draft.sources.get_mut(seat) {
            source.preparation = Preparation::Failed {
                said: failure.to_string(),
            };
        }
        return save(library, set_id, &read, draft, expected_revision, at);
    }

    if page.pages_total == 0 || page.pages_total > limits::MAX_PDF_PAGES {
        return Err(Refusal::TooManyPages {
            pages: page.pages_total,
        }
        .into());
    }
    if page.number != done.saturating_add(1) || page.number > page.pages_total {
        return Err(Refusal::PageOutOfOrder {
            next: done.saturating_add(1),
        }
        .into());
    }

    // BUDŻET POCHODNYCH SPRAWDZANY PRZY KAŻDEJ STRONIE, nie tylko przy imporcie: to strony
    // rosną najszybciej. Dwustustronicowy skan dokłada dwieście obrazów do tego samego
    // budżetu, więc sprawdzenie wyłącznie przy dodaniu pliku pilnowałoby liczby, która wtedy
    // wynosi zero. Odmowa pada PRZED publikacją, więc nad limitem nie ląduje ani jedna strona.
    let (drawn, coming) = page_weight(page)?;
    room_for_derived(&draft, coming)?;

    let grew = publish_page(&folder, source_id, &file.revision, page, drawn.as_deref())?;
    file.pages = Some(page.pages_total);
    file.derived = file.derived.saturating_add(grew);
    if let Some(source) = draft.sources.get_mut(seat) {
        source.file = Some(file);
        source.preparation = if page.number == page.pages_total {
            Preparation::Ready
        } else {
            Preparation::Needs {
                pages_done: page.number,
            }
        };
    }
    save(library, set_id, &read, draft, expected_revision, at)
}

/// Oddaje kawałek zatwierdzonego źródła do podglądu.
pub fn read_source(
    library: &Path,
    set_id: &str,
    source_id: &str,
    page: Option<u32>,
) -> Result<SourcePart, Error> {
    let folder = files::folder_of(library, set_id)?;
    let read = files::read_set(library, set_id)?;
    let source = read
        .draft
        .sources
        .into_iter()
        .find(|one| one.id == source_id)
        .ok_or(Error::NoSuchSource)?;
    let Some(file) = source.file.clone() else {
        // Materiał wpisany w polu nie ma pliku i nie ma go mieć: jego treść JEST w szkicu.
        return Ok(part_of_text(&source.text));
    };
    let revision = folder.join(SOURCES).join(source_id).join(&file.revision);
    match source.kind {
        SourceKind::Image => Ok(SourcePart::Image {
            image: PreviewImage {
                mime: "image/png".to_owned(),
                base64: encoded(&fs::read(revision.join(THUMBNAIL))?),
            },
        }),
        // Brak numeru strony przy dokumencie znaczy „daj mi ten plik", bo strony zero nie ma.
        // Tak czyta go lokalny worker przed przygotowaniem; podgląd zawsze podaje numer.
        SourceKind::Pdf => match page {
            Some(number) => read_page(&revision, &file, number),
            None => whole_file(&folder, &revision, source_id, &file),
        },
        // Nieznany rodzaj z nowszego Loadouta czyta się jak tekst: pokazanie początku pliku jest
        // uczciwsze niż odmowa nad materiałem, który po prostu ma nowszą nazwę (niezmiennik 5).
        SourceKind::Text | SourceKind::Document | SourceKind::Unknown => {
            let bytes = fs::read(revision.join(original_name(&file.mime)))?;
            Ok(part_of_text(&String::from_utf8_lossy(&bytes)))
        }
    }
}

/// Zdejmuje źródło z zestawu i oddaje zestaw bez niego.
///
/// BAJTY ZOSTAJĄ NA DYSKU i to jest wybór, nie przeoczenie. Oryginał jest niezmienny od chwili
/// publikacji, a kasowanie cudzej pracy pod jednym kliknięciem jest operacją, po której nie ma
/// powrotu. Sprzątanie nieużywanych rewizji należy do retencji (PLAN §11), która wie, czy ktoś
/// jeszcze z nich nie czyta.
pub fn remove_source(
    library: &Path,
    set_id: &str,
    source_id: &str,
    expected_revision: Option<&str>,
    at: &str,
) -> Result<ContextSetRead, Error> {
    let read = files::read_set(library, set_id)?;
    if !read.draft.sources.iter().any(|one| one.id == source_id) {
        return Err(Error::NoSuchSource);
    }
    let mut draft = read.draft.clone();
    draft.sources.retain(|one| one.id != source_id);
    draft.excluded.retain(|one| one != source_id);
    // Szew wskazujący źródło, którego już nie ma, jest gorszy niż brak szwu: podpis mówiłby
    // wtedy o obrazie, którego nikt nie zobaczy.
    for source in &mut draft.sources {
        if source.companion_of.as_deref() == Some(source_id) {
            source.companion_of = None;
        }
    }
    save(library, set_id, &read, draft, expected_revision, at)
}

/// Jeden wiersz wyniku dla jednej pozycji żądania.
fn one_item(
    folder: &Path,
    draft: &mut ContextDraft,
    request: &ImportRequest,
    item: &ImportItem,
) -> ImportResult {
    let name = row_name(item);
    match take_in(folder, draft, request, item, &name) {
        Ok(stored) => ImportResult {
            name,
            added: stored.ids,
            refused: None,
            notes: stored.notes,
        },
        Err(refusal) => ImportResult {
            name,
            added: Vec::new(),
            // Zdanie składamy TU, a nie na froncie: front nie ma prawa wyciągać sensu
            // z surowej odmowy (D5, niezmiennik 14).
            refused: Some(refusal.to_string()),
            notes: Vec::new(),
        },
    }
}

/// Nazwa, którą człowiek zobaczy przy tym wierszu — jego własna albo nazwa pliku.
fn row_name(item: &ImportItem) -> String {
    if !item.name.trim().is_empty() {
        return item.name.trim().to_owned();
    }
    item.path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .map_or_else(
            || PASTED.to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
}

/// Kładzie jedną pozycję żądania w bibliotece.
///
/// Wklejenie niosące tekst I obraz daje DWA źródła, a obraz zapamiętuje, przy którym tekście
/// stanął. Tekst idzie pierwszy, żeby miał już identyfikator, kiedy obraz go wskazuje.
fn take_in(
    folder: &Path,
    draft: &mut ContextDraft,
    request: &ImportRequest,
    item: &ImportItem,
    name: &str,
) -> Result<Stored, Refusal> {
    if let Some(path) = item.path.as_deref() {
        return from_disk(folder, draft, request, path, name);
    }

    let mut stored = Stored::default();
    let mut companion = None;
    if let Some(text) = item.text.as_deref().filter(|text| !text.trim().is_empty()) {
        if text.len() > limits::MAX_PASTED_TEXT_BYTES {
            return Err(Refusal::TextTooLarge);
        }
        let id = new_id();
        draft.sources.push(ContextSource {
            id: id.clone(),
            kind: SourceKind::Text,
            name: name.to_owned(),
            text: text.to_owned(),
            ..ContextSource::default()
        });
        companion = Some(id.clone());
        stored.ids.push(id);
    }
    if let Some(image) = item.image.as_ref() {
        let kind = Accepted::by_mime(&image.mime).ok_or(Refusal::Unsupported)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(image.base64.trim())
            .map_err(|_error| Refusal::WrongBytes)?;
        let one = keep(
            folder,
            draft,
            &Keeping {
                operation_id: &request.operation_id,
                name,
                kind,
                companion_of: companion,
            },
            &bytes,
        )?;
        stored.ids.extend(one.ids);
        stored.notes.extend(one.notes);
    }
    if stored.ids.is_empty() {
        return Err(Refusal::NothingToAdd);
    }
    Ok(stored)
}

/// Plik wskazany ścieżką: Rust go otwiera, sprawdza i KOPIUJE do biblioteki.
fn from_disk(
    folder: &Path,
    draft: &mut ContextDraft,
    request: &ImportRequest,
    path: &str,
    name: &str,
) -> Result<Stored, Refusal> {
    let kind = Accepted::by_extension(name).ok_or(Refusal::Unsupported)?;
    let path = Path::new(path);
    let weight = fs::metadata(path).map_err(Refusal::Unreadable)?.len();
    // Rozmiar PRZED odczytem, bo odczyt sam jest kosztem: plik ponad sufitem ma się odbić,
    // zanim jego bajty w ogóle wejdą do pamięci.
    if weight > limits::MAX_FILE_BYTES {
        return Err(Refusal::FileTooLarge);
    }
    // Cały plik do pamięci, i to jest granica świadoma: `durable_file` publikuje z wycinka
    // bajtów, nie ze strumienia, a sufit powyżej trzyma to przy 50 MiB na plik.
    let bytes = fs::read(path).map_err(Refusal::Unreadable)?;
    keep(
        folder,
        draft,
        &Keeping {
            operation_id: &request.operation_id,
            name,
            kind,
            companion_of: None,
        },
        &bytes,
    )
}

/// Wszystko, czego [`keep`] potrzebuje poza samymi bajtami.
#[derive(Debug)]
struct Keeping<'a> {
    operation_id: &'a str,
    name: &'a str,
    kind: Accepted,
    companion_of: Option<String>,
}

/// Sprawdza bajty, publikuje je jako niezmienny oryginał i dopisuje źródło do szkicu.
fn keep(
    folder: &Path,
    draft: &mut ContextDraft,
    keeping: &Keeping<'_>,
    bytes: &[u8],
) -> Result<Stored, Refusal> {
    if !keeping.kind.magic_matches(bytes) {
        return Err(Refusal::WrongBytes);
    }
    let fingerprint = format!("{:x}", Sha256::digest(bytes));
    // Deduplikacja jest wąska z premedytacją: te same bajty POD TĄ SAMĄ nazwą I W TEJ SAMEJ
    // roli to ten sam materiał. Ten sam plik dodany pod inną nazwą zostaje osobnym powiązaniem,
    // bo nazwa jest tym, po co człowiek go tu położył (PLAN §5).
    //
    // 2026-09-08 — `companion_of` DOSZŁO DO KLUCZA PO INCYDENCIE. Screenshot wklejony dwa razy
    // z dwoma różnymi podpisami ma tę samą nazwę („Pasted material") i te same bajty, więc bez
    // tego warunku drugie wklejenie zapisywało nowy podpis i oddawało STARY obraz — dalej
    // powiązany z pierwszym tekstem. Człowiek zostawał z dwoma podpisami, z których jeden
    // opisywał obraz należący do drugiego. Bajty wolno współdzielić, powiązania nie.
    if let Some(known) = draft.sources.iter().find(|source| {
        source.name == keeping.name
            && source.companion_of == keeping.companion_of
            && source
                .file
                .as_ref()
                .is_some_and(|file| file.fingerprint == fingerprint)
    }) {
        return Ok(Stored {
            ids: vec![known.id.clone()],
            notes: vec![ALREADY_HERE.to_owned()],
        });
    }
    // POCHODNE POWSTAJĄ PRZED PYTANIEM O MIEJSCE, choć kosztują: dopiero wtedy znamy ich wagę,
    // a limit sprawdzony na wadze zerowej wpuszcza każdy obraz i odmawia dopiero następnemu.
    // Dekodowanie i tak musi się zdarzyć — obraz, który się nie otwiera, ma odpaść przed
    // zapisaniem czegokolwiek.
    let derived = if keeping.kind.is_image() {
        Some(derive_image(bytes)?)
    } else {
        None
    };
    let weight = derived.as_ref().map_or(0, |pictures| {
        (pictures.thumbnail.len() + pictures.for_the_agent.len()) as u64
    });
    room_for(draft, bytes.len() as u64, weight)?;

    let id = new_id();
    let at = format!("{SOURCES}/{id}/{FIRST_REVISION}");
    let original = format!("{at}/{}", original_name(keeping.kind.mime()));
    files::publish_source_file(folder, &original, bytes).map_err(unwritable)?;
    files::publish_source_file(
        folder,
        &format!("{at}/{PREPARED}"),
        prepared_bytes(keeping.operation_id, &fingerprint)?.as_bytes(),
    )
    .map_err(unwritable)?;
    if let Some(pictures) = derived.as_ref() {
        files::publish_source_file(folder, &format!("{at}/{THUMBNAIL}"), &pictures.thumbnail)
            .map_err(unwritable)?;
        files::publish_source_file(
            folder,
            &format!("{at}/{FOR_THE_AGENT}"),
            &pictures.for_the_agent,
        )
        .map_err(unwritable)?;
    }

    draft.sources.push(ContextSource {
        id: id.clone(),
        kind: keeping.kind.kind(),
        name: keeping.name.to_owned(),
        preparation: if keeping.kind == Accepted::Pdf {
            Preparation::Needs { pages_done: 0 }
        } else {
            Preparation::NotNeeded
        },
        companion_of: keeping.companion_of.clone(),
        notes: derived
            .as_ref()
            .map(|pictures| pictures.notes.clone())
            .unwrap_or_default(),
        file: Some(StoredFile {
            path: original,
            revision: FIRST_REVISION.to_owned(),
            mime: keeping.kind.mime().to_owned(),
            bytes: bytes.len() as u64,
            fingerprint,
            derived: weight,
            pages: None,
        }),
        ..ContextSource::default()
    });
    // Uwaga o obrazie NIE wraca w wyniku importu, choć powstała przy nim. Ograniczenie jest
    // trwałą własnością tego źródła i stoi przy jego wierszu — pokazane w OBU miejscach naraz
    // byłoby dwoma żywymi regionami na jeden fakt (niezmiennik 13). W wyniku importu zostają
    // uwagi o samym imporcie, jak ta o pliku, który już tu leżał.
    Ok(Stored {
        ids: vec![id],
        notes: Vec::new(),
    })
}

/// Czy ten zestaw ma jeszcze miejsce na kolejny materiał — ORYGINAŁ i jego pochodne razem.
///
/// Oba budżety w jednym miejscu i oba liczone Z PRZYBYWAJĄCĄ wagą: sprawdzenie „ile już jest"
/// bez „ile dojdzie" przepuszcza dokładnie ten plik, który przekracza limit, i odmawia dopiero
/// następnemu.
fn room_for(draft: &ContextDraft, arriving: u64, deriving: u64) -> Result<(), Refusal> {
    if draft.sources.len() >= limits::MAX_SOURCES {
        return Err(Refusal::SetFull);
    }
    if weighed(draft).0.saturating_add(arriving) > limits::MAX_ORIGINAL_BYTES {
        return Err(Refusal::OriginalsFull);
    }
    room_for_derived(draft, deriving)
}

/// Czy zmieszczą się jeszcze pochodne tej wagi.
///
/// Osobno od [`room_for`], bo pochodne rosną także PO imporcie: każda przygotowana strona
/// dokłada tekst i obraz do tego samego budżetu, a wtedy nie dochodzi ani jeden oryginał.
fn room_for_derived(draft: &ContextDraft, deriving: u64) -> Result<(), Refusal> {
    if weighed(draft).1.saturating_add(deriving) > limits::MAX_DERIVED_BYTES {
        return Err(Refusal::DerivedFull);
    }
    Ok(())
}

/// Ile ważą dziś oryginały tego zestawu i ile jego pochodne.
fn weighed(draft: &ContextDraft) -> (u64, u64) {
    let mut originals = 0_u64;
    let mut derived = 0_u64;
    for file in draft
        .sources
        .iter()
        .filter_map(|source| source.file.as_ref())
    {
        originals = originals.saturating_add(file.bytes);
        derived = derived.saturating_add(file.derived);
    }
    (originals, derived)
}

/// Miniatura i wariant dla agenta — po PRAWDZIWYM zdekodowaniu obrazu.
///
/// Wymiary czytamy z nagłówka i sądzimy je PRZED pełnym dekodowaniem: nagłówek deklarujący
/// dwa miliardy pikseli jest tanim sposobem na zajęcie całej pamięci procesu, a odmowa po
/// alokacji nie jest odmową.
fn derive_image(bytes: &[u8]) -> Result<Derived, Refusal> {
    let (wide, tall) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_error| Refusal::Undecodable)?
        .into_dimensions()
        .map_err(|_error| Refusal::Undecodable)?;
    let pixels = u64::from(wide) * u64::from(tall);
    if pixels > limits::MAX_IMAGE_PIXELS {
        return Err(Refusal::TooManyPixels { pixels });
    }
    let picture = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_error| Refusal::Undecodable)?
        .decode()
        .map_err(|_error| Refusal::Undecodable)?;

    let mut notes = Vec::new();
    if wide.max(tall) > limits::AGENT_EDGE.saturating_mul(4) {
        notes.push(SHRUNK.to_owned());
    }
    Ok(Derived {
        thumbnail: as_png(&picture.thumbnail(limits::THUMBNAIL_EDGE, limits::THUMBNAIL_EDGE))?,
        for_the_agent: as_png(&picture.thumbnail(limits::AGENT_EDGE, limits::AGENT_EDGE))?,
        notes,
    })
}

/// Pochodna zawsze jako PNG: jeden format pochodnych to jedno miejsce, w którym podgląd
/// i czytelnik kontekstu muszą się zgadzać co do tego, co dostaną.
fn as_png(picture: &image::DynamicImage) -> Result<Vec<u8>, Refusal> {
    let mut out = Vec::new();
    picture
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|_error| Refusal::Undecodable)?;
    Ok(out)
}

/// Bajty obrazu tej strony i to, o ile urośnie budżet pochodnych, gdyby ją przyjąć.
///
/// Rozkodowane RAZ i oddane dalej: liczenie wagi z długości base64, a potem dekodowanie do
/// zapisu, byłoby dwoma odpowiedziami na pytanie „ile to waży" — a limit sprawdzony na
/// oszacowaniu nie jest limitem.
fn page_weight(page: &PreparedPage) -> Result<(Option<Vec<u8>>, u64), Error> {
    let drawn = match page.image.as_ref() {
        Some(image) => Some(
            base64::engine::general_purpose::STANDARD
                .decode(image.base64.trim())
                .map_err(|_error| Error::Refused(Refusal::WrongBytes))?,
        ),
        None => None,
    };
    let weight = (page.text.len() as u64)
        .saturating_add(drawn.as_ref().map_or(0, |bytes| bytes.len() as u64));
    Ok((drawn, weight))
}

/// Publikuje tekst i wygląd jednej strony; oddaje, o ile urosły pochodne.
fn publish_page(
    folder: &Path,
    source_id: &str,
    revision: &str,
    page: &PreparedPage,
    drawn: Option<&[u8]>,
) -> Result<u64, Error> {
    let at = format!("{SOURCES}/{source_id}/{revision}/{PAGES}");
    let numbered = format!("page-{:04}", page.number);
    files::publish_source_file(
        folder,
        &format!("{at}/{numbered}.txt"),
        page.text.as_bytes(),
    )?;
    let mut grew = page.text.len() as u64;
    if let Some(bytes) = drawn {
        files::publish_source_file(folder, &format!("{at}/{numbered}.png"), bytes)?;
        grew = grew.saturating_add(bytes.len() as u64);
    }
    Ok(grew)
}

/// Cały plik plus stemple, którymi worker podpisze każdą stronę.
fn whole_file(
    folder: &Path,
    revision: &Path,
    source_id: &str,
    file: &StoredFile,
) -> Result<SourcePart, Error> {
    let record = read_prepared(folder, source_id, &file.revision)?;
    Ok(SourcePart::Whole {
        mime: file.mime.clone(),
        base64: encoded(&fs::read(revision.join(original_name(&file.mime)))?),
        operation_id: record.operation_id,
        fingerprint: record.fingerprint,
    })
}

/// Jedna strona przygotowanego dokumentu, tak jak widzi ją podgląd.
fn read_page(revision: &Path, file: &StoredFile, number: u32) -> Result<SourcePart, Error> {
    let numbered = revision.join(PAGES).join(format!("page-{number:04}"));
    // Strona, której jeszcze nie ma, ma WŁASNE zdanie. Surowy „no such file" opowiadałby
    // człowiekowi o dysku wtedy, gdy prawdą jest, że ten plik nie jest jeszcze gotowy.
    let text = fs::read_to_string(numbered.with_extension("txt"))
        .map_err(|_error| Error::Refused(Refusal::PageNotReady { number }))?;
    let drawn = fs::read(numbered.with_extension("png")).ok();
    Ok(SourcePart::Page {
        number,
        pages_total: file.pages.unwrap_or(0),
        text,
        image: drawn.map(|bytes| PreviewImage {
            mime: "image/png".to_owned(),
            base64: encoded(&bytes),
        }),
    })
}

/// Początek materiału tekstowego i informacja, czy coś za nim zostało.
fn part_of_text(text: &str) -> SourcePart {
    let mut cut = text.len().min(limits::READ_TEXT_BYTES);
    while cut < text.len() && !text.is_char_boundary(cut) {
        cut = cut.saturating_sub(1);
    }
    SourcePart::Text {
        text: text.get(..cut).unwrap_or_default().to_owned(),
        more: cut < text.len(),
    }
}

/// Zapis operacji tej rewizji źródła.
fn read_prepared(folder: &Path, source_id: &str, revision: &str) -> Result<Prepared, Error> {
    let bytes = fs::read(
        folder
            .join(SOURCES)
            .join(source_id)
            .join(revision)
            .join(PREPARED),
    )?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn prepared_bytes(operation_id: &str, fingerprint: &str) -> Result<String, Refusal> {
    let record = Prepared {
        schema: SCHEMA,
        operation_id: operation_id.to_owned(),
        fingerprint: fingerprint.to_owned(),
    };
    let mut text = serde_json::to_string_pretty(&record).map_err(|_error| Refusal::NothingToAdd)?;
    text.push('\n');
    Ok(text)
}

/// Zapisuje szkic po zmianie źródeł, zachowując tytuł i opis, których ta droga nie dotyka.
fn save(
    library: &Path,
    set_id: &str,
    read: &ContextSetRead,
    draft: ContextDraft,
    expected_revision: Option<&str>,
    at: &str,
) -> Result<ContextSetRead, Error> {
    files::save_draft(
        library,
        &DraftEdit {
            id: set_id.to_owned(),
            title: read.set.title.clone(),
            description: read.set.description.clone(),
            draft,
            expected_revision: expected_revision.map(str::to_owned),
            at: at.to_owned(),
        },
    )
}

/// Nazwa pliku oryginału. Rozszerzenie bierze się z rodzaju, nie z nazwy, którą podał człowiek:
/// katalog źródła ma być czytelny w Finderze także wtedy, gdy nazwa była pusta albo dziwna.
fn original_name(mime: &str) -> String {
    let extension = Accepted::by_mime(mime).map_or("bin", Accepted::extension);
    format!("original.{extension}")
}

fn encoded(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn new_id() -> String {
    crate::commands::mint::new_id_inner().to_string()
}

/// Awaria zapisu jest odmową TEJ pozycji, nie całego importu: cztery dobre pliki nie mają
/// znikać przez piąty, którego dysk nie przyjął (PLAN §5).
fn unwritable(error: Error) -> Refusal {
    Refusal::Unwritable(match error {
        Error::Unwritable(io) => io,
        other => std::io::Error::other(other.to_string()),
    })
}
