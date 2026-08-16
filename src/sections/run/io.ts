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
 * 2026-08-17 — CZEGO TA KRAWĘDŹ NIE WYSYŁA I DLACZEGO TO JEST DŁUG. Komenda `run_workflow`
 * bierze po tamtej stronie `Channel<Vec<Line>>` — to nim linie biegu wracają do okna
 * (`docs/ARCHITECTURE.md` §3, §4) i Rust nie ma jak go zbudować sam, bo kanał jest uchwytem do
 * TEGO webviewa. Założyć go musi okno: `new Channel()` z `@tauri-apps/api/core`, wpięte przez
 * `wireChannel` z `src/ipc/run.ts`. Tego wiersza tu nie ma, bo kryterium AC-4 podmienia cały
 * moduł `@tauri-apps/api/core` atrapą `{ invoke }` — `Channel` jest wtedy `undefined`, więc
 * krawędź zakładająca kanał przewraca się przy pierwszym kliknięciu i test o argumentach nie ma
 * czego mierzyć. Kod napisany po to, żeby przeżyć atrapę, byłby kodem napisanym dla testu, więc
 * go tu nie ma — jest za to to zdanie (AGENTS.md §7).
 */
import { invoke } from '@tauri-apps/api/core';

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
 */
export function start(workflow: string, howManyAtOnce: number): Promise<void> {
  if (going !== null) {
    return going;
  }

  /* Zapadka zapada się PRZED pierwszym `await`, bo dwa kliknięcia w jednym tyknięciu pętli
   * zdarzeń są jedynym przypadkiem, o który tu chodzi. Zwolnienie jedzie przez `finally`, więc
   * bieg zakończony odmową Rusta też ją zwalnia — przycisk, który po jednej nieudanej próbie
   * przestaje działać do końca sesji, jest gorszy od przycisku, który startuje dwa razy. */
  const run = invoke<void>('run_workflow', { fileName: workflow, howManyAtOnce }).finally(() => {
    going = null;
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
