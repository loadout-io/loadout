//! Czytanie gospodarza. **Zero zapisu** — ani jednej ścieżki, pod którą ten plik coś tworzy.
//!
//! Trzy funkcje, wszystkie czyste: nad katalogiem podanym argumentem albo nad tekstem. Żadna
//! nie zna pojęcia „bieżący katalog", bo katalog gospodarza wybiera człowiek w interfejsie,
//! a nie miejsce, z którego przypadkiem wystartował proces.
//!
//! Reguła „co jest tekstem, a co maszynerią" mieszka **tutaj i tylko tutaj** (niezmiennik 23).
//! Druga lista pól do zdjęcia, dopisana przy zapisie, byłaby drugim znaczeniem tego samego
//! słowa — dokładnie tak umarło skanowanie sekretów w repo, z którego bierzemy tę lekcję.
//!
//! Podział na front-matter i ciało jest **lustrem** `skills::ingest::parse_doc`, przepisanym tu
//! świadomie: `parse_doc` jest prywatny, a `ingest.rs` nie należy do tego zadania. Reguła
//! brzmi tak samo w obu miejscach — front-matter bez domknięcia **nie jest** front-matterem,
//! `---` w pierwszej linii pliku, który nigdy się nie domyka, to pozioma kreska.
//!
//! CZEGO TEN PLIK NIE ROBI, a co należy do jego wołającego: tekst gospodarza jest nieufany
//! dokładnie tak samo jak umiejętność pobrana z sieci — to cudze repozytorium, w którym nikt nie
//! audytował komentarzy HTML — więc **ciało, które jedzie do promptu, przechodzi przez
//! [`ingest::review`]**, i dotyczy to obu wyjść tego pliku: sekcji z learnings i ciała podagenta.
//! Nie robimy tego tutaj, bo obie te funkcje zwracają **wycinek tego, co przyszło**, a `review`
//! zwraca tekst zmieniony razem ze znaleziskami; znaleziska są faktem o imporcie, który ma
//! dojechać do człowieka, a nie zostać po drodze porzucony (niezmiennik 5). Miejsce, w którym
//! prompt się składa, nie należy do tego zadania — patrz `tasks/T-54.md`, „Świadomie poza
//! zakresem".

use std::fs;
use std::io::{self, BufRead as _, BufReader, Read as _};
use std::path::{Component, Path, PathBuf};

use super::{HostSkill, Lendable, Result};
use crate::skills::ingest;

/// Nazwa pliku, po której poznaje się umiejętność w cudzym repozytorium.
const SKILL_FILE: &str = "SKILL.md";

/// Fraza, od której zaczyna się wiersz nagłówka sekcji z regułami.
///
/// To PRZEDROSTEK wiersza, nie cały wiersz i nie napis szukany w pliku. Obie te pomyłki są
/// zmierzone i obie kończą się cicho — patrz [`recurring_patterns`].
const PATTERNS_HEADING: &str = "## Recurring patterns";

/// Początek każdego wiersza, który kończy sekcję: nagłówek tego samego poziomu.
const HEADING_MARK: &str = "## ";

/// Katalog, w którym cudze repozytorium trzyma to, co da się z niego pożyczyć.
///
/// Ten napis stoi w tym repo RAZ. Do 2026-08-23 miał dwie kopie — tę i drugą w `wire.rs` —
/// i przez cały ten czas obie odpowiadały tak samo, więc rozjazd nie miał jak się pokazać;
/// dzień, w którym jedna z nich by się zmieniła, byłby dniem, w którym ekran wyboru pokazuje
/// półkę, z której bieg nie czyta (niezmiennik 23).
pub(super) const HOST_DIR: &str = ".claude";

/// Półka z umiejętnościami: `<projekt>/.claude/skills/<nazwa>/SKILL.md`.
pub(super) const SKILLS_DIR: &str = "skills";

/// Półka z plikami ról: `<projekt>/.claude/learnings/<rola>.md`.
pub(super) const LEARNINGS_DIR: &str = "learnings";

/// Półka z podagentami: `<projekt>/.claude/agents/<rola>.md`.
pub(super) const SUBAGENTS_DIR: &str = "agents";

