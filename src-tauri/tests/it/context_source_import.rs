//! CT-02: pliki wchodzą do biblioteki jako KOPIE, a każdy importowany kawałek dostaje wynik.
//!
//! SZEŚĆ RZECZY, KTÓRE PSUJĄ SIĘ OSOBNO — i dlatego jest tu sześć przypadków:
//!
//! 1. **Mieszany paste zostawia OBA.** Słabą wersją jest „po wklejeniu jest źródło": przechodzi
//!    ją implementacja, która bierze obraz i gubi podpis albo odwrotnie. Dlatego sądzimy dwa
//!    źródła i szew między nimi — bez niego nikt już nie wie, że były jednym materiałem.
//! 2. **Obraz ma się NAPRAWDĘ zdekodować.** Magiczne bajty mówią tylko, czym plik się
//!    przedstawia. Ten sam nagłówek ma plik ucięty w połowie i plik, w którym ktoś podmienił
//!    środek — a produkt, który przyjmie taki obraz, pokaże go człowiekowi dopiero za tydzień.
//! 3. **Kopia, nie dowiązanie.** Asercja o „dodanym pliku" przechodzi także dla implementacji,
//!    która zapisała ścieżkę; różnicę widać dopiero, kiedy oryginał zniknie z `Downloads`.
//! 4. **Pięć plików to pięć NAZWANYCH wyników.** Jeden zły plik nie ma prawa unieważnić
//!    czterech dobrych, a nieobsługiwany rodzaj ma dostać zdanie, nie ciszę.
//! 5. **Przygotowanie przerwane w połowie wznawia się od brakującej strony.** Nie od początku:
//!    powtarzanie gotowych stron jest tym, co człowiek nazywa zawieszeniem.
//! 6. **Spóźniony wynik trafia do SWOJEJ operacji.** Odpowiedź poprzedniego importu, która
//!    doszła po następnym, nie ma prawa wylądować w tym, co akurat trwa (PLAN §5).
//!
//! Katalog domowy jest świeżym `tempfile::tempdir()`, wzorem `context_library_survives_restart`.
//! `loadout.db` nie jest tu otwierany ani razu i nie ma jak być: biblioteka Context nie zna
//! `store::` (niezmiennik 4).

use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::Engine as _;

use loadout_lib::context::files::{
    DraftEdit, create_set, folder_of, library_root, read_set, save_draft,
};
use loadout_lib::context::limits::Unopenable;
use loadout_lib::context::sources::{
    self, ImportItem, ImportReport, ImportRequest, PastedBytes, PreparedPage, SourcePart,
};
use loadout_lib::context::{ContextSource, Preparation, SourceKind};

/// Chwila, w której człowiek kliknął. Podawana, bo biblioteka nie ma zegara i mieć nie będzie.
const CLICKED: &str = "2026-09-07T11:00:00Z";

const TITLE: &str = "Checkout redesign";

/// Fikstury: JEDNA kopia bajtów na Rusta i na przeglądarkę.
/// Powód stoi w `e2e/fixtures/context/README.md`.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../e2e/fixtures/context");

const SCREENSHOT_BASE64: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../e2e/fixtures/context/screenshot.png.b64"
));
const PATTERN_BASE64: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../e2e/fixtures/context/pattern.webp.b64"
));

/// Podpis, który człowiek wkleił razem ze screenshotem.
const CAPTION: &str = "This is the total after the quantity changes.";

fn bytes_of(base64: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    Ok(base64::engine::general_purpose::STANDARD.decode(base64.trim())?)
}

/// Biblioteka z jednym zestawem — wszystko, czego potrzebuje import.
#[derive(Debug)]
struct Library {
    /// Trzymany, bo katalog żyje tak długo, jak ten uchwyt.
    _home: tempfile::TempDir,
    root: PathBuf,
    set: String,
}

