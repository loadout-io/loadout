//! Limity biblioteki Context i nazwane odmowy, które z nich wynikają.
//!
//! **RUST JEST ŹRÓDŁEM PRAWDY O LIMITACH DLA UI** (PLAN §9). Okno wolno uprzedzić człowieka
//! wcześniej, ale liczba, o którą się rozbija, mieszka tutaj — inaczej pole na ekranie odmawia
//! przy innej wartości niż plik na dysku i człowiek nie wie, która jest prawdziwa.
//!
//! # Dlaczego to NIE jest `ValidatedImages`
//!
//! `engine::drivers::ValidatedImages` opisuje JEDNĄ WIADOMOŚĆ do vendora: cztery obrazy, 5 MiB
//! każdy, 12 MiB razem. To są liczby rozmowy i mają tam zostać nietknięte. Biblioteka na dysku
//! odpowiada na inne pytanie — ile materiału wolno trzymać — więc ma własne liczby. Z tamtej
//! bramy pożyczona jest DYSCYPLINA, nie wartości: zamknięta lista rodzajów i sprawdzenie
//! magicznych bajtów przed czymkolwiek innym.
//!
//! Lista jest tu WĘŻSZA o GIF-a, i to jest treść, nie przeoczenie: PLAN §2 stawia GIF, HEIC
//! i dokumenty Office poza dostawą, a rodzaj przyjęty przez import i pominięty przy budowaniu
//! jest gorszy niż nazwana odmowa.

use std::fmt;
use std::io;

use serde::Deserialize;

/// Ile źródeł mieści jeden zestaw.
pub const MAX_SOURCES: usize = 200;

/// Ile ważą razem ORYGINAŁY jednego zestawu.
pub const MAX_ORIGINAL_BYTES: u64 = 512 * 1024 * 1024;

/// Ile ważą razem POCHODNE jednego zestawu — miniatury, warianty dla agenta i strony PDF.
/// Osobny budżet od oryginałów, bo rosną z innego powodu i w innym tempie.
pub const MAX_DERIVED_BYTES: u64 = 512 * 1024 * 1024;

/// Ile waży jeden importowany plik.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// Ile waży jedno wklejenie tekstu.
pub const MAX_PASTED_TEXT_BYTES: usize = 2 * 1024 * 1024;

/// Ile stron ma PDF, który ta biblioteka umie przygotować.
pub const MAX_PDF_PAGES: u32 = 200;

/// Sufit pikseli przy dekodowaniu obrazu. Wymiary sprawdzamy PRZED pełnym dekodowaniem, bo
/// nagłówek z absurdalnym rozmiarem jest tanim sposobem na zajęcie całej pamięci procesu.
pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;

/// Najdłuższy bok miniatury podglądu.
pub const THUMBNAIL_EDGE: u32 = 512;

/// Najdłuższy bok wariantu, który dostaje agent. Oryginał zostaje niezmienny obok.
pub const AGENT_EDGE: u32 = 1_568;

/// Ile tekstu oddaje jeden odczyt podglądu (PLAN §9: do 16 KiB na wynik, z kursorem).
pub const READ_TEXT_BYTES: usize = 16 * 1024;

/// Ile trafień mieści jedna odpowiedź wyszukiwania.
pub const SEARCH_RESULTS: usize = 10;

/// Ile waży cała tekstowa odpowiedź wyszukiwania po serializacji dla vendora.
pub const SEARCH_ANSWER_BYTES: usize = 8 * 1024;

/// Ile bajtów obrazu może nieść jeden wynik narzędzia. Każdy następny obraz to osobny odczyt.
pub const IMAGE_ANSWER_BYTES: usize = 5 * 1024 * 1024;

/// Zamknięta lista rodzajów plików, które ta biblioteka przyjmuje.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accepted {
    Png,
    Jpeg,
    Webp,
    Pdf,
    Markdown,
    Text,
}

