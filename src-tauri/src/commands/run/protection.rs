//! WF-18: przygotowanie istniejących sterowników; wykonawca nadal widzi zwykły graf.
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{Job, Plan, WORK_DIR, tile_key_of, where_the_job_works, work_key_of};
use crate::{
    durable_file::{DurableFilePublisher, ModePolicy, PRIVATE_FILE_MODE},
    engine::{
        drivers::{AgentDriver, StepSettings, command::CommandDriver},
        supervisor::{self, FilesystemFence, PublicationIdentity, PublicationRoot},
    },
    workflow::execution::Examiner,
};

pub(super) struct Boundary {
    fence: FilesystemFence,
    temporary: Option<PathBuf>,
    examiner: Option<FrozenExaminer>,
}

struct FrozenExaminer {
    directory: PathBuf,
    program: PathBuf,
    source: PathBuf,
    held: PublicationRoot,
    identity: PublicationIdentity,
    bytes: Vec<u8>,
}

impl FrozenExaminer {
    fn prepare(plan: &Plan, key: &str, definition: Examiner) -> io::Result<Self> {
        let Examiner::Python { program, source } = definition else {
            return Err(io::Error::other(
                "This kind of independent examiner is not supported.",
            ));
        };
        let program = fs::canonicalize(program)?;
        let project = supervisor::publication_root_key(&plan.project)?;
        if !program.is_file() || program.starts_with(&project) {
            return Err(io::Error::other(
                "The independent examiner needs an interpreter outside the measured project.",
            ));
        }
        let relative = Path::new("examiners").join(key);
        PublicationRoot::open(&plan.dir)?.ensure_directory(&relative, 0o700)?;
        let directory = plan.dir.join(relative);
        let held = PublicationRoot::open(&directory)?;
        let source_path = directory.join("examiner.py");
        let bytes = source.into_bytes();
        let identity = match held.open_regular_file(Path::new("examiner.py")) {
            Ok(file) => supervisor::publication_identity(&file)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                DurableFilePublisher::new(&directory)
                    .with_publication(|batch| {
                        batch.atomic_create_if_absent_with_identity(
                            &source_path,
                            &bytes,
                            ModePolicy::Exact(PRIVATE_FILE_MODE),
                        )
                    })
                    .map_err(io::Error::other)?
            }
            Err(error) => return Err(error),
        };
        let saved = Self {
            directory,
            program,
            source: source_path,
            held,
            identity,
            bytes,
        };
        saved.validate()?;
        Ok(saved)
    }

    fn validate(&self) -> io::Result<()> {
        self.held.validate_path_identity(&self.directory)?;
        let file = self.held.open_regular_file(Path::new("examiner.py"))?;
        if supervisor::publication_identity(&file)? != self.identity {
            return Err(changed());
        }
        let mut bytes = Vec::new();
        file.take(256 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes != self.bytes {
            return Err(changed());
        }
        Ok(())
    }
}

fn changed() -> io::Error {
    io::Error::other("The evaluation data changed. This result was not measured.")
}

/// Wszystkie odmowy możliwości adaptera stoją PRZED jakimkolwiek płatnym procesem.
pub(super) fn prepare(plan: &mut Plan) -> io::Result<BTreeMap<usize, Boundary>> {
    if !plan.inputs.configuration.isolate_contexts {
        return Ok(BTreeMap::new());
    }
    let mut prepared = BTreeMap::new();
    for id in 0..plan.steps.len() {
        let step = &plan.steps[id];
        let scope = plan
            .inputs
            .configuration
            .context_key(&step.tile_key)
            .ok_or_else(|| {
                io::Error::other("Every protected step needs its own declared context.")
            })?;
        let key = work_key_of(&step.node_key);
        let cwd = where_the_job_works(&step.job).ok_or_else(|| {
            io::Error::other("Protected runs do not support interactive or background steps.")
        })?;
        if !cwd.starts_with(plan.dir.join(WORK_DIR)) {
            return Err(io::Error::other(
                "Protected steps must use a managed working copy, not the project or a chosen external folder.",
            ));
        }
        let mut writable = plan
            .steps
            .iter()
            .filter(|other| plan.inputs.configuration.context_key(&other.tile_key) == Some(scope))
            .filter_map(|other| where_the_job_works(&other.job))
            .map(Path::to_path_buf)
            .collect::<Vec<_>>();
        writable.sort();
        writable.dedup();
        let mut readable = vec![plan.dir.join("context").join(scope)];
        let mut files = Vec::new();
        let temporary = if matches!(step.job, Job::Check(_)) {
            let relative = Path::new("private-checks").join(key).join("tmp");
            PublicationRoot::open(&plan.dir)?.ensure_directory(&relative, 0o700)?;
            let temporary = plan.dir.join(relative);
            writable.push(temporary.clone());
            Some(temporary)
        } else {
            None
        };
        let examiner = definition(plan, id)?
            .map(|one| FrozenExaminer::prepare(plan, key, one))
            .transpose()?;
        if let Some(examiner) = &examiner {
            readable.push(examiner.directory.clone());
        }
        let replacement = match &step.job {
            Job::Agent(job) => {
                let runtime = prepare_agent_runtime(plan, step, job, cwd, key)?;
                writable.extend(runtime.writable_roots);
                readable.extend(runtime.readable_roots);
                files.extend(runtime.readable_files);
                Some(runtime.driver)
            }
            Job::Check(_) => None,
            Job::Serve(_) | Job::Ask { .. } => {
                return Err(io::Error::other(
                    "Protected runs do not support interactive or background steps.",
                ));
            }
        };
        // Ukryty jest również Git gospodarza: wspólne objects mogą ujawnić pracę innej komórki.
        let fence = FilesystemFence::new(
            writable,
            readable,
            vec![plan.project.clone(), plan.dir.clone()],
        )?
        .reading_files(files)?;
        if let Some(driver) = replacement {
            if driver.with_filesystem_fence(&fence).is_none() {
                return Err(io::Error::other(
                    "This agent app cannot enforce protected file access. No agent was started.",
                ));
            }
            if let Job::Agent(job) = &mut plan.steps[id].job {
                job.driver = driver;
            }
        }
        prepared.insert(
            id,
            Boundary {
                fence,
                temporary,
                examiner,
            },
        );
    }
    Ok(prepared)
}

