//! T-139 AC-2: Move records completed durability operations, never mere attempts.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use loadout_lib::memory::notes::{
    Actor, MoveIo, NoteId, RealMoveIo, discard, move_note_file_with_io,
};

const FIXTURE_BYTES: &[u8] = b"T139 literal move fixture\nwith a second line\n";
const COMPETING_BYTES: &[u8] = b"a concurrent writer owns this target\n";
const BASELINE_NOTE: &[u8] = b"---\nscope: this-project\nkind: fact\ntitle: Baseline move\nrule: Keep the bytes\nbecause: The archive is recoverable\nstatus: suggested\noccurrences: 1\nmodified: 2026-08-26T10:00:00Z\nlast_used_at: null\n---\n\nBody kept exactly.\n";

#[test]
fn existing_memory_move_preserves_literal_bytes_instead_of_deleting_them()
-> Result<(), Box<dyn StdError>> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("notes"))?;
    fs::write(root.path().join("notes/baseline-move.md"), BASELINE_NOTE)?;

    let landing = discard(
        root.path(),
        &NoteId("baseline-move".to_owned()),
        Actor::You {
            at: "2026-08-26T10:30:00Z".to_owned(),
        },
    )?;

    assert_eq!(fs::read(landing)?, BASELINE_NOTE);
    assert!(!root.path().join("notes/baseline-move.md").exists());
    Ok(())
}

#[test]
fn success_has_the_exact_post_success_trace_and_one_full_copy() -> Result<(), Box<dyn StdError>> {
    let fixture = Fixture::new()?;
    let mut io = RecordingMoveIo::new(&fixture, None);

    move_note_file_with_io(&mut io, &fixture.source, &fixture.target)?;

    let staged = assert_staged_first(&io.completed, &fixture)?;
    assert_eq!(
        io.completed,
        vec![
            Operation::StageIn {
                target_dir: fixture.target_dir.clone(),
                bytes: FIXTURE_BYTES.to_vec(),
                staged: staged.clone(),
            },
            Operation::SyncFile(staged.clone()),
            Operation::PersistNoClobber {
                staged,
                target: fixture.target.clone(),
            },
            Operation::SyncDir(fixture.target_dir.clone()),
            Operation::RemoveFile(fixture.source.clone()),
            Operation::SyncDir(fixture.source_dir.clone()),
        ]
    );
    assert!(!fixture.source.exists());
    assert_eq!(fs::read(&fixture.target)?, FIXTURE_BYTES);
    assert_eq!(full_copies(&fixture)?, 1);
    Ok(())
}

fn assert_staged_first(
    completed: &[Operation],
    fixture: &Fixture,
) -> Result<PathBuf, Box<dyn StdError>> {
    let Some(Operation::StageIn {
        target_dir,
        bytes,
        staged,
    }) = completed.first()
    else {
        return Err(format!(
            "first completed operation was not StageIn: {:?}",
            completed.first()
        )
        .into());
    };
    assert_eq!(target_dir, &fixture.target_dir);
    assert_eq!(bytes, FIXTURE_BYTES);
    assert_eq!(staged.parent(), Some(fixture.target_dir.as_path()));
    assert_ne!(staged, &fixture.target);
    Ok(staged.clone())
}

#[test]
fn a_toctou_target_is_never_clobbered_and_the_failed_publish_is_not_recorded()
-> Result<(), Box<dyn StdError>> {
    let fixture = Fixture::new()?;
    let mut io = RecordingMoveIo::new(&fixture, Some(Failure::PublishRace));

    let result = move_note_file_with_io(&mut io, &fixture.source, &fixture.target);

    assert!(
        result.is_err(),
        "the competing target was reported as a successful Move"
    );
    assert_eq!(io.attempts.get(&Failure::PublishRace), Some(&1));
    assert_eq!(io.refusals.get(&Failure::PublishRace), Some(&1));
    assert_exact_successful_prefix(Failure::PublishRace, &fixture, &io.completed)?;
    assert_no_publish_or_later_operation(&io.completed);
    assert_eq!(fs::read(&fixture.source)?, FIXTURE_BYTES);
    assert_eq!(fs::read(&fixture.target)?, COMPETING_BYTES);
    Ok(())
}

fn assert_no_publish_or_later_operation(completed: &[Operation]) {
    assert!(
        !completed
            .iter()
            .any(|one| matches!(one, Operation::PersistNoClobber { .. }))
    );
    assert!(
        !completed
            .iter()
            .any(|one| matches!(one, Operation::SyncDir(_)))
    );
    assert!(
        !completed
            .iter()
            .any(|one| matches!(one, Operation::RemoveFile(_)))
    );
}