impl Library {
    fn fresh() -> Result<Self, Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        let root = library_root(home.path());
        let made = create_set(&root, TITLE, CLICKED)?;
        Ok(Self {
            _home: home,
            root,
            set: made.set.id,
        })
    }

    /// Rewizja `draft.json`, którą okno właśnie przeczytało.
    fn revision(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(read_set(&self.root, &self.set)?.revision)
    }

    fn import(
        &self,
        operation: &str,
        items: Vec<ImportItem>,
    ) -> Result<ImportReport, Box<dyn std::error::Error>> {
        Ok(sources::import(
            &self.root,
            &ImportRequest {
                set_id: self.set.clone(),
                operation_id: operation.to_owned(),
                expected_revision: Some(self.revision()?),
                items,
                at: CLICKED.to_owned(),
            },
        )?)
    }

    fn sources(&self) -> Result<Vec<ContextSource>, Box<dyn std::error::Error>> {
        Ok(read_set(&self.root, &self.set)?.draft.sources)
    }

    fn part(
        &self,
        source: &str,
        page: Option<u32>,
    ) -> Result<SourcePart, Box<dyn std::error::Error>> {
        Ok(sources::read_source(&self.root, &self.set, source, page)?)
    }
}

/// Pozycja importu wskazująca plik na dysku — okno oddaje ŚCIEŻKĘ, nie bajty.
fn from_disk(path: &Path) -> ImportItem {
    ImportItem {
        name: String::new(),
        path: Some(path.display().to_string()),
        text: None,
        image: None,
    }
}

/// Kopia pliku w katalogu, z którego człowiek go bierze.
fn put(folder: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = folder.join(name);
    fs::write(&path, bytes)?;
    Ok(path)
}

/// Obraz o 16 008 001 pikselach — o osiem tysięcy ponad sufit.
///
/// Robiony tutaj, a nie trzymany w repo: jednolita szarość ściska się do kilkudziesięciu
/// kilobajtów, ale rozpakowana zajmuje 16 MB i taki plik nie ma po co leżeć w drzewie.
fn over_the_pixel_ceiling() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let wide = image::DynamicImage::ImageLuma8(image::GrayImage::new(4_001, 4_001));
    let mut bytes = Vec::new();
    wide.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)?;
    Ok(bytes)
}

/// Prawdziwy JPEG, zakodowany tą samą skrzynią, która potem ma go przeczytać.
fn a_real_jpeg() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let picture = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        8,
        8,
        image::Rgb([200, 30, 60]),
    ));
    let mut bytes = Vec::new();
    picture.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Jpeg)?;
    Ok(bytes)
}

#[test]
fn a_mixed_paste_keeps_the_text_the_image_and_what_joins_them()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    let report = library.import(
        "op-paste",
        vec![ImportItem {
            name: "Cart total".to_owned(),
            path: None,
            text: Some(CAPTION.to_owned()),
            image: Some(PastedBytes {
                mime: "image/png".to_owned(),
                base64: SCREENSHOT_BASE64.trim().to_owned(),
            }),
        }],
    )?;

    let described = format!("{report:?}");
    assert_eq!(
        report.results.len(),
        1,
        "one paste is one row of the import report, whatever it turned into: {described}"
    );
    assert_eq!(
        report.results.first().map(|one| one.added.len()),
        Some(2),
        "a paste carrying a screenshot AND its caption has to keep both. Keeping one of the two \
         is the half that a person notices tomorrow, when the note explains a picture that is \
         not there: {described}"
    );

    let kept = library.sources()?;
    let kinds: Vec<SourceKind> = kept.iter().map(|source| source.kind).collect();
    assert!(
        kinds.contains(&SourceKind::Text) && kinds.contains(&SourceKind::Image),
        "the set has to hold the text and the image, and it holds {kinds:?}"
    );
    let picture = kept
        .iter()
        .find(|source| source.kind == SourceKind::Image)
        .ok_or("no image landed in the set")?;
    let note = kept
        .iter()
        .find(|source| source.kind == SourceKind::Text)
        .ok_or("no text landed in the set")?;
    assert_eq!(
        picture.companion_of.as_deref(),
        Some(note.id.as_str()),
        "the image does not say which text it came in with. Two sources that were one paste and \
         no longer know it are two sources nobody can put back together"
    );
    assert_eq!(
        note.text, CAPTION,
        "the pasted caption came back changed, and every byte of it is a person's work"
    );
    Ok(())
}

