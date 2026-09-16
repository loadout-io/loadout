//! Atomowe zapisanie zatwierdzonego draftu.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::library::agents::write_agent_file;
use crate::skills::ingest;

use super::{ADAPTER_VERSION, ImportError, MigrationDraft, Result, translate};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReceipt {
    pub id: String,
    pub adapter_version: u32,
    pub source_hashes: BTreeMap<PathBuf, String>,
    pub workflow_hashes: BTreeMap<PathBuf, String>,
    pub written: Vec<PathBuf>,
    /// Docelowy plik -> źródło i oba odciski potrzebne do bezpiecznego reimportu.
    #[serde(default)]
    pub files: BTreeMap<PathBuf, ImportedFileReceipt>,
    pub enabled_connections: Vec<String>,
    /// Agenci, którym ten import dopisał nazwy połączeń — pusto, kiedy nie dopisał nikomu.
    ///
    /// 2026-09-16 — PLIK NA DYSKU NIESIE TĘ SAMĄ LISTĘ, CO OKNO. Do tego dnia było odwrotnie
    /// i opisane tu jako własność: `stage_all` zapisuje paragon z pustą listą, bo przebieg po
    /// agentach, którzy leżeli w bibliotece przed importem, biegnie dopiero po atomowym
    /// przeniesieniu. Tyle że plik, który człowiek otwiera w edytorze, mówił przez to, że nikt
    /// niczego nie dostał — podczas gdy na tym samym dysku dwa pliki agentów naprawdę dostały
    /// cztery połączenia. Listę dopina [`record_filled_agents`], po przeniesieniu i po
    /// dopełnieniu. `#[serde(default)]`, bo paragon zapisany przed tym dniem tego klucza nie ma,
    /// a jego brak znaczy dokładnie „nikomu niczego nie dopisano".
    #[serde(default)]
    pub filled_agents: Vec<crate::connections::fill::FilledAgent>,
    pub vendor_configurations: crate::connections::runtime::VendorConfigurations,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFileReceipt {
    pub source_path: PathBuf,
    pub source_hash: String,
    pub written_hash: String,
}

/// Zapisuje cały zaakceptowany setup albo nie zostawia żadnego z jego plików.
pub fn apply(home: &Path, draft: &MigrationDraft) -> Result<ImportReceipt> {
    apply_with_hook(home, draft, |_| Ok(()))
}

/// Hak jest miejscem fault-injection dla kryterium atomowości, nie polityką produktu.
pub fn apply_with_hook<F>(
    home: &Path,
    draft: &MigrationDraft,
    mut after_move: F,
) -> Result<ImportReceipt>
where
    F: FnMut(usize) -> std::result::Result<(), String>,
{
    if !draft.runnable() {
        let blockers = if draft.items.is_empty() {
            draft.report.blockers()
        } else {
            draft
                .items
                .iter()
                .filter(|item| item.status != super::ImportStatus::Ready)
                .count()
        };
        return Err(ImportError::Blocked(blockers));
    }
    let fresh = translate::preview(&draft.root)?;
    if fresh.draft.source_hashes != draft.source_hashes {
        return Err(ImportError::Changed);
    }

    let receipt_id = Uuid::now_v7().to_string();
    preflight(home, draft)?;
    fs::create_dir_all(home).map_err(save_error)?;
    let stage = home.join(format!(".import-{receipt_id}.staging"));
    fs::create_dir(&stage).map_err(save_error)?;

    let result = stage_all(&stage, draft, &receipt_id)
        .and_then(|receipt| commit(home, &stage, receipt, &mut after_move));
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(save_error)?;
    }
    result
}

