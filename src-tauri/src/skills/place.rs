//! Rozmieszczanie: walidacja, emiter `SpecStrict`, plan, kopiowanie, usuwanie i werdykt
//! „czy vendor to widzi".
//!
//! Cicha porażka, przed którą stoi cały ten plik, to zielony ptaszek „Installed for 6 tools"
//! postawiony dlatego, że `fs::write` zwróciło `Ok`. Plik leży, ścieżka jest o poziom obok tej,
//! w którą vendor zagląda, a użytkownik dowiaduje się o tym nigdy — bo „agent nie wie
//! o umiejętności" nie da się odróżnić od „model nie uznał, że warto jej użyć". Dlatego
//! [`discovery_from_init`] czyta zdarzenie od vendora zamiast wnioskować z kodu powrotu.
//!
//! **Kopiujemy, nie symlinkujemy** [T5 §4.5]. Dowiązanie działa u Claude Code i jest tam nawet
//! udokumentowane, ale: u pozostałych pięciu vendorów jest niezweryfikowane, rozpada się
//! u każdego kolegi z zespołu, który sklonuje repo z umiejętnością w zakresie projektu, i na
//! Windowsie wymaga trybu dewelopera albo uprawnień administratora — czyli jest największym
//! zagrożeniem dla przenośności w całym tym projekcie. `fs::copy` zachowuje przy tym
//! uprawnienia, więc `scripts/run.sh` zostaje wykonywalny.
//!
//! Kodu platformowego tu nie ma (niezmiennik 3): dowiązanie wykrywamy `fs::symlink_metadata`,
//! nie `#[cfg(unix)]`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{
    DESTINATION_DIRS, Error, Missing, NON_SPEC_FIELDS, RESERVED_DIR_NAME, Result, Roots,
    SHELF_THE_OTHER_FIVE_READ, SPEC_FIELDS, Scope, Skill, SkillDoc, StepSkills, Why,
};

/// Nazwa pliku umiejętności. Jedna u wszystkich sześciu vendorów [T5 §2.2] — zmienna, żeby
/// „ten sam plik pod dwiema ścieżkami" znaczyło dosłownie to samo w zapisie i w odczycie
/// pierwszego wiersza cudzego katalogu.
const SKILL_FILE: &str = "SKILL.md";

/// Zdanie, którym `context: fork` wraca do ciała [T5 §4.2].
///
/// `context` jest polem Claude Code i żaden z pozostałych pięciu vendorów go nie zna, więc
/// jedyne, co z niego zostaje przenośne, to instrukcja napisana wprost do modelu.
const FORK_SENTENCE: &str = "Run this as an isolated task.";

/// Tablica pól ma sześć pozycji i [`spec_line`] ma sześć gałęzi. Siódme pole dopisane do
/// [`SPEC_FIELDS`] bez gałęzi tutaj nie jest błędem kompilacji — po cichu **nie jedzie do
/// pliku**, a `SKILL.md` bez pola wygląda jak `SKILL.md`, w którym autor go nie podał.
/// Ta linia zamienia tę ciszę w błąd kompilacji.
const _: () = assert!(SPEC_FIELDS.len() == 6);

/// Co instalacja zapisze — pokazywane człowiekowi, zanim cokolwiek się wydarzy.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// Dwa katalogi umiejętności: `<korzeń>/.claude/skills/<name>` i `…/.agents/skills/<name>`.
    pub writes: Vec<PathBuf>,
    /// Katalogi o tej nazwie, które już tam są — nasze do nadpisania albo cudze.
    pub conflicts: Vec<Conflict>,
    /// Sidecar, w którym [`apply`] zapisze, że te katalogi napisał Loadout.
    ///
    /// Plan niesie tę ścieżkę, żeby [`apply`] nie potrzebowało [`Roots`]: plan jest pełnym
    /// opisem tego, co się stanie, a zapis „to jest nasze" jest częścią tego, co się stanie.
    /// Sidecar leży poza oboma drzewami docelowymi — jest zapisem Loadouta o wyjściu builda,
    /// nie kolejnym plikiem obok `SKILL.md` (niezmiennik 21: nikt nie czyta `.loadout-marker`,
    /// a sidecar czytają [`plan`] i [`remove`]).
    pub sidecar: PathBuf,
}

/// Katalog o tej samej nazwie już istnieje w miejscu docelowym.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// Loadout pisał ten katalog wcześniej — jest w sidecarze. Instalacja go nadpisze,
    /// i tak ma być: katalogi vendorów są wyjściem builda (niezmiennik 4).
    Update { path: PathBuf },
    /// Katalogu nie ma w sidecarze, więc nie jest nasz. Nie nadpisujemy go bez pytania.
    Foreign {
        path: PathBuf,
        /// Pierwszy wiersz cudzego `SKILL.md`, zacytowany dosłownie — żeby człowiek
        /// zobaczył, czyj to plik, zanim zdecyduje.
        first_line: String,
    },
}

/// Wynik usunięcia.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    /// Obie kopie zdjęte. Kanoniczna umiejętność w danych aplikacji zostaje — usuwamy
    /// wyjście builda, nie źródło (niezmiennik 4).
    Done { paths: Vec<PathBuf> },
    /// Co najmniej jeden katalog o tej nazwie nie jest nasz. Nie kasujemy **niczego**:
    /// pół usunięcia zostawia stan, którego nikt nie umie opisać, a cudza umiejętność
    /// skasowana „przy okazji" jest nie do odzyskania.
    Skipped {
        path: PathBuf,
        /// Zdanie dla człowieka: dlaczego to zostało.
        why: String,
    },
}

/// Czy vendor naprawdę widzi umiejętność [T5 §6.3, poziom 3].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    /// Vendor wymienił ją na swojej liście.
    Seen,
    /// Vendor wymienił swoje umiejętności i tej wśród nich nie ma.
    NotSeen {
        /// Ścieżki, w które pisaliśmy. To jest cała treść zgłoszenia dla człowieka:
        /// „napisaliśmy tu i tu, a vendor tego nie widzi".
        looked_in: Vec<PathBuf>,
    },
    /// Nie wiadomo — i to nie jest błąd (niezmiennik 5). Brak CLI, zdarzenie o nieznanym
    /// kształcie, nowa wersja vendora: żadne z tego nie może zaświecić się na czerwono.
    Unknown(&'static str),
}

// ── Umiejętności jednego kroku ─────────────────────────────────────────────────────────────
//
// Zachowanie stoi tutaj, a typ w `skills/mod.rs`, dokładnie jak reszta tego modułu: tamten plik
// trzyma dane, ten trzyma reguły. Funkcje wiążące, nie wolne funkcje modułu, i to nie jest
// kwestia gustu — obie odpowiadają na pytanie o KONKRETNY zbiór umiejętności, więc zbiór jest
// ich pierwszym argumentem, a `StepSkills::for_the_step` jest jedynym sposobem, żeby taki zbiór
// w ogóle powstał. Ten sam kształt, z tego samego powodu, stoi przy
// `engine::drivers::ValidatedImages::validate`.

