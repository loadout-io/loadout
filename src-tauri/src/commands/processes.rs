//! Rejestr rzeczy, które Loadout uruchomił dla człowieka — i których jest właścicielem.
//!
//! # Po co to w ogóle istnieje
//!
//! Zgłoszenie właściciela 2026-08-20: „jak napiszę aby coś odpalił jakąś apkę to chcę mieć też
//! po prawej gdzie są agenci info o procesach odpalonych itp, i po kliku mogę tam wejść".
//!
//! Do tego dnia nie było czego pokazać, i nie było to kwestią brakującego ekranu. Aplikacja
//! odpalona przez agenta stoi w JEGO grupie procesów; Loadout widzi po niej wyłącznie
//! `Line::Ran` — wiersz o czynności **zakończonej** (`engine::line`, pole `ok`). Nośnika na
//! „to biegnie teraz" nie było wcale. Kafelek zbudowany z wiersza `ran` byłby relacją, której
//! w danych nie ma (niezmiennik 17), a przycisk „stop" pod nim nie miałby czego ubić: tej grupy
//! nie założyliśmy, więc nie mamy jak dowieść jej śmierci (niezmiennik 6).
//!
//! Dlatego rzecz zamawia się komendą, a właścicielem jest Loadout: startuje ją
//! [`crate::engine::drivers::command::CommandDriver::start_to_stay`], czyli ta sama droga, którą
//! idzie każdy inny proces tego produktu — `process_group(0)`, `env_clear()` plus jawna lista,
//! potoki czytane do EOF. Nie jest to PTY i nie udaje terminala (decyzja D4 zostaje w mocy).
//!
//! # Cicha porażka, przed którą stoi ten plik
//!
//! Kafelek, który zostaje po rzeczy, która zeszła. „Running" przy komendzie zeszłej dwie minuty
//! temu jest tym samym kłamstwem, co widmowy agent z T-66 — a tamta fala pokazała, że ta klasa
//! wady wraca powierzchnia po powierzchni. Stąd [`StartedProcess::alive`] jest polem, a nie
//! założeniem: rejestr mówi, co wie, a kafelka nie rysuje wcale temu, kto zszedł
//! (`src/sections/run/rail/processes.ts`).
//!
//! # Dlaczego w `commands/`, a nie w `engine/`
//!
//! Bo to jest stan JEDNEJ aplikacji, trzymany między wywołaniami komend — dokładnie ta rola, dla
//! której `ipc::AppState` w ogóle istnieje. Silnik nie ma prawa o oknie wiedzieć (niezmiennik 1),
//! a start i eskalacja mieszkają tam, gdzie mieszkały: w sterowniku i w `supervisor.rs`
//! (niezmiennik 23). Tutaj jest wyłącznie mapa uchwytów i cztery czasowniki nad nią.

use std::io;

use crate::engine::drivers::command::StartSpec;
use crate::engine::supervisor::GroupProof;

/// Co okno wie o jednej uruchomionej rzeczy.
///
/// Trzy pola i ani jednego więcej — każde z nich odpowiada na pytanie, które człowiek naprawdę
/// zadaje kafelkowi: co to jest, którą grupę ubić, i czy to jeszcze biegnie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedProcess {
    /// Wiersz powłoki, co do znaku — patrz [`StartSpec::command`].
    pub command: String,
    /// `pgid` grupy. Jedyna liczba, która tę rzecz naprawdę identyfikuje, i jednocześnie ta,
    /// po której człowiek pozna ją w `ps`, a odzyskiwanie po awarii po niej sprząta.
    ///
    /// `i32`, nie `u32`: POSIX-owy `pid_t` jest znakowany, a `kill(-pgid, …)` używa znaku jako
    /// selektora grupy (powód w całości przy `engine::supervisor::GroupId`).
    pub pgid: i32,
    /// Czy w tej grupie ktoś jeszcze jest.
    ///
    /// POLE, nie założenie, i to jest cała obrona przed „Running" nad rzeczą, która zeszła:
    /// rejestr, który po prostu zapomina wpis w chwili śmierci, nie ma jak POWIEDZIEĆ oknu, że
    /// coś zeszło — a okno, które o tym nie usłyszy, zostawia kafelek na ekranie. Kafelka nie
    /// rysuje wtedy widok, nie ten plik.
    pub alive: bool,
}