/// Co ten plan zapisze w bibliotece — jedna odpowiedź na pytanie, które zadają dwie drogi.
///
/// [`preflight`] pyta o to tuż przed zapisem, a [`mark_what_the_library_already_has`] — zanim
/// ekran w ogóle pokaże plan. Dwa rachunki tych samych ścieżek rozjechałyby się przy pierwszej
/// zmianie którejkolwiek nazwy pliku, a rozjazd byłoby widać dopiero jako import, który odmawia
/// zapisania czegoś, o czym ekran mówił, że tego nie ma.
fn would_write(draft: &MigrationDraft) -> Vec<PathBuf> {
    let mut targets = Vec::new();
    targets.extend(
        draft
            .agents
            .iter()
            .map(|agent| PathBuf::from("agents").join(format!("{}.md", slug(&agent.name)))),
    );
    targets.extend(
        draft
            .skills
            .iter()
            .map(|skill| PathBuf::from("skills").join(&skill.name)),
    );
    targets.extend(
        draft
            .connections
            .iter()
            .map(|connection| PathBuf::from("connections").join(format!("{}.json", connection.id))),
    );
    targets.extend(
        draft.workflows.iter().map(|workflow| {
            PathBuf::from("workflows").join(format!("{}.json", slug(&workflow.name)))
        }),
    );
    // Nazwa pliku notatki jest funkcją jej tytułu (`memory::slugify`), więc dwie strony pamięci
    // o tym samym tytule wskazują jeden plik. Bez tej linii druga po cichu podbiłaby licznik
    // wystąpień pierwszej i zniknęłaby jako osobne zdanie.
    targets.extend(draft.notes.iter().map(|note| note_target(&note.title)));
    targets
}

/// Wpisuje w plan to, na co odpowiada wyłącznie dysk biblioteki: czego już nie trzeba wnosić.
///
/// # Dlaczego to nie jest praca [`preflight`]
///
/// 2026-09-16 — bo odpowiedź jest znana PRZY SKANIE, a padała jako odmowa całego zapisu.
/// Drugi import tego samego projektu wywracał się na pierwszym pliku, który już leżał
/// w bibliotece („agents/project-manager-backlog.md already exists. Nothing was imported."),
/// a ekran nie oznaczał ani jednego z wierszy, które przyjechały przy pierwszym imporcie.
/// Stan „już to mam" jest normalny; awaryjny jest dopiero wtedy, kiedy człowiek każe wnieść
/// to jeszcze raz — i wtedy dalej odmawia [`preflight`], bo nadpisania tu nie ma
/// (`library::agents::write_agent_file` bez oczekiwanej rewizji odmawia istniejącemu plikowi,
/// a `commit` przenosi przez `fs::rename`).
///
/// # Dlaczego WSZYSTKIE pliki pozycji, a nie którykolwiek
///
/// Bo jedna pozycja bywa kilkoma plikami: strona pamięci wzięta z wiązki `MEMORY.md` i plik
/// `.mcp.json` z trzema serwerami. Biblioteka, która ma połowę z nich, nie jest biblioteką,
/// która ma tę pozycję — takiej dalej odmawia [`preflight`], zamiast po cichu wnieść połowę.
pub fn mark_what_the_library_already_has(home: &Path, draft: &mut MigrationDraft) {
    draft.already_in_the_library = would_write(draft)
        .into_iter()
        .filter(|target| home.join(target).exists())
        .collect();
    // Dwa przebiegi, bo `files_of` czyta CAŁY draft (notatki i połączenia stoją w jego wektorach),
    // a pisać trzeba do pozycji w tym samym drafcie.
    let already: Vec<bool> = draft
        .items
        .iter()
        .map(|item| {
            let files = files_of(draft, item);
            !files.is_empty() && files.iter().all(|file| home.join(file).exists())
        })
        .collect();
    for (item, here) in draft.items.iter_mut().zip(already) {
        item.already_here = here;
    }
}