#[test]
fn every_injected_refusal_records_only_earlier_successes_and_never_loses_all_copies()
-> Result<(), Box<dyn StdError>> {
    for failure in [
        Failure::SyncFile,
        Failure::SyncTargetDir,
        Failure::RemoveSource,
        Failure::SyncSourceDir,
    ] {
        assert_one_refusal(failure)?;
    }
    Ok(())
}

fn assert_one_refusal(failure: Failure) -> Result<(), Box<dyn StdError>> {
    let fixture = Fixture::new()?;
    let mut io = RecordingMoveIo::new(&fixture, Some(failure));

    let result = move_note_file_with_io(&mut io, &fixture.source, &fixture.target);

    assert!(result.is_err(), "{failure:?} was reported as success");
    assert_eq!(
        io.attempts.get(&failure),
        Some(&1),
        "{failure:?} did not reach its real adapter exactly once"
    );
    assert_eq!(
        io.refusals.get(&failure),
        Some(&1),
        "{failure:?} did not report exactly one delegate refusal"
    );
    assert!(
        !io.completed
            .iter()
            .any(|operation| failure.matches(operation, &fixture)),
        "{failure:?} was recorded even though its delegate refused"
    );
    assert_exact_successful_prefix(failure, &fixture, &io.completed)?;
    assert_refusal_copy_boundary(failure, &fixture, &io.completed)?;
    Ok(())
}

fn assert_exact_successful_prefix(
    failure: Failure,
    fixture: &Fixture,
    completed: &[Operation],
) -> Result<(), Box<dyn StdError>> {
    let staged = assert_staged_first(completed, fixture)?;
    let mut full = vec![
        Operation::StageIn {
            target_dir: fixture.target_dir.clone(),
            bytes: FIXTURE_BYTES.to_vec(),
            staged: staged.clone(),
        },
        Operation::SyncFile(staged.clone()),
        Operation::PersistNoClobber {
            staged,
            target: fixture.target.clone(),
        },
        Operation::SyncDir(fixture.target_dir.clone()),
        Operation::RemoveFile(fixture.source.clone()),
        Operation::SyncDir(fixture.source_dir.clone()),
    ];
    let successful = match failure {
        Failure::SyncFile => 1,
        Failure::PublishRace => 2,
        Failure::SyncTargetDir => 3,
        Failure::RemoveSource => 4,
        Failure::SyncSourceDir => 5,
    };
    full.truncate(successful);
    assert_eq!(
        completed, full,
        "{failure:?} has the wrong successful prefix"
    );
    Ok(())
}

fn assert_refusal_copy_boundary(
    failure: Failure,
    fixture: &Fixture,
    completed: &[Operation],
) -> Result<(), Box<dyn StdError>> {
    let wanted = match failure {
        Failure::SyncFile | Failure::SyncSourceDir => 1,
        Failure::SyncTargetDir | Failure::RemoveSource => 2,
        Failure::PublishRace => unreachable!("the race has its own scene"),
    };
    assert_eq!(
        full_copies(fixture)?,
        wanted,
        "{failure:?} crossed the promised destruction boundary"
    );
    assert_files_at_refusal_boundary(failure, fixture, completed)?;
    Ok(())
}

fn assert_files_at_refusal_boundary(
    failure: Failure,
    fixture: &Fixture,
    completed: &[Operation],
) -> Result<(), Box<dyn StdError>> {
    match failure {
        Failure::SyncFile => {
            assert!(fixture.source.exists());
            assert!(!fixture.target.exists());
        }
        Failure::SyncTargetDir | Failure::RemoveSource => {
            assert_eq!(fs::read(&fixture.source)?, FIXTURE_BYTES);
            assert_eq!(fs::read(&fixture.target)?, FIXTURE_BYTES);
            assert!(!completed.contains(&Operation::SyncDir(fixture.source_dir.clone())));
        }
        Failure::SyncSourceDir => {
            assert!(!fixture.source.exists());
            assert_eq!(fs::read(&fixture.target)?, FIXTURE_BYTES);
        }
        Failure::PublishRace => unreachable!("the race has its own scene"),
    }
    Ok(())
}

#[derive(Debug)]
struct Fixture {
    _root: tempfile::TempDir,
    source_dir: PathBuf,
    target_dir: PathBuf,
    source: PathBuf,
    target: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, io::Error> {
        let root = tempfile::tempdir()?;
        let source_dir = root.path().join("library/notes");
        let target_dir = root.path().join("project/.loadout/memory/notes");
        fs::create_dir_all(&source_dir)?;
        fs::create_dir_all(&target_dir)?;
        let source = source_dir.join("legacy.md");
        let target = target_dir.join("legacy.md");
        fs::write(&source, FIXTURE_BYTES)?;
        Ok(Self {
            _root: root,
            source_dir,
            target_dir,
            source,
            target,
        })
    }
}

