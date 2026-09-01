//! Strona aplikacji: gniazdo, powitanie i rozdzielnik wywołań.
//!
//! # Jedno gniazdo na PROCES AGENTA, nie jedno na aplikację
//!
//! Tożsamością wołającego jest **samo gniazdo**. Nie ma więc tokenów do porównywania ani sposobu,
//! żeby jedna sesja odpowiedziała na wywołanie drugiej — a przy narzędziu, które blokuje turę,
//! taka zamiana byłaby odpowiedzią na cudze pytanie.
//!
//! # Gniazdo jest przepustką
//!
//! Plik ma prawa `0600` i leży w katalogu, który Loadout zakłada dla tej jednej sesji. Ścieżka
//! **nie jest sekretem** i wolno jej stać w argv mostu — niezmiennik 9 dotyczy promptu i sekretów,
//! a zdolnością jest tu prawo do pliku, nie znajomość ścieżki.
//!
//! Ograniczenie, które trzeba znać: proces tego samego użytkownika może się podłączyć. Nie jest to
//! nowa dziura — taki proces równie dobrze czyta `~/.loadout` — ale jest granicą tego mechanizmu
//! i nie wolno na nim budować niczego mocniejszego.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinHandle;

use super::{Answer, Call, Greeting, Reply, Role, verbs};
use crate::connections::{Connection, Origin, Transport};

/// Ile znaków niesie nazwa gniazda po `loadout-`. Krótka **z pomiaru, nie z gustu**: adres
/// gniazda uniksowego mieści się na macOS w ~104 bajtach RAZEM ze ścieżką katalogu, a ścieżka
/// katalogu tymczasowego tego systemu ma sama w sobie około pięćdziesięciu. Pełny identyfikator
/// (32 znaki) zostawiałby kilka bajtów zapasu i przewracał się na maszynie z dłuższym `TMPDIR` —
/// z błędem, który nie mówi o długości ani słowa.
const NAME_LENGTH: usize = 16;

/// Flaga, którą binarka Loadouta przechodzi w tryb mostu. Stoi tu, a nie w `main.rs`, bo to jest
/// FAKT O ARGV i musi dać się przeczytać razem z tym, kto go tam wstawia.
pub const FLAG: &str = "--bridge";

/// Kto umie odpowiedzieć na wywołanie czasownika.
///
/// Trait, a nie konkretny typ, i to nie jest trait wymyślony: druga implementacja jest w tym
/// samym commicie i jest nią dubler w kryteriach. Bez niej gniazda nie da się osądzić bez
/// biblioteki człowieka na dysku.
#[async_trait]
pub trait Answers: Send + Sync {
    /// Odpowiedź na jedno wywołanie. **Nigdy `Err`**: odmowa jest wartością i niesie gotowe
    /// zdanie dla człowieka (`Answer::Refused`).
    async fn answer(&self, call: Call) -> Answer;
}

/// Żywe gniazdo jednej sesji.
///
/// Porzucenie tej wartości zamyka nasłuch i kasuje plik gniazda — czyli sesja, której most już
/// nie ma, nie zostawia po sobie pliku, po którym ktoś mógłby się podłączyć.
#[derive(Debug)]
pub struct Bridge {
    /// Gdzie leży gniazdo. Ta sama ścieżka, która stoi w argv mostu.
    at: PathBuf,
    /// Zadanie przyjmujące połączenia. Kończy się razem z porzuceniem gniazda.
    accepting: JoinHandle<()>,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.accepting.abort();
        /* Plik gniazda znika razem z sesją. Zostawiony, byłby ścieżką, pod którą nikt już nie
         * słucha — a most, który się pod nią podłączy, czeka na powitanie, które nie przyjdzie.
         * Błąd kasowania porzucamy: katalog sesji bywa już sprzątnięty przez warstwę wyżej. */
        let _ = std::fs::remove_file(&self.at);
    }
}

impl Bridge {
    /// Zakłada gniazdo w katalogu tej sesji i zaczyna przyjmować połączenia.
    ///
    /// `role` rozstrzyga, co most dostanie w powitaniu — czyli **całą** powierzchnię tej sesji.
    /// Liczone tutaj, po stronie, która zna człowieka, i nigdy przez most.
    pub async fn open(directory: &Path, role: Role, answers: Arc<dyn Answers>) -> io::Result<Self> {
        tokio::fs::create_dir_all(directory).await?;

        /* NAZWA WŁASNA NA SESJĘ, nie stała `bridge.sock`. Dwa powody, oba realne:
         * dwie rozmowy w jednym projekcie to zwykły stan od T-71, a stała nazwa dawałaby im jedno
         * gniazdo; i gniazdo po sesji, która nie zdążyła posprzątać, przewracałoby `bind` na
         * `EADDRINUSE` u następnej. Identyfikator jest tu tańszy niż obie te naprawy. */
        let name = uuid::Uuid::now_v7().simple().to_string();
        let at = directory.join(format!("loadout-{}.sock", &name[..NAME_LENGTH]));

        let listener = UnixListener::bind(&at)?;
        /* PRAWO DO PLIKU JEST ZDOLNOŚCIĄ: kto otworzy to gniazdo, ten dostaje czasowniki tej
         * sesji. Prymityw mieszka w supervisorze, bo niezmiennik 3 daje kodowi zależnemu od
         * systemu jeden dom — gałąź warunkowa po systemie w tym pliku przewraca bramkę, i to
         * dotyczy także napisania jej nazwy w komentarzu. */
        crate::engine::supervisor::owner_only(&at)?;

        let tools = verbs::tool_list(role);
        let accepting = tokio::spawn(accept_forever(listener, tools, answers));
        Ok(Self { at, accepting })
    }