/// Które pliki biblioteki należą do TEJ pozycji.
///
/// Dwa rodzaje mają ich więcej niż jeden i oba liczą się z wektorów draftu, a nie z `target`:
/// wiersz pamięci wzięty z wiązki `MEMORY.md` odpowiada za wszystkie notatki tamtego katalogu,
/// a jeden `.mcp.json` bywa trzema plikami połączeń. `target` mówi wtedy goły katalog
/// (`memory/notes`, `connections`), czyli ścieżkę, której żaden powstały plik nie ma.
fn files_of(draft: &MigrationDraft, item: &super::ImportItem) -> Vec<PathBuf> {
    match item.kind {
        super::ItemKind::Memory => draft
            .notes
            .iter()
            .filter(|note| item.sources.iter().any(|source| source.path == note.source))
            .map(|note| note_target(&note.title))
            .collect(),
        super::ItemKind::Connection => draft
            .connections
            .iter()
            .filter(|connection| {
                item.sources
                    .iter()
                    .any(|source| source.path == connection.source)
            })
            .map(|connection| PathBuf::from("connections").join(format!("{}.json", connection.id)))
            .collect(),
        super::ItemKind::Agent | super::ItemKind::Skill | super::ItemKind::Workflow => {
            item.target.clone().into_iter().collect()
        }
    }
}

fn preflight(home: &Path, draft: &MigrationDraft) -> Result<()> {
    let mut unique = BTreeSet::new();
    let mut clashing = BTreeSet::new();
    for target in would_write(draft) {
        if !unique.insert(target.clone()) {
            return Err(ImportError::Save(format!(
                "Two imported items would both become {}. Choose different names before importing.",
                target.display()
            )));
        }
        if home.join(&target).exists() {
            clashing.insert(target);
        }
    }
    if clashing.is_empty() {
        return Ok(());
    }
    Err(ImportError::Save(already_in_the_library_says(
        rows_that_clash(draft, &clashing),
        clashing.len(),
    )))
}

/// Ile WIERSZY planu koliduje — nie ile plików.
///
/// 2026-09-17 — jedna pozycja bywa kilkoma plikami, więc licznik plików podany jako licznik
/// wierszy każe człowiekowi szukać trzech ptaszków tam, gdzie stoi jeden. Zmierzone na wiązce
/// `.claude/agent-memory/<agent>/MEMORY.md`: dwie strony pamięci, JEDEN wiersz na ekranie.
///
/// Dopasowanie po przedrostku, a nie po równości, bo [`would_write`] nazywa wiązkę umiejętności
/// jej KATALOGIEM (`skills/<nazwa>` — to jest ścieżka, którą przenosi `commit`), a wiersz nazywa
/// swój plik (`skills/<nazwa>/SKILL.md`). Bez tego wiersz umiejętności wypadałby z licznika.
fn rows_that_clash(draft: &MigrationDraft, clashing: &BTreeSet<PathBuf>) -> usize {
    draft
        .items
        .iter()
        .filter(|item| {
            files_of(draft, item).iter().any(|file| {
                clashing
                    .iter()
                    .any(|clash| file == clash || file.starts_with(clash))
            })
        })
        .count()
}

/// Odmowa dla planu, który zapisałby pliki leżące już w bibliotece — licznikiem, nie pierwszą
/// ścieżką, na którą trafiliśmy.
///
/// 2026-09-16 — do tego dnia to zdanie nazywało jeden plik z pięćdziesięciu i kończyło się na
/// nim, więc drugi import tego samego projektu trzeba było uruchamiać raz na kolidujący plik,
/// żeby w ogóle poznać ich listę. Ta odmowa jest dziś OSTATNIĄ linią obrony, a nie pierwszą:
/// wiersze, których biblioteka już ma, wracają ze skanu odznaczone
/// ([`mark_what_the_library_already_has`]), więc żeby tu dojść, trzeba było je zaznaczyć
/// z powrotem — czyli jawnie poprosić o wniesienie ich jeszcze raz.
///
/// DWIE LICZBY, BO TO SĄ DWA RÓŻNE FAKTY (2026-09-17). Pliki mówią, ile rzeczy w bibliotece ten
/// import by ruszył; wiersze mówią, ile ptaszków trzeba zdjąć. `rows == 0` znaczy „tych plików
/// nie da się przypisać do żadnego wiersza" — tak wygląda ręcznie złożony draft sprzed `items`,
/// i wtedy zdanie nie każe szukać ptaszka, którego nie ma.
fn already_in_the_library_says(rows: usize, files: usize) -> String {
    let next = if rows == 0 {
        "Move those files out of your library first.".to_owned()
    } else {
        format!(
            "Untick the {rows} row(s) marked Already in your library, or move those files out of \
             your library first."
        )
    };
    format!(
        "Nothing was imported: your library already has {files} of the file(s) this import would \
         write, and Loadout does not replace files it did not write. {next}"
    )
}

