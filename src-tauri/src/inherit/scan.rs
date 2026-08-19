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

use std::path::Path;

use super::{HostSkill, Result};

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
    let _ = project;
    Ok(Vec::new())
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
    let _ = text;
    Ok(String::new())
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
    &text[..0]
}