impl Accepted {
    /// Rodzaj po rozszerzeniu nazwy pliku — tak nazywa go człowiek, wybierając plik z dysku.
    ///
    /// Rozszerzenie nie JEST dowodem: mówi wyłącznie, czym plik się przedstawia. Dowodem jest
    /// [`Self::magic_matches`] zaraz obok, a dla obrazu dopiero prawdziwe dekodowanie.
    #[must_use]
    pub fn by_extension(name: &str) -> Option<Self> {
        let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
        match extension.as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            "pdf" => Some(Self::Pdf),
            "md" | "markdown" => Some(Self::Markdown),
            "txt" | "text" => Some(Self::Text),
            _ => None,
        }
    }

    /// Rodzaj po MIME z okna. Tą drogą przychodzi schowek, który nazwy pliku nie ma.
    #[must_use]
    pub fn by_mime(mime: &str) -> Option<Self> {
        match mime {
            "image/png" => Some(Self::Png),
            "image/jpeg" => Some(Self::Jpeg),
            "image/webp" => Some(Self::Webp),
            "application/pdf" => Some(Self::Pdf),
            "text/markdown" => Some(Self::Markdown),
            "text/plain" => Some(Self::Text),
            _ => None,
        }
    }

    #[must_use]
    pub const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Pdf => "application/pdf",
            Self::Markdown => "text/markdown",
            Self::Text => "text/plain",
        }
    }

    /// Rozszerzenie, pod którym oryginał leży w bibliotece.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Pdf => "pdf",
            Self::Markdown => "md",
            Self::Text => "txt",
        }
    }

    /// Czy to rodzaj, który trzeba zdekodować jako obraz.
    #[must_use]
    pub const fn is_image(self) -> bool {
        matches!(self, Self::Png | Self::Jpeg | Self::Webp)
    }

    /// Rodzaj źródła, którym ten plik stanie się w zestawie.
    #[must_use]
    pub const fn kind(self) -> super::SourceKind {
        match self {
            Self::Png | Self::Jpeg | Self::Webp => super::SourceKind::Image,
            Self::Pdf => super::SourceKind::Pdf,
            Self::Markdown | Self::Text => super::SourceKind::Document,
        }
    }

    /// Czy bajty są tym, czym plik się przedstawia.
    ///
    /// Ta sama technika, co `engine::drivers::magic_matches` — i ten sam powód: plik nazwany
    /// `.png`, w którym leży cokolwiek innego, ma odbić się na granicy, a nie w dekoderze.
    /// Tekst i Markdown nie mają magicznych bajtów, więc ich dowodem jest poprawny UTF-8:
    /// materiał, którego nie da się przeczytać jako tekst, tekstem nie jest.
    #[must_use]
    pub fn magic_matches(self, bytes: &[u8]) -> bool {
        match self {
            Self::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            Self::Jpeg => bytes.starts_with(b"\xff\xd8\xff"),
            Self::Webp => {
                bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
            }
            Self::Pdf => bytes.starts_with(b"%PDF-"),
            Self::Markdown | Self::Text => std::str::from_utf8(bytes).is_ok(),
        }
    }
}

/// Dlaczego dokumentu w ogóle nie da się otworzyć.
///
/// Rozpoznaje to lokalny worker, bo to on ten plik otwiera; NAZYWA to Rust, bo zdanie widoczne
/// dla człowieka powstaje po tej stronie granicy (D5). Stan jest TRWAŁY: idzie do szkicu jako
/// [`super::Preparation::Failed`], więc plik zaszyfrowany albo uszkodzony nie wraca po restarcie
/// jako „czeka na przygotowanie" i nie udaje pustego dokumentu (PLAN §5).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Unopenable {
    /// Plik jest zamknięty hasłem.
    Locked,
    /// Plik jest uszkodzony albo nie jest tym dokumentem, za który się podaje.
    Damaged,
    /// Powód, którego ten build nie zna (niezmiennik 5). Nowszy Loadout dopisze kolejny,
    /// a starszy ma go minąć zdaniem ogólnym, nie przewróconym odczytem.
    #[serde(other)]
    Unknown,
}

impl fmt::Display for Unopenable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locked => formatter.write_str(
                "This file is locked with a password, so nothing could be prepared from it. \
                 Add a copy saved without the password.",
            ),
            Self::Damaged => formatter.write_str(
                "This file could not be opened, so nothing was prepared from it. Save it again \
                 from the app that made it and add that copy.",
            ),
            Self::Unknown => formatter.write_str(
                "This file could not be prepared, and Loadout cannot say why. Save it again from \
                 the app that made it and add that copy.",
            ),
        }
    }
}