impl StepSkills {
    /// Efektywny zbiór umiejętności jednego kroku: agent, zawężony nadpisaniem kroku.
    ///
    /// `data` to korzeń danych aplikacji (`~/.loadout`), czyli ten sam, który [`Roots::data`]
    /// wskazuje przy instalacji — kanoniczna kopia leży pod `<data>/skills/<nazwa>/`. Pytamy
    /// **biblioteki**, a nie katalogów vendorów, i to jest cała treść tego argumentu: katalogi
    /// vendorów bywają cudze (człowiek mógł napisać tam własną umiejętność ręcznie), a bieg ma
    /// podać agentowi wyłącznie to, co Loadout naprawdę posiada.
    ///
    /// `agent` to `Agent.skills` z definicji efektywnej, `step` to `Overrides.skills` tego
    /// kroku: `None` znaczy „brak klucza", czyli **weź to, co ma agent** — dokładnie tak, jak
    /// czyta to `library::agents::resolve` (RFC 7396). `Some(&[])` to co innego i musi być czym
    /// innym: człowiek, który wyczyścił listę na kroku, powiedział „żadnych".
    ///
    /// `step_name` wchodzi tu wyłącznie po to, żeby odmowa miała czym nazwać kafelek. Zdanie bez
    /// nazwy kroku zostawia człowieka z przeszukiwaniem workflow (niezmiennik 29).
    ///
    /// # Odmowa pada TUTAJ, przy budowie zadania
    ///
    /// Niezmiennik 12: odmowa najpóźniej przy Starcie, nigdy w trakcie biegu. Alternatywa —
    /// przycięcie listy i jazda dalej — jest najdroższą wersją tej wady: agent, któremu po cichu
    /// zabrano umiejętność, wygląda dokładnie jak agent, który „nie umiał".
    pub fn for_the_step(
        data: &Path,
        agent: &[String],
        step: Option<&[String]>,
        step_name: &str,
    ) -> std::result::Result<Self, Missing> {
        let refuse = |skill: &str, why: Why| Missing {
            step: step_name.to_owned(),
            skill: skill.to_owned(),
            why,
        };

        // KROK ZAWĘŻA, NIGDY NIE POSZERZA, i to pytanie stoi PIERWSZE — przed biblioteką. Nazwa
        // zapisana w bibliotece, ale nieprzypisana temu agentowi, jest odmową o innej naprawie
        // (dopisz ją agentowi) niż nazwa, której nie ma nigdzie (zapisz ją). Odwrotna kolejność
        // odpowiadałaby na drugie pytanie wtedy, gdy człowiek zadał pierwsze.
        if let Some(picked) = step
            && let Some(extra) = picked
                .iter()
                .find(|want| !agent.iter().any(|has| has == *want))
        {
            return Err(refuse(extra, Why::NotOnTheAgent));
        }

        // KOLEJNOŚĆ Z DEFINICJI AGENTA, nie z listy kroku: to ta pierwsza stoi na ekranie agenta
        // i to w niej człowiek te nazwy widzi. Bez powtórzeń, bo agent zapisany ręcznie z tą samą
        // nazwą dwa razy nie znaczy „dwie umiejętności" — a `--plugin-dir` i tak zobaczyłby jeden
        // katalog.
        let mut names: Vec<String> = Vec::new();
        for name in agent {
            let wanted = step.is_none_or(|picked| picked.iter().any(|want| want == name));
            if wanted && !names.contains(name) {
                names.push(name.clone());
            }
        }

        let mut dirs = Vec::with_capacity(names.len());
        for name in &names {
            // Nazwa z definicji WYZNACZA CZYTANĄ ŚCIEŻKĘ, a definicję agenta człowiek pisze
            // ręcznie: `..` albo `a/b` w tym polu znaczyłoby odczyt poza biblioteką. Pytamy
            // [`is_slug`], czyli tą samą regułą, którą [`validate_strict`] wymusza na każdej
            // zapisanej umiejętności — nazwa, która jej nie spełnia, nie może w bibliotece leżeć.
            if !is_slug(name) {
                return Err(refuse(name, Why::NotInTheLibrary));
            }
            let dir = data.join(SKILLS_DIR).join(name);
            // DWA RÓŻNE STANY, DWA RÓŻNE ZDANIA. Katalogu nie ma → tej umiejętności po prostu nie
            // zapisano. Katalog jest, a pliku nie da się przeczytać albo nie przechodzi
            // walidatora → jest, tylko nie jest umiejętnością; te dwie rzeczy naprawia się
            // inaczej. `symlink_metadata`, nie `exists()`: dowiązanie w tym miejscu też jest
            // czymś, co tam stoi.
            if fs::symlink_metadata(&dir).is_err() {
                return Err(refuse(name, Why::NotInTheLibrary));
            }
            let Ok(text) = fs::read_to_string(dir.join(SKILL_FILE)) else {
                return Err(refuse(name, Why::Unusable));
            };
            // PYTAMY „CZY TO JEST UMIEJĘTNOŚĆ", NIE „CZY TRZYMA SIĘ SPECYFIKACJI". Powód
            // w całości stoi przy [`validate_usable`]: reguła wydawnicza w tym miejscu wywracała
            // bieg na pliku, który jest poprawną umiejętnością Claude Code i różni się od
            // specyfikacji jednym polem w nagłówku (niezmiennik 5).
            if validate_usable(name, &read_doc(&text)).is_err() {
                return Err(refuse(name, Why::Unusable));
            }
            dirs.push(dir);
        }

        Ok(Self { names, dirs })
    }

