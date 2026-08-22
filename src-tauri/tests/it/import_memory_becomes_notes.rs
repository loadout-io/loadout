//! AC-3 dla T-80: zaimportowana pamięć jest **notatką**, nie akapitem w instrukcji.
//!
//! `.claude/agent-memory/` i `.claude/learnings/` są dziś pozycjami rodzaju `ItemKind::Memory`,
//! czyli **wyborem do rozstrzygnięcia** (`import/adapters.rs`) i niczym poza tym. `MigrationDraft`
//! dostał w commicie kontraktowym pole `notes`, ale `translate` zostawia je pustą listą, a
//! `apply` nie zapisuje do `memory/notes/` ani jednego pliku. Skutek jest podwójny: wiedza
//! jednego agenta jedzie w **stałej** instrukcji w każdym jego promptcie, a ta sama treść
//! potrafi wejść drugi raz przez learnings.
//!
//! **Słabą wersją tego kryterium jest `assert!(!draft.notes.is_empty())`.** Przechodzi dla
//! implementacji, która wkłada tam jedno zdanie bez pochodzenia — a notatka bez pochodzenia
//! jest zdaniem, którego nie da się ani sprawdzić, ani wycofać [T6 §5.1]. Dlatego każda notatka
//! niżej jest sądzona za ścieżkę źródłową, odcisk, zakres, właściciela i to, z czyjego katalogu
//! przyszła.
//!
//! **Drugą słabą wersją jest sprawdzenie samego draftu.** Draft jest wartością w pamięci; pytanie
//! brzmi „czy w bibliotece leży plik notatki", a na to odpowiada wyłącznie dysk po `apply`
//! (niezmiennik 4: plik jest prawdą). Dlatego drugi test czyta katalog notatek, a nie strukturę.
//!
//! **Trzecią jest brak drugiej strony porównania.** Import, który robi z pamięci notatkę I ZOSTAWIA
//! tę samą treść w instrukcjach agenta, kosztuje dwa razy w każdej turze i wygląda w drafcie
//! poprawnie. Dlatego reguła notatki jest tu porównywana z instrukcjami KAŻDEGO agenta.
//!
//! ŚCIEŻKA WYPROWADZAJĄCA POZA KORZEŃ. Indeks `MEMORY.md` wskazuje pliki względne i to jest
//! normalne. `../../../../` też jest ścieżką względną — i prowadzi do cudzego repozytorium obok.
//! Import nie ma prawa jej przeczytać, a pominięcie w ciszy wygląda na ekranie identycznie jak
//! odczyt, więc raport ma ją nazwać.

// `unwrap()`/`expect()` w teście: panika w teście JEST jego wynikiem, a `?` w tej samej linii
// zamieniłby nazwany komunikat asercji w bezimienne `Err`. `checks/full-clippy.sh` biegnie
// `--all-targets -- -D warnings`, więc bez tej linii ląduje to w bramce, nie tutaj.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use loadout_lib::commands::memory::notes_root;
use loadout_lib::import::{ImportPreview, ItemKind, MemoryNote, SourceItem, SourceKind};
use loadout_lib::memory::notes::scan_notes;
use tempfile::TempDir;

/// Znaczniki treści. Na tyle dziwne, żeby nie mogły powstać z żadnego innego fragmentu tekstu.
const QUEUE: &str = "MOOSE-THE-QUEUE-IS-DRAINED-IN-ONE-PLACE";
const TENANT: &str = "MOOSE-THE-TENANT-IS-RESOLVED-BEFORE-THE-GUARD";
const BEYOND: &str = "MOOSE-SOMEBODY-ELSES-REPOSITORY";

/// Nazwa agenta tak, jak zapisał ją człowiek w katalogu pamięci.
const OWNER: &str = "backend-dev";

/// Plik, którego ten import nie ma prawa przeczytać. Nazwa jest tym, czego szukamy w raporcie:
/// każde uczciwe nazwanie pominiętej ścieżki niesie ją w sobie, choćby ją po drodze znormalizowało.
const FORBIDDEN_FILE: &str = "secret.md";

/// Jak indeks pamięci wskazuje pliki obok siebie — i jak wskazuje coś, czego wskazywać nie wolno.
///
/// Cztery kropkodwójki z `.claude/agent-memory/backend-dev/` wychodzą **nad** korzeń importu:
/// `backend-dev` → `agent-memory` → `.claude` → korzeń → rodzic korzenia.
const INDEX: &str = "# What backend-dev learned here\n\
                     \n\
                     - [The queue is drained in one place](queue.md) — one drain, one place\n\
                     - [Somebody else's repository](../../../../outside/secret.md) — not ours\n";