fn stage_all(stage: &Path, draft: &MigrationDraft, receipt_id: &str) -> Result<ImportReceipt> {
    let mut written = Vec::new();
    let mut workflow_hashes = BTreeMap::new();
    let mut files = BTreeMap::new();
    stage_agents(stage, draft, &mut written, &mut files)?;
    stage_skills(stage, draft, &mut written, &mut files)?;
    stage_connections(stage, draft, &mut written, &mut files)?;
    stage_notes(stage, draft, &mut written, &mut files)?;
    stage_workflows(stage, draft, &mut written, &mut files, &mut workflow_hashes)?;

    written.sort();
    let mut enabled_connections: Vec<String> = draft
        .connections
        .iter()
        .filter(|connection| connection.enabled)
        .map(|connection| connection.id.clone())
        .collect();
    enabled_connections.sort();
    let receipt_path = receipt_target(receipt_id);
    written.push(receipt_path.clone());
    let receipt = ImportReceipt {
        id: receipt_id.to_owned(),
        adapter_version: ADAPTER_VERSION,
        source_hashes: draft.source_hashes.clone(),
        workflow_hashes,
        written,
        files,
        enabled_connections,
        // Pusto: kto co dostał, wie dopiero `commands::import`, po przeniesieniu plików —
        // i dopisuje to tutaj z powrotem przez [`record_filled_agents`].
        filled_agents: Vec::new(),
        vendor_configurations: crate::connections::runtime::for_connections(&draft.connections),
    };
    write_json(&stage.join(&receipt_path), &receipt)?;
    Ok(receipt)
}

/// Gdzie w bibliotece leży paragon tego importu. Jedno miejsce na tę odpowiedź, bo pytają o nią
/// dwie drogi: [`stage_all`], która paragon tworzy, i [`record_filled_agents`], która go dopina.
fn receipt_target(receipt_id: &str) -> PathBuf {
    PathBuf::from("imports").join(format!("{receipt_id}.json"))
}

/// Dopisuje do paragonu NA DYSKU listę agentów, którym ten import dopisał połączenia.
///
/// Wołane po atomowym przeniesieniu i po dopełnieniu agentów zastanych w bibliotece
/// (`commands::import`) — czyli w jedynej chwili, w której ta lista jest już pełna. Powód
/// w całości stoi przy [`ImportReceipt::filled_agents`].
///
/// Zapis idzie przez plik obok i `rename`, a nie prosto w cel: paragon nadpisywany w miejscu
/// jest tym samym plikiem, który czyta reimport, więc przerwanie w połowie zostawiłoby po sobie
/// połowę pliku zamiast poprzedniej — prawdziwej, choć niepełnej — treści.
pub fn record_filled_agents(home: &Path, receipt: &ImportReceipt) -> Result<()> {
    let target = home.join(receipt_target(&receipt.id));
    let writing = home
        .join("imports")
        .join(format!("{}.json.writing", receipt.id));
    write_json(&writing, receipt)?;
    fs::rename(&writing, &target).map_err(save_error)
}

