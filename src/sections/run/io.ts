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
import { useRun } from '../../state/run';
import type { Step } from '../../state/run';
import { runFeed } from './feed/live';

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
   * `src/sections/run/feed/live.ts`: `runFeed.appendLines` niesie wiersze widoku,
   * `useRun.appendLines` okno linii. Pętla po paczce mieszka w `wireChannel`, żeby zysk
   * z pompy w Ruście przeżył granicę: jedna wiadomość to jedna aktualizacja stanu, nigdy
   * jedna na wiersz. */
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
  const lines = new Channel<unknown[]>();
  wireChannel(lines, (batch) => {
    const at = Date.now();
    const stamped = batch.map((line) => {
      stamp += 1;
      return { ...line, id: stamp, at };
    });
    runFeed.appendLines(stamped);
    useRun.getState().appendLines(stamped);
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
  useRun.getState().nowRunning(what.name, what.steps);

  const run = invoke<void>('run_workflow', {
    fileName: workflow,
    howManyAtOnce,
    lines,
  }).finally(() => {
    going = null;
    /* Bieg zszedł — także wtedy, gdy zszedł odmową Rusta. Bez tego Stop zostaje na ekranie na
     * zawsze i jest kontrolką bez roboty (niezmiennik 16), a pasek loadoutu opisuje bieg,
     * którego nie ma. `finally`, nie `then`: odmowa jest zejściem tak samo jak koniec. */
    useRun.getState().nowRunning('', []);
  });
  going = run;
  return run;
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
 * Bez argumentów, bo punkt kontrolny nie ma identyfikatora po tamtej stronie: `continue_run`
 * bierze samo `State` i podbija licznik zgód biegu (`RunControl::go_on` — licznik, nie flaga,
 * żeby bieg z dwoma punktami kontrolnymi zapytał dwa razy). Front, który dokleiłby tu numer
 * kroku, byłby drugim miejscem, w którym mieszka odpowiedź na pytanie „na czym stoimy".
 *
 * Rozwiązuje się dopiero wtedy, kiedy bieg NAPRAWDĘ ruszył (`wait_until_moving` po tamtej
 * stronie) — tak samo jak Stop wraca dopiero z dowodem. Ekran, który wróci wcześniej, pokazuje
 * człowiekowi dalej stojący bieg tuż po tym, jak ten człowiek go puścił.
 */
export function continueRun(): Promise<void> {
  return invoke<void>('continue_run');
}
