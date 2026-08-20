/* Jedyne miejsce w sekcji Bieg, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, krawędź po pięć linii).
 *
 * DLACZEGO OSOBNY PLIK, A NIE `invoke()` pod przyciskiem. Start jest pierwszą krawędzią tej
 * sekcji, ale nie ostatnią: za nim stoi Stop, a za Stopem Continue. Trzy `invoke` rozsiane po
 * trzech komponentach to trzy miejsca, w których mieszka nazwa komendy — a wtedy zapadka
 * „drugie kliknięcie nie startuje drugiego biegu" musi istnieć w każdym z nich osobno i
 * w jednym zawsze jej zabraknie.
 *
 * DLACZEGO IDENTYFIKATOR I LIMIT SĄ ARGUMENTAMI, A NIE ODCZYTEM ZE STANU. Magazyn otwartego
 * dokumentu (`src/state/workflows.ts`) jest FABRYKĄ, nie singletonem, a liczba „ile naraz" jest
 * stanem całej aplikacji, nie tej sekcji (`src/state/workspaces.ts`, akapit „czego tu nie ma").
 * Krawędź, która sięgałaby po nie sama, byłaby drugim miejscem, w którym mieszka odpowiedź na
 * pytanie „co jest otwarte" — i pierwszym, które by się rozjechało.
 *
 * 2026-08-18 — TA KRAWĘDŹ WYSYŁA KANAŁ, i to jest cały sens T-38. Do 2026-08-17 stał tu
 * akapit o długu: `run_workflow` bierze po tamtej stronie `Channel<Vec<Line>>`, okno go nie
 * zakładało, więc Tauri odrzucało wywołanie na deserializacji argumentów, zanim weszło
 * w ciało komendy — Start odbijał się przy KAŻDYM kliknięciu. Powodem, dla którego wiersza
 * tu nie było, była atrapa w kryterium T-30: oddawała samo `{ invoke }`, więc `Channel`
 * był `undefined`. To był prawdziwy powód i zła konkluzja — atrapa transportu jest
 * rzeczą do poprawienia, a nie granicą dla produktu. Atrapa umie dziś oddać `Channel`,
 * a kryterium AC-1 z T-38 czyta listę parametrów WPROST z `src-tauri/src/ipc.rs`, więc
 * czwarty argument dołożony po tamtej stronie zapala test sam.
 */
import { Channel, invoke } from '@tauri-apps/api/core';
import { wireChannel } from '../../ipc/run';
/* 2026-08-18 — POMPA PISZE DO SESJI ZAKRESU, NIE DO TEJ, KTÓRĄ WIDAĆ, i to jest cały wymóg
 * właściciela „przełączam zakres i nie tracę sesji". Ta krawędź wie, do którego folderu wysłała
 * bieg — sama go podaje `run_workflow` czwartym argumentem — więc paczki mają dokąd trafić
 * niezależnie od tego, na co człowiek patrzy. Wersja pisząca przez uchwyt aktywnego zakresu
 * przepisywałaby linie biegu z zakresu A do sesji zakresu B w chwili przełączenia, i wyglądałoby
 * to na ekranie jak dwa pomieszane biegi. Nagłówki `./feed/live` i `../../state/run` mówią to
 * samo z drugiej strony. */
import { runFor } from '../../state/run';
import type { Step } from '../../state/run';
import { feedFor } from './feed/live';

/**
 * Co dokładnie rusza — dwa pola paska loadoutu, oba znane oknu, zanim Rust cokolwiek powie.
 *
 * DLACZEGO OKNO, A NIE ODPOWIEDŹ KOMENDY. `run_workflow` oddaje `()` i jest to zapisany dług
 * (`src-tauri/src/ipc.rs`, akapit „WSZYSTKIE TRZY ODDAJĄ `()`"): `RunReport` nie jest
 * `Serialize`, a jego plik nie należy do tego zadania. Plan jest jednak w oknie już wcześniej —
 * sekcja Bieg czyta katalog workflow, żeby zbudować listę wyboru — więc pasek może pokazać
 * plan od pierwszej sekundy, zamiast dorysowywać go z linii `step` w trakcie biegu
 * (niezmiennik 17, i tak mówi komentarz przy `RunState.steps`).
 */