fn stage_agents(
    stage: &Path,
    draft: &MigrationDraft,
    written: &mut Vec<PathBuf>,
    files: &mut BTreeMap<PathBuf, ImportedFileReceipt>,
) -> Result<()> {
    for agent in &draft.agents {
        // `None`: katalog przygotowania powstaje świeżo na każdy import (`fs::create_dir`
        // w `apply_setup_inner`), więc każdy plik ma tu powstać, a nie kogokolwiek nadpisać.
        // Dwaj agenci o tej samej nazwie pliku są wtedy odmową, a nie cichą stratą jednego z nich.
        let landed = write_agent_file(&stage.join("agents"), agent, None)
            .map_err(|error| ImportError::Save(error.to_string()))?;
        let target = relative(stage, &landed.path)?;
        if let Some(source) = source_for_target(draft, &target)? {
            record_file(stage, draft, &target, &source.path, &source.hash, files)?;
        }
        written.push(target);
    }
    Ok(())
}

fn stage_skills(
    stage: &Path,
    draft: &MigrationDraft,
    written: &mut Vec<PathBuf>,
    files: &mut BTreeMap<PathBuf, ImportedFileReceipt>,
) -> Result<()> {
    for skill in &draft.skills {
        let imported = ingest::from_folder(&skill.source_dir)
            .map_err(|error| ImportError::Save(error.to_string()))?;
        /* ODMOWA MIESZKA TUTAJ, A NIE W OKNIE (2026-08-31, niezmiennik 23).
         *
         * Do tego dnia te trzy linie nie istniały, a `imported` — który NIESIE wynik przeglądu —
         * służył wyłącznie do wyliczenia dołączonych plików. Werdykt czytał adapter przy skanie
         * (`adapters::skill`) i okno przy stopce (`blocked > 0` w `setup.tsx`), czyli polityka
         * bezpieczeństwa stała po obu stronach granicy IPC i po żadnej w rdzeniu. Każdy inny
         * wołający `apply` — a to jest funkcja `pub` — wnosił wtedy na dysk umiejętność
         * z ukrytym tekstem albo z linią wysyłającą dane, bo kopia `SKILL.md` szła
         * BEZWARUNKOWO, dwie linie niżej.
         *
         * ZAWĘŻENIE ŚWIADOME: odmawiamy ZAWSZE, także wtedy, gdy człowiek znaleziska przeczytał.
         * Zgoda per znalezisko istnieje po drugiej stronie (`acknowledge` w `src/state/skills.ts`),
         * ale nie ma nośnika w `ApplySetup` i nie ma ekranu, który by te znaleziska pokazał —
         * furtka bez ekranu byłaby zgodą na coś, czego człowiek nie widział. Nośnik dokłada
         * `ImportItem::reviewed`; zgoda jest osobnym ruchem i osobnym polem.
         */
        if imported.reviewed.verdict == ingest::Verdict::Blocked {
            return Err(ImportError::Unsafe {
                skill: skill.name.clone(),
                reason: blocked_reason(&imported.reviewed),
            });
        }
        let destination = stage.join("skills").join(&skill.name);
        fs::create_dir_all(&destination).map_err(save_error)?;
        // Review rozstrzyga bezpieczeństwo bundle, ale import jest migawką. Ponowna emisja
        // frontmatteru gubiłaby komentarze i formatowanie, choć raport obiecuje zachowanie pliku.
        let source = skill.source_dir.join("SKILL.md");
        let target = PathBuf::from("skills").join(&skill.name).join("SKILL.md");
        fs::copy(&source, destination.join("SKILL.md")).map_err(save_error)?;
        record_source_file(stage, draft, &target, &source, files)?;
        written.push(target);
        for bundled in &imported.skill.files {
            let target = destination.join(&bundled.relative);
            let parent = target.parent().ok_or_else(|| {
                ImportError::Save("A bundled skill file has no parent folder.".to_owned())
            })?;
            fs::create_dir_all(parent).map_err(save_error)?;
            fs::copy(&bundled.source, &target).map_err(save_error)?;
            let relative = PathBuf::from("skills")
                .join(&skill.name)
                .join(&bundled.relative);
            record_source_file(stage, draft, &relative, &bundled.source, files)?;
            written.push(relative);
        }
    }
    Ok(())
}