    /// Kładzie te umiejętności w katalogu roboczym kroku, pod `.agents/skills/<nazwa>/`.
    ///
    /// PIĘCIU Z SZEŚCIU VENDORÓW CZYTA TĘ PÓŁKĘ i żaden z nich nie umie przyjąć jej ścieżki
    /// argumentem [T5 §3.1] — więc dla nich „agent ma umiejętność" znaczy dosłownie „plik leży
    /// w jego katalogu roboczym". To jest druga droga tego zadania; pierwszą, katalog pluginu
    /// podany argumentem, składa [`crate::inherit::Rewritten::from_the_library`].
    ///
    /// `ours` mówi, czy ten katalog roboczy jest NASZ — czyli czy leży pod katalogiem tego biegu.
    /// [`Folder::FreshCopy`] daje `true`; folder projektu i folder wskazany ręcznie dają `false`.
    /// Krok pracujący w folderze człowieka jest **odmową** ([`Why::WouldWriteIntoYourFolder`]),
    /// nie cichym zapisem: katalog dopisany do cudzego repozytorium jest zmianą, o której jego
    /// właściciel dowiaduje się z `git status`, a po biegu zostaje tam na zawsze.
    ///
    /// 2026-08-22 (T-79) — PYTANIE BRZMI „CZY LEŻY POD BIEGIEM", A NIE „CZY TEN KROK GO ZAŁOŻYŁ",
    /// i różnica jest widoczna dokładnie raz: krok `same-copy` pracuje w drzewie, które założył
    /// krok przed nim (`commands::run::where_it_works` daje mu `ours: false`, bo katalogu NIE
    /// zakłada). To drzewo leży pod katalogiem biegu i jest nasze — odmowa dla niego byłaby
    /// odmową o folderze człowieka wystawioną krokowi, który w folderze człowieka nie pracuje.
    ///
    /// Oddaje ścieżki, które naprawdę powstały — to samo pytanie i ta sama odpowiedź, co przy
    /// [`Discovery::NotSeen::looked_in`]: bez nich zgłoszenie „vendor tego nie widzi" nie ma czym
    /// nazwać miejsca, w które pisaliśmy.
    ///
    /// [`Folder::FreshCopy`]: crate::workflow::file::Folder::FreshCopy
    pub fn into_the_step_folder(
        &self,
        cwd: &Path,
        ours: bool,
        step_name: &str,
    ) -> Result<Vec<PathBuf>> {
        // Pusty zbiór nie ma czego położyć i **nie zakłada półki**: katalog `.agents/skills/`
        // stojący pusto w drzewie kroku jest tym samym fałszywym zielonym ptaszkiem, co plugin
        // ładujący się z zerem umiejętności. To jest też jedyny powód, dla którego krok bez
        // umiejętności pracujący w folderze człowieka nie ma o co odmawiać.
        if self.names.is_empty() {
            return Ok(Vec::new());
        }
        if !ours {
            return Err(Missing {
                step: step_name.to_owned(),
                // PIERWSZA Z LISTY, nie wszystkie: odmowa ma nazwać jedną rzecz do odznaczenia.
                // Człowiek, który zdejmie ją i naciśnie Start, usłyszy o następnej — a lista
                // pięciu nazw w jednym zdaniu nie mówi, od której zacząć.
                skill: self.names[0].clone(),
                why: Why::WouldWriteIntoYourFolder {
                    folder: cwd.to_path_buf(),
                },
            }
            .into());
        }

        let mut wrote = Vec::with_capacity(self.names.len());
        for (name, source) in self.names.iter().zip(&self.dirs) {
            let landing = cwd.join(SHELF_THE_OTHER_FIVE_READ).join(name);
            copy_the_skill(source, &landing)?;
            wrote.push(landing);
        }
        Ok(wrote)
    }
}

/// Kopiuje kanoniczną umiejętność — **całą** — do wskazanego katalogu.
///
/// CAŁĄ, czyli razem z `scripts/`, `references/` i `assets/` [T5 §2.2]. Umiejętność, której
/// `SKILL.md` każe uruchomić `scripts/check.sh`, a skryptu przy niej nie ma, jest umiejętnością
/// zepsutą w sposób nieodróżnialny z zewnątrz od „model nie uznał, że warto po nią sięgnąć" —
/// czyli dokładnie tą cichą porażką, przed którą stoi całe to zadanie.
///
/// `fs::copy`, nie `write(read(..))`: copy zachowuje uprawnienia, czyli bit wykonywalności
/// `scripts/run.sh`. Zapis samych bajtów gubi go po cichu i skrypt przestaje się dać uruchomić
/// dopiero u użytkownika (ten sam powód stoi przy [`apply`]). Kopiujemy TU treść, którą Loadout
/// **posiada** — dla umiejętności, którą Loadout tylko cytuje, obowiązuje zasada odwrotna
/// i mieszka przy [`crate::inherit::rewrite::plugin_dir`].
///
/// Iteracyjnie, nie rekurencyjnie: głębokie drzewo nie ma prawa przepełnić stosu (ta sama
/// zasada, co przy obchodach w `workflow::check`). Dowiązania **pomijamy** — zapisujemy wyłącznie
/// to, co sami postanowiliśmy zapisać, a dowiązanie wskazuje poza katalog, który kopiujemy
/// (niezmienniki 3 i 4).
pub fn copy_the_skill(from: &Path, into: &Path) -> std::io::Result<()> {
    let mut stack = vec![(from.to_path_buf(), into.to_path_buf())];
    while let Some((source, target)) = stack.pop() {
        fs::create_dir_all(&target)?;
        for entry in fs::read_dir(&source)? {
            let entry = entry?;
            let path = entry.path();
            let landing = target.join(entry.file_name());
            // `symlink_metadata`, nie `metadata`: `metadata` przechodzi dowiązanie na wylot
            // i skopiowałoby treść pliku, który leży gdzie indziej.
            let kind = fs::symlink_metadata(&path)?;
            if kind.is_dir() {
                stack.push((path, landing));
            } else if kind.is_file() {
                fs::copy(&path, &landing)?;
            }
        }
    }
    Ok(())
}

/// Treść `SKILL.md` → front-matter i ciało, permisywnie (niezmiennik 5).
///
/// Stoi tutaj, przy [`validate_strict`], bo tylko razem odpowiadają na pytanie „czy ten plik
/// jest umiejętnością" — a to jest dokładnie to pytanie, które [`StepSkills::for_the_step`]
/// musi zadać każdej wybranej pozycji, zanim uzna ją za dojechaną. `SKILL.md`, którego nie da
/// się przeczytać, jest tą samą odmową co nazwa spoza biblioteki ([`Why::Unusable`]):
/// z zewnątrz agent bez umiejętności i agent z umiejętnością nie do odczytania odpowiadają
/// identycznie.
///
/// Nieznany klucz **nie jest błędem** (niezmiennik 5): ląduje w polach dokumentu, a decyzję
/// o nim podejmuje dopiero [`validate_strict`] albo [`emit`]. Front-matter bez zamknięcia nie
/// jest front-matterem — `---` w pierwszej linii pliku, który nigdy się nie domyka, to pozioma
/// kreska, a nie nagłówek.
///
/// [`Why::Unusable`]: super::Why::Unusable
#[must_use]
pub fn read_doc(text: &str) -> SkillDoc {
    let whole = || SkillDoc {
        fields: Vec::new(),
        body: text.to_owned(),
    };
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return whole();
    };

    let mut fields: Vec<(String, String)> = Vec::new();
    let mut consumed = 0usize;
    let mut closed = false;

    for line in rest.split_inclusive('\n') {
        consumed += line.len();
        let content = line.trim_end_matches(['\n', '\r']);
        if content.trim_end() == "---" {
            closed = true;
            break;
        }
        if line.starts_with([' ', '\t']) {
            // Wcięcie to ciąg dalszy poprzedniego klucza (`metadata:` i jego pary). Surowy tekst
            // wystarczy: walidator pyta o obecność pola, a emiter i tak pisze mapę po swojemu.
            if let Some((_, value)) = fields.last_mut() {
                value.push('\n');
                value.push_str(content);
            }
        } else if let Some((key, value)) = content.split_once(':') {
            fields.push((key.trim().to_owned(), unquote(value.trim())));
        }
    }

    if closed {
        SkillDoc {
            fields,
            body: rest[consumed..].to_owned(),
        }
    } else {
        whole()
    }
}