#[test]
fn the_same_picture_pasted_twice_keeps_a_link_for_each_caption()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    let second = "And this is what it looks like after the promo code drops.";

    // TE SAME BAJTY, dwa razy, pod dwoma różnymi podpisami. Nazwa obu jest identyczna, bo
    // schowek żadnej nie podaje — więc rozróżnia je wyłącznie to, przy czym stanęły.
    library.import("op-first", vec![paste_of(CAPTION)])?;
    library.import("op-second", vec![paste_of(second)])?;

    // ŚWIEŻY odczyt z dysku: powiązanie trzymane wyłącznie w pamięci znikłoby tutaj.
    let kept = library.sources()?;
    let described = format!("{kept:?}");
    let pictures: Vec<&ContextSource> = kept
        .iter()
        .filter(|source| source.kind == SourceKind::Image)
        .collect();
    assert_eq!(
        pictures.len(),
        2,
        "two pastes of one picture under two captions left {} image rows. Sharing the bytes is \
         allowed; sharing the LINK means one of the two captions now describes a picture that \
         belongs to the other: {described}",
        pictures.len()
    );

    let captions: Vec<String> = pictures
        .iter()
        .map(|picture| {
            kept.iter()
                .find(|source| Some(&source.id) == picture.companion_of.as_ref())
                .map(|source| source.text.clone())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(
        captions,
        vec![CAPTION.to_owned(), second.to_owned()],
        "each picture has to point at the caption it was pasted with, and they point at \
         {captions:?}. A second paste that hands back the first picture leaves the newer \
         caption explaining somebody else's screenshot"
    );
    Ok(())
}

/// Jedno wklejenie: ten sam obraz, ten podpis. Nazwy nie ma — schowek jej nie podaje.
fn paste_of(caption: &str) -> ImportItem {
    ImportItem {
        name: String::new(),
        path: None,
        text: Some(caption.to_owned()),
        image: Some(PastedBytes {
            mime: "image/png".to_owned(),
            base64: SCREENSHOT_BASE64.trim().to_owned(),
        }),
    }
}

#[test]
fn png_jpeg_and_webp_are_really_decoded_before_they_are_kept()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    let downloads = tempfile::tempdir()?;
    let png = put(downloads.path(), "shot.png", &bytes_of(SCREENSHOT_BASE64)?)?;
    let jpeg = put(downloads.path(), "scan.jpg", &a_real_jpeg()?)?;
    let webp = put(downloads.path(), "pattern.webp", &bytes_of(PATTERN_BASE64)?)?;
    // Ten sam nagłówek, co PNG wyżej, i nic poza nim. Magiczne bajty przechodzi, dekoder nie —
    // i o tę różnicę chodzi w całym tym przypadku.
    let truncated = put(
        downloads.path(),
        "half.png",
        bytes_of(SCREENSHOT_BASE64)?.get(..20).unwrap_or_default(),
    )?;

    let report = library.import(
        "op-images",
        vec![
            from_disk(&png),
            from_disk(&jpeg),
            from_disk(&webp),
            from_disk(&truncated),
        ],
    )?;

    let described = format!("{report:?}");
    let landed: Vec<bool> = report
        .results
        .iter()
        .map(|one| one.refused.is_none())
        .collect();
    assert_eq!(
        landed,
        [true, true, true, false],
        "PNG, JPEG and WebP have to land and a file with the right header and no picture behind \
         it has to be turned down. Matching magic bytes is what a broken file also does: {described}"
    );

    let refusal = report
        .results
        .get(3)
        .and_then(|one| one.refused.clone())
        .unwrap_or_default();
    assert!(
        !refusal.is_empty(),
        "the file that could not be read has to say so in a sentence a person can act on"
    );
    Ok(())
}

#[test]
fn a_file_still_opens_after_the_one_it_came_from_is_deleted()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    let downloads = tempfile::tempdir()?;
    let taken = put(downloads.path(), "shot.png", &bytes_of(SCREENSHOT_BASE64)?)?;

    let report = library.import("op-copy", vec![from_disk(&taken)])?;
    let id = report
        .results
        .first()
        .and_then(|one| one.added.first().cloned())
        .ok_or("the file was not added at all, so there is nothing to delete under it")?;

    // Człowiek sprząta `Downloads`. Katalog też, nie sam plik — tak wygląda posprzątany folder.
    drop(downloads);
    assert!(
        !taken.exists(),
        "the file this case is about is still on disk at {}, so nothing was proven",
        taken.display()
    );

    let part = library.part(&id, None)?;
    let described = format!("{part:?}");
    match part {
        SourcePart::Image { image } => assert!(
            !image.base64.is_empty(),
            "the preview came back empty after the file it was taken from was deleted, which is \
             what a link looks like when the other end goes away"
        ),
        other => {
            let seen = format!("{other:?}");
            return Err(format!(
                "an image source has to preview as an image; it answered with {seen}. {described}"
            )
            .into());
        }
    }
    Ok(())
}