/// Dlaczego ta umiejętność nie weszła — zdaniem, które nazywa MIEJSCE, nie regułę.
///
/// Id reguł (`hidden-text`, `exfiltration`, …) są enumem z drutu i nigdy nie trafiają na ekran
/// (niezmiennik 14); ich angielskie zdania stoją w jednym miejscu, po stronie okna
/// (`src/sections/skills/review-card.tsx`). Ta odmowa jest łańcuchem, który idzie do człowieka
/// bez tłumacza, więc mówi to, co da się powiedzieć bez tamtej tabeli: ile linii, które i co
/// z tym zrobić. Drugie wypisanie tamtej tabeli tutaj byłoby drugim miejscem, w którym
/// znalezisko dostaje słowa.
fn blocked_reason(reviewed: &crate::skills::ingest::Reviewed) -> String {
    let mut places: Vec<String> = reviewed
        .findings
        .iter()
        .filter(|finding| finding.weight == crate::skills::ingest::Weight::Block)
        .map(|finding| {
            finding.line.map_or_else(
                || "the whole file".to_owned(),
                |line| format!("line {line}"),
            )
        })
        .collect();
    places.dedup();
    let count = places.len();
    let places = if places.is_empty() {
        "somewhere in it".to_owned()
    } else {
        places.join(", ")
    };
    format!(
        "its SKILL.md has {count} line(s) that would change what your agents do, and Loadout does not import those unread ({places}). Take this skill out of the import, or change those lines in the project and scan it again."
    )
}

fn stage_connections(
    stage: &Path,
    draft: &MigrationDraft,
    written: &mut Vec<PathBuf>,
    files: &mut BTreeMap<PathBuf, ImportedFileReceipt>,
) -> Result<()> {
    for connection in &draft.connections {
        let target = PathBuf::from("connections").join(format!("{}.json", connection.id));
        write_json(&stage.join(&target), connection)?;
        record_file(
            stage,
            draft,
            &target,
            &connection.source,
            &connection.source_hash,
            files,
        )?;
        written.push(target);
    }
    Ok(())
}

fn stage_notes(
    stage: &Path,
    draft: &MigrationDraft,
    written: &mut Vec<PathBuf>,
    files: &mut BTreeMap<PathBuf, ImportedFileReceipt>,
) -> Result<()> {
    // Pamięć cudzego projektu jako PLIKI NOTATEK (2026-08-22, T-80). Draft jest wartością
    // w pamięci; pytanie brzmi „czy w bibliotece leży notatka", a na to odpowiada wyłącznie
    // dysk (niezmiennik 4). Zegar stoi jeden na cały import: dwie notatki przywiezione jednym
    // kliknięciem, które różnią się o sekundę, czytają się jak dwa zdarzenia.
    let at = crate::commands::now_utc();
    for note in &draft.notes {
        let written_note = crate::memory::notes::record_imported(
            &crate::commands::memory::notes_root(stage),
            crate::memory::notes::NoteDraft {
                title: note.title.clone(),
                rule: note.rule.clone(),
                because: note.because.clone(),
                scope: scope_from_word(&note.scope),
                kind: crate::memory::notes::Kind::Fact,
                status: crate::memory::notes::Status::Suggested,
                at: at.clone(),
            },
            note.agent.as_deref(),
            &crate::memory::notes::Origin {
                from: super::project_name(&draft.root),
                source: note.source.clone(),
                source_hash: note.source_hash.clone(),
                app: app_word(note.app).to_owned(),
            },
        )
        .map_err(|error| ImportError::Save(error.to_string()))?;
        let target = relative(stage, &written_note.path)?;
        record_file(
            stage,
            draft,
            &target,
            &note.source,
            &note.source_hash,
            files,
        )?;
        written.push(target);
    }
    Ok(())
}