fn memory_page(sentinel: &str, title: &str, why: &str) -> String {
    format!(
        "# {title}\n\
         \n\
         {sentinel} and this is the sentence that would reach the model.\n\
         \n\
         Why: {why}\n"
    )
}

const AGENT_FILE: &str = "---\n\
                          name: backend-dev\n\
                          description: Works where the data is\n\
                          model: opus\n\
                          tools: [Read, Write]\n\
                          ---\n\
                          Build the backend and leave the interface alone.\n";

/// Repo z pamięcią dwóch kształtów: katalog agenta z indeksem i płaski katalog learnings.
///
/// Korzeń importu jest PODKATALOGIEM tymczasowego katalogu, bo `outside/secret.md` ma leżeć
/// poza nim — a plik, którego nie ma, byłby pominięty także przez import, który niczego nie
/// pilnuje, i ta asercja nie odróżniałaby wtedy niczego od niczego.
struct Repo {
    home: TempDir,
}

impl Repo {
    fn new(with_the_forbidden_link: bool) -> Result<Self, Box<dyn Error>> {
        let home = TempDir::new()?;
        let root = home.path().join("repo");
        let owned = root.join(".claude").join("agent-memory").join(OWNER);
        fs::create_dir_all(&owned)?;
        fs::create_dir_all(root.join(".claude").join("agents"))?;
        fs::create_dir_all(root.join(".claude").join("learnings"))?;
        fs::create_dir_all(home.path().join("outside"))?;

        fs::write(root.join(".claude/agents/backend-dev.md"), AGENT_FILE)?;
        fs::write(
            owned.join("MEMORY.md"),
            if with_the_forbidden_link {
                INDEX.to_owned()
            } else {
                INDEX
                    .lines()
                    .filter(|line| !line.contains(FORBIDDEN_FILE))
                    .collect::<Vec<_>>()
                    .join("\n")
            },
        )?;
        fs::write(
            owned.join("queue.md"),
            memory_page(
                QUEUE,
                "The queue is drained in one place",
                "two runs put the same job through twice before anybody noticed",
            ),
        )?;
        fs::write(
            root.join(".claude/learnings/tenant.md"),
            memory_page(
                TENANT,
                "The tenant is resolved before the guard",
                "auth.e2e.spec.ts:88 reproduced it on the second try",
            ),
        )?;
        fs::write(
            home.path().join("outside").join(FORBIDDEN_FILE),
            memory_page(
                BEYOND,
                "Somebody else's repository",
                "this file belongs to a project nobody asked Loadout to read",
            ),
        )?;
        Ok(Self { home })
    }

    fn root(&self) -> PathBuf {
        self.home.path().join("repo")
    }

    /// Świeża biblioteka, do której `apply` ma dopiero zapisać.
    fn library(&self) -> PathBuf {
        self.home.path().join("library")
    }
}

/// Notatka, której reguła niesie ten znacznik.
fn note_carrying<'a>(preview: &'a ImportPreview, sentinel: &str) -> Option<&'a MemoryNote> {
    preview
        .draft
        .notes
        .iter()
        .find(|note| note.rule.contains(sentinel))
}

/// Pozycja skanu, z której ta notatka mogła przyjść: albo dokładnie ten plik, albo wpis
/// zbiorczy stojący nad katalogiem, w którym ten plik leży (`MEMORY.md` niesie odcisk całego
/// katalogu, bo pliki obok niego nie są osobnymi pozycjami).
fn covering_item<'a>(preview: &'a ImportPreview, note: &MemoryNote) -> Option<&'a SourceItem> {
    preview.snapshot.items.iter().find(|item| {
        item.kind == ItemKind::Memory
            && (item.path == note.source
                || item
                    .path
                    .parent()
                    .is_some_and(|dir| !dir.as_os_str().is_empty() && note.source.starts_with(dir)))
    })
}

/// Wszystko, co ta notatka niesie jako tekst. Znacznik spoza korzenia nie ma prawa stać w żadnym
/// z tych pól — nie tylko w regule.
fn every_word_of(note: &MemoryNote) -> String {
    format!(
        "{} | {} | {} | {} | {}",
        note.title,
        note.rule,
        note.because,
        note.scope,
        note.source.display()
    )
}