#[test]
fn a_document_imported_before_reader_derivatives_still_opens()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    let downloads = tempfile::tempdir()?;
    let expected = fs::read(Path::new(FIXTURES).join("notes.md"))?;
    let taken = put(downloads.path(), "notes.md", &expected)?;
    let report = library.import("op-old-document", vec![from_disk(&taken)])?;
    let id = report
        .results
        .first()
        .and_then(|one| one.added.first().cloned())
        .ok_or("the document was not added, so its old layout cannot be exercised")?;
    let source = library
        .sources()?
        .into_iter()
        .find(|source| source.id == id)
        .ok_or("the imported document is absent from the saved draft")?;
    let revision = source
        .file
        .as_ref()
        .map(|file| file.revision.as_str())
        .ok_or("the imported document does not name its saved revision")?;
    let reader = folder_of(&library.root, &library.set)?
        .join("sources")
        .join(&id)
        .join(revision)
        .join("for-the-reader.txt");

    // 2026-09-08 — CT-03b zaczęło tworzyć tę pochodną dopiero przy nowych importach. Jej
    // usunięcie odtwarza układ dokumentu zapisanego przez CT-02, bez podrabiania szkicu.
    fs::remove_file(&reader)?;
    assert!(
        !reader.exists(),
        "the reader derivative still exists, so this case did not recreate an older document"
    );

    let part = library.part(&id, None)?;
    assert_eq!(
        part,
        SourcePart::Text {
            text: String::from_utf8(expected)?,
            more: false,
        },
        "a document saved before reader derivatives existed has to show the same text"
    );
    assert!(
        !reader.exists(),
        "previewing an older document silently wrote a derivative without adding it to the saved budget"
    );
    Ok(())
}

#[test]
fn five_files_give_five_named_results_and_one_refusal_keeps_the_rest()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    let downloads = tempfile::tempdir()?;
    let chosen = [
        put(downloads.path(), "shot.png", &bytes_of(SCREENSHOT_BASE64)?)?,
        put(
            downloads.path(),
            "notes.md",
            &fs::read(Path::new(FIXTURES).join("notes.md"))?,
        )?,
        put(
            downloads.path(),
            "paper.pdf",
            &fs::read(Path::new(FIXTURES).join("paper.pdf"))?,
        )?,
        put(
            downloads.path(),
            "not-really.png",
            &fs::read(Path::new(FIXTURES).join("not-really.png"))?,
        )?,
        put(downloads.path(), "enormous.png", &over_the_pixel_ceiling()?)?,
    ];

    let report = library.import(
        "op-five",
        chosen
            .iter()
            .map(|path| from_disk(path.as_path()))
            .collect(),
    )?;
    let described = format!("{report:?}");

    assert_eq!(
        report.results.len(),
        5,
        "five chosen files have to give five results, in the order they were chosen. A report \
         shorter than the request is the silent tail this whole feature exists to end: {described}"
    );
    let names: Vec<String> = report
        .results
        .iter()
        .map(|one| one.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "shot.png".to_owned(),
            "notes.md".to_owned(),
            "paper.pdf".to_owned(),
            "not-really.png".to_owned(),
            "enormous.png".to_owned(),
        ],
        "every result has to name the file it is about, or a person reading five lines cannot \
         tell which two were turned down"
    );

    let refused: Vec<bool> = report
        .results
        .iter()
        .map(|one| one.refused.is_some())
        .collect();
    assert_eq!(
        refused,
        [false, false, false, true, true],
        "three of these five are readable and two are not, and one bad file may not void the \
         others: {described}"
    );
    assert_eq!(
        library.sources()?.len(),
        3,
        "the three readable files have to be in the set after the import: {described}"
    );

    let pixels = report
        .results
        .get(4)
        .and_then(|one| one.refused.clone())
        .unwrap_or_default();
    assert!(
        pixels.contains("16 million"),
        "the refusal for the oversized image has to say what the ceiling is, or a person cannot \
         tell how much smaller a copy has to be; it said: {pixels}"
    );
    Ok(())
}