export interface WhatIsRunning {
  /** Jak workflow nazywa SAM SIEBIE. To jest napis, który zobaczy człowiek — nie nazwa pliku. */
  readonly name: string;
  /** Kroki w kolejności z grafu; na starcie wszystkie czekają. */
  readonly steps: readonly Step[];
}

/**
 * Bieg, który idzie **teraz**, albo `null`.
 *
 * Stan modułu, nie stan komponentu, i to jest ta sama decyzja, co przy `runFeed`
 * (`src/sections/run/feed/live.ts`): bieg nie kończy się dlatego, że człowiek wszedł do
 * Agentów. Zapadka trzymana w komponencie znika razem z ekranem sekcji, a wtedy powrót do
 * Pracy i kliknięcie Start startują drugi bieg tego samego workflow.
 */
let going: Promise<void> | null = null;

/**
 * Start: uruchamia otwarty workflow.
 *
 * Rozwiązuje się dopiero wtedy, kiedy bieg się skończy — komenda po stronie Rusta trwa tyle,
 * co bieg — i to jest zarazem cała definicja słowa „w trakcie" dla zapadki: **drugie kliknięcie,
 * zanim pierwsze wróci, nie ma prawa zawołać komendy drugi raz**. Dwa biegi tego samego
 * workflow to dwa zestawy agentów piszących po tych samych plikach, czyli dokładnie to, czego
 * walidator odmawia przy zapisie (niezmiennik 12) — tylko że tutaj nikt nie odmawia, bo z
 * punktu widzenia Rusta to są dwa poprawne żądania.
 *
 * Drugie kliknięcie dostaje **ten sam** bieg, a nie odmowę: pytanie „kiedy to się skończy" ma
 * jedną odpowiedź, więc oddajemy tę, którą mamy. Wyjątek zmuszałby każde wywołanie do `catch`
 * wokół czegoś, co nie jest błędem (niezmiennik 7 w duchu).
 *
 * @param workflow identyfikator otwartego workflow — to samo, czym front nazywa jego plik.
 *   Katalog rozwiązuje Rust [T3 §8.3]; front, który dokleiłby ścieżkę sam, byłby drugim
 *   miejscem, w którym mieszka odpowiedź na pytanie „gdzie to leży".
 * @param howManyAtOnce ile kroków ma NAPRAWDĘ biec naraz. Liczba jedzie w żądaniu, nigdy ze
 *   stałej po tamtej stronie (niezmiennik 11): cicha wersja złamania wygląda jak pole, które
 *   jest wczytywane, logowane i nigdzie nie podawane, a semafor dostaje `1`.
 * @param folder katalog, w którym mają pracować agenci — ścieżka z aktywnej karty, albo `null`,
 *   kiedy nie ma otwartej żadnej. `null`, a nie pominięty klucz: powód stoi przy `invoke` niżej.
 *   Do 2026-08-18 tego argumentu nie było i wybrany folder nie dojeżdżał do biegu w ogóle.
 * @param task zdanie z wiersza wejścia — co ten bieg ma zbudować — albo `null`, kiedy człowiek
 *   nic nie napisał i biegnie tylko to, co stoi w pliku. `null`, a nie pominięty klucz: dokładnie
 *   ten sam powód, co przy `folder`, i ta sama klasa awarii, która wywaliła Start 2026-08-17.
 *   Nazwa jest nazwą parametru `run_workflow` z `src-tauri/src/ipc.rs`, przepisaną STAMTĄD.
 *   Co Rust z tym robi: wpisuje je w prompt każdego kroku agenta — w miejsce `{{task}}`, jeśli plik
 *   je wskazał, a w przeciwnym razie na górę promptu pod nagłówkiem (`commands::run::with_the_task`).
 * @param what nazwa i plan tego workflow — to, co ta krawędź zapisuje w magazynie biegu.
 *   Wartość domyślna jest MOSTEM, nie wygodą: dwa cudze kryteria (`start-invokes.test.tsx`
 *   z T-30 i `start-args-complete.test.tsx` z T-38 AC-1) wołają tę krawędź dwoma argumentami
 *   i żadnego z nich nie wolno tknąć, więc trzeci parametr musi być opcjonalny. Wtedy zostaje
 *   nazwa pliku: prawdziwa, ale nie ta, którą workflow nadał sobie sam. Jedyny wołający
 *   produkcyjny — `src/sections/run/start.tsx` — podaje komplet.
 */
