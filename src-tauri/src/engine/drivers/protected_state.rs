//! Host przygotowuje własne katalogi, adapter opisuje potrzeby, supervisor nadaje granicę.
use std::io;
use std::path::{Component, Path, PathBuf};

use super::StepSettings;
use crate::engine::supervisor::{self, PublicationRoot};

pub(super) fn runtime(
    settings: &StepSettings,
    vendor: &str,
) -> anyhow::Result<(PathBuf, PublicationRoot)> {
    let mut pieces = Path::new(&settings.work_key).components();
    if !matches!(pieces.next(), Some(Component::Normal(_))) || pieces.next().is_some() {
        anyhow::bail!("the private runtime needs one exact physical work key");
    }
    let run = PublicationRoot::open(&settings.dir)?;
    let relative = Path::new(vendor).join(&settings.work_key);
    run.ensure_directory(&relative, 0o700)?;
    let path = supervisor::publication_root_key(&settings.dir)?.join(relative);
    let held = PublicationRoot::open(&path)?;
    Ok((path, held))
}

pub(super) fn plain_file(directory: &PublicationRoot, relative: &Path) -> anyhow::Result<()> {
    match directory.open_regular_file(relative) {
        Ok(_) => Ok(()),
        Err(why) if why.kind() == io::ErrorKind::NotFound => {
            directory.create_regular(relative)?.sync_all()?;
            Ok(())
        }
        Err(why) => Err(why.into()),
    }
}