fn stage_workflows(
    stage: &Path,
    draft: &MigrationDraft,
    written: &mut Vec<PathBuf>,
    files: &mut BTreeMap<PathBuf, ImportedFileReceipt>,
    workflow_hashes: &mut BTreeMap<PathBuf, String>,
) -> Result<()> {
    for workflow in &draft.workflows {
        let relative = PathBuf::from("workflows").join(format!("{}.json", slug(&workflow.name)));
        let target = stage.join(&relative);
        create_parent(&target)?;
        // `None` z tego samego powodu, co przy agentach: katalog przygotowania jest świeży,
        // więc dwa workflow o tej samej nazwie pliku są odmową, a nie cichą stratą jednego.
        crate::workflow::file::save(workflow, &target, None)
            .map_err(|error| ImportError::Save(error.to_string()))?;
        workflow_hashes.insert(
            relative.clone(),
            fingerprint(&fs::read(&target).map_err(save_error)?),
        );
        if let Some(source) = source_for_target(draft, &relative)? {
            record_file(stage, draft, &relative, &source.path, &source.hash, files)?;
        }
        written.push(relative);
    }
    Ok(())
}

fn source_for_target<'a>(
    draft: &'a MigrationDraft,
    target: &Path,
) -> Result<Option<&'a super::ImportSource>> {
    if draft.items.is_empty() {
        // Addytywna ścieżka dla ręcznie składanych draftów sprzed T-78. Świeży Scan zawsze
        // ma `items`, więc produkt nigdy nie zapisuje nowego pliku bez provenance.
        return Ok(None);
    }
    draft
        .items
        .iter()
        .find(|item| item.target.as_deref() == Some(target))
        .and_then(|item| {
            item.sources
                .iter()
                .find(|source| source.role == super::ImportSourceRole::Definition)
                .or_else(|| item.sources.first())
        })
        .map(Some)
        .ok_or_else(|| {
            ImportError::Save(format!(
                "{} has no source in the accepted import plan.",
                target.display()
            ))
        })
}

fn record_source_file(
    stage: &Path,
    draft: &MigrationDraft,
    target: &Path,
    source: &Path,
    files: &mut BTreeMap<PathBuf, ImportedFileReceipt>,
) -> Result<()> {
    let source_path = relative(&draft.root, source)?;
    let source_hash = fingerprint(&fs::read(source).map_err(save_error)?);
    record_file(stage, draft, target, &source_path, &source_hash, files)
}

fn record_file(
    stage: &Path,
    draft: &MigrationDraft,
    target: &Path,
    source_path: &Path,
    known_source_hash: &str,
    files: &mut BTreeMap<PathBuf, ImportedFileReceipt>,
) -> Result<()> {
    if draft.items.is_empty() {
        return Ok(());
    }
    let source_hash = fs::read(draft.root.join(source_path)).map_or_else(
        |_| known_source_hash.to_owned(),
        |bytes| fingerprint(&bytes),
    );
    let written_hash = fingerprint(&fs::read(stage.join(target)).map_err(save_error)?);
    let previous = files.insert(
        target.to_path_buf(),
        ImportedFileReceipt {
            source_path: source_path.to_path_buf(),
            source_hash,
            written_hash,
        },
    );
    if previous.is_some() {
        return Err(ImportError::Save(format!(
            "{} was staged twice during one import.",
            target.display()
        )));
    }
    Ok(())
}

fn commit<F>(
    home: &Path,
    stage: &Path,
    receipt: ImportReceipt,
    after_move: &mut F,
) -> Result<ImportReceipt>
where
    F: FnMut(usize) -> std::result::Result<(), String>,
{
    for relative in &receipt.written {
        if home.join(relative).exists() {
            return Err(ImportError::Save(format!(
                "{} already exists. Nothing was imported.",
                relative.display()
            )));
        }
    }

    let mut moved = Vec::new();
    let mut made_dirs = Vec::new();
    for relative in &receipt.written {
        let source = stage.join(relative);
        let target = home.join(relative);
        if let Err(error) = create_parent_recording(&target, &mut made_dirs)
            .and_then(|()| fs::rename(&source, &target))
        {
            rollback(home, stage, &moved, &made_dirs).map_err(|rollback| {
                ImportError::Save(format!(
                    "Import failed ({error}) and Loadout could not fully restore the library ({rollback})."
                ))
            })?;
            return Err(save_error(error));
        }
        moved.push(relative.clone());
        if let Err(detail) = after_move(moved.len()) {
            rollback(home, stage, &moved, &made_dirs).map_err(|rollback| {
                ImportError::Save(format!(
                    "Import stopped ({detail}) and Loadout could not fully restore the library ({rollback})."
                ))
            })?;
            return Err(ImportError::Save(detail));
        }
    }
    Ok(receipt)
}