export function start(
  workflow: string,
  howManyAtOnce: number,
  what: WhatIsRunning = { name: workflow, steps: [] },
  folder: string | null = null,
  task: string | null = null,
): Promise<void> {
  if (going !== null) {
    return going;
  }

  /* Zapadka zapada się PRZED pierwszym `await`, bo dwa kliknięcia w jednym tyknięciu pętli
   * zdarzeń są jedynym przypadkiem, o który tu chodzi. Zwolnienie jedzie przez `finally`, więc
   * bieg zakończony odmową Rusta też ją zwalnia — przycisk, który po jednej nieudanej próbie
   * przestaje działać do końca sesji, jest gorszy od przycisku, który startuje dwa razy. */
  /* Kanał zakłada OKNO, bo jest uchwytem do tego webviewa i Rust nie ma go jak zbudować sam
   * (`docs/ARCHITECTURE.md` §3, §4). Powstaje na bieg, nie na moduł: uchwyt przeżywający bieg
   * kierowałby linie drugiego biegu do odbiorcy pierwszego.
   *
   * Paczka wchodzi DWOMA wywołaniami i nigdzie indziej — tak, jak mówi
   * `src/sections/run/feed/live.ts`: `feedFor(…).appendLines` niesie wiersze widoku,
   * `runFor(…).appendLines` okno linii. Pętla po paczce mieszka w `wireChannel`, żeby zysk
   * z pompy w Ruście przeżył granicę: jedna wiadomość to jedna aktualizacja stanu, nigdy
   * jedna na wiersz.
   *
   * OBIE SESJE ROZSTRZYGNIĘTE RAZ, PRZED PIERWSZĄ PACZKĄ, i to nie jest oszczędność wywołań:
   * ten bieg należy do TEGO zakresu przez cały swój czas, a rozstrzyganie sesji w środku
   * domknięcia dawałoby uchwyt, który mógłby się przesunąć razem z widokiem. */
  /* STEMPEL POWSTAJE TUTAJ, I TO JEST ROZBIEŻNOŚĆ DO ZGŁOSZENIA (AGENTS.md §7).
   * `src/state/run.ts` opisuje `Stamped.id` jako „ściśle rosnący numer nadawany po stronie
   * Rusta [T2 §6.3]" — a `src-tauri/src/engine/line.rs` nie serializuje ani `id`, ani `at`:
   * `at_ms` istnieje wyłącznie w `Seen`, czyli w WEJŚCIU kuratora, i nigdy nie wychodzi na drut.
   * Dopóki tak jest, jedynym miejscem, w którym te dwa pola mogą powstać, jest granica — czyli
   * to miejsce. `at` jest tu poprawne z definicji („kiedy zdarzenie NAPŁYNĘŁO"), `id` jest
   * zastępcze: zachowuje kolejność przybycia, ale nie przeżyje przeładowania okna i nie zgodzi
   * się z żadnym numerem po stronie Rusta. Prawdziwa naprawa to pole na drucie, czyli
   * `engine/line.rs`, który należy do T-05 — poza OWNS tego zadania. */
  let stamp = 0;
  /* Klucz sesji tego biegu. Pusty napis znaczy „bez wskazanego folderu" — Rust bierze wtedy
   * katalog, pod którym wstała aplikacja (`AppState::project_for`), i to też jest jedna,
   * konkretna sesja, a nie „żadna". Ten sam sentinel czyta rejestr strumienia. */
  const session = runFor(folder);
  const view = feedFor(folder ?? '');
  const lines = new Channel<unknown[]>();
  wireChannel(lines, (batch) => {
    const at = Date.now();
    const stamped = batch.map((line) => {
      stamp += 1;
      return { ...line, id: stamp, at };
    });
    view.appendLines(stamped);
    session.getState().appendLines(stamped);
  });

  /* MAGAZYN DOWIADUJE SIĘ TUTAJ, I TO JEST DRUGA POŁOWA T-38 AC-3.
   *
   * `RunState.workflow` startowało `''` i do 2026-08-18 NIE MIAŁO PISARZA — komentarz przy polu
   * obiecywał, że „wypełnia je komenda startu biegu", a nie robiło tego nic. Skutkiem nie był
   * pusty napis: Stop renderuje się wyłącznie przy biegu, a „czy bieg trwa" to dokładnie
   * `workflow !== ''`, więc przycisk Stop nie montował się NIGDY, pasek loadoutu był trwale
   * pusty, a bieg dało się zacząć i nie dało się zatrzymać z okna.
   *
   * Przed `invoke`, nie po nim: komenda po tamtej stronie trwa tyle, co bieg, więc zapis po
   * jej powrocie ogłaszałby start biegu w chwili, w której bieg właśnie się skończył. */
  /* FOLDER JEDZIE DO MAGAZYNU TĄ SAMĄ DROGĄ, i to jest połowa naprawy „zamknięcie dowolnej
   * karty ubija jedyny bieg". `stop_run` nie bierze identyfikatora, więc jedyne, co okno może
   * zrobić uczciwie, to nie wołać go dla karty, do której ten bieg nie należy — a do tego
   * musi wiedzieć, gdzie on idzie. Wie, bo sam ten folder tu wysyła (patrz `invoke` niżej). */
  session.getState().nowRunning(what.name, what.steps, folder);

  const run = invoke<void>('run_workflow', {
    fileName: workflow,
    howManyAtOnce,
    /* KLUCZ JEST OBECNY ZAWSZE, TAKŻE JAKO `null`, i to nie jest ozdoba. Tauri dopasowuje
     * argumenty PO NAZWIE i deserializuje je PRZED wejściem w ciało komendy, więc brakujący
     * klucz nie jest mniejszym wywołaniem — jest odrzuconym. `Option<String>` po tamtej
     * stronie przyjmuje `null` i znaczy „biegnij tam, gdzie aplikacja wstała"; pominięcie
     * klucza znaczyłoby „odrzuć to wywołanie". */
    folder,
    /* TEN SAM POWÓD, CO PRZY `folder` WYŻEJ, i nie jest to powtórka dla ozdoby: `task` doszedł
     * do `run_workflow` po stronie Rusta, a ta krawędź do 2026-08-19 nadal wysyłała cztery
     * klucze z pięciu. Tauri deserializuje argumenty PO NAZWIE i PRZED wejściem w ciało
     * komendy, więc brakujący klucz odrzuca całe wywołanie — Start odbijałby się przy każdym
     * kliknięciu, zdaniem, którego człowiek nie zobaczy. `Option<String>` przyjmuje `null`
     * i znaczy „biegnij tym, co stoi w pliku". */
    task,
    lines,
  }).finally(() => {
    going = null;
    /* Bieg zszedł — także wtedy, gdy zszedł odmową Rusta. Bez tego Stop zostaje na ekranie na
     * zawsze i jest kontrolką bez roboty (niezmiennik 16), a pasek loadoutu opisuje bieg,
     * którego nie ma. `finally`, nie `then`: odmowa jest zejściem tak samo jak koniec. */
    session.getState().nowRunning('', [], null);
    /* Bieg zszedł, więc nie stoi już na niczyim pytaniu: kontrolka „dalej" ma zniknąć razem
     * z nim, także wtedy, gdy człowiek odpowiedział na punkt kontrolny i biegu nie puścił.
     * Bez tej linii zostawałaby na ekranie po biegu, którego nie ma (niezmiennik 16).
     *
     * W SESJI TEGO BIEGU, nie w tej, którą widać: bieg zakresu A kończący się wtedy, kiedy
     * człowiek patrzy na zakres B, zdejmowałby przypięte pytanie z cudzej sesji. */
    view.runEnded();
  });
  going = run;
  return run;
}