#[test]
fn a_half_prepared_document_carries_on_from_the_missing_page()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    let (_downloads, id, fingerprint) = a_document_in(&library, "op-pdf")?;

    // Strona tekstowa, potem strona-skan. Okno zamyka się po drugiej z trzech.
    finish_page(
        &library,
        &id,
        "op-pdf",
        &fingerprint,
        1,
        "Page one, in words.",
        true,
    )?;
    finish_page(&library, &id, "op-pdf", &fingerprint, 2, "", true)?;

    let waiting = one_source(&library, &id)?;
    assert!(
        matches!(waiting.preparation, Preparation::Needs { pages_done: 2 }),
        "a document left in the middle has to say how far it got, or coming back means starting \
         again; it says {:?}",
        waiting.preparation
    );
    // NAZWY NA DRUCIE, nie tylko wartości. Okno czyta `pagesDone`; pole oddane jako `pages_done`
    // jest dla niego polem, którego nie ma — i wtedy każdy przerwany plik czyta się jak nietknięty.
    // Wartości sprawdzone po stronie Rusta tego nie łapią: obie strony są zielone osobno.
    let on_the_wire = serde_json::to_string(&waiting.preparation)?;
    assert!(
        on_the_wire.contains("\"pagesDone\""),
        "the progress of a half-prepared file goes across under a name the window does not read: \
         {on_the_wire}"
    );

    // Wznowienie. Strona 3 jest mieszana: ma i tekst, i wygląd — bez obrazu ekstrakcja
    // tekstowa gubi diagram, a bez tekstu odwołanie nie ma czego wskazać.
    finish_page(
        &library,
        &id,
        "op-pdf",
        &fingerprint,
        3,
        "Page three, with a diagram.",
        true,
    )?;
    let done = one_source(&library, &id)?;
    assert_eq!(
        done.preparation,
        Preparation::Ready,
        "every page is prepared, so the document has to read as ready"
    );

    let on_the_wire = serde_json::to_string(&library.part(&id, Some(3))?)?;
    assert!(
        on_the_wire.contains("\"pagesTotal\""),
        "the page count goes to the window under a name it does not read, so a prepared file \
         says \"page 3 of nothing\": {on_the_wire}"
    );

    // Trzy rodzaje dokumentu na trzech stronach jednego pliku: tekstowa, skan i mieszana.
    // Odwołanie ma wskazywać SWOJĄ stronę — plik, którego strona 2 oddaje tekst strony 1,
    // przechodzi każdą asercję mówiącą „tekst jest", więc numer sądzimy przy każdej z trzech.
    let (words, drawing) = page_parts(&library, &id, 1)?;
    assert!(
        words.contains("Page one"),
        "page one is the one with words on it, and it came back as {words:?}"
    );
    assert!(
        drawing,
        "a text page keeps its look too. Without it a diagram between the paragraphs is gone the \
         moment the text is extracted (PLAN §5)"
    );
    let (scanned, scan_drawing) = page_parts(&library, &id, 2)?;
    assert!(
        scanned.is_empty() && scan_drawing,
        "a scanned page has no text and has to keep its picture, or there is nothing left of it \
         at all; it came back as text {scanned:?} with a picture: {scan_drawing}"
    );
    let (mixed, mixed_drawing) = page_parts(&library, &id, 3)?;
    assert!(
        mixed.contains("diagram") && mixed_drawing,
        "a mixed page has to keep BOTH: the text word for word and the look it had. It came back \
         as text {mixed:?} with a picture: {mixed_drawing}"
    );
    Ok(())
}

/// Dokument w bibliotece i wszystko, czego potrzebują jego strony: identyfikator i odcisk.
///
/// Katalog, z którego plik wzięto, wraca RAZEM z nimi: żyje tak długo, jak jego uchwyt, a
/// upuszczony tutaj zabrałby ze sobą plik, o którym mówi reszta przypadku.
fn a_document_in(
    library: &Library,
    operation: &str,
) -> Result<(tempfile::TempDir, String, String), Box<dyn std::error::Error>> {
    let downloads = tempfile::tempdir()?;
    let paper = put(
        downloads.path(),
        "paper.pdf",
        &fs::read(Path::new(FIXTURES).join("paper.pdf"))?,
    )?;
    let report = library.import(operation, vec![from_disk(&paper)])?;
    let id = report
        .results
        .first()
        .and_then(|one| one.added.first().cloned())
        .ok_or("the document was not added, so there is nothing to prepare")?;
    let fingerprint = library
        .sources()?
        .into_iter()
        .find(|source| source.id == id)
        .and_then(|source| source.file.map(|file| file.fingerprint))
        .ok_or("the document landed without a fingerprint to bind its pages to")?;
    Ok((downloads, id, fingerprint))
}

