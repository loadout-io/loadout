//! AC-1 dla T-36: flaga eskalująca z **definicji agenta** nie dochodzi do argv, i wiadomo która.
//!
//! `DECISIONS-LOCKED.md` §D6 stawia na przelotce `vendorOptions` dwa ograniczenia, żeby nie stała
//! się dziurą. Drugie brzmi dosłownie: „przelotka nie omija diala bezpieczeństwa. Pole «co agent
//! może zrobić z plikami» jest tłumaczone przez nas na flagi vendora; przelotka nie może go
//! podnieść".
//!
//! Zmierzone na wyładowanym trunku (przegląd zewnętrzny 2026-08-16): filtr stoi **wyłącznie** na
//! przelotce kroku workflow. Definicja agenta ma własną przelotkę, którą `vendor_args` tłumaczy
//! prosto do argv **bez ani jednego sprawdzenia** — więc plik `~/.loadout/agents/*.json`
//! z `"--dangerously-skip-permissions": ""` omija dial całkowicie.
//!
//! **Słabą wersją tego kryterium jest „argv jest krótsze".** Przechodzi ją implementacja, która
//! wycina przypadkową flagę — na przykład każdą, której wartość jest pusta, albo po prostu
//! pierwszą z brzegu. Dlatego asercje są tu trzy i dopiero razem coś znaczą: **niewinna flaga
//! ZOSTAJE** (razem ze swoją wartością, obok siebie), **każda eskalująca znika Z OSOBNA, po
//! nazwie**, a funkcja **NAZYWA** to, co odrzuciła. Cicha odmowa uczy użytkownika, że przelotka
//! nie działa, zamiast tego, że została zablokowana — a wtedy naprawia ją, wpisując flagę
//! jeszcze raz, innym zapisem.
//!
//! Trzy nazwy podniesień są tu **wypisane wprost**, a nie zaimportowane z polityki. Import
//! sprawdzałby, że test i kod czytają tę samą stałą — także wtedy, gdy ktoś ją opróżni
//! (niezmiennik 20). Za to, że jest jedna i wspólna, odpowiada AC-2, i tam import jest
//! całą treścią kryterium.

// 2026-08-16 — `panic!` w teście JEST jego wynikiem, a `[workspace.lints]` ma `panic = "deny"`
// dla całego drzewa. `checks/full-clippy.sh` biegnie `--all-targets -- -D warnings`, więc bez
// tej linii gałąź `unwrap_or_else(|| panic!(…))` niżej zatrzymuje się w BRAMCE, a nie tutaj —
// i zatrzymuje ją na kształcie komunikatu, nie na zachowaniu, które to kryterium sądzi.
//
// Ani jedna asercja przez to nie znika: `panic!` w `unwrap_or_else` cytuje CAŁY raport odmów,
// czyli mówi, czego w nim zabrakło. `expect()` na jego miejscu wypisałby samo „None".
// Ta sama linia i ten sam powód stoją w `tests/skills_ingest_scanner.rs`.
#![allow(clippy::panic)]

use std::collections::BTreeMap;

use loadout_lib::library::agents::{
    Agent, Passthrough, VendorOptions, passthrough_refused, vendor_args, vendor_args_filtered,
};

/// Podniesienie zapisane jako **wartość** zwykłej flagi. Nośnik nie jest zarezerwowany, więc
/// łapie to wyłącznie reguła, która czyta wartość.
///
/// 2026-08-24 (T-98, mandat właściciela D-1) — **NOŚNIKIEM BYŁO TU `--settings` I PRZESTAŁO
/// NIM BYĆ.** Od T-92 tę flagę ustawia Loadout sam: to nią jedzie plik z przepisanymi regułami
/// `deny` gospodarza, więc T-98 dopisało ją do listy zarezerwowanych. Przesłanka tego pliku
/// zmieniła się przez to świadomie, a nie jego wynik — trzy asercje niżej są co do jednej te
/// same, tylko wartość niosą pod flagą, której Loadout nie ustawia i nie ustawi.
///
/// Nowy nośnik jest wybrany tak, żeby przewracał filtr dopasowujący po POCZĄTKU napisu:
/// `--verbose` JEST zarezerwowane (bez niego CLI odmawia startu), a `--verbose-tool-output`
/// jest inną flagą tej samej aplikacji. Gdyby okazał się kiedyś zarezerwowany, ten plik
/// przestałby mierzyć filtr i zacząłby mierzyć listę — pilnuje tego osobno
/// `the_example_this_file_leans_on_is_free_and_the_old_one_is_not` na dole.
const CLAUDE_CARRIER: &str = "--verbose-tool-output";
const BYPASS: &str = "bypassPermissions";