/**
 * Kogo pytamy — dwa pola definicji agenta, oba potrzebne po dwóch różnych stronach granicy.
 *
 * IDENTYFIKATOR JEDZIE NA DRUT, bo przeżywa zmianę nazwy [T3 §3.1] i bo `run_agent` po tamtej
 * stronie szuka nim agenta w bibliotece. NAZWA zostaje w oknie: staje na pasku loadoutu i na
 * karcie, a Rust jej nie potrzebuje — weźmie ją z tej samej definicji.
 *
 * Kształt, nie typ `Agent`: ta krawędź nie ma powodu wiedzieć o dziewięciu polach agenta,
 * a definicja z biblioteki pasuje tu bez ani jednej konwersji.
 */
export interface Asked {
  readonly id: string;
  readonly name: string;
}

/**
 * `/ask`: uruchamia JEDNEGO agenta z jednym zdaniem — i jest to zwykły bieg.
 *
 * Rozwiązuje się dopiero wtedy, kiedy bieg się skończy, dokładnie jak [`start`]: komenda po
 * tamtej stronie trwa tyle, co bieg.
 *
 * # Dlaczego tu NIE MA zapadki `going`
 *
 * Bo drugie `/ask` ma dostać ZDANIE, a nie ten sam bieg co pierwsze. Zapadka pod Startem
 * odpowiada na pytanie „drugie kliknięcie tego samego przycisku" i oddaje wtedy bieg, który
 * już idzie — bo pytanie „kiedy to się skończy" ma jedną odpowiedź. Tutaj drugie `/ask` jest
 * pytaniem o INNEGO agenta z INNYM zdaniem, więc oddanie mu cudzego biegu byłoby ciszą
 * w miejscu, w którym człowiek właśnie o coś poprosił. Odmawia Rust
 * (`AppState::begin_a_run`), jednym zdaniem, które mówi, co zrobić — i to jest jedyne miejsce,
 * które WIE, czy jakiś bieg naprawdę jeszcze nie zszedł.
 *
 * @param who kogo pytamy — z biblioteki agentów, nie z pola tekstowego: rozbiór linii
 *   tłumaczy wpisane słowo na definicję, zanim cokolwiek pojedzie na drut (`../ask-command.ts`).
 * @param task zdanie człowieka, co do znaku. Puste odmawia po stronie rozbioru — agent bez
 *   polecenia to tura, za którą ktoś płaci, choć nikt o nic nie zapytał.
 * @param howManyAtOnce ile kroków ma NAPRAWDĘ biec naraz. Ta sama liczba, co przy biegu
 *   z pliku, i nigdy stała `1` po tamtej stronie: bieg jednokrokowy bierze miejsce z TEJ SAMEJ
 *   puli (niezmiennik 11).
 * @param folder katalog, w którym ma pracować agent, albo `null`. Klucz jedzie ZAWSZE, także
 *   jako `null` — powód w całości stoi przy `invoke` w [`start`].
 */