/// Tekst tej strony i to, czy ma obraz — po sprawdzeniu, że to JEST ta strona, o którą pytano.
fn page_parts(
    library: &Library,
    id: &str,
    number: u32,
) -> Result<(String, bool), Box<dyn std::error::Error>> {
    match library.part(id, Some(number))? {
        SourcePart::Page {
            number: answered,
            text,
            image,
            ..
        } => {
            assert_eq!(
                answered, number,
                "the preview was asked for page {number} and answered about page {answered}. \
                 A reference that points at another page is worse than none"
            );
            Ok((text, image.is_some()))
        }
        other => {
            let seen = format!("{other:?}");
            Err(format!("page {number} of a document answered with {seen}").into())
        }
    }
}

#[test]
fn a_result_from_an_earlier_import_does_not_land_on_the_newer_one()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    // Import, do którego należy plik leżący dziś w bibliotece. Jego numer jest zapisany razem
    // z bajtami i to O NIEGO pyta każde zatwierdzenie strony.
    let (_downloads, id, fingerprint) = a_document_in(&library, "op-later")?;
    finish_page(
        &library,
        &id,
        "op-later",
        &fingerprint,
        1,
        "Page one.",
        false,
    )?;

    // Okno, które przygotowywało ten sam plik pod POPRZEDNIM importem, odpowiada dopiero teraz.
    // Bajty się zgadzają i numer strony też — różni się wyłącznie operacja, i to ma wystarczyć.
    let late = PreparedPage {
        operation_id: "op-earlier".to_owned(),
        fingerprint: fingerprint.clone(),
        pages_total: PAGES_IN_THE_PAPER,
        number: 2,
        text: "Page two, from the import a person already replaced.".to_owned(),
        image: None,
        failed: None,
    };
    let answer = sources::complete_preparation(
        &library.root,
        &library.set,
        &id,
        &late,
        Some(&library.revision()?),
        CLICKED,
    );
    let described = format!("{answer:?}");
    assert!(
        answer.is_err(),
        "a page prepared for an import a person already replaced was taken anyway. Whoever \
         answers last then wins, and the file ends up holding pages from two different runs \
         through it: {described}"
    );

    let after = one_source(&library, &id)?;
    assert!(
        matches!(after.preparation, Preparation::Needs { pages_done: 1 }),
        "the late answer moved the work it did not belong to; the source now says {:?}",
        after.preparation
    );
    Ok(())
}

#[test]
fn a_document_that_will_not_open_keeps_a_named_state_on_disk()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    let (_downloads, id, fingerprint) = a_document_in(&library, "op-locked")?;

    // Lokalny worker odbił się od hasła. To NIE jest odmowa wywołania — to jest fakt o pliku,
    // który ma zostać na dysku.
    sources::complete_preparation(
        &library.root,
        &library.set,
        &id,
        &PreparedPage {
            operation_id: "op-locked".to_owned(),
            fingerprint: fingerprint.clone(),
            pages_total: 0,
            number: 0,
            text: String::new(),
            image: None,
            failed: Some(Unopenable::Locked),
        },
        Some(&library.revision()?),
        CLICKED,
    )?;

    // ŚWIEŻY odczyt z dysku, czyli to samo, co powrót na ekran po zamknięciu okna.
    let after = one_source(&library, &id)?;
    let said = match after.preparation {
        Preparation::Failed { ref said } => said.clone(),
        ref other => {
            let seen = format!("{other:?}");
            return Err(format!(
                "a file that cannot be opened came back as {seen}. Left as \"needs preparation\" \
                 it invites a person to press Prepare forever, and nothing ever says why"
            )
            .into());
        }
    };
    assert!(
        said.contains("password"),
        "a file locked with a password has to say THAT, not a general failure — the two are \
         fixed in different ways; it said: {said}"
    );

    // Uszkodzony plik dostaje SWOJE zdanie, inne niż zamknięty hasłem.
    let damaged = Unopenable::Damaged.to_string();
    assert_ne!(
        damaged, said,
        "a damaged file and a locked one are two different problems and may not share one \
         sentence, or the sentence tells a person nothing about what to do"
    );
    Ok(())
}

