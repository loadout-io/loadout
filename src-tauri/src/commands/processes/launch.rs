//! WF-27: opis agenta staje się wyłącznie argumentami istniejącego startu usługi.

use std::io;
use std::path::{Component, Path, PathBuf};

use crate::engine::supervisor::{self, PublicationRoot};
use crate::workflow::LaunchDescription;

pub(super) fn folder(copy: &Path, relative: &str) -> io::Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return Err(io::Error::other(
            "the app folder must stay inside its working folder",
        ));
    }
    let root = PublicationRoot::open(copy)?;
    let clean: PathBuf = path
        .components()
        .filter(|part| matches!(part, Component::Normal(_)))
        .collect();
    let target = copy.join(&clean);
    // target() otwiera łańcuch katalogów przez no-follow. Niczego nie tworzymy;
    // fikcyjny leaf służy wyłącznie uzyskaniu tożsamości katalogu nadrzędnego.
    let expected = if clean.as_os_str().is_empty() {
        root.identity()
    } else {
        root.target(&clean.join(".loadout-cwd-check"))?
            .parent_identity()?
    };
    let opened = PublicationRoot::open(&target)?;
    if opened.identity() != expected {
        return Err(io::Error::other("the app folder changed before start"));
    }
    root.validate_path_identity(copy)?;
    let canonical = supervisor::publication_root_key(&target)?;
    if !canonical.starts_with(supervisor::publication_root_key(copy)?) {
        return Err(io::Error::other(
            "the app folder is outside its working folder",
        ));
    }
    Ok(canonical)
}

pub(crate) fn parse(text: &str) -> Result<LaunchDescription, String> {
    if text.len() > 32768 {
        return Err("The app description is too large; keep it below 32768 bytes.".to_owned());
    }
    // Nie wypisujemy błędu serde zawierającego nieznaną wartość: opis może zawierać sekret.
    let launch: LaunchDescription = serde_json::from_str(text).map_err(|_| {
        "The selected result did not contain a supported app description.".to_owned()
    })?;
    crate::workflow::check::launch_description(&launch)?;
    Ok(launch)
}
