//! Sędzia pętli, który nie ma czego sądzić, nie odbija się trzy razy — pyta o to gita.
//!
//! # Co to mierzy
//!
//! 2026-08-22, prośba właściciela po jego biegu na `urc-monorepo`: „jak backend nie ma czego
//! implementować, to żeby bez sensu się nie odbijać". Zmierzone tam: `Backend check` przeszedł
//! trzy pełne rundy nad pracą, której nie było, napisał w każdej to samo — *„there are no backend
//! code or schema changes to verify"* — i skończył jako `failed`, bo jedynym wyjściem z pętli był
//! werdykt `pass`. Kara za uczciwość, płacona prawdziwymi procesami i tokenami.
//!
//! # Dlaczego git, a nie słowo agenta
//!
//! „Nic nie zmieniłem" jest tym, co agent POWIEDZIAŁ; diff jest tym, co się STAŁO — a na tej
//! różnicy stoi cały ten produkt. Trzeci werdykt (`OUTCOME: NOTHING-TO-CHECK`) byłby nową, wygodną
//! drogą ucieczki dla modelu, któremu nie chce się pracować. Diffu nie da się ograć.
//!
//! # Słabą wersją tego kryterium jest sprawdzenie samego „pominięto"
//!
//! Przechodzi ją implementacja, która pomija sędziego ZAWSZE — czyli kasuje weryfikację w całości
//! i wygląda przy tym na oszczędną. Dlatego druga połowa każdego przypadku niżej pyta o drzewo,
//! w którym coś się wydarzyło, i wymaga, żeby sędzia jednak pobiegł.

use std::error::Error;
use std::path::Path;
use std::process::Command;

use loadout_lib::commands::isolate;

/// Repozytorium z jednym commitem — najkrótsze, na którym `touched` ma o czym mówić.
fn repo(at: &Path) -> Result<(), Box<dyn Error>> {
    for args in [
        vec!["init", "--quiet"],
        vec!["config", "user.email", "test@example.test"],
        vec!["config", "user.name", "Test"],
    ] {
        Command::new("git").args(args).current_dir(at).status()?;
    }
    std::fs::write(at.join("README.md"), "one\n")?;
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(at)
        .status()?;
    Command::new("git")
        .args(["commit", "--quiet", "-m", "first"])
        .current_dir(at)
        .status()?;
    Ok(())
}

#[test]
fn a_tree_nobody_touched_has_nothing_to_check() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    repo(project.path())?;
    let work = project.path().join("work/s_impl");
    isolate::make(project.path(), &work, "loadout/test/s_impl")?;

    assert!(
        !isolate::touched(project.path(), &work),
        "a fresh tree in which the step wrote nothing is exactly the case the owner hit: the \
         judge spent three rounds saying there was nothing to verify, and the run then called \
         that a failure"
    );
    Ok(())
}

#[test]
fn an_uncommitted_change_is_something_to_check() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    repo(project.path())?;
    let work = project.path().join("work/s_impl");
    isolate::make(project.path(), &work, "loadout/test/s_impl")?;
    std::fs::write(work.join("README.md"), "one\ntwo\n")?;

    assert!(
        isolate::touched(project.path(), &work),
        "without this line the rule also passes for an implementation that skips the judge every \
         time — which deletes verification altogether and looks thrifty while doing it"
    );
    Ok(())
}

#[test]
fn work_the_step_committed_is_still_something_to_check() -> Result<(), Box<dyn Error>> {
    let project = tempfile::tempdir()?;
    repo(project.path())?;
    let work = project.path().join("work/s_impl");
    isolate::make(project.path(), &work, "loadout/test/s_impl")?;
    std::fs::write(work.join("README.md"), "one\ntwo\n")?;
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(&work)
        .status()?;
    Command::new("git")
        .args(["commit", "--quiet", "-m", "the step's work"])
        .current_dir(&work)
        .status()?;

    assert!(
        isolate::touched(project.path(), &work),
        "`git status` alone goes quiet the moment a step commits its own work — and implementers \
         do commit: measured on the owner's run, `Front` committed 605fa3e5 and left the tree \
         clean. Asking only about dirtiness would skip the judge over finished work"
    );
    Ok(())
}

/// Kiedy git nie odpowiada, odpowiadamy „jest co sprawdzać".
#[test]
fn a_folder_that_is_not_a_repository_counts_as_touched() -> Result<(), Box<dyn Error>> {
    let plain = tempfile::tempdir()?;
    std::fs::write(plain.path().join("note.txt"), "hello\n")?;

    assert!(
        isolate::touched(plain.path(), plain.path()),
        "silence has to fall towards verification, never away from it: a skipped check lets work \
         nobody looked at through, and one needless round costs a minute"
    );
    Ok(())
}