#[test]
fn a_page_over_the_derived_budget_is_refused_before_it_is_written()
-> Result<(), Box<dyn std::error::Error>> {
    let library = Library::fresh()?;
    let (_downloads, id, fingerprint) = a_document_in(&library, "op-full")?;

    // Zestaw, którego pochodne stoją już na suficie. Liczba idzie do szkicu, bo to szkic jest
    // rachunkiem tego budżetu — 512 MiB prawdziwych bajtów dowiodłoby tego samego i trwałoby
    // minutę. Oryginał zostaje nietknięty, więc drugi budżet ma dalej zapas.
    let mut draft = read_set(&library.root, &library.set)?.draft;
    if let Some(file) = draft
        .sources
        .iter_mut()
        .find(|source| source.id == id)
        .and_then(|source| source.file.as_mut())
    {
        file.derived = 512 * 1024 * 1024;
    }
    save_draft(
        &library.root,
        &DraftEdit {
            id: library.set.clone(),
            title: TITLE.to_owned(),
            description: String::new(),
            draft,
            expected_revision: Some(library.revision()?),
            at: CLICKED.to_owned(),
        },
    )?;

    let answer = sources::complete_preparation(
        &library.root,
        &library.set,
        &id,
        &PreparedPage {
            operation_id: "op-full".to_owned(),
            fingerprint,
            pages_total: PAGES_IN_THE_PAPER,
            number: 1,
            text: "Page one.".to_owned(),
            image: Some(PastedBytes {
                mime: "image/png".to_owned(),
                base64: SCREENSHOT_BASE64.trim().to_owned(),
            }),
            failed: None,
        },
        Some(&library.revision()?),
        CLICKED,
    );
    let described = format!("{answer:?}");
    assert!(
        answer.is_err(),
        "a page was taken into a set whose derived files are already at the ceiling. The budget \
         that is only checked when a file is added is a budget for a number that is zero at that \
         moment: {described}"
    );
    assert!(
        pages_of(&library, &id).is_empty(),
        "the refused page was written anyway, so the ceiling is announced after the bytes are \
         already on disk: {:?}",
        pages_of(&library, &id)
    );
    Ok(())
}

/// Nazwy plików, które naprawdę leżą w katalogu stron tego źródła.
fn pages_of(library: &Library, id: &str) -> Vec<String> {
    let Ok(folder) = folder_of(&library.root, &library.set) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(folder.join("sources").join(id).join("r1").join("pages")) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Jedno źródło zestawu, po identyfikatorze.
fn one_source(library: &Library, id: &str) -> Result<ContextSource, Box<dyn std::error::Error>> {
    library
        .sources()?
        .into_iter()
        .find(|source| source.id == id)
        .ok_or_else(|| format!("no source under {id} in this set").into())
}

/// Jedna strona tak, jak przywozi ją lokalny worker: numer, tekst i wygląd.
///
/// Trzy strony na dokument, bo to najmniejsza liczba, przy której „wznów od brakującej" różni
/// się od „zacznij od nowa" i od „zrób ostatnią".
const PAGES_IN_THE_PAPER: u32 = 3;

/// Zatwierdza jedną stronę pod bieżącą rewizją szkicu i pod nazwanym importem.
fn finish_page(
    library: &Library,
    id: &str,
    operation: &str,
    fingerprint: &str,
    number: u32,
    text: &str,
    drawn: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let drawing = if drawn {
        Some(PastedBytes {
            mime: "image/png".to_owned(),
            base64: SCREENSHOT_BASE64.trim().to_owned(),
        })
    } else {
        None
    };
    sources::complete_preparation(
        &library.root,
        &library.set,
        id,
        &PreparedPage {
            operation_id: operation.to_owned(),
            fingerprint: fingerprint.to_owned(),
            pages_total: PAGES_IN_THE_PAPER,
            number,
            text: text.to_owned(),
            image: drawing,
            failed: None,
        },
        Some(&library.revision()?),
        CLICKED,
    )?;
    Ok(())
}