export function ask(
  who: Asked,
  task: string,
  howManyAtOnce: number,
  folder: string | null = null,
): Promise<void> {
  const session = runFor(folder);
  const view = feedFor(folder ?? '');
  const lines = new Channel<unknown[]>();
  let stamp = 0;
  wireChannel(lines, (batch) => {
    const at = Date.now();
    const stamped = batch.map((line) => {
      stamp += 1;
      return { ...line, id: stamp, at };
    });
    view.appendLines(stamped);
    session.getState().appendLines(stamped);
  });

  /* PLAN JEST JEDEN I OKNO GO ZNA, zanim Rust cokolwiek powie — tak samo jak przy biegu
   * z pliku. Klucz kroku to IDENTYFIKATOR AGENTA i musi nim być: pasek dopasowuje linie stanu
   * do bloków po tym kluczu (`state/run.ts`, `withStepStates`), a po tamtej stronie ten sam
   * klucz nosi kafelek jednokrokowego planu (`commands::run::plan_ask`). Uuid kroku powstaje
   * w Ruście, więc okno nigdy go nie widziało — pasek stałby na „waiting" do końca biegu. */
  session
    .getState()
    .nowRunning(who.name, [{ id: who.id, name: who.name, state: 'pending' }], folder);

  return invoke<void>('run_agent', {
    agent: who.id,
    task,
    howManyAtOnce,
    folder,
    lines,
  }).finally(() => {
    /* Bieg zszedł — także wtedy, gdy zszedł odmową Rusta. Bez tego Stop zostaje na ekranie na
     * zawsze i jest kontrolką bez roboty (niezmiennik 16). Powód w całości stoi przy [`start`],
     * razem z tym, dlaczego to jest `finally`, a nie `then`. */
    session.getState().nowRunning('', [], null);
    view.runEnded();
  });
}

