//! Jeden odczyt, dwa ujścia: surowe bajty na dysk, zdarzenia do kuratora [T7 §4.2].
//!
//! Kolejność w [`pump`] jest częścią kontraktu, nie stylem: **najpierw tee, potem parsowanie**
//! (`docs/ARCHITECTURE.md` §4). Plik `logs/agent-<id>.jsonl` jest źródłem prawdy — to on
//! pozwala skasować `loadout.db` bez straty (`ARCHITECTURE.md` §2 pyt. 2) i to jego użytkownik
//! wysyła jako dowód. W chwili, w której przestaje być bajtowo tym, co wypluło dziecko,
//! kasowanie indeksu przestaje być bezpieczne.
//!
//! Stąd dwie decyzje, które wyglądają na drobiazgi i nie są:
//!
//! - **`read_until(b'\n')`, nigdy `BufReader::lines()`.** `lines()` zjada `\r` i gubi to, czy
//!   linia w ogóle miała znak końca; po takim przejściu bajtowa identyczność jest nie do
//!   spełnienia, a diff, w którym ktoś „posprząta" pętlę do `lines()`, wygląda niewinnie.
//! - **Bufor leci na dysk bez tknięcia `serde_json`.** Runda przez `serde_json` w obie strony
//!   zamienia `0.14836290000000002` na `0.148362`, rozwija escape `<` i zmienia kolejność
//!   kluczy — a każda z tych trzech zmian jest niewidoczna w porównaniu stringów po `trim()`.
//!
//! Linia niesparsowalna jest w tee tak samo jak każda inna, bo tee dzieje się przed
//! dekodowaniem. Sama pętla nigdy nie kończy biegu na nieznanym zdarzeniu (niezmiennik 5):
//! ścieżka dysku nie gubi nigdy, ścieżka widoku wolno gubić [T7 §4.1].
//!
//! # Stan tego pliku: SZKIELET (2026-08-16)
//!
//! Ciało [`pump`] zwraca **świadomie złą wartość** i jest tak oznaczone komentarzem
//! `SZKIELET`: test ma się skompilować i paść **w czasie wykonania, na braku ZACHOWANIA**
//! (`AGENTS.md` §2a p. 5). `todo!()` tu nie stoi, bo `todo` jest `deny`
//! w `[workspace.lints.clippy]`.
//!
//! Szew wobec vendora (`decode` → `Decoded`, wypełniające `line::Tool`) dokłada implementacja
//! razem z pętlą — dopóki pętli nie ma, szew nie ma czego rozdzielać, a wymyślony na zapas
//! kształt jest dokładnie tym, czego T-10 nie będzie umiało użyć.

use std::path::Path;

use tokio::io::AsyncBufRead;
use tokio::sync::mpsc;

use super::line::Line;

/// Ile strumień miał linii i ile z nich nic nam nie powiedziało.
///
/// Licznik istnieje, bo ma czytelnika (niezmiennik 21): zero przy niepustym biegu znaczy
/// „dekoder połknął śmieci jako nic", a to jest dokładnie ta awaria, której nie widać po
/// niczym innym — bieg kończy się `Ok`, historia jest krótsza, nikt nie pyta dlaczego.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// Ile linii przeszło przez pętlę.
    pub lines: usize,
    /// Ile z nich nie dało się przeczytać jako znane zdarzenie: nie-JSON, nieznany `type`,
    /// znany `type` bez wymaganej treści.
    pub unrecognised: usize,
}

/// Czyta NDJSON linia po linii, kopiuje **bajty** do `tee` przed parsowaniem i wysyła gotowe
/// wiersze na `lines`.
///
/// Kończy się dopiero na końcu wejścia i **nigdy nie kończy biegu na jednej linii**: linia,
/// której nie da się przeczytać, podnosi [`Stats::unrecognised`] i pętla idzie dalej. Cicha
/// wersja złamania to `?` w środku pętli — pierwsze zdarzenie, które vendor doda w przyszłym
/// tygodniu, urywa bieg w połowie i wygląda to jak awaria agenta, nie jak nasz parser
/// (niezmiennik 5).
pub async fn pump<R>(
    _reader: R,
    tee: &Path,
    agent: &str,
    _lines: mpsc::Sender<Line>,
) -> anyhow::Result<Stats>
where
    R: AsyncBufRead + Unpin + Send,
{
    // SZKIELET (2026-08-16): pętla, tee i wołanie kuratora są całą treścią AC-1, AC-5 i AC-6,
    // więc tutaj ich nie ma. Wejście jest porzucane bez czytania, nadajnik ginie na końcu tej
    // funkcji bez wysłania ani jednego wiersza, a plik tee nie powstaje. Podkreślenie w nazwie
    // znika razem ze szkieletem — sygnatura jest już ta docelowa.
    tracing::debug!(
        agent,
        tee = %tee.display(),
        "SZKIELET: the stream was not read, nothing was teed and no line was sent"
    );
    // Jedyne `await` w szkielecie. Sygnatura ma być TA, którą wypełni implementacja — test
    // skompilowany dziś przeciwko innej jutro nie skompiluje się wcale — a `async fn` bez
    // `await` przewraca `clippy::unused_async` w pełnej bramce.
    tokio::task::yield_now().await;
    Ok(Stats::default())
}