/// Wartość YAML-a bez cudzysłowu, jeśli w cudzysłowie przyszła. Lustro [`scalar`], które cytuje
/// przy zapisie.
///
/// Bez tego `name: "pdf"` nie zgadza się z nazwą katalogu `pdf`, a `description: "a: b"` wraca
/// z cudzysłowami w środku pola i po drugim „Update" umiejętność ma tytuł w trzech warstwach
/// znaków cytowania.
fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return value[1..value.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\\\", "\\");
        }
    }
    value.to_owned()
}

/// Dwa katalogi docelowe dla danego zakresu — same korzenie, bez `<name>`.
///
/// `project = None` przy [`Scope::Project`] daje ścieżki względne, czyli „tutaj": tak samo
/// rozwiązuje je Codex (`$CWD/.agents/skills`). Do dysku to nigdy nie dochodzi — [`plan`]
/// odmawia zakresu projektowego bez korzenia zwrotem [`super::Error::NoProjectRoot`].
#[must_use]
pub fn destinations(scope: Scope, home: &Path, project: Option<&Path>) -> [PathBuf; 2] {
    let root = match scope {
        Scope::Global => home,
        Scope::Project => project.unwrap_or(Path::new("")),
    };
    DESTINATION_DIRS.map(|dir| root.join(dir))
}

/// Reguły walidatora referencyjnego, przepisane w Ruście [T5 §6.2].
///
/// Przepisane, nie wywołane: `agentskills` jest w Pythonie, a `uv` jako zależność środowiska
/// uruchomieniowego aplikacji desktopowej w Ruście to zły interes za ~40 linii reguł. CLI
/// zostaje wyrocznią różnicową w naszym własnym CI, nie w produkcie.
///
/// Komunikaty są **dosłownie** te z wyroczni, bo użytkownik zobaczy je też wtedy, gdy vendor
/// odmówi po swojemu — a dwa różne zdania o tej samej przyczynie to dwa różne zgłoszenia.
/// Jedna przyczyna, jeden komunikat: wspólne „invalid skill" na osiem przyczyn nie mówi
/// nikomu, co poprawić.
///
/// Nazwa katalogu jest osobnym argumentem, bo dwie reguły dotyczą **jej**, nie pliku:
/// zgodność z `name` i zarezerwowane [`super::RESERVED_DIR_NAME`].
pub fn validate_strict(dir_name: &str, doc: &SkillDoc) -> Result<(), Vec<String>> {
    /// Limity ze specyfikacji [T5 §2.3]. Stoją tutaj, przy jedynej funkcji, która ich używa;
    /// druga kopia „dla podglądu w UI" jest tym, jak jeden z nich zostaje na starej wartości.
    const DESCRIPTION_MAX: usize = 1024;
    const COMPATIBILITY_MAX: usize = 500;

    let mut said = validate_usable(dir_name, doc).err().unwrap_or_default();

    if field(doc, "description").is_some_and(|value| value.chars().count() > DESCRIPTION_MAX) {
        said.push(format!(
            "Skill description must be {DESCRIPTION_MAX} characters or less"
        ));
    }
    if field(doc, "compatibility").is_some_and(|value| value.chars().count() > COMPATIBILITY_MAX) {
        said.push(format!(
            "Skill compatibility must be {COMPATIBILITY_MAX} characters or less"
        ));
    }

    // Zbiór, nie lista: to samo pole wpisane dwa razy jest jedną przyczyną. Posortowany, bo
    // komunikat ma być ten sam niezależnie od kolejności pól w pliku, który go wywołał.
    let unexpected: BTreeSet<&str> = doc
        .fields
        .iter()
        .map(|(key, _)| key.as_str())
        .filter(|key| !SPEC_FIELDS.contains(key))
        .collect();
    if !unexpected.is_empty() {
        let named: Vec<&str> = unexpected.into_iter().collect();
        said.push(format!(
            "Unexpected fields in frontmatter: {}. Only [{}] are allowed.",
            named.join(", "),
            allowed_list()
        ));
    }

    if said.is_empty() { Ok(()) } else { Err(said) }
}

/// Wartość pola front-mattera, o ile w ogóle coś niesie.
///
/// Puste i samo z białych znaków znaczy „nie ma go": pole `description:` bez treści jest tym
/// samym brakiem, co pole niewpisane wcale, i naprawia się je tym samym ruchem.
///
/// PUBLICZNA od 2026-08-23, żeby lista umiejętności miała czym opisać kafelek. Drugi czytnik
/// front-mattera napisany po stronie `commands::skills` byłby trzecią kopią tej samej reguły
/// (`read_doc` już raz ją zdublował) — a kopia, która nie wie o cudzysłowach, pokazywałaby
/// człowiekowi opis w apostrofach.
#[must_use]
pub fn field<'a>(doc: &'a SkillDoc, key: &str) -> Option<&'a str> {
    doc.fields
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
        .filter(|value| !value.trim().is_empty())
}