#[test]
fn another_apps_memory_arrives_as_notes_that_say_where_they_came_from() -> Result<(), Box<dyn Error>>
{
    let repo = Repo::new(true)?;
    let root = repo.root();
    let preview = loadout_lib::import::translate::preview(&root)?;

    assert!(
        !preview.draft.notes.is_empty(),
        "this project keeps memory for one agent and a folder of learnings, and the import came \
         back with no notes at all. Until it does, the only place that knowledge can land is the \
         agent's instructions — where it rides in EVERY prompt that agent ever gets, cannot be \
         retired one sentence at a time, and can arrive twice by two roads. What the scan did see \
         was: {:?}",
        preview
            .snapshot
            .items
            .iter()
            .filter(|item| item.kind == ItemKind::Memory)
            .map(|item| item.path.display().to_string())
            .collect::<Vec<_>>()
    );

    // ── (a) NOTATKA JEDNEGO AGENTA, Z CAŁYM POCHODZENIEM ─────────────────────────────────
    let owned = note_carrying(&preview, QUEUE).ok_or_else(|| {
        format!(
            "nothing in the imported notes carries what this agent's own memory folder says. \
             The notes that did come back read: {:?}",
            preview
                .draft
                .notes
                .iter()
                .map(|note| note.rule.clone())
                .collect::<Vec<_>>()
        )
    })?;

    assert_eq!(
        owned.agent.as_deref(),
        Some(OWNER),
        "the file sits in that agent's own memory folder and the note came back owned by {:?}. \
         A note that cannot say whose it is has no scope narrower than the whole project, so the \
         one thing the folder was telling us is the first thing the import throws away",
        owned.agent
    );
    assert_eq!(
        owned.scope, "this-agent",
        "and the scope has to agree with the owner. A note with a name on it and a scope that \
         reaches every step in the project is delivered to more prompts than anybody agreed to, \
         and nothing on any screen says the reach was widened"
    );
    assert!(
        owned.source.is_relative(),
        "the source path is absolute ({}). An absolute path is a fact about the machine that \
         happened to run the scan, not about the note — copy the library to another machine and \
         it stops being true without anybody touching it",
        owned.source.display()
    );
    assert!(
        root.join(&owned.source).is_file(),
        "and the path has to point at the file this sentence really came from, relative to the \
         imported project. {} does not resolve to a file under {}",
        owned.source.display(),
        root.display()
    );

    let covering = covering_item(&preview, owned).ok_or_else(|| {
        format!(
            "the note names {} as its source, and the scan reported no memory item covering that \
             path. Provenance nobody can walk back to a scanned file is decoration",
            owned.source.display()
        )
    })?;
    assert_eq!(
        owned.source_hash,
        covering.hash,
        "the note carries a source fingerprint that does not match the one the scan took of {}. \
         The fingerprint is the whole answer to \"is the sentence Loadout copied still the \
         sentence that project has\" — a value invented at write time answers it with yes forever",
        covering.path.display()
    );
    assert_eq!(
        owned.app,
        SourceKind::Claude,
        "and the note says which app's folder it was lifted out of. Two apps keep memory in two \
         places and the same sentence can sit in both; without this field the second copy looks \
         like a second fact"
    );
    assert!(
        !owned.because.trim().is_empty(),
        "and it says why it is true. No because, no memory [T6 §10.3] holds for a sentence \
         Loadout imported exactly as hard as for one somebody typed: an instruction without a \
         reason cannot be retired later without deriving its interaction with every other one"
    );

    // ── (b) NOTATKA NICZYJA ZOSTAJE NICZYJA ──────────────────────────────────────────────
    let shared = note_carrying(&preview, TENANT).ok_or(
        "the project's learnings folder reached no note. It is memory too, and it is the road by \
         which the same sentence gets imported a second time",
    )?;
    assert_eq!(
        shared.agent, None,
        "this sentence sits in a folder that belongs to nobody, and the import gave it an owner \
         ({:?}). An implementation that answers \"whose is this\" with the last name it saw \
         passes every assertion above and quietly narrows a project-wide fact down to one agent",
        shared.agent
    );
    assert_ne!(
        shared.scope, "this-agent",
        "and a note nobody owns cannot have the scope of one agent: that pair names an agent that \
         does not exist, and the note reaches no step at all"
    );
    assert_eq!(
        shared.source,
        Path::new(".claude").join("learnings").join("tenant.md"),
        "and it points back at the file it came from"
    );

    // ── (c) TA SAMA TREŚĆ NIE STOI DRUGI RAZ W INSTRUKCJACH ──────────────────────────────
    for agent in &preview.draft.agents {
        assert!(
            !agent.instructions.contains(&owned.rule),
            "the sentence is a note AND a paragraph in {}'s instructions. Then it rides in every \
             prompt that agent gets whether or not anybody put it to use, it is counted against \
             no ceiling, and turning the note off changes nothing. The instructions read:\n{}",
            agent.name,
            agent.instructions
        );
        for sentinel in [QUEUE, TENANT] {
            assert!(
                !agent.instructions.contains(sentinel),
                "the memory folder's text was pasted into {}'s instructions verbatim. The \
                 instructions read:\n{}",
                agent.name,
                agent.instructions
            );
        }
    }

    // ── (d) INDEKS WSKAZUJE PLIKI OBOK, NIGDY CUDZE REPOZYTORIUM ─────────────────────────
    for note in &preview.draft.notes {
        assert!(
            !every_word_of(note).contains(BEYOND),
            "the index pointed four levels up, out of the imported project, and the import \
             followed it. Every relative link in a file somebody else wrote is an instruction to \
             read a path of their choosing; the only safe reading is the one that stops at the \
             root the person picked. The note reads:\n{}",
            every_word_of(note)
        );
    }
    for agent in &preview.draft.agents {
        assert!(
            !agent.instructions.contains(BEYOND),
            "a file from outside the imported project reached {}'s instructions",
            agent.name
        );
    }
    assert!(
        preview
            .draft
            .report
            .mappings
            .iter()
            .any(|mapping| mapping.message.contains(FORBIDDEN_FILE)),
        "the path that leaves the root was skipped and named nowhere a person can read. Skipped \
         in silence looks exactly like read-and-found-nothing, so the person who asked for this \
         import cannot tell whether their memory came over or was dropped. The report says: {:?}",
        preview
            .draft
            .report
            .mappings
            .iter()
            .map(|mapping| mapping.message.clone())
            .collect::<Vec<_>>()
    );

    Ok(())
}