/// Wszystko, co Loadout uruchomił dla człowieka i jeszcze o tym wie.
///
/// Jeden na aplikację, w `ipc::AppState`, obok uchwytu biegu i rozmowy z liderem. Nie jeden na
/// zakres: rzecz uruchomiona w jednym folderze biegnie dalej po przełączeniu widoku, a lista,
/// która by ją wtedy ukryła, jest listą, po której zostaje osierocony proces palący maszynę.
#[derive(Debug, Default)]
pub struct Processes;

impl Processes {
    /// Ani jednej rzeczy — stan aplikacji, która właśnie wstała.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Odpala komendę, która ma zostać, i zapisuje ją w rejestrze.
    ///
    /// Wraca **natychmiast**, z `pgid` już w ręku, i to jest cała różnica wobec kroku „sprawdź":
    /// rzecz żyje po powrocie tego wywołania. Wersja czekająca do końca komendy oddawałaby
    /// wołającemu wyłącznie nekrolog — nie byłoby czego pokazać na kafelku ani czego ubić przez
    /// cały czas, kiedy to naprawdę biegnie.
    pub fn start(&self, _spec: &StartSpec) -> io::Result<StartedProcess> {
        todo!("T-72: start przez CommandDriver::start_to_stay i wpis w rejestrze")
    }

    /// Wszystko, o czym ten rejestr jeszcze wie — także to, co zeszło i nie zostało jeszcze
    /// sprzątnięte.
    ///
    /// Zeszłe rzeczy zostają w tej odpowiedzi z rozmysłu: to jest jedyna droga, którą okno
    /// dowiaduje się o śmierci czegoś, czego nie zatrzymało samo. Kafelka takiemu wpisowi nie
    /// rysuje widok (`src/sections/run/rail/processes.ts`), więc lista może być uczciwa, a ekran
    /// mimo to nie kłamie.
    #[must_use]
    pub fn list(&self) -> Vec<StartedProcess> {
        todo!("T-72: rejestr oddaje to, co wie, razem z aktualnym stanem każdej grupy")
    }

    /// „Stop" na kafelku: prosi TĘ grupę o zejście i oddaje **dowód**.
    ///
    /// `None`, kiedy takiej grupy rejestr nie zna — wartość, nie błąd (niezmiennik 7): rzecz,
    /// która zeszła sama między jednym a drugim kliknięciem, nie jest awarią.
    ///
    /// `GroupProof`, nigdy `io::Result<()>`: `Ok(())` znaczyłoby „wysłałem sygnał", a wołający
    /// przeczytałby „nie żyje" i zgasił kafelek nad żywym procesem (niezmiennik 6).
    pub async fn stop(&self, _pgid: i32) -> Option<GroupProof> {
        todo!("T-72: eskalacja przez Staying::stop, zdjęcie wpisu, dowód do wołającego")
    }

    /// Zamknięcie okna: schodzą **wszystkie** i każda oddaje dowód śmierci swojej grupy.
    ///
    /// Powód stoi w nagłówku `recovery.rs`: rzecz, która przeżyje Loadouta, przechodzi pod PID 1
    /// i pracuje dalej — a odzyskiwanie po niej nie posprząta, bo nie ma wpisu w indeksie biegów.
    /// To jest ten sam defekt, który 2026-08-19 naprawiono dla biegów, i to samo, co
    /// [`super::chat::Threads::close`] robi dla rozmów.
    ///
    /// Po jednym dowodzie na rzecz, bo bilans jest kompletny tylko wtedy, kiedy widać KAŻDY
    /// z nich: jeden `Alive` wśród pięciu `Dead` jest dokładnie tym stanem, o którym nikt się nie
    /// dowie z liczby „zamknięto pięć".
    pub async fn close(&self) -> Vec<GroupProof> {
        todo!("T-72: każda grupa przez eskalację, po jednym dowodzie, rejestr zostaje pusty")
    }
}