/// `<projekt>/.claude/<półka>` — kształt gospodarza, składany **w jednym miejscu**.
///
/// Ścieżkę do konkretnego pliku wydaje [`skill_file`], a nie składa jej u siebie ten, kto jej
/// potrzebuje: drugi taki `join` gdziekolwiek indziej byłby drugą definicją tego samego pojęcia
/// (niezmiennik 23).
pub(super) fn shelf(project: &Path, name: &str) -> PathBuf {
    project.join(HOST_DIR).join(name)
}

fn skills_root(project: &Path) -> PathBuf {
    shelf(project, SKILLS_DIR)
}

/// `SKILL.md` wybranej umiejętności u gospodarza — albo `None`, gdy `name` nie jest nazwą
/// **jednego** katalogu.
///
/// DLACZEGO ten warunek stoi tutaj, a nie po stronie zapisu: `name` przychodzi z ekranu wyboru,
/// czyli z drutu, a wyznacza jednocześnie ścieżkę czytaną u gospodarza i podkatalog zapisywany
/// u nas. `..`, `/etc` albo `a/b` w tym polu znaczyłyby odczyt i zapis poza dwoma katalogami,
/// których dotyczy ta operacja. Pytanie zadajemy raz, w miejscu, w którym mieszka kształt
/// gospodarza — druga taka lista przy zapisie byłaby drugą definicją tej samej reguły
/// (niezmiennik 23).
pub(crate) fn skill_file(project: &Path, name: &str) -> Option<PathBuf> {
    let mut parts = Path::new(name).components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(folder)), None) => {
            Some(skills_root(project).join(folder).join(SKILL_FILE))
        }
        _ => None,
    }
}

/// Pierwszy wiersz pliku, dosłownie — albo `None`, jeśli pliku nie da się przeczytać.
///
/// Czytamy **wiersz**, nie plik: `read_until` staje na pierwszym `\n`, więc 73 KB learnings ani
/// megabajtowy `SKILL.md` nie wchodzą do pamięci po jedno zdanie. Sufit jest ten sam, co na
/// ścieżce importu ([`ingest::FILE_CAP`]), bo „ile wolno przeczytać z cudzego `SKILL.md`" jest
/// jedną decyzją, nie dwiema (niezmiennik 23).
///
/// Bajty spoza UTF-8 nie kasują wpisu: plik JEST tam, więc umiejętność jest tam. Pominięcie
/// dałoby człowiekowi listę, na której jej nie ma — a to jest ta sama cicha porażka, przed którą
/// stoi całe to zadanie, tylko od drugiej strony.
fn first_line(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut raw = Vec::new();
    BufReader::new(file.take(ingest::FILE_CAP))
        .read_until(b'\n', &mut raw)
        .ok()?;
    Some(
        String::from_utf8_lossy(&raw)
            .trim_end_matches(['\n', '\r'])
            .to_owned(),
    )
}