/**
 * Stop: zatrzymuje bieg, który idzie.
 *
 * Rozwiązuje się dopiero z **dowodem**, że po biegu nic nie żyje — `stop_run` po tamtej stronie
 * wraca po `kill(-pgid, 0) == ESRCH`, nie po wysłaniu sygnału (niezmiennik 6). Ekran, który
 * powie „zatrzymane" wcześniej, kłamie o agencie, który dalej pisze i dalej płaci.
 */
export function stop(): Promise<void> {
  return invoke<void>('stop_run');
}

/**
 * Dalej: puszcza bieg zza punktu kontrolnego.
 *
 * DLACZEGO TA FUNKCJA W OGÓLE POWSTAŁA. `continue_run` jest po stronie Rusta zarejestrowana,
 * stoi na `src-tauri/commands.golden.txt` i do 2026-08-18 miała w całym `src/` ZERO wołających.
 * Kafelek punktu kontrolnego zatrzymuje przy tym cały bieg, nie sam krok
 * (`commands::run::wait_for_a_person`), więc workflow z takim kafelkiem parkował na zawsze
 * i z okna wyglądał dokładnie jak zawieszony agent. Mechanizm bez wołającego przechodzi każdą
 * bramkę, jaką mamy — dokładnie jak `wireChannel` przed tym zadaniem.
 *
 * Bez identyfikatora punktu kontrolnego, bo takiego po tamtej stronie nie ma: `continue_run`
 * podbija licznik zgód biegu (`RunControl::go_on_with` — licznik, nie flaga, żeby bieg z dwoma
 * punktami kontrolnymi zapytał dwa razy). Front, który dokleiłby tu numer kroku, byłby drugim
 * miejscem, w którym mieszka odpowiedź na pytanie „na czym stoimy".
 *
 * 2026-08-18 — ODPOWIEDŹ CZŁOWIEKA JEDZIE RAZEM ZE ZGODĄ, i ten argument dołożył Rust w tej
 * samej fali (`ipc.rs`: `continue_run(state, answer: Option<String>)`,
 * `commands::run::continue_run_inner` → `go_on_with(answer)`). Bez klucza `answer` w żądaniu to
 * wywołanie było ODRZUCANE, nie mniejsze: Tauri dopasowuje argumenty PO NAZWIE i deserializuje
 * je przed wejściem w ciało komendy, więc kontrolka „dalej" odbijałaby się przy każdym
 * kliknięciu, z komunikatem, którego nikt nie widzi. Dokładnie tak Start był zepsuty
 * 2026-08-17 (`checks/quick-invoke-args.sh` istnieje z tego powodu).
 *
 * `null`, kiedy człowiek puścił bieg bez pisania — to jest cała treść `Option<String>` po
 * tamtej stronie. Argument jest opcjonalny, bo cudze kryterium
 * (`continue-at-checkpoint.test.tsx`) woła tę krawędź bez argumentów i nie wolno go tknąć;
 * klucz jedzie jednak ZAWSZE, bo pominięty klucz to odrzucone wywołanie.
 *
 * Rozwiązuje się dopiero wtedy, kiedy bieg NAPRAWDĘ ruszył (`wait_until_moving` po tamtej
 * stronie) — tak samo jak Stop wraca dopiero z dowodem. Ekran, który wróci wcześniej, pokazuje
 * człowiekowi dalej stojący bieg tuż po tym, jak ten człowiek go puścił.
 */