/// Podniesienie zapisane jako **nazwa** flagi, z pustą wartością. To jest dokładnie ten wiersz
/// z `~/.loadout/agents/*.json`, którym otwiera się TASK.md.
const SKIP_PERMISSIONS: &str = "--dangerously-skip-permissions";

/// To samo od strony drugiego vendora: `--sandbox` nie jest zarezerwowane, a jego wartość
/// omija dial tak samo skutecznie jak `-s`.
const CODEX_CARRIER: &str = "--sandbox";
const FULL_ACCESS: &str = "danger-full-access";

/// Trzy podniesienia, po nazwie — te same trzy, które wymienia D6 i TASK.md.
const ESCALATIONS: [&str; 3] = [BYPASS, SKIP_PERMISSIONS, FULL_ACCESS];

/// Flaga, którą Loadout ustawia sam od T-92 — nośnik przepisanych reguł `deny` gospodarza.
/// Do T-98 stała w tym pliku jako [`CLAUDE_CARRIER`], czyli jako przykład flagi WOLNEJ.
const SETTINGS: &str = "--settings";
/// Ścieżka, którą przelotka podstawiłaby zamiast pliku napisanego przez Loadouta. Niewinna
/// z rozmysłu: gdyby niosła podniesienie, odmowę tłumaczyłaby reguła o dialu i pozycja na
/// liście zarezerwowanych mogłaby nie istnieć.
const SETTINGS_VALUE: &str = "mine.json";

/// Flaga, o której Loadout nigdy nie słyszał i której nie ma prawa tknąć. Bez niej przelotka
/// przestaje istnieć, a kryterium przechodzi implementacja odrzucająca wszystko.
const INNOCENT_CLAUDE: &str = "--some-new-flag";
const INNOCENT_CLAUDE_VALUE: &str = "value";
/// Przykład wzięty wprost z D6.
const INNOCENT_CODEX: &str = "model_reasoning_summary";
const INNOCENT_CODEX_VALUE: &str = "detailed";

/// Agent z przelotką, w której obaj vendorzy próbują podnieść dial, a przy okazji stoi po
/// jednej fladze, która nikomu nie wadzi.
fn agent_that_tries_to_raise_the_dial() -> Agent {
    let mut claude = BTreeMap::new();
    claude.insert(CLAUDE_CARRIER.to_string(), BYPASS.to_string());
    claude.insert(SKIP_PERMISSIONS.to_string(), String::new());
    claude.insert(
        INNOCENT_CLAUDE.to_string(),
        INNOCENT_CLAUDE_VALUE.to_string(),
    );

    let mut codex = BTreeMap::new();
    codex.insert(CODEX_CARRIER.to_string(), FULL_ACCESS.to_string());
    codex.insert(INNOCENT_CODEX.to_string(), INNOCENT_CODEX_VALUE.to_string());

    let mut options = VendorOptions::new();
    options.insert("claude".to_string(), claude);
    options.insert("codex".to_string(), codex);

    Agent {
        vendor_options: options,
        ..Agent::example()
    }
}

/// Wartość, która stoi zaraz za tym kluczem. `--effort` bez `high` to albo błąd składni przy
/// starcie, albo — gorzej — flaga, która znaczy wtedy co innego, więc sama obecność klucza
/// niczego nie dowodzi.
fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let at = args.iter().position(|item| item == flag)?;
    args.get(at + 1).map(String::as_str)
}

/// Żadne z trzech podniesień nie przecieka do argumentów — **każde sprawdzane z osobna, po
/// nazwie**, żeby wiadomo było które.
fn nothing_raises_the_dial(args: &[String], vendor: &str) {
    for escalation in ESCALATIONS {
        assert!(
            !args.iter().any(|item| item.contains(escalation)),
            "`{escalation}` reached the argv handed to {vendor}. What an agent may do with your \
             files is set on the dial and nowhere else (D6); a passthrough that can raise it \
             makes the dial decorative. Got: {args:?}"
        );
    }
}