/// Umiejętności gospodarza z `<projekt>/.claude/skills/**`: nazwa katalogu i pierwszy wiersz
/// jego `SKILL.md`.
///
/// Wynik jest **posortowany po nazwie**. Kolejność z systemu plików nie jest ustalona, a tę
/// listę czyta człowiek na ekranie wyboru — lista, która przestawia się przy każdym otwarciu,
/// jest listą, w której nie da się niczego znaleźć dwa razy. Ta sama decyzja, z tego samego
/// powodu, stoi w `skills::ingest::bundled_files`.
///
/// Katalog **bez** `SKILL.md` nie ma wpisu i nie jest błędem: u gospodarza taki katalog zostaje
/// po ręcznym usunięciu pliku i po nieudanym `git checkout`. Repozytorium **bez** katalogu
/// `.claude/skills` daje pustą listę i `Ok` — to jest większość repozytoriów, nie awaria
/// (niezmiennik 5). Cicho łamie się to przez `?`, który zamienia „ten host nie ma
/// umiejętności" w odmowę startu biegu.
pub fn skills(project: &Path) -> Result<Vec<HostSkill>> {
    let listing = match fs::read_dir(skills_root(project)) {
        Ok(listing) => listing,
        // Rozstrzyga RODZAJ błędu, nie sam fakt porażki. Brak katalogu to większość
        // repozytoriów; nieczytelny katalog to stan, o którym człowiek ma się dowiedzieć, bo
        // umiejętności tam są i nie widać ich na ekranie wyboru.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    let mut found = Vec::new();
    for entry in listing {
        let entry = entry?;
        // `file_type()` z `read_dir` NIE idzie za dowiązaniem — i o to chodzi. Dowiązanie
        // prowadzi poza wybrane repozytorium, a wtedy zarówno wiersz na ekranie, jak i bajty
        // w katalogu pluginu pochodzą z pliku, którego człowiek nie wybierał. Ta sama decyzja,
        // z tego samego powodu, stoi w `ingest::walk_into`.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            // `README.md` leżący obok katalogów nie jest umiejętnością i nie jest błędem.
            continue;
        }
        // Nazwa katalogu jest zarazem kluczem wyboru i nazwą, pod którą plik ląduje w katalogu
        // pluginu. Nazwa nie-UTF-8 nie wróciłaby z drutu tą samą ścieżką, więc wpisu nie ma.
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        // Katalog bez `SKILL.md` nie ma wpisu i nie jest błędem — u gospodarza zostaje po
        // ręcznym usunięciu pliku i po nieudanym `git checkout`.
        let Some(first_line) = first_line(&entry.path().join(SKILL_FILE)) else {
            continue;
        };
        found.push(HostSkill { name, first_line });
    }

    found.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(found)
}

/// Sekcja `## Recurring patterns` z pliku learnings — od nagłówka do następnego `## `.
///
/// NAGŁÓWEK ROZPOZNAJESZ JAKO NAGŁÓWEK, nie jako napis w pliku. Zmierzone u gospodarza
/// 2026-08-19: każdy z dziewięciu plików ról niesie w trzeciej linii cytat blokowy zawierający
/// dosłownie `` `## Recurring patterns` ``, więc naiwne `text.find("## Recurring patterns")`
/// trafia w ten cytat, a nie w nagłówek — na `backend-dev.md` daje **131 bajtów** zdania o tym,
/// że reguły są wiążące, zamiast **1701 bajtów** reguł. Prompt jest wtedy dłuższy, agent nie
/// dostaje żadnej reguły i nikt tego nie widzi, bo pole „lekcje" jest niepuste.
///
/// I druga strona tej samej pułapki: nagłówek niesie przyrostek
/// (`## Recurring patterns (BINDING — do NOT repeat)`), a nagłówka **równego** dosłownie
/// `## Recurring patterns` nie ma w żadnym z dziesięciu plików gospodarza.
///
/// BUDŻET, czyli po co to w ogóle jest [zmierzone u gospodarza 2026-08-19]: `backend-dev.md`
/// to **1701 z 32922 bajtów (5,2%)**, `orchestrator.md` **2016 z 73258 bajtów (2,8%)**. Reszta
/// pliku, do 73 KB `## Run journal`, nigdy nie wchodzi do budżetu tokenów — i to jest cała
/// różnica między wstrzykiwaczem a wklejeniem pliku.
///
/// Plik **bez** tej sekcji daje pusty wynik i `Ok`. Typ `Result` stoi tu po to, żeby ta
/// obietnica była zapisana w sygnaturze, a nie tylko w prozie: brak sekcji jest normalnym
/// stanem cudzego repozytorium (niezmiennik 5).
pub fn recurring_patterns(text: &str) -> Result<String> {
    let mut offset = 0usize;
    let mut section: Option<usize> = None;
    let mut end = text.len();

    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if section.is_some() {
            if content.starts_with(HEADING_MARK) {
                // Nagłówek jest GRANICĄ, nie treścią: cięcie stoi na jego pierwszym znaku,
                // więc `## Run journal` zostaje po tamtej stronie razem z całym journalem.
                // `## `, ze spacją: `### Cokolwiek` jest podnagłówkiem WEWNĄTRZ sekcji.
                end = offset;
                break;
            }
        } else if content.starts_with(PATTERNS_HEADING) {
            // NAGŁÓWEK ROZPOZNAJEMY JAKO WIERSZ, i to jest cała różnica wobec
            // `text.find("## Recurring patterns")`. Trzecia linia każdego pliku roli
            // u gospodarza niesie cytat blokowy z tą samą frazą w odwrotnych apostrofach:
            // szukanie po całym tekście trafia w cytat trzy wiersze wyżej i zwraca 131 bajtów
            // zdania o tym, że reguły są wiążące, zamiast 1701 bajtów reguł.
            //
            // `starts_with`, nie `==`: prawdziwy nagłówek niesie przyrostek
            // (`## Recurring patterns (BINDING — do NOT repeat)`), a nagłówka równego dosłownie
            // samej frazie nie ma w żadnym z dziesięciu plików gospodarza [2026-08-19].
            section = Some(offset + line.len());
        }
        offset += line.len();
    }

    // Sekcja obecna i pusta ma być nieodróżnialna od nieobecnej, dlatego końce obcinamy: pole
    // „lekcje", które jest niepuste i nie niesie ani jednej reguły, to dokładnie ta cicha
    // porażka, przed którą stoi ta funkcja.
    Ok(section.map_or_else(String::new, |from| text[from..end].trim().to_owned()))
}