fn rollback(
    home: &Path,
    stage: &Path,
    moved: &[PathBuf],
    made_dirs: &[PathBuf],
) -> std::io::Result<()> {
    for relative in moved.iter().rev() {
        let source = home.join(relative);
        let target = stage.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(source, target)?;
    }
    for directory in made_dirs.iter().rev() {
        fs::remove_dir(directory)?;
    }
    Ok(())
}

fn create_parent_recording(path: &Path, made: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let mut missing = Vec::new();
    let mut cursor = parent;
    while !cursor.exists() {
        missing.push(cursor.to_path_buf());
        let Some(next) = cursor.parent() else {
            break;
        };
        cursor = next;
    }
    for directory in missing.iter().rev() {
        fs::create_dir(directory)?;
        made.push(directory.clone());
    }
    Ok(())
}

fn create_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ImportError::Save("An imported file has no parent folder.".to_owned()))?;
    fs::create_dir_all(parent).map_err(save_error)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    create_parent(path)?;
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|error| ImportError::Save(error.to_string()))?;
    text.push('\n');
    fs::write(path, text).map_err(save_error)
}

fn relative(root: &Path, path: &Path) -> Result<PathBuf> {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|error| ImportError::Save(error.to_string()))
}

fn save_error(error: std::io::Error) -> ImportError {
    let detail = error.to_string();
    drop(error);
    ImportError::Save(detail)
}

/// Gdzie w bibliotece wyląduje notatka o tym tytule.
///
/// Ta sama nazwa katalogu, co [`crate::commands::memory::notes_root`], i ta sama nazwa pliku,
/// co [`crate::memory::notes::record_imported`] — obie policzone ich własnymi funkcjami. Druga
/// odpowiedź na pytanie „gdzie leży notatka" rozjechałaby się z pierwszą przy pierwszej zmianie
/// któregokolwiek z nich, a rozjazd byłoby widać dopiero jako import, który nie odmówił nadpisania.
fn note_target(title: &str) -> PathBuf {
    let root = crate::commands::memory::notes_root(Path::new(""));
    root.join("notes")
        .join(format!("{}.md", crate::memory::slugify(title)))
}

/// Słowo zakresu z drafu na typ notatki. Nieznane czyta się jako zakres projektu — ten sam
/// kierunek błędu, co w `memory::notes::scope_from`: nigdy szerzej, niż napisano.
fn scope_from_word(word: &str) -> crate::memory::notes::Scope {
    match word {
        "everywhere" => crate::memory::notes::Scope::Everywhere,
        "this-agent" => crate::memory::notes::Scope::ThisAgent,
        _ => crate::memory::notes::Scope::ThisProject,
    }
}

/// Z czyjego katalogu wzięliśmy to zdanie — słowem, nie numerem wariantu.
///
/// Wypisane, a nie wzięte z `serde`: ta wartość ląduje w pliku, który czyta człowiek w edytorze,
/// więc przemianowanie wariantu w Ruście nie ma prawa zmienić tego, co stoi w notatkach, które
/// już leżą na dysku.
const fn app_word(app: super::SourceKind) -> &'static str {
    match app {
        super::SourceKind::Claude => "claude",
        super::SourceKind::Codex => "codex",
        super::SourceKind::AgentSkills => "agent-skills",
        super::SourceKind::Rulesync => "rulesync",
        super::SourceKind::OpenStandard => "open-standard",
        super::SourceKind::Unknown => "unknown",
    }
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            out.push(character);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