#[test]
fn imported_memory_lands_in_the_library_as_note_files() -> Result<(), Box<dyn Error>> {
    let repo = Repo::new(false)?;
    let library = repo.library();
    let preview = loadout_lib::import::translate::preview(&repo.root())?;

    let saved = loadout_lib::import::apply::apply(&library, &preview.draft).map_err(|refusal| {
        format!(
            "saving this import wrote nothing, and the project it read has memory in it: \
             {refusal}. Memory that is still an open question at the end of an import is memory \
             that never becomes a file — which is the state this criterion exists to end. The \
             report says: {:?}",
            preview
                .draft
                .report
                .mappings
                .iter()
                .map(|mapping| (mapping.compatibility, mapping.message.clone()))
                .collect::<Vec<_>>()
        )
    })?;

    let root = notes_root(&library);
    let landed = scan_notes(&root)
        .map_err(|error| format!("{} could not be read back: {error}", root.display()))?;

    for sentinel in [QUEUE, TENANT] {
        assert!(
            landed.iter().any(|note| note.rule.contains(sentinel)),
            "the imported memory is not a note file in the library. A draft that holds the \
             sentence and a folder that does not is the same thing as no import at all: the next \
             run reads the folder. The library now holds {:?}, and these files were saved: {:?}",
            landed
                .iter()
                .map(|note| note.id.to_string())
                .collect::<Vec<_>>(),
            saved.written
        );
    }

    let owned = landed
        .iter()
        .find(|note| note.rule.contains(QUEUE))
        .ok_or("the note this agent owns is not in the library")?;
    let text = fs::read_to_string(&owned.path)
        .map_err(|error| format!("the note file could not be opened: {error}"))?;
    assert!(
        text.contains(&format!("agent: {OWNER}")),
        "the owner has to be IN THE FILE, on its own line of the front-matter. A value that lives \
         only in the value the writer returned is gone the moment anybody reads the folder again, \
         and from that moment the note has the scope of one agent and belongs to nobody. The file \
         reads:\n{text}"
    );
    assert!(
        text.contains("scope: this-agent"),
        "and so is the scope it was imported with. The file reads:\n{text}"
    );

    Ok(())
}
