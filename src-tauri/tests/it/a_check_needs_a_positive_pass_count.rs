//! Niezmiennik 19: realny proces Check, nie sam helper dopasowania ani exit 0.
use std::error::Error;
use std::time::Duration;

use loadout_lib::engine::drivers::command::{CheckHow, CheckSpec, CommandDriver};
use loadout_lib::engine::supervisor;
use tokio_util::sync::CancellationToken;

async fn check(command: &str, proof: &str, expected: bool) -> Result<(), Box<dyn Error>> {
    let workspace = tempfile::tempdir()?;
    let spec = CheckSpec {
        command: command.to_owned(),
        proof: proof.to_owned(),
        cwd: workspace.path().to_path_buf(),
    };
    let driver = CommandDriver::new();
    let cancel = CancellationToken::new();
    let running = driver.run(&spec, &cancel);
    tokio::pin!(running);
    // A timeout must still let the real driver stop and prove death of its group.
    let result = tokio::select! {
        result = &mut running => result?,
        () = tokio::time::sleep(Duration::from_secs(5)) => {
            cancel.cancel();
            running.await?
        }
    };
    assert!(
        supervisor::group_is_empty(result.group.pgid),
        "fixture process remained alive"
    );
    let CheckHow::Ran(report) = result.how else {
        return Err("the command fixture never completed".into());
    };
    assert_eq!(
        report.exit_code,
        Some(0),
        "fixture did not reach its normal exit"
    );
    assert_eq!(
        report.passed, expected,
        "a real Check misjudged the primary pass counter: {}",
        report.output
    );
    Ok(())
}

#[tokio::test]
async fn zero_passed_does_not_pass_even_when_the_command_exits_zero() -> Result<(), Box<dyn Error>>
{
    check(
        "printf 'test result: ok. 0 passed; 8 ignored\\n'",
        r"(\d+) passed",
        false,
    )
    .await
}

#[tokio::test]
async fn padded_zero_is_still_no_passed_tests() -> Result<(), Box<dyn Error>> {
    check("printf '000000 passed\\n'", r"(\d+) passed", false).await
}

#[tokio::test]
async fn a_later_counter_cannot_make_the_zero_primary_counter_positive()
-> Result<(), Box<dyn Error>> {
    check(
        "printf '0 of 19 passed\\n'",
        r"(\d+) of (\d+) passed",
        false,
    )
    .await
}

#[tokio::test]
async fn secondary_zero_counts_do_not_erase_real_passes() -> Result<(), Box<dyn Error>> {
    check(
        "printf '9 passed; 0 failed\\n'",
        r"(\d+) passed; (\d+) failed",
        true,
    )
    .await
}

#[tokio::test]
async fn a_counter_at_the_end_must_also_be_positive() -> Result<(), Box<dyn Error>> {
    check("printf 'passed 0\\n'", r"passed (\d+)", false).await
}

#[tokio::test]
async fn a_primary_count_split_between_pipe_chunks_keeps_its_value() -> Result<(), Box<dyn Error>> {
    check(
        "printf 'passed 0'; sleep 0.03; printf '002\\n'",
        r"passed (\d+)",
        true,
    )
    .await
}

#[tokio::test]
async fn literal_proofs_keep_their_existing_diagnostic_contract() -> Result<(), Box<dyn Error>> {
    check("printf 'BUILD COMPLETE\\n'", "BUILD COMPLETE", true).await
}
