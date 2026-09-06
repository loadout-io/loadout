//! P-03a: czy na tej maszynie DA SIĘ w ogóle obsłużyć natywne okno — i czym się to różni
//! od wady aplikacji.
//!
//! # Co ten moduł jest, a czym świadomie nie jest
//!
//! **Nie jest** systemem Computer Use. Nie klika, nie pisze i nie zna pojęcia scenariusza.
//! Plan P-03 mówi wprost: nie pisz własnego ogólnego sterowania interfejsem jako skutku
//! ubocznego. Droga do okna istnieje i jest zwyczajna — agent QA ma `Bash`, a macOS ma
//! `osascript` i `System Events`, które potrafią wskazać proces po `unix id` i przeczytać
//! jego drzewo kontrolek. Zmierzone 2026-09-06 na tej maszynie: odczyt liczby okien procesu
//! wskazanego PID-em odpowiada normalnie.
//!
//! **Jest** rozpoznaniem możliwości. Zgoda na sterowanie cudzą aplikacją (Automation)
//! i na czytanie jej kontrolek (Accessibility) to **uprawnienia systemu**, przyznawane
//! per binarka i wyłącznie przez człowieka, w oknie, którego żaden model nie otworzy.
//! Bez nich każdy scenariusz natywny kończy się tak samo — niczym — a krok QA, który tego
//! nie odróżnia, melduje wadę produktu za brak własnej zgody.
//!
//! # Trzy różne odpowiedzi
//!
//! Incydent I-06/I-07 (2026-09-06): wymagania dopuszczały mocked-IPC albo samo uruchomienie
//! aplikacji, a QA nie miało żadnego narzędzia do okna i mimo to wydało werdykt. Brak zgody,
//! awaria automatyzacji i defekt aplikacji muszą być trzema różnymi wynikami, bo prowadzą
//! do trzech różnych czynności człowieka.

use std::path::PathBuf;
use std::time::Duration;

use tokio::process::Command;

/// Gdzie stoi interpreter skryptów systemowych na macOS.
const OSASCRIPT: &str = "/usr/bin/osascript";

/// Sufit czasu jednej sondy. Zapytanie o okna nie ma prawa trwać dłużej niż chwilę,
/// a `System Events` bez zgody potrafi czekać na dialog, którego nikt nie kliknie.
const PATIENCE: Duration = Duration::from_secs(10);

/// Czy da się stąd obsłużyć natywne okno.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeUiAccess {
    /// Sterowanie odpowiada. To NIE jest obietnica, że scenariusz przejdzie — tylko tyle,
    /// że jest czym go wykonać.
    Ready,
    /// Zgoda systemu nie została przyznana temu programowi. Wymaga człowieka i okna
    /// systemowego; żaden model tego nie nada.
    NotPermitted {
        /// Zdanie dla człowieka, po angielsku, gotowe na ekran.
        said: String,
    },
    /// Nie ma czym sterować: brak interpretera albo nie ten system.
    Missing { said: String },
    /// Sonda nie odpowiedziała rozpoznawalnie. Nie zgadujemy, co to znaczyło.
    Unknown { said: String },
}

impl NativeUiAccess {
    /// Czy wolno na tej podstawie orzec cokolwiek o produkcie.
    #[must_use]
    pub const fn can_judge_the_product(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Zdanie dla człowieka. Puste, kiedy nie ma czego zgłaszać.
    #[must_use]
    pub fn said(&self) -> &str {
        match self {
            Self::Ready => "",
            Self::NotPermitted { said } | Self::Missing { said } | Self::Unknown { said } => said,
        }
    }
}

/// Zdanie o braku zgody. Jedno miejsce, bo czyta je i widok wyniku, i krok QA.
const NOT_PERMITTED: &str = "This computer has not allowed Loadout to control other \
     applications, so nothing here could open, click or read the application's window. This is a \
     permission on this machine, not a result about the application: allow it under Privacy & \
     Security, then run the check again.";

const NOT_A_MAC: &str = "Driving a native application window is only supported on macOS here, \
     and this computer does not provide it. The scenarios that need a real window were not run.";

/// Sonda możliwości — **tylko odczyt**, żadnego kliknięcia.
///
/// Pyta o liczbę procesów pierwszoplanowych, czyli o najtańszą rzecz, która wymaga dokładnie
/// tych samych dwóch zgód, co każdy scenariusz.
pub async fn can_drive_native_ui() -> NativeUiAccess {
    probe_with(PathBuf::from(OSASCRIPT)).await
}

/// Ta sama sonda z podanym interpreterem — szew, którym testy podstawiają odpowiedzi systemu
/// bez zmieniania uprawnień maszyny, na której biegną.
pub async fn probe_with(program: PathBuf) -> NativeUiAccess {
    let mut command = Command::new(&program);
    command
        .arg("-e")
        .arg("tell application \"System Events\" to count processes")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = match tokio::time::timeout(PATIENCE, command.output()).await {
        Err(_) => {
            return NativeUiAccess::Unknown {
                said: "Asking this computer whether it can drive an application window took too \
                       long, so the scenarios that need a real window were not run."
                    .to_owned(),
            };
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return NativeUiAccess::Missing {
                said: NOT_A_MAC.to_owned(),
            };
        }
        Ok(Err(error)) => {
            return NativeUiAccess::Unknown {
                said: format!(
                    "This computer could not be asked whether it can drive an application \
                     window ({}). The scenarios that need a real window were not run.",
                    error.kind()
                ),
            };
        }
        Ok(Ok(output)) => output,
    };
    if output.status.success() {
        return NativeUiAccess::Ready;
    }
    let complained = String::from_utf8_lossy(&output.stderr);
    if refused_for_permission(&complained) {
        NativeUiAccess::NotPermitted {
            said: NOT_PERMITTED.to_owned(),
        }
    } else {
        NativeUiAccess::Unknown {
            said: "This computer answered something unexpected when asked whether it can drive \
                   an application window, so the scenarios that need a real window were not run."
                .to_owned(),
        }
    }
}

/// Czy skarga systemu mówi o BRAKU ZGODY, a nie o czymkolwiek innym.
///
/// Numery, nie słowa: komunikat `osascript` jest tłumaczony na język systemu — na tej maszynie
/// wychodzi po polsku — więc dopasowanie po angielskiej frazie działałoby wyłącznie u części
/// ludzi i cicho myliłoby brak zgody z awarią u reszty. Numery są stałe niezależnie od języka.
///
/// * `-1743` — „Not authorized to send Apple events", czyli brak zgody Automation.
/// * `-25211` — asystent dostępności odmawia, czyli brak zgody Accessibility.
/// * `-1728` w odpowiedzi na `count processes` znaczy to samo co wyżej: proces bez zgody
///   nie widzi ani jednego procesu, więc nie ma czego policzyć.
fn refused_for_permission(complained: &str) -> bool {
    ["-1743", "-25211", "-10004"]
        .iter()
        .any(|code| complained.contains(code))
}