/// Nazwane odmowy biblioteki. Każda jest gotowym zdaniem po angielsku (D5, niezmiennik 14),
/// bo po każdej człowiek robi coś innego — a front nie ma prawa wyciągać sensu z surowego błędu.
#[derive(Debug)]
pub enum Refusal {
    /// Rodzaj spoza zamkniętej listy.
    Unsupported,
    /// Bajty nie są tym, czym plik się przedstawia.
    WrongBytes,
    /// Plik cięższy niż [`MAX_FILE_BYTES`].
    FileTooLarge,
    /// Wklejony tekst cięższy niż [`MAX_PASTED_TEXT_BYTES`].
    TextTooLarge,
    /// Obraz ponad [`MAX_IMAGE_PIXELS`]. Oryginał zostaje u człowieka nietknięty.
    TooManyPixels { pixels: u64 },
    /// Magiczne bajty się zgadzały, a dekoder i tak nie przeczytał obrazu.
    Undecodable,
    /// Zestaw ma już [`MAX_SOURCES`] źródeł.
    SetFull,
    /// Oryginały tego zestawu ważą już [`MAX_ORIGINAL_BYTES`].
    OriginalsFull,
    /// Pochodne tego zestawu ważą już [`MAX_DERIVED_BYTES`].
    DerivedFull,
    /// PDF dłuższy niż [`MAX_PDF_PAGES`].
    TooManyPages { pages: u32 },
    /// Pozycja importu, w której nie ma ani pliku, ani tekstu, ani obrazu.
    NothingToAdd,
    /// Pliku nie da się otworzyć tam, gdzie okno go wskazało.
    Unreadable(io::Error),
    /// Dysk nie przyjął kopii. Odmowa TEJ pozycji, nie całego importu.
    Unwritable(io::Error),
    /// Wynik przygotowania z INNEGO importu tego pliku. Rozliczamy po identyfikatorze operacji,
    /// więc spóźniona odpowiedź trafia do swojej operacji, a nie do tej, która akurat trwa.
    LateResult,
    /// Wynik przygotowania opisujący inne bajty niż te, które leżą w bibliotece.
    DifferentFile,
    /// Strona nie na swoim miejscu w kolejce — przyjęta zostawiłaby dziurę w środku dokumentu.
    PageOutOfOrder { next: u32 },
    /// Temu źródłu nie ma czego przygotowywać.
    NothingToPrepare,
    /// Ta strona jeszcze nie powstała.
    PageNotReady { number: u32 },
}

impl fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => formatter.write_str(
                "Loadout adds PNG, JPEG and WebP images, PDF files, Markdown and plain text. \
                 This one is a kind it does not read yet, so it was left out.",
            ),
            Self::WrongBytes => formatter.write_str(
                "What is inside this file is not what its name says it is, so it was left out.",
            ),
            Self::FileTooLarge => formatter.write_str(
                "Each file has to be 50 MiB or smaller. Split the material or add a smaller copy.",
            ),
            Self::TextTooLarge => formatter.write_str(
                "Pasted text has to be 2 MiB or smaller. Split it, or add it as a file.",
            ),
            Self::TooManyPixels { pixels } => write!(
                formatter,
                "This image is {pixels} pixels, and Loadout reads images up to 16 million. \
                 Your file is untouched — add a smaller copy of it.",
            ),
            Self::Undecodable => formatter.write_str(
                "This image could not be read, so it was left out. Open it, save it again as \
                 PNG or JPEG, and add that copy.",
            ),
            Self::SetFull => formatter.write_str(
                "This set already holds 200 sources, which is as many as one set takes. \
                 Everything already in it stays where it is.",
            ),
            Self::OriginalsFull => formatter.write_str(
                "This set already holds 512 MiB of files, which is as much as one set takes. \
                 Everything already in it stays where it is.",
            ),
            Self::DerivedFull => formatter.write_str(
                "This set has no room left for what Loadout makes out of your files. \
                 Everything already in it stays where it is.",
            ),
            Self::TooManyPages { pages } => write!(
                formatter,
                "This file has {pages} pages, and Loadout prepares up to 200. \
                 Add the pages you need as a shorter file.",
            ),
            Self::NothingToAdd => {
                formatter.write_str("There was nothing to add here, so nothing was added.")
            }
            Self::Unreadable(error) => {
                write!(formatter, "This file could not be read: {error}.")
            }
            Self::Unwritable(error) => write!(
                formatter,
                "This file could not be copied into your library: {error}. \
                 Everything else you picked is unaffected.",
            ),
            Self::LateResult => formatter.write_str(
                "This page belongs to an earlier import of this file, so it was not kept. \
                 The file is being prepared again from the beginning of what is missing.",
            ),
            Self::DifferentFile => formatter.write_str(
                "This page was prepared from other bytes than the file saved here, so it was \
                 not kept.",
            ),
            Self::PageOutOfOrder { next } => write!(
                formatter,
                "This file is ready up to the page before {next}, so page {next} is the one to \
                 prepare next.",
            ),
            Self::NothingToPrepare => {
                formatter.write_str("There is nothing left to prepare in this file.")
            }
            Self::PageNotReady { number } => write!(
                formatter,
                "Page {number} of this file is not prepared yet. Prepare the file to read it.",
            ),
        }
    }
}

impl std::error::Error for Refusal {}