fn full_copies(fixture: &Fixture) -> Result<usize, io::Error> {
    [&fixture.source, &fixture.target]
        .into_iter()
        .try_fold(0, |count, path| match fs::read(path) {
            Ok(bytes) if bytes == FIXTURE_BYTES => Ok(count + 1),
            Ok(_) => Ok(count),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(count),
            Err(error) => Err(error),
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Failure {
    SyncFile,
    PublishRace,
    SyncTargetDir,
    RemoveSource,
    SyncSourceDir,
}

impl Failure {
    fn matches(self, operation: &Operation, fixture: &Fixture) -> bool {
        match (self, operation) {
            (Self::SyncFile, Operation::SyncFile(_))
            | (Self::PublishRace, Operation::PersistNoClobber { .. })
            | (Self::RemoveSource, Operation::RemoveFile(_)) => true,
            (Self::SyncTargetDir, Operation::SyncDir(path)) => path == &fixture.target_dir,
            (Self::SyncSourceDir, Operation::SyncDir(path)) => path == &fixture.source_dir,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Operation {
    StageIn {
        target_dir: PathBuf,
        bytes: Vec<u8>,
        staged: PathBuf,
    },
    SyncFile(PathBuf),
    PersistNoClobber {
        staged: PathBuf,
        target: PathBuf,
    },
    SyncDir(PathBuf),
    RemoveFile(PathBuf),
}

struct RecordingMoveIo {
    real: RealMoveIo,
    completed: Vec<Operation>,
    attempts: BTreeMap<Failure, usize>,
    refusals: BTreeMap<Failure, usize>,
    failure: Option<Failure>,
    source: PathBuf,
    source_dir: PathBuf,
    target: PathBuf,
    target_dir: PathBuf,
}

impl RecordingMoveIo {
    fn new(fixture: &Fixture, failure: Option<Failure>) -> Self {
        Self {
            real: RealMoveIo,
            completed: Vec::new(),
            attempts: BTreeMap::new(),
            refusals: BTreeMap::new(),
            failure,
            source: fixture.source.clone(),
            source_dir: fixture.source_dir.clone(),
            target: fixture.target.clone(),
            target_dir: fixture.target_dir.clone(),
        }
    }

    fn refuse(&mut self, failure: Failure) -> io::Result<()> {
        if self.failure != Some(failure) {
            return Ok(());
        }
        *self.attempts.entry(failure).or_default() += 1;
        *self.refusals.entry(failure).or_default() += 1;
        Err(io::Error::other(format!("injected {failure:?} refusal")))
    }
}

impl MoveIo for RecordingMoveIo {
    fn read(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        self.real.read(path)
    }

    fn exists(&mut self, path: &Path) -> io::Result<bool> {
        self.real.exists(path)
    }

    fn stage_in(&mut self, target_dir: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
        let staged = self.real.stage_in(target_dir, bytes)?;
        self.completed.push(Operation::StageIn {
            target_dir: target_dir.to_path_buf(),
            bytes: bytes.to_vec(),
            staged: staged.clone(),
        });
        Ok(staged)
    }

    fn sync_file(&mut self, path: &Path) -> io::Result<()> {
        self.refuse(Failure::SyncFile)?;
        self.real.sync_file(path)?;
        self.completed.push(Operation::SyncFile(path.to_path_buf()));
        Ok(())
    }

    fn persist_no_clobber(&mut self, staged: &Path, target: &Path) -> io::Result<()> {
        if self.failure == Some(Failure::PublishRace) {
            *self.attempts.entry(Failure::PublishRace).or_default() += 1;
            fs::write(target, COMPETING_BYTES)?;
        }
        let result = self.real.persist_no_clobber(staged, target);
        if self.failure == Some(Failure::PublishRace) && result.is_err() {
            *self.refusals.entry(Failure::PublishRace).or_default() += 1;
        }
        result?;
        self.completed.push(Operation::PersistNoClobber {
            staged: staged.to_path_buf(),
            target: target.to_path_buf(),
        });
        Ok(())
    }

    fn sync_dir(&mut self, path: &Path) -> io::Result<()> {
        if path == self.target_dir {
            self.refuse(Failure::SyncTargetDir)?;
        } else if path == self.source_dir {
            self.refuse(Failure::SyncSourceDir)?;
        }
        self.real.sync_dir(path)?;
        self.completed.push(Operation::SyncDir(path.to_path_buf()));
        Ok(())
    }

    fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        self.refuse(Failure::RemoveSource)?;
        if path != self.source
            || fs::read(&self.target)? != FIXTURE_BYTES
            || !self
                .completed
                .contains(&Operation::SyncDir(self.target_dir.clone()))
        {
            return Err(io::Error::other(
                "source unlink happened before the target was published and synced",
            ));
        }
        self.real.remove_file(path)?;
        self.completed
            .push(Operation::RemoveFile(path.to_path_buf()));
        Ok(())
    }
}