/// Ciało podagenta gospodarza — wszystko za drugim `---`. **Cały** front-matter zostaje po
/// jego stronie granicy.
///
/// FRONT-MATTER JEST GRANICĄ MASZYNERII, a nie brudem do posprzątania. `.claude/agents/`
/// gospodarza niesie w nagłówku `mcpServers: playwright: command: npx, args: ["-y",
/// "@playwright/mcp@0.0.75"]` [zmierzone 2026-08-19, trzy pliki na trzynaście]. Jedno pole
/// YAML-a, a znaczy „uruchom `npx` i pobierz z sieci paczkę": proces startuje **poza grupą
/// procesów Loadouta**, więc nie wchodzi ani do dowodu śmierci grupy (niezmiennik 6), ani do
/// żadnego licznika kosztu. `tools` i `permissionMode` przepisują politykę biegu z miejsca,
/// którego nasze UI nie pokazuje; `memory` wskazuje cudzy katalog pamięci; `model` cicho
/// zmienia rachunek (niezmiennik 9).
///
/// DLATEGO WYCINAMY BLOK, A NIE FILTRUJEMY PÓL. Czarna lista pól jest z definicji niekompletna
/// i cicho pęknie przy następnym wydaniu CLI — a filtr, który zdejmuje sam wiersz `mcpServers:`,
/// zostawia w wyniku jego wcięte dzieci, czyli dokładnie te dwie wartości, które uruchamiają
/// proces.
///
/// Plik **bez** front-mattera zwraca całe swoje ciało nietknięte, a `---` w pierwszej linii
/// pliku, który nigdy się nie domyka, zostaje w wyniku razem z tą kreską — to jest lustro
/// reguły `skills::ingest::parse_doc`.
#[must_use]
pub fn agent_body(text: &str) -> &str {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        // Plik bez nagłówka jest samym ciałem. Nie ma tu nic do zdejmowania i nie jest to stan
        // wyjątkowy: podagent bez front-mattera to normalny plik cudzego repozytorium.
        return text;
    };

    let mut consumed = 0usize;
    for line in rest.split_inclusive('\n') {
        consumed += line.len();
        if line.trim_end_matches(['\n', '\r']).trim_end() == "---" {
            // WYCINAMY CAŁY BLOK, nie filtrujemy z niego znanych pól. Czarna lista jest
            // z definicji niekompletna i pęknie po cichu przy następnym wydaniu CLI — a filtr,
            // który zdejmuje sam wiersz `mcpServers:`, zostawia jego wcięte dzieci
            // (`command: npx`, `args: ["-y", "@playwright/mcp@0.0.75"]`), czyli dokładnie te
            // dwie wartości, które startują proces poza naszą grupą procesów.
            return &rest[consumed..];
        }
    }

    // Front-matter bez domknięcia NIE JEST front-matterem: `---` w pierwszej linii pliku, który
    // nigdy się nie domyka, to pozioma kreska. Cięcie na niej zjadłoby pierwszy akapit
    // podagenta bez jednego słowa. Lustro reguły `skills::ingest::parse_doc`.
    text
}