    /// Ścieżka gniazda — ta sama, którą dostaje most w argv.
    #[must_use]
    pub fn at(&self) -> &Path {
        &self.at
    }

    /// Most jako **połączenie**, czyli w kształcie, który sterowniki już umieją przyjąć.
    ///
    /// # Dlaczego akurat tak, a nie osobnym szwem w sterowniku
    ///
    /// Bo `connections::runtime::for_driver` pisze konfigurację obu vendorów i wypełnia
    /// `DriverConfiguration::servers`, a z tego pola `mcp__<serwer>` trafia do `--allowedTools`
    /// (`drivers/claude.rs`). Most podany jako kolejne połączenie dostaje więc całą tę drogę
    /// **bez zmiany ani jednej linii w sterownikach** — a to jest ta sama droga, którą zmierzyłem
    /// jako działającą 2026-08-29.
    ///
    /// # Ścieżka bezwzględna, nigdy nazwa
    ///
    /// `current_exe()`, bo `claude` startuje most **własnym** środowiskiem, a `PATH` dziecka to
    /// cztery zapieczętowane katalogi systemowe (`engine/supervisor.rs`). Nazwa binarki nie
    /// rozwiązałaby się tam nigdy, a most, który nie wstaje, wygląda jak lider bez narzędzi.
    pub fn as_connection(&self) -> io::Result<Connection> {
        let binary = std::env::current_exe()?;
        Ok(Connection {
            id: serve_name().to_owned(),
            name: serve_name().to_owned(),
            /* WŁĄCZONE Z DEFINICJI, i to nie omija zgody człowieka. Połączenia z biblioteki
             * startują wyłączone, bo są cudzymi serwerami, o których człowiek ma zdecydować
             * (`Connection::imported`). Ten jest Loadoutem rozmawiającym sam ze sobą — zgoda
             * na niego jest tą samą zgodą, którą człowiek wyraził, wskazując lidera. */
            enabled: true,
            transport: Transport::Stdio {
                command: binary.display().to_string(),
                args: vec![FLAG.to_owned(), self.at.display().to_string()],
                environment: Vec::new(),
            },
            /* Źródłem jest gniazdo, bo nie ma pliku, z którego to połączenie by pochodziło —
             * a ścieżka jest tu jedyną wartością, która cokolwiek znaczy w dzienniku. */
            source: self.at.clone(),
            /* Odcisku nie ma czego liczyć: to połączenie nie pochodzi z pliku, więc pusty napis
             * jest tu ODPOWIEDZIĄ, nie brakiem. Wartość zmyślona (choćby odcisk ścieżki) mówiłaby,
             * że jest co porównać przy następnym skanie — a nie ma. */
            source_hash: String::new(),
            origin: Origin::Project,
        })
    }
}

/// Nazwa serwera — jedno miejsce, wspólne z [`super::serve::SERVER`].
fn serve_name() -> &'static str {
    super::serve::SERVER
}

/// Przyjmuje połączenia, dopóki gniazdo żyje. Jedno połączenie to jedna sesja mostu.
async fn accept_forever(listener: UnixListener, tools: Value, answers: Arc<dyn Answers>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            // Gniazdo padło albo zostało zamknięte. Nie ma czego naprawiać z tej strony.
            return;
        };
        let tools = tools.clone();
        let answers = Arc::clone(&answers);
        tokio::spawn(async move {
            if let Err(error) = talk(stream, tools, answers).await {
                tracing::debug!(%error, "a bridge connection ended");
            }
        });
    }
}

/// Powitanie, potem pary wywołanie–odpowiedź, aż most zejdzie.
async fn talk(stream: UnixStream, tools: Value, answers: Arc<dyn Answers>) -> anyhow::Result<()> {
    let (reading, mut writing) = stream.into_split();
    let mut reading = BufReader::new(reading);

    /* APLIKACJA ODZYWA SIĘ PIERWSZA. Powód w całości stoi przy `Greeting`: most, który liczyłby
     * własną listę, mógłby sam sobie nadać uprawnienia. */
    let greeting = Greeting { tools };
    write_line(&mut writing, &serde_json::to_value(&greeting)?).await?;

    let mut said = String::new();
    loop {
        said.clear();
        if reading.read_line(&mut said).await? == 0 {
            return Ok(());
        }
        let Ok(call) = serde_json::from_str::<Call>(said.trim()) else {
            // Nieczytelna linia jest porzucana, nigdy nie wywala nasłuchu (niezmiennik 5).
            continue;
        };
        let id = call.id.clone();
        let answer = answers.answer(call).await;
        /* JEDEN TYP NA CAŁĄ LINIĘ, a nie doklejanie klucza do zserializowanego enuma. Powód
         * w całości stoi przy `Reply`: enum z zewnętrznym tagiem jest obiektem o dokładnie
         * jednym kluczu, więc dopisane obok `id` czyniło tę linię nieczytelną dla mostu —
         * i było to widać dopiero na żywym vendorze. */
        write_line(&mut writing, &serde_json::to_value(&Reply { id, answer })?).await?;
    }
}

/// Jedna wiadomość, jedna linia — ten sam kształt, co po stronie mostu.
async fn write_line<W>(sink: &mut W, value: &Value) -> anyhow::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    sink.write_all(&bytes).await?;
    sink.flush().await?;
    Ok(())
}