/// Czy ten plik **jest umiejętnością** — pytanie węższe niż „czy trzyma się specyfikacji".
///
/// 2026-08-22 — TA FUNKCJA POWSTAŁA Z ODMOWY, KTÓREJ NIKT NIE ROZUMIAŁ. Bieg właściciela stanął
/// na zdaniu „its SKILL.md could not be read as a skill" dla pliku, który był całkowicie
/// poprawną umiejętnością Claude Code — miał tylko w nagłówku `user-invocable: false`, czyli
/// pole spoza specyfikacji `agentskills`. Krok pytał [`validate_strict`], a ta reguła jest
/// regułą **wydawniczą**: mówi, co Loadout wolno ZAPISAĆ, nie co wolno mu PRZECZYTAĆ.
///
/// Skala była na to jednoznaczna: w bibliotece właściciela **dwanaście z czternastu**
/// zaimportowanych umiejętności niosło takie pole (`argument-hint`, `when_to_use`, `model`,
/// `disable-model-invocation`, `user-invocable`), więc każda z nich wywracała bieg w chwili,
/// w której jakiś agent po nią sięgnął. Import kopiuje `SKILL.md` bajt w bajt z rozmysłem
/// (`import::apply`: „import jest migawką"), więc obie połowy produktu były wewnętrznie
/// spójne i sprzeczne ze sobą.
///
/// Niezmiennik 5 rozstrzyga to wprost i [`read_doc`] powtarza to w swoim nagłówku: **nieznany
/// klucz nie jest błędem**. Zostaje więc to, bez czego pliku naprawdę nie da się użyć: brak
/// nazwy albo opisu, nazwa niezgodna z katalogiem (umiejętność miałaby wtedy dwie nazwy,
/// zależnie od tego, kto pyta) i katalog zarezerwowany, który Claude Code pomija w ciszy.
/// Limity długości i lista dozwolonych pól zostają przy [`validate_strict`], bo pilnują tego,
/// co piszemy sami — i tam nadal obowiązują co do znaku.
pub fn validate_usable(dir_name: &str, doc: &SkillDoc) -> Result<(), Vec<String>> {
    /// Limit ze specyfikacji [T5 §2.3].
    const NAME_MAX: usize = 64;

    /// Słowa zastrzeżone w `name`. Nie ma ich w specyfikacji agentskills.io, są w dokumentacji
    /// platformy Anthropic — i kosztują jedno porównanie, więc egzekwujemy [T5 §2.3].
    const RESERVED_WORDS: [&str; 2] = ["anthropic", "claude"];

    let mut said = Vec::new();

    match field(doc, "name") {
        // Nazwy nie ma wcale — reszta reguł o nazwie nie ma o czym mówić, a jedna przyczyna
        // to jedno zdanie (nie osiem zdań o jednym brakującym polu).
        None => said.push("Missing required field in frontmatter: name".to_owned()),
        Some(name) => {
            if name != name.to_lowercase() {
                said.push(format!("Skill name '{name}' must be lowercase"));
            }
            if !is_slug(name) {
                said.push(format!(
                    "Skill name '{name}' must be lowercase letters, digits and single hyphens"
                ));
            }
            if name.chars().count() > NAME_MAX {
                said.push(format!(
                    "Skill name '{name}' must be {NAME_MAX} characters or less"
                ));
            }
            if let Some(word) = RESERVED_WORDS
                .iter()
                .find(|word| name.to_lowercase().contains(*word))
            {
                said.push(format!(
                    "Skill name '{name}' must not contain the reserved word '{word}'"
                ));
            }
            // Nazwa katalogu jest tym, co użytkownik wpisuje jako `/name`. Kiedy nie zgadza
            // się z polem, Claude Code bierze katalog, a lista bierze pole — i umiejętność
            // ma dwie nazwy zależnie od tego, kto pyta.
            if dir_name != name {
                said.push(format!(
                    "Directory name '{dir_name}' must match skill name '{name}'"
                ));
            }
        }
    }

    if field(doc, "description").is_none() {
        said.push("Missing required field in frontmatter: description".to_owned());
    }

    // Umiejętność napisana w folderze o tej nazwie jest poprawna, zainstalowana i niewidoczna
    // — Claude Code taki folder pomija, w dowolnej wielkości liter [T5 fact-check]. To ta sama
    // klasa cichej porażki co zła ścieżka, więc blokujemy, zamiast ostrzegać.
    if dir_name.eq_ignore_ascii_case(RESERVED_DIR_NAME) {
        said.push(format!(
            "Directory name '{dir_name}' is reserved: Claude Code skips a skill written in a \
             folder called '{RESERVED_DIR_NAME}'"
        ));
    }

    if said.is_empty() { Ok(()) } else { Err(said) }
}