/// Co ten folder ma do pożyczenia — trzy półki naraz, same nazwy, zero zapisu.
///
/// # Dlaczego to jest jedna funkcja, a nie trzy wołania z okna
///
/// Ekran wyboru zadaje JEDNO pytanie („co da się stąd wziąć") i ma dostać jedną odpowiedź.
/// Trzy komendy znaczyłyby trzy stany ładowania, trzy sposoby, na które wiersz bywa w połowie
/// wypełniony, i trzy okazje, żeby pokazać człowiekowi półkę z poprzedniego folderu.
///
/// **Nazwy, ani jednego bajtu treści.** Cudze repozytorium to tekst, którego nikt nie
/// audytował; do okna wchodzi więc lista nazw, a treść czyta dopiero bieg — i wyłącznie tę,
/// którą człowiek zaznaczył ([`super::wire::from_the_host`]).
///
/// **Brak półki to pusta lista i `Ok`, nie błąd** (niezmiennik 5): większość folderów nie ma
/// `.claude/` w ogóle, a wiersz wyboru, który przy takim folderze odmawia, jest nie do użycia
/// w każdym repozytorium, które nigdy nie widziało Claude Code. Katalog NIECZYTELNY to co
/// innego i jedzie jako błąd — pozycje tam są, a na ekranie ich nie widać.
pub fn what_this_project_can_lend(project: &Path) -> Result<Lendable> {
    Ok(Lendable {
        // Pytamy `skills`, a nie systemu plików: to ta funkcja wyznacza, co jest umiejętnością
        // (katalog bez `SKILL.md` nie ma wpisu), i ta sama odpowiedź musi rozstrzygać przy
        // odmowie w `wire::nothing_is_missing`. Drugi warunek tutaj byłby drugą definicją
        // słowa „znalezione" (niezmiennik 23).
        skills: skills(project)?.into_iter().map(|one| one.name).collect(),
        learnings: markdown_names(project, LEARNINGS_DIR)?,
        subagents: markdown_names(project, SUBAGENTS_DIR)?,
    })
}

/// Nazwy plików `*.md` leżących wprost na jednej półce, bez rozszerzenia, posortowane.
///
/// POSORTOWANE, bo kolejność z systemu plików nie jest ustalona, a tę listę czyta człowiek:
/// lista, która przestawia się przy każdym otwarciu, jest listą, w której nie da się niczego
/// znaleźć dwa razy. Ta sama decyzja, z tego samego powodu, stoi w [`skills`].
///
/// WPROST NA PÓŁCE, bez schodzenia w podkatalogi: nazwa z tej listy wraca do nas polem `borrow`
/// i wyznacza czytaną ścieżkę, a `wire::one_file_named` przyjmuje wyłącznie nazwę **jednego**
/// segmentu. Pozycja, której nie dałoby się potem pożyczyć, jest na ekranie wyboru obietnicą
/// bez pokrycia (niezmiennik 16).
///
/// Dowiązania odpadają razem z resztą: `file_type()` z `read_dir` nie idzie za dowiązaniem,
/// a dowiązanie prowadzi poza wybrane repozytorium — czyli do pliku, którego człowiek nie
/// wybierał. Ta sama decyzja, z tego samego powodu, stoi w [`skills`].
fn markdown_names(project: &Path, shelf_name: &str) -> Result<Vec<String>> {
    let listing = match fs::read_dir(shelf(project, shelf_name)) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    let mut found = Vec::new();
    for entry in listing {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().is_none_or(|kind| kind != "md") {
            continue;
        }
        // `file_stem`, nie ręczne obcinanie: `borrow.learnings` niesie nazwę BEZ rozszerzenia,
        // a `wire::host_text` dokłada `.md` z powrotem. Nazwa obcięta tu inaczej niż tam
        // wskazywałaby plik, którego człowiek nie zaznaczył — bez jednego słowa o podmianie.
        if let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) {
            found.push(name.to_owned());
        }
    }
    found.sort();
    Ok(found)
}