export function continueRun(answer: string | null = null): Promise<void> {
  return invoke<void>('continue_run', { answer });
}

/**
 * Powiedz coś agentowi, który pracuje — kolejna tura w jego żywej sesji.
 *
 * 2026-08-18 — POWSTAŁO ZE ZGŁOSZENIA WŁAŚCICIELA: „dalej nie działa pisanie do agenta przez
 * terminal". Wiersz wejścia odpowiadał na prozę zdaniem „That one is not known here", bo nie
 * istniała ŻADNA droga do żywej sesji — nie z braku komendy, a z powodu, który leżał trzy
 * warstwy niżej: `stdin` był polem uchwytu, więc pisanie wymagało `&mut`, a uchwyt jest
 * pożyczony mutowalnie przez całą turę. Naprawa poszła w przyczynę (`engine::drivers::Voice`).
 *
 * @param text co człowiek napisał. Puste odmawia po tamtej stronie — nie zgadujemy tu, co znaczy
 *   pusty Enter.
 * @param agent nazwa kroku, do którego mówimy, albo `null`. `null` znaczy „ten jeden, który
 *   pracuje": przy dwóch i więcej Rust odmawia z listą nazw, zamiast wysyłać do losowego.
 */
export function sayToAgent(text: string, agent: string | null = null): Promise<void> {
  return invoke<void>('say_to_agent', { agent, text });
}

/**
 * Otwiera strumień rozmowy z orchestratorem — bez uruchamiania procesu.
 *
 * # Po co osobne otwarcie, a nie jedno wywołanie z tekstem
 *
 * Bo kanał do okna umie zbudować **tylko okno** (`docs/ARCHITECTURE.md` §3, §4), więc musi wejść
 * argumentem — a sesji u dostawcy nie wolno tu wstawiać: tura wystartowana przy montażu ekranu
 * jest turą, za którą ktoś płaci, choć nikt o nic nie zapytał. Ta krawędź zakłada więc pompę,
 * a proces wstaje dopiero przy pierwszym zdaniu (`say_to_orchestrator`).
 *
 * # Gdzie lądują te wiersze
 *
 * W TYM SAMYM strumieniu, co bieg: rozmowa o tym, co ma się stać, i praca, która się dzieje, są
 * jedną historią tego zakresu. Dlatego zapis idzie przez `feedFor(folder)` i `runFor(folder)` —
 * tą samą drogą i tym samym stemplem, co paczki biegu (patrz `start`), bo dwie drogi do jednego
 * widoku dałyby dwa porządki wierszy i pierwszy sklejony wiersz by je rozjechał.
 */
export function openChat(folder: string | null = null): Promise<void> {
  const session = runFor(folder);
  const view = feedFor(folder ?? '');
  const lines = new Channel<unknown[]>();
  let stamp = 0;
  wireChannel(lines, (batch) => {
    const at = Date.now();
    const stamped = batch.map((line) => {
      stamp += 1;
      return { ...line, id: stamp, at };
    });
    view.appendLines(stamped);
    session.getState().appendLines(stamped);
  });
  return invoke<void>('open_chat', { lines });
}

/**
 * Powiedz zdanie orchestratorowi — rozmowa, nie praca.
 *
 * ORCHESTRATOR NIE URUCHAMIA BIEGU I NIE MA JAK. Rozstrzygnięcie właściciela 2026-08-19: „tylko
 * komendy determinują akcje workflow". Po tamtej stronie nie jest to prośba w promptcie
 * systemowym, a własność struktury — `commands::chat` nie zna ani biegu, ani jego bazy.
 *
 * @param folder katalog, w którym rozmowa ma patrzeć — ścieżka aktywnego zakresu albo `null`.
 *   Klucz jest obecny zawsze, także jako `null`: Tauri dopasowuje argumenty PO NAZWIE
 *   i deserializuje je PRZED wejściem w ciało komendy, więc brakujący klucz odrzuca wywołanie.
 */
export function sayToOrchestrator(text: string, folder: string | null = null): Promise<void> {
  return invoke<void>('say_to_orchestrator', { folder, text });
}
