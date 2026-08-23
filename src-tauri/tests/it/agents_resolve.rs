//! Kryterium 3 dla T-11: nadpisanie przeżywa edycję szablonu, a pole nienadpisane za nią idzie.
//!
//! To jest dokładnie przebieg z T4 §4.4, ten, który tam **uruchomiono** na `json-patch` 4.2.0.
//!
//! Słabe wersje tego kryterium to `assert_eq!(changed.len(), 2)` albo
//! `assert_eq!(eff.thinking, Thinking::Deep)`. Obie przechodzą dla wariantu A z T4 §4.1 —
//! pełnej kopii agenta na kroku — czyli dla implementacji, w której **edycja szablonu nigdy
//! nie dociera do workflow**, a cała funkcja jest dekoracją. Użytkownik poprawia agenta
//! w jednym miejscu i nic się nie zmienia; dowiaduje się o tym po biegu.
//!
//! Rozróżnia to wyłącznie druga połowa: zmieniamy szablon i pytamy jeszcze raz. Pełna kopia
//! zwróci tam `"opus"`, bo nosi w sobie zdjęcie agenta sprzed edycji.
//!
//! Pusty patch jest tożsamością. Bez tej asercji „krok bez zmian" mógłby po cichu przepisywać
//! wartości domyślnymi — i wtedy `Overrides::default()` znaczyłoby coś innego niż „nic tu nie
//! zmieniałem".

use std::error::Error;

use loadout_lib::library::agents::{
    Agent, Color, FileAccess, Overrides, Thinking, Tools, Vendor, VendorOptions, resolve,
};
use uuid::Uuid;

/// Szablon z T4 §4.4: `model: "opus"`, `thinking: Balanced`, `giveUpAfterMinutes: 15`.
fn template() -> Result<Agent, Box<dyn Error>> {
    Ok(Agent {
        schema: 1,
        id: Uuid::parse_str("019897b4-8f3a-7c21-9d44-0b6a1e2c5f77")?,
        name: "Scout".to_string(),
        summary: "Reads docs and reports back with sources".to_string(),
        color: Color::Slate,
        instructions: "Find primary sources. Report in bullets.".to_string(),
        runs_with: Vendor::ClaudeCode,
        model: "opus".to_string(),
        thinking: Thinking::Balanced,
        file_access: FileAccess::LookOnly,
        give_up_after_minutes: 15,
        tools: Tools::Everything,
        reaches_the_web: false,
        skills: Vec::new(),
        connections: Vec::new(),
        write_results_to: "memory/research.md".to_string(),
        vendor_options: VendorOptions::new(),
    })
}

#[test]
fn an_override_outlives_a_template_edit_and_an_untouched_setting_follows_it()
-> Result<(), Box<dyn Error>> {
    let mut base = template()?;
    let step = Overrides {
        thinking: Some(Thinking::Deep),
        give_up_after_minutes: Some(45),
        ..Overrides::default()
    };

    let before = resolve(&base, &step)?;
    assert_eq!(
        before.changed,
        ["giveUpAfterMinutes", "thinking"],
        "the badge on the step says how many settings this step changed, and which. It is the \
         names of the keys the step stores, sorted — nothing else"
    );
    assert_eq!(
        before.agent.thinking,
        Thinking::Deep,
        "a changed setting has to win over the agent's"
    );
    assert_eq!(
        before.agent.give_up_after_minutes, 45,
        "a changed setting has to win over the agent's"
    );
    assert_eq!(
        before.agent.model, "opus",
        "a setting nobody changed on the step has to come from the agent"
    );

    // Ta linia jest całym kryterium. Wszystko nad nią przechodzi także dla implementacji,
    // która trzyma na kroku pełną kopię agenta.
    base.model = "sonnet".to_string();

    let after = resolve(&base, &step)?;
    assert_eq!(
        after.agent.model, "sonnet",
        "editing the agent has to reach every step that did not change its model. If this says \
         opus, the step is holding a copy of the agent and editing the agent is decoration"
    );
    assert_eq!(
        after.agent.give_up_after_minutes, 45,
        "and the same edit must not disturb a setting this step did change"
    );
    assert_eq!(
        after.agent.thinking,
        Thinking::Deep,
        "and the same edit must not disturb a setting this step did change"
    );
    Ok(())
}

#[test]
fn a_step_that_changed_nothing_runs_the_agent_as_it_stands() -> Result<(), Box<dyn Error>> {
    let base = template()?;

    let resolved = resolve(&base, &Overrides::default())?;

    assert_eq!(
        resolved.agent, base,
        "a step with nothing changed has to run exactly the agent, field for field. Anything \
         else means an empty step quietly rewrites settings with defaults"
    );
    let changed = resolved.changed;
    assert!(
        changed.is_empty(),
        "and it has to report nothing changed, so the badge on the step stays off: {changed:?}"
    );
    Ok(())
}