#[test]
pub(crate) fn claude_gets_the_harmless_flag_and_not_one_escalation() {
    let handed: Passthrough = vendor_args_filtered(&agent_that_tries_to_raise_the_dial(), "claude");

    nothing_raises_the_dial(&handed.args, "Claude Code");

    assert_eq!(
        value_after(&handed.args, INNOCENT_CLAUDE),
        Some(INNOCENT_CLAUDE_VALUE),
        "the flag that raises nothing has to survive WITH its value next to it. This is the \
         assertion an implementation that refuses every passthrough cannot pass — and the \
         passthrough exists so that a flag announced this morning is usable this afternoon, \
         without a release of Loadout (D6). Got: {:?}",
        handed.args
    );
    assert_eq!(
        handed.args,
        [INNOCENT_CLAUDE, INNOCENT_CLAUDE_VALUE],
        "and nothing else: two entries dropped means exactly two entries dropped. `argv is \
         shorter` is the assertion this one replaces — it passes for a filter that cuts a flag \
         at random. Got: {:?}",
        handed.args
    );
}

#[test]
pub(crate) fn codex_gets_the_harmless_flag_and_not_one_escalation() {
    let handed = vendor_args_filtered(&agent_that_tries_to_raise_the_dial(), "codex");

    nothing_raises_the_dial(&handed.args, "Codex");

    assert_eq!(
        value_after(&handed.args, INNOCENT_CODEX),
        Some(INNOCENT_CODEX_VALUE),
        "the same positive case from the other vendor's side — the rule is about the dial, not \
         about one agent app. Got: {:?}",
        handed.args
    );
    assert_eq!(
        handed.args,
        [INNOCENT_CODEX, INNOCENT_CODEX_VALUE],
        "and nothing else. Got: {:?}",
        handed.args
    );
}

#[test]
pub(crate) fn the_refusal_says_which_line_to_delete_and_what_it_tried_to_raise() {
    let agent = agent_that_tries_to_raise_the_dial();

    let claude = vendor_args_filtered(&agent, "claude");
    let codex = vendor_args_filtered(&agent, "codex");

    // Pary `(vendor, klucz przelotki, podniesienie)`. Klucz jest tym, co użytkownik ma skasować;
    // podniesienie tym, dlaczego. Nazwanie samego klucza nie wystarcza: `--settings` samo w sobie
    // jest legalne, więc odmowa bez powodu czyta się jak awaria Loadouta.
    let expected = [
        (&claude, CLAUDE_CARRIER, BYPASS),
        // Flaga, która JEST podniesieniem: kasuje się ten sam wiersz, którego nazwę widać.
        (&claude, SKIP_PERMISSIONS, SKIP_PERMISSIONS),
        (&codex, CODEX_CARRIER, FULL_ACCESS),
    ];

    for (handed, flag, escalation) in expected {
        let named = handed
            .refused
            .iter()
            .find(|refusal| refusal.flag == flag)
            .unwrap_or_else(|| {
                panic!(
                    "nothing in the report names `{flag}`. A silent refusal teaches the user that \
                     the passthrough does not work, instead of that it was blocked — and then \
                     they type the same thing again, spelled differently. Report: {:?}",
                    handed.refused
                )
            });
        assert_eq!(
            named.escalation, escalation,
            "`{flag}` was refused for the wrong reason, or for none that can be shown. The \
             sentence a user reads has to name what this line tried to raise. Report: {:?}",
            handed.refused
        );
    }

    assert!(
        !claude
            .refused
            .iter()
            .any(|refusal| refusal.flag == INNOCENT_CLAUDE),
        "`{INNOCENT_CLAUDE}` raises nothing, so it may not appear among the refusals. A report \
         that names every entry names none of them. Report: {:?}",
        claude.refused
    );
    assert_eq!(
        claude.refused.len(),
        2,
        "two entries in this passthrough raise the dial, so there are two refusals — one per \
         line the user has to delete. Report: {:?}",
        claude.refused
    );
}