fn prepare_agent_runtime(
    plan: &Plan,
    step: &super::Planned,
    job: &super::AgentJob,
    cwd: &Path,
    key: &str,
) -> io::Result<crate::engine::drivers::PreparedProtectedStep> {
    if !job.connections.is_empty() || !job.service_access.is_empty() {
        return Err(io::Error::other(
            "Protected runs cannot grant external Connections or background app controls. Use a diagnostic run with these settings.",
        ));
    }
    let memory = plan.dir.join("mem").join(tile_key_of(&step.node_key));
    PublicationRoot::open(&plan.dir)?
        .ensure_directory(&Path::new("mem").join(tile_key_of(&step.node_key)), 0o700)?;

    let settings = StepSettings {
        dir: plan.dir.clone(),
        work_key: key.to_owned(),
        memory: memory.clone(),
        deny: crate::engine::drivers::host::deny_rules(cwd),
    };
    let mut runtime = job
        .driver
        .prepare_protected_step(&settings)
        .ok_or_else(|| {
            io::Error::other(
                "This agent app cannot prepare protected run state. No agent was started.",
            )
        })?
        .map_err(|error| io::Error::other(error.to_string()))?;
    runtime.writable_roots.push(memory);
    // Flagi pochodzą wyłącznie z hostowych materializatorów bundle, nigdy z tekstu modelu.
    for flags in [job.plugin_flags.as_slice(), job.borrowed.flags()] {
        for pair in flags.windows(2) {
            if pair[0] == "--plugin-dir" {
                runtime.readable_roots.push(PathBuf::from(&pair[1]));
            }
        }
    }
    Ok(runtime)
}

fn definition(plan: &Plan, id: usize) -> io::Result<Option<Examiner>> {
    let Some(step) = plan
        .graph
        .get("steps")
        .and_then(serde_json::Value::as_array)
        .and_then(|steps| {
            steps.iter().find(|one| {
                one.get("id").and_then(serde_json::Value::as_str)
                    == Some(plan.steps[id].tile_key.as_str())
            })
        })
    else {
        return Err(io::Error::other(
            "The protected step has no saved definition.",
        ));
    };
    step.get("examiner")
        .map(Examiner::read)
        .transpose()
        .map_err(io::Error::other)
}

impl Boundary {
    pub(super) fn agent(&self, driver: &dyn AgentDriver) -> anyhow::Result<Arc<dyn AgentDriver>> {
        driver
            .with_filesystem_fence(&self.fence)
            .ok_or_else(|| anyhow::anyhow!("This agent app cannot enforce protected file access."))
    }

    pub(super) fn check(
        &self,
        driver: CommandDriver,
        input: Option<&str>,
    ) -> io::Result<CommandDriver> {
        let mut readable = Vec::new();
        if let Some(input) = input {
            let value: serde_json::Value = serde_json::from_str(input).map_err(io::Error::other)?;
            for one in value
                .get("results")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(path) = one
                    .pointer("/files/root")
                    .and_then(serde_json::Value::as_str)
                {
                    readable.push(PathBuf::from(path));
                }
            }
        }
        let fence = self.fence.clone().reading_roots(readable)?;
        let mut driver = driver.with_filesystem_fence(&fence);
        if let Some(directory) = &self.temporary {
            driver = driver.with_temporary_directory(directory.clone());
        }
        if let Some(examiner) = &self.examiner {
            examiner.validate()?;
            // -I wyklucza cwd/PYTHONPATH subjectu; zaufany test dostaje wynik jako dane stdin.
            driver = driver.with_executable(
                examiner.program.clone(),
                vec!["-I".into(), examiner.source.clone().into_os_string()],
            );
        }
        Ok(driver)
    }

    pub(super) fn check_cwd<'a>(&'a self, fallback: &'a Path) -> &'a Path {
        self.examiner
            .as_ref()
            .map_or(fallback, |one| one.directory.as_path())
    }

    pub(super) fn validate(&self) -> io::Result<()> {
        if let Some(examiner) = &self.examiner {
            examiner.validate()?;
        }
        Ok(())
    }
}