/// `^[a-z0-9]+(-[a-z0-9]+)*$`, przepisane bez zależności na wyrażenia regularne.
///
/// Trzy warunki na łączniku to całe wyrażenie: bez wiodącego, bez końcowego, bez podwójnego.
fn is_slug(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Sześć dozwolonych pól tak, jak wylicza je walidator referencyjny: alfabetycznie,
/// w apostrofach, po przecinku.
///
/// Liczone z [`SPEC_FIELDS`], nie wpisane drugi raz: literał obok tablicy jest tym, jak
/// komunikat zaczyna wymieniać pole, którego emiter już nie zna.
fn allowed_list() -> String {
    let mut names = SPEC_FIELDS;
    names.sort_unstable();
    names
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Jeden `SKILL.md` w trybie `SpecStrict` i lista pól, które zostały zdjęte.
///
/// Jeden plik do obu katalogów, nie dwa warianty: jeden plik to jeden diff i jedna rzecz do
/// zdebugowania. `ClaudeExtended` jest świadomie poza zakresem [T5 §11].
///
/// Zdjęte pola **wracają**, nie giną: `argument-hint` jako wiersz `Arguments: …`, a
/// `context: fork` jako `Run this as an isolated task.` przed pierwszym akapitem [T5 §4.2].
/// Reszta z czternastki nie ma przenośnego odpowiednika i zostaje tylko na zwróconej liście —
/// po to, żeby UI mogło powiedzieć, co dokładnie zniknęło.
#[must_use]
pub fn emit(skill: &Skill) -> (String, Vec<&'static str>) {
    // Kolejność bierze się z SPEC_FIELDS, a nie z kolejności gałęzi w `spec_line`. Dzięki
    // temu przestawienie pól w tablicy przestawia plik, zamiast rozjechać się z nim.
    let front: String = SPEC_FIELDS
        .into_iter()
        .filter_map(|field| spec_line(skill, field))
        .collect();

    // Zdjęte nie znaczy skasowane. Te dwa pola mają przenośny odpowiednik w prozie i wracają
    // PRZED pierwszym akapitem: agent, który przeczyta instrukcję wcześniej niż to, jak go
    // wywołano, wykona ją z niepełną wiedzą [T5 §4.2].
    let mut preamble = String::new();
    if let Some(hint) = skill
        .extras
        .get("argument-hint")
        .filter(|hint| !hint.trim().is_empty())
    {
        preamble.push_str("Arguments: ");
        preamble.push_str(hint);
        preamble.push('\n');
    }
    // Tylko `fork`. Inna wartość `context` znaczy coś, czego nie umiemy przetłumaczyć, a
    // zdanie postawione „na wszelki wypadek" kłamie o tym, jak umiejętność pobiegnie.
    if skill
        .extras
        .get("context")
        .is_some_and(|context| context.trim() == "fork")
    {
        preamble.push_str(FORK_SENTENCE);
        preamble.push('\n');
    }
    if !preamble.is_empty() && !skill.body.is_empty() {
        preamble.push('\n');
    }

    // Zwracamy `&'static str` z tablicy, nie klucze z mapy: lista zdjętych pól ma nazywać
    // czternaście pól, o których wiemy, dlaczego spadły. Pole spoza tej czternastki (import
    // z jutrzejszej wersji vendora) też nie trafia do front-mattera — ale nie umiemy o nim
    // powiedzieć nic ponad „nie ma go w specyfikacji", więc go nie nazywamy.
    let stripped: Vec<&'static str> = NON_SPEC_FIELDS
        .into_iter()
        .filter(|field| skill.extras.contains_key(*field))
        .collect();

    (
        format!("---\n{front}---\n{preamble}{}", skill.body),
        stripped,
    )
}

/// Jeden wiersz (albo blok) front-mattera dla nazwanego pola specyfikacji — albo `None`,
/// kiedy pola nie ma.
///
/// `None` nie jest tym samym, co pusta wartość: `license:` bez niczego za dwukropkiem jest
/// wartością, o którą następny czytelnik musi zapytać, a `metadata:` bez par jest mapą pustą,
/// nie mapą nieobecną.
fn spec_line(skill: &Skill, field: &str) -> Option<String> {
    match field {
        "name" => (!skill.name.is_empty()).then(|| format!("name: {}\n", scalar(&skill.name))),
        "description" => (!skill.description.is_empty())
            .then(|| format!("description: {}\n", scalar(&skill.description))),
        "license" => skill
            .license
            .as_ref()
            .map(|value| format!("license: {}\n", scalar(value))),
        "compatibility" => skill
            .compatibility
            .as_ref()
            .map(|value| format!("compatibility: {}\n", scalar(value))),
        // `BTreeMap`, więc pary wychodzą posortowane po kluczu — ta sama mapa daje ten sam
        // plik, niezależnie od kolejności, w jakiej klucze przyszły z importu.
        "metadata" => (!skill.metadata.is_empty()).then(|| {
            skill
                .metadata
                .iter()
                .fold("metadata:\n".to_owned(), |mut block, (key, value)| {
                    block.push_str("  ");
                    block.push_str(key);
                    block.push_str(": ");
                    block.push_str(&scalar(value));
                    block.push('\n');
                    block
                })
        }),
        "allowed-tools" => skill
            .allowed_tools
            .as_ref()
            .map(|value| format!("allowed-tools: {}\n", scalar(value))),
        // Nieosiągalne, dopóki stoi `const _: () = assert!(SPEC_FIELDS.len() == 6)` u góry
        // pliku: siódma nazwa w tablicy nie skompiluje się, zamiast po cichu tu wpaść.
        _ => None,
    }
}

/// Wartość YAML-a: gołym tekstem, kiedy to bezpieczne, w cudzysłowie, kiedy nie.
///
/// DLACZEGO nie zawsze w cudzysłowie: `SKILL.md` w zakresie projektu ląduje w repo zespołu
/// i człowiek go czyta. DLACZEGO nie zawsze gołym: `description` przychodzi z importu
/// z sieci, a wartość zaczynająca się od `[`, zawierająca `: ` albo wyglądająca jak `true`
/// zmienia typ pola — i pięciu vendorów odmawia wtedy czegoś, czego autor nigdy nie napisał.
fn scalar(value: &str) -> String {
    const INDICATORS: [char; 15] = [
        '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '%',
    ];

    let plain = !value.is_empty()
        && value.trim() == value
        && !value.starts_with(INDICATORS)
        && !value.starts_with(['\'', '"', '@', '`'])
        && !value.contains(": ")
        && !value.contains(" #")
        && !value.ends_with(':')
        && !value.chars().any(char::is_control)
        // Gołe `true`, `null` i `42` wczytują się jako wartość innego typu niż tekst.
        && value.parse::<f64>().is_err()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~"
        );

    if plain {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// Co się wydarzy, jeszcze zanim cokolwiek się wydarzy.
///
/// Waliduje **przed** pierwszym zapisem i nie tworzy ani jednego katalogu — żadnego
/// „utwórzmy, żeby sprawdzić uprawnienia". Dwa powody: użytkownik ma zobaczyć listę zmian,
/// zanim je zatwierdzi, a odmowa w połowie zostawia katalog, którego nikt nie posprząta.
pub fn plan(skill: &Skill, scope: Scope, roots: &Roots) -> Result<InstallPlan> {
    // Odmowa jest PIERWSZĄ rzeczą, jaka się tu dzieje — przed odczytem sidecara i przed
    // policzeniem czegokolwiek, co dotyka dysku.
    let mut messages = validate_strict(&skill.name, &as_doc(skill))
        .err()
        .unwrap_or_default();
    // Ścieżka dołączonego pliku jest ścieżką WZGLĘDNĄ wewnątrz katalogu umiejętności.
    // `dir.join("/etc/x")` zwraca `/etc/x`, a `dir.join("../../x")` wychodzi poza oba drzewa
    // docelowe — czyli plan przestaje opisywać to, co się wydarzy. Źródło waliduje T-19,
    // ale zapis, który wychodzi poza katalog, jest sprawą tego pliku.
    messages.extend(
        skill
            .files
            .iter()
            .filter(|file| !stays_inside(&file.relative))
            .map(|file| {
                format!(
                    "Bundled file '{}' must stay inside the skill folder",
                    file.relative.display()
                )
            }),
    );
    if !messages.is_empty() {
        return Err(Error::Invalid { messages });
    }

    // Zakres projektu bez znanego korzenia to odmowa, nie katalog roboczy: zgadnięty korzeń
    // zapisuje umiejętność w losowym miejscu i nikt się o tym nie dowie.
    if scope == Scope::Project && roots.project.is_none() {
        return Err(Error::NoProjectRoot);
    }

    let sidecar = sidecar_path(&roots.data);
    let ours = recorded(&sidecar);
    let writes: Vec<PathBuf> = destinations(scope, &roots.home, roots.project.as_deref())
        .into_iter()
        .map(|dir| dir.join(&skill.name))
        .collect();

    // `symlink_metadata`, nie `exists()`: katalog będący dowiązaniem ma zostać zauważony jako
    // kolizja, a nie po cichu przejrzany na wylot do tego, na co wskazuje.
    let conflicts = writes
        .iter()
        .filter(|dir| fs::symlink_metadata(dir).is_ok())
        .map(|dir| {
            if ours.contains(dir) {
                Conflict::Update { path: dir.clone() }
            } else {
                Conflict::Foreign {
                    path: dir.clone(),
                    first_line: first_line(dir),
                }
            }
        })
        .collect();

    Ok(InstallPlan {
        writes,
        conflicts,
        sidecar,
    })
}

/// [`Skill`] widziany jako dokument, który za chwilę powstanie.
///
/// Walidujemy to, co **wyjdzie na dysk**, a nie to, co przyszło z importu: pola spoza szóstki
/// zdejmuje [`emit`], więc gdyby trafiły tutaj, każda umiejętność z `argument-hint` byłaby
/// odrzucona za pole, którego w zapisanym pliku i tak nie będzie.
///
/// Pole puste = pole nieobecne. `description: ""` jest dla modelu dokładnie tym samym, co brak
/// `description`: nie ma po czym zdecydować, czy w ogóle sięgnąć po umiejętność.
fn as_doc(skill: &Skill) -> SkillDoc {
    let mut fields = Vec::new();
    let mut put = |key: &str, value: &str| {
        if !value.is_empty() {
            fields.push((key.to_owned(), value.to_owned()));
        }
    };
    put("name", &skill.name);
    put("description", &skill.description);
    put("license", skill.license.as_deref().unwrap_or_default());
    put(
        "compatibility",
        skill.compatibility.as_deref().unwrap_or_default(),
    );
    if !skill.metadata.is_empty() {
        put("metadata", "present");
    }
    put(
        "allowed-tools",
        skill.allowed_tools.as_deref().unwrap_or_default(),
    );

    SkillDoc {
        fields,
        body: skill.body.clone(),
    }
}

/// Czy ścieżka dołączonego pliku zostaje wewnątrz katalogu umiejętności.
///
/// Jedno miejsce dla tej reguły, bo pyta o nią i [`plan`] (żeby odmówić), i [`apply`] (żeby
/// nie zapisać, gdyby ktoś podał mu plan i umiejętność z dwóch różnych wywołań).
fn stays_inside(relative: &Path) -> bool {
    !relative.as_os_str().is_empty()
        && relative.is_relative()
        && !relative
            .components()
            .any(|part| matches!(part, Component::ParentDir))
}

/// Wykonuje plan: `SKILL.md` z [`emit`] do obu katalogów, pliki dołączone przez `fs::copy`,
/// wpis do sidecara.
///
/// Tworzy dokładnie te ścieżki, które plan wymienił, i ani jednej więcej.
pub fn apply(plan: &InstallPlan, skill: &Skill) -> Result<()> {
    // Ta sama reguła co w `plan`, sprawdzona drugi raz, bo `apply` dostaje plan i umiejętność
    // jako dwa osobne argumenty — nic nie gwarantuje, że pochodzą z jednego wywołania.
    let escaping: Vec<String> = skill
        .files
        .iter()
        .filter(|file| !stays_inside(&file.relative))
        .map(|file| {
            format!(
                "Bundled file '{}' must stay inside the skill folder",
                file.relative.display()
            )
        })
        .collect();
    if !escaping.is_empty() {
        return Err(Error::Invalid { messages: escaping });
    }

    // Jeden `emit` na całą instalację, nie jeden na katalog. Dwie ścieżki, jeden plik:
    // drugie brzmienie tej samej umiejętności jest drugą rzeczą do zdebugowania.
    let (doc, _) = emit(skill);

    for dir in &plan.writes {
        fs::create_dir_all(dir)?;
        fs::write(dir.join(SKILL_FILE), &doc)?;

        for file in &skill.files {
            let target = dir.join(&file.relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            // `fs::copy`, nie `write(read(..))`: copy zachowuje uprawnienia, czyli bit
            // wykonywalności `scripts/run.sh`. Zapis samych bajtów gubi go po cichu
            // i skrypt przestaje się dać uruchomić dopiero u użytkownika.
            fs::copy(&file.source, &target)?;
        }
    }

    // Sidecar na końcu: zapis „to napisał Loadout" ma być prawdziwy w chwili, w której
    // powstaje. Postawiony przed kopiowaniem przeżyłby błąd w połowie i uczyniłby naszym
    // katalog, którego nigdy nie dokończyliśmy.
    let mut ours = recorded(&plan.sidecar);
    ours.extend(plan.writes.iter().cloned());
    write_sidecar(&plan.sidecar, &ours)
}

/// Zdejmuje obie kopie umiejętności — i nic poza nimi.
///
/// Kanoniczna kopia w danych aplikacji zostaje: katalogi vendorów są wyjściem builda,
/// a źródło jest jedno (niezmiennik 4). Katalog, którego nie ma w sidecarze, jest cudzy
/// i nie jest kasowany.
pub fn remove(name: &str, scope: Scope, roots: &Roots) -> Result<Removed> {
    if scope == Scope::Project && roots.project.is_none() {
        return Err(Error::NoProjectRoot);
    }

    let sidecar = sidecar_path(&roots.data);
    let ours = recorded(&sidecar);

    // DWA PRZEBIEGI, i to jest cała treść tej funkcji. Najpierw decyzja o OBU katalogach,
    // dopiero potem pierwsze skasowanie: pół usunięcia zostawia stan, którego nikt nie umie
    // opisać, a cudza umiejętność skasowana „przy okazji" jest nie do odzyskania.
    let mut mine = Vec::new();
    for root in destinations(scope, &roots.home, roots.project.as_deref()) {
        let dir = root.join(name);
        // Katalogu nie ma — nie ma czego bronić i nie ma czego kasować. `symlink_metadata`,
        // bo dowiązanie w tym miejscu też jest czymś, co tam stoi.
        if fs::symlink_metadata(&dir).is_err() {
            continue;
        }
        if ours.contains(&dir) {
            mine.push(dir);
        } else {
            // Kolizja nazw jest normalna, nie wyjątkowa: `pdf` to oczywista nazwa i ktoś mógł
            // napisać swoją ręcznie. Sidecar jest jedyną rzeczą, która mówi, która z dwóch
            // jest nasza — a kiedy mówi „nie nasza", nie kasujemy NICZEGO, także drugiej kopii.
            return Ok(Removed::Skipped {
                path: dir,
                why: "Loadout did not write this folder, so removing it would take somebody \
                      else's skill"
                    .to_owned(),
            });
        }
    }

    for dir in &mine {
        fs::remove_dir_all(dir)?;
    }

    // Kanoniczna kopia w danych aplikacji ZOSTAJE (niezmiennik 4): katalogi vendorów są
    // wyjściem builda, a źródło jest jedno. Usunięcie, które kasuje źródło, zamienia
    // „odinstaluj z Codeksa" w „skasuj umiejętność".
    if !mine.is_empty() {
        let mut left = ours;
        left.retain(|path| !mine.contains(path));
        write_sidecar(&sidecar, &left)?;
    }

    Ok(Removed::Done { paths: mine })
}

// ── Sidecar ────────────────────────────────────────────────────────────────────────────────
//
// Jedyny zapis o tym, KTÓRY katalog vendora napisał Loadout. Bez niego „usuń umiejętność"
// nie umie odróżnić naszego `pdf/` od cudzego `pdf/`, a kolizja nazw jest normalna, nie
// wyjątkowa.
//
// DLACZEGO nie plik-znacznik obok `SKILL.md` (niezmiennik 21): znacznik leżałby w katalogu,
// który jest wyjściem builda i który wolno w całości odtworzyć — więc pierwsze `git clean`
// albo pierwsza ręczna kopia katalogu robi z cudzej umiejętności naszą. Sidecar leży
// w danych aplikacji, poza oboma drzewami docelowymi, i czytają go [`plan`] i [`remove`].

/// Katalog umiejętności w danych aplikacji: kanoniczne kopie i ten plik obok nich.
const SKILLS_DIR: &str = "skills";

/// Nazwa sidecara. Jedna, w jednym miejscu — [`plan`] wkłada ścieżkę do [`InstallPlan`],
/// a [`apply`] bierze ją stamtąd, zamiast liczyć drugi raz.
const SIDECAR_FILE: &str = "installed.json";

/// Zawartość sidecara. `#[serde(default)]` plus domyślne ignorowanie nieznanych pól:
/// plik zapisany przez nowszą wersję Loadouta ma się wczytać, a nie wywrócić bieg
/// (niezmiennik 5).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Sidecar {
    /// Katalogi vendorów, które napisaliśmy. Posortowane, żeby `git diff` na tym pliku
    /// pokazywał zmianę, a nie przetasowanie.
    #[serde(default)]
    installed: Vec<String>,
}

fn sidecar_path(data: &Path) -> PathBuf {
    data.join(SKILLS_DIR).join(SIDECAR_FILE)
}

/// Katalogi, o których sidecar mówi „to nasze".
///
/// Brak pliku, plik nieczytelny i plik o nieznanym kształcie dają ten sam wynik: pusty zbiór.
/// To jest wybór w bezpieczną stronę — nie wiedząc, czy katalog jest nasz, traktujemy go jak
/// cudzy, więc najgorsze, co się stanie, to odmowa usunięcia. Odwrotny domyślny („skoro nie
/// wiem, to pewnie moje") kasuje cudzą pracę.
fn recorded(sidecar: &Path) -> BTreeSet<PathBuf> {
    fs::read_to_string(sidecar)
        .ok()
        .and_then(|text| serde_json::from_str::<Sidecar>(&text).ok())
        .map(|record| record.installed.into_iter().map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn write_sidecar(sidecar: &Path, paths: &BTreeSet<PathBuf>) -> Result<()> {
    if let Some(parent) = sidecar.parent() {
        fs::create_dir_all(parent)?;
    }
    let record = Sidecar {
        // `to_string_lossy`, nie `to_str().ok_or(..)`: ścieżka spoza UTF-8 ma nie wywrócić
        // instalacji. Zapis jest wtedy stratny i przy usuwaniu nie dopasuje się do katalogu,
        // czyli znowu wypadamy na „nie wiem, więc nie kasuję".
        installed: paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
    };
    let text = serde_json::to_string_pretty(&record).map_err(std::io::Error::other)?;
    fs::write(sidecar, text + "\n")?;
    Ok(())
}

/// Pierwszy wiersz `SKILL.md` w cudzym katalogu — dosłownie, żeby człowiek zobaczył, czyj to
/// plik, zanim zdecyduje.
///
/// Katalog bez czytelnego `SKILL.md` daje pusty łańcuch. Zdanie wymyślone w zastępstwie
/// byłoby zacytowane użytkownikowi tak, jakby stało w pliku.
fn first_line(dir: &Path) -> String {
    fs::read_to_string(dir.join(SKILL_FILE))
        .ok()
        .and_then(|text| text.lines().next().map(ToOwned::to_owned))
        .unwrap_or_default()
}

/// Werdykt „czy Claude to widzi", odczytany ze zdarzenia `system`/`init`.
///
/// Reguła ma **kolejność** i to jest jej cała treść: jeśli zdarzenie niesie tablicę `skills`,
/// liczy się wyłącznie ona; jeśli nie — liczy się `slash_commands`, bo tak umiejętność
/// z `~/.claude/skills` objawia się w CLI v2.1.233; jeśli nie ma żadnej z nich, odpowiedź
/// brzmi [`Discovery::Unknown`], nigdy „nie widzi".
///
/// DLACZEGO nie `init_line.contains(name)`: nazwa umiejętności potrafi wystąpić w `cwd`
/// (`/home/u/review-pull-requests/x`) i w `mcp_servers[].name`, nie występując w żadnej
/// z dwóch tablic. Wyszukiwanie po całej linii mówi wtedy „widzi" i to jest dokładnie ten
/// fałszywy zielony ptaszek, o który chodzi w tym zadaniu.
///
/// Pusty `init_line` znaczy „CLI nigdy nie wystartowało", czyli `Unknown("not installed")` —
/// brak vendora nie może być czerwony [T5 §6.3].
#[must_use]
pub fn discovery_from_init(name: &str, init_line: &str, wrote: &[PathBuf]) -> Discovery {
    if init_line.trim().is_empty() {
        return Discovery::Unknown("not installed");
    }

    let Ok(event) = serde_json::from_str::<serde_json::Value>(init_line) else {
        return Discovery::Unknown("the answer was not readable");
    };

    // Kolejność JEST regułą. `skills` jest listą autorytatywną; kiedy istnieje, zejście na
    // `slash_commands` zamieniłoby ją w sugestię i umiejętność, której CLI przestało wczytywać,
    // dalej raportowałaby się jako widziana.
    //
    // Nie sprawdzamy `type`/`subtype`: linię wybiera wołający, a vendor, który przemianuje
    // podtyp, kazałby nam odpowiadać „nie wiem" na zdarzenie, które niesie komplet odpowiedzi.
    for key in ["skills", "slash_commands"] {
        let Some(listed) = event.get(key).and_then(serde_json::Value::as_array) else {
            continue;
        };
        return if listed
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|entry| entry == name)
        {
            Discovery::Seen
        } else {
            // Porównanie z ELEMENTAMI tablicy, nigdy `init_line.contains(name)`. Nazwa
            // umiejętności potrafi wystąpić w `cwd` i w nazwie serwera narzędzi, nie będąc
            // w żadnej z dwóch tablic — a wtedy szukanie po całej linii mówi „widzi" i to
            // jest dokładnie ten fałszywy zielony ptaszek, przed którym stoi to zadanie.
            Discovery::NotSeen {
                looked_in: wrote.to_vec(),
            }
        };
    }

    // Brak obu kluczy to brak odpowiedzi, nie odpowiedź odmowna. Vendorzy zmieniają kształt
    // zdarzeń co tydzień i po cichu (niezmiennik 5); „nie ma klucza" przetłumaczone na „nie
    // widzi" daje fałszywy alarm przy pierwszej takiej zmianie.
    Discovery::Unknown("the answer did not list skills")
}