#[test]
pub(crate) fn the_plain_argv_builder_is_filtered_too_not_only_its_talking_twin() {
    let agent = agent_that_tries_to_raise_the_dial();

    // `vendor_args` jest funkcją, którą TASK.md nazywa dziurą: to ona tłumaczy przelotkę prosto
    // do argv i to ją podepnie sterownik. Filtr, który mieszka wyłącznie w drugiej funkcji, jest
    // filtrem, którego bieg nie zawoła — czyli kontrolką bez handlera (niezmiennik 16).
    nothing_raises_the_dial(&vendor_args(&agent, "claude"), "Claude Code");
    nothing_raises_the_dial(&vendor_args(&agent, "codex"), "Codex");

    let claude = vendor_args(&agent, "claude");
    assert_eq!(
        value_after(&claude, INNOCENT_CLAUDE),
        Some(INNOCENT_CLAUDE_VALUE),
        "and the harmless flag still goes through this door too. Got: {claude:?}"
    );

    // Obie funkcje mają odpowiadać to samo. Najtańszym sposobem, żeby to było prawdą, jest jedna
    // implementacja filtra, którą druga funkcja woła — a nie dwie, które trzeba pamiętać naraz.
    assert_eq!(
        claude,
        vendor_args_filtered(&agent, "claude").args,
        "the two entry points disagree about the same passthrough. Two filters are two answers, \
         and the older one is always the one still wired up"
    );
}

/// Agent, którego przelotka niesie dokładnie jeden wpis Claude'a.
fn agent_offering(flag: &str, value: &str) -> Agent {
    let mut flags = BTreeMap::new();
    flags.insert(flag.to_string(), value.to_string());

    let mut options = VendorOptions::new();
    options.insert("claude".to_string(), flags);

    Agent {
        vendor_options: options,
        ..Agent::example()
    }
}

/// 2026-08-24 (T-98, mandat właściciela D-1) — STRAŻNIK PRZESŁANKI, NIE CZWARTA ASERCJA O FILTRZE.
///
/// Trzy testy wyżej stoją na jednym założeniu: że [`CLAUDE_CARRIER`] jest flagą, której Loadout
/// nie ustawia. Kiedy to założenie przestaje być prawdziwe — a właśnie przestało być prawdziwe
/// dla `--settings`, którym ten plik posługiwał się do dziś — tamte trzy **nie pękają**. Zaczynają
/// mierzyć listę zarezerwowanych zamiast filtra podniesień, cicho, i naprawia się to zwykle
/// osłabieniem asercji.
///
/// Dlatego przesłanka ma tu własny test i pyta o nią tam, gdzie odmowa jest widoczna dla
/// człowieka: [`passthrough_refused`] jest zdaniem, którym bieg odmawia startu (niezmiennik 29).
/// Ta sama funkcja odpowiada na obie połowy pytania — że nowy przykład jest wolny i że stary
/// naprawdę zmienił stronę.
#[test]
pub(crate) fn the_example_this_file_leans_on_is_free_and_the_old_one_is_not() {
    let free = passthrough_refused(&agent_offering(CLAUDE_CARRIER, BYPASS));
    assert_eq!(
        free.len(),
        1,
        "`{CLAUDE_CARRIER}` carrying `{BYPASS}` has to be refused exactly once, for the value it \
         carries. Two refusals mean the flag NAME is now ours to set as well — and then the three \
         tests above stopped measuring the filter and started measuring the list, without saying \
         a word. Loadout said: {free:?}"
    );

    let harmless = passthrough_refused(&agent_offering(CLAUDE_CARRIER, "on"));
    assert!(
        harmless.is_empty(),
        "`{CLAUDE_CARRIER}` with a harmless value is refused: {harmless:?}. It begins with \
         `--verbose`, which IS ours to set, and is a different argument of the same app — a rule \
         matching by the start of the text kills it. The passthrough exists so that an argument \
         announced this morning is usable this afternoon, without a release of Loadout (D6)"
    );

    // I druga połowa mandatu D-1: flaga, która była tu przykładem WOLNYM, jest teraz odmową —
    // po samej nazwie, z wartością, w której nie ma czego podnosić. Loadout pisze ten plik raz
    // na bieg i to z niego biorą się przepisane reguły `deny` gospodarza; przelotka wskazująca
    // inny plik podmienia je w całości, po cichu, a wszystko dalej wygląda tak samo jak zwykle.
    let settings = passthrough_refused(&agent_offering(SETTINGS, SETTINGS_VALUE));
    assert!(
        settings.iter().any(|one| one.contains(SETTINGS)),
        "`{SETTINGS} {SETTINGS_VALUE}` starts the run anyway, or stops it without naming the line \
         to delete: {settings:?}"
    );
}
