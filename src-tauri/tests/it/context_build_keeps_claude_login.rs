//! 2026-09-09: prawdziwy Claude buduje kontekst w izolowanej bibliotece bez zmiany HOME.

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use loadout_lib::commands::Drivers;
use loadout_lib::commands::context::{create_context_set_inner, save_context_draft_inner};
use loadout_lib::commands::context_build::{BuildContextRequest, build_context_inner};
use loadout_lib::context::{BuildEnd, ContextDraft, ContextSource, SourceKind};
use loadout_lib::engine::drivers::AgentDriver;
use loadout_lib::engine::drivers::claude::ClaudeDriver;
use loadout_lib::engine::limits::Limiter;
use loadout_lib::library::agents::Vendor;
use tokio_util::sync::CancellationToken;

/// Płatna próba przechodzi przez resolver okna, prawdziwy build, jego granicę plików i CLI.
/// Nie zastępuje ręcznego kliknięcia w oknie. Syntetyczny materiał nie zawiera danych osoby.
#[test]
#[ignore = "uses the real Claude account to build synthetic context; requires sign-in"]
fn a_real_claude_builds_in_the_isolated_app_library() -> Result<(), Box<dyn Error>> {
    const CHILD: &str = "LOADOUT_CONTEXT_LOGIN_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let library = tempfile::tempdir()?;
        let result = Command::new(std::env::current_exe()?)
            .args([
                "context_build_keeps_claude_login::a_real_claude_builds_in_the_isolated_app_library",
                "--exact", "--ignored", "--nocapture",
            ])
            .env(CHILD, "1")
            .env("LOADOUT_DATA_DIR", library.path())
            .output()?;
        assert!(
            result.status.success(),
            "native Context build failed: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        println!(
            "Claude published a context version in an isolated app library with the normal login."
        );
        return Ok(());
    }
    let data = loadout_lib::loadout_dir();
    assert_eq!(
        data,
        PathBuf::from(std::env::var_os("LOADOUT_DATA_DIR").ok_or("missing isolated library")?)
    );
    let project = tempfile::tempdir()?;
    let made = create_context_set_inner(&data, "Native login regression")?;
    let saved = save_context_draft_inner(
        &data, &made.set.id, &made.set.title, &made.set.description,
        ContextDraft {
            schema: 1,
            sources: vec![ContextSource {
                id: "typed".to_owned(), kind: SourceKind::Text, name: "Checkout brief".to_owned(),
                text: "Keep the promo code when the cart is edited. Only remove it when the person asks.".to_owned(),
                ..ContextSource::default()
            }],
            ..ContextDraft::default()
        }, Some(made.revision),
    )?;
    let concrete: Arc<dyn AgentDriver> = Arc::new(ClaudeDriver::new());
    let drivers: Drivers = Arc::new(move |_vendor| Arc::clone(&concrete));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let built = runtime.block_on(build_context_inner(
        &data,
        project.path(),
        &drivers,
        &Limiter::new(1),
        &BuildContextRequest {
            set_id: saved.set.id,
            operation_id: "native-claude-login".to_owned(),
            app: Some(Vendor::ClaudeCode),
            model: Some("sonnet".to_owned()),
            generation: 1,
            deadline: Duration::from_secs(180),
            budget_usd: Some(1.0),
        },
        &CancellationToken::new(),
    ))?;
    let state = built.build.ok_or("missing build state")?;
    assert_eq!(state.end, BuildEnd::Ready, "native build: {}", state.said);
    assert_eq!((state.batches_done, state.batches_total), (1, 1));
    assert!(
        built.revision.is_some(),
        "the build must publish a usable version"
    );
    Ok(())
}
