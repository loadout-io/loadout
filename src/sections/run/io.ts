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
import { nextStamp, runFor, type RunStore } from '../../state/run';
import type { Step } from '../../state/run';
/* WYŁĄCZNIE TYP: strzałka „po" jest kształtem z pliku workflow, a ta krawędź tylko ją przewozi.
 * `import type` znika w kompilacji (`verbatimModuleSyntax`), więc sekcja Bieg nie zyskuje ani
 * jednej zależności W CZASIE WYKONANIA od magazynu otwartego dokumentu. */
import type { Link } from '../../state/workflows';
import type { Line } from '../../ipc/types';
import type { ConversationImage } from './entry/images';
import { runSuggestion } from './feed/suggested';
import { autoStarts } from './auto-start';
import { feedFor } from './feed/live';
import { takeTheBudget } from './limits/chosen';
/* SUFIT DLA BIEGU, KTÓREGO NIKT NIE ZAMÓWIŁ Z PASKA — dostawa triggera bierze domyślną kwotę
 * z Settings, a nie to, co akurat trzyma pasek. Powód przy `theCeilingFor` niżej. */
import { defaultBudgetUsd } from '../../state/settings';
import type { TriggerClaim } from '../triggers/io';
import { ONE_RUN_AT_A_TIME, aRunIsGoing, holdTheRun, letTheRunGo } from './going';

/**
 * Ile wolno wydać na bieg, który właśnie rusza — i skąd ta kwota pochodzi.
 *
 * 2026-08-29, DRUGA POPRAWKA — DWA RODZAJE STARTU, DWA ŹRÓDŁA. Ręczny bieg bierze to, co
 * człowiek ma w pasku, i **zjada** to nadpisanie: dotyczyło jednego biegu, więc następny wraca
 * do kwoty z Settings. Bieg z dostawy triggera bierze WPROST kwotę domyślną — nikt przy nim nie
 * siedzi, a pasek opisuje wtedy zamiar człowieka wobec JEGO następnego biegu. Pierwsza wersja
 * dawała triggerowi nadpisanie z paska, więc jedno zdjęcie sufitu przed wyjściem z domu puszczało
 * bez ograniczenia każdą sprawę, która przyszła w nocy.
 *
 * `undefined` na argumencie znaczy „rozstrzygnij to sam"; jawna liczba albo jawne `null` od
 * wołającego wygrywają i nie ruszają nadpisania.
 */
function theCeilingFor(
  asked: number | null | undefined,
  claim: TriggerClaim | null,
): number | null {
  if (asked !== undefined) return asked;
  return claim === null ? takeTheBudget() : defaultBudgetUsd();
}

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
  /**
   * Strzałki „po" z pliku tego workflow — brak pola znaczy „nie wiemy".
   *
   * 2026-08-31 — DLACZEGO DOPIERO TERAZ I DLACZEGO OPCJONALNIE. Kroki jechały tędy od
   * początku, kolejność między nimi nie jechała wcale, więc widok biegu miał listę i nie miał
   * ani jednej relacji — a rysunek postawiony na takim stanie rysowałby kolejność, której nikt
   * nie zapisał (niezmiennik 17). Pole jest opcjonalne DOKŁADNIE tą samą drogą, którą
   * 2026-08-28 weszło `kind`: wartość domyślna argumentu `what` w [`start`] jest mostem dla
   * dwóch cudzych kryteriów wołających tę krawędź dwoma argumentami, więc nic nowego nie ma
   * prawa być wymagane. Brak pola dojeżdża do magazynu jako `null`, czyli jako „nie wiemy",
   * i to jest prawda o starcie, który pliku workflow nie czytał.
   *
   * `| undefined` JAWNIE, bo `exactOptionalPropertyTypes` odróżnia „klucza nie ma" od „klucz
   * niesie undefined", a jedyny produkcyjny wołający (`./launch.ts`) przepisuje tu pole
   * pozycji listy, które samo bywa nieobecne.
   */
  readonly links?: readonly Link[] | undefined;
}

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
 * Drugie kliknięcie dostaje **odmowę ze zdaniem**, a nie bieg poprzedni, i to jest zmiana
 * z 2026-08-20 (T-69). Do tego dnia stało tu, że oddajemy ten sam bieg, bo „pytanie »kiedy to
 * się skończy« ma jedną odpowiedź" — tylko że naciskający nie zadał tego pytania. Zadał inne:
 * „czy moje naciśnięcie coś zrobiło". Odpowiedzią na nie był wynik biegu PIERWSZEGO, czyli przy
 * udanym biegu `null` — czyli cisza. Człowiek naciskał Run i nie miał jak odróżnić biegu, który
 * ruszył, od biegu, który nie ruszył nigdy; jedynym czytelnym śladem była linia w dzienniku,
 * którego nikt nie otwiera. Przycisk, który tak odpowiada, czyta się jak martwy
 * (niezmiennik 16 w duchu).
 *
 * ODMOWA JEST NAPISEM, nie `Error`em, i nie jest to skrót: dokładnie tym kształtem odrzuca
 * Tauri (`.map_err(|e| e.to_string())` po tamtej stronie, `reject(napis)` po tej), więc wołający
 * ma z tej krawędzi JEDEN kształt odmowy na wszystkie powody i wyjmuje z niego zdanie tym samym
 * `why()`, którym wyjmuje odmowę Rusta (niezmiennik 23 — kształt drutu zna jeden adapter).
 *
 * PODPIS ZOSTAJE `Promise<void>`. `Promise<string | null>` nie jest przypisywalne do
 * `Promise<void>`, a `start-invokes.test.tsx` — cudze kryterium — trzyma wynik tej funkcji pod
 * adnotacją `Promise<void> | null`. Powód wraca więc drogą odmowy, tą samą, którą wraca każda
 * inna.
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
  /** Durable trigger delivery; every ordinary Start carries an explicit null. */
  claim: TriggerClaim | null = null,
  /**
   * Sufit wydatku tego biegu w dolarach, albo `null` — „bez limitu".
   *
   * OSTATNI I OPCJONALNY, bo dwa cudze kryteria (`start-invokes.test.tsx`,
   * `start-args-complete.test.tsx`) wołają tę krawędź dwoma argumentami i nie wolno ich tknąć.
   * Klucz na drucie jedzie mimo to ZAWSZE, także jako `null`: Tauri dopasowuje argumenty po
   * nazwie i deserializuje je przed wejściem w ciało komendy, więc brakujący klucz nie jest
   * mniejszym wywołaniem — jest odrzuconym.
   *
   * POMINIĘTY ZNACZY „ROZSTRZYGNIJ TO SAM", nie „bez limitu": sufit jest faktem CAŁEJ aplikacji,
   * tak samo jak „ile naraz" (`./limits/chosen`), i tak samo ma jechać każdą drogą startu —
   * przyciskiem, `/run` i zielonym Run z edytora. Podanie go osobno w każdej z nich byłoby czwartą
   * kopią tej samej decyzji, a rozjechałaby się ta droga, o której ktoś zapomni (niezmiennik 23).
   * Skąd wtedy pochodzi liczba i co to robi z nadpisaniem paska, stoi przy [`theCeilingFor`].
   */
  budgetUsd?: number | null,
  /** Whether Loadout should take its private learning turn after this run. */
  reflectionEnabled = true,
): Promise<void> {
  if (aRunIsGoing()) {
    /* ZAPADKA ZOSTAJE I NIC NIE WOŁA — zmienia się tylko to, co z niej wypada. Drugi bieg tego
     * samego workflow nadal nie ma prawa dojść do Rusta (niezmiennik 12, `start-invokes.test.tsx`
     * tego pilnuje), a `going` zwalnia dopiero `finally` pierwszego biegu. */
    return Promise.reject(ONE_RUN_AT_A_TIME);
  }

  /* ZA ZAPADKĄ, i to jest wymóg, nie porządek czytania: [`theCeilingFor`] ZJADA nadpisanie
   * z paska, a drugie kliknięcie w tym samym tyknięciu pętli zdarzeń nigdy nie dojdzie do Rusta.
   * Policzone w wartości domyślnej argumentu wykonałoby się przed tym `return` i zabrało kwotę
   * biegowi, który dopiero co ruszył. */
  const ceiling = theCeilingFor(budgetUsd, claim);

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
   * `src-tauri/src/engine/line.rs` nie serializuje ani `id`, ani `at`: `at_ms` istnieje wyłącznie
   * w `Seen`, czyli w WEJŚCIU kuratora, i nigdy nie wychodzi na drut. Dopóki tak jest, jedynym
   * miejscem, w którym te dwa pola mogą powstać, jest granica — czyli to miejsce. `at` jest tu
   * poprawne z definicji („kiedy zdarzenie NAPŁYNĘŁO"), `id` jest zastępcze. Prawdziwa naprawa
   * to pole na drucie, czyli `engine/line.rs`, który należy do T-05 — poza OWNS tego zadania.
   *
   * 2026-09-02 — NUMER JEDZIE Z JEDNEGO LICZNIKA NA CAŁE OKNO (`nextStamp`), a nie z własnego
   * licznika tej pompy. Powód w całości stoi przy tamtej funkcji; w skrócie: rozmowa i bieg
   * wchodzą do TEJ SAMEJ historii, więc pompa licząca od siebie wydaje numer, który już w niej
   * stoi — a numer wiersza jest jego adresem dla `toggle`, dla bloku „Answered" i dla Reacta. */
  /* Klucz sesji tego biegu. Pusty napis znaczy „bez wskazanego folderu" — Rust bierze wtedy
   * katalog, pod którym wstała aplikacja (`AppState::project_for`), i to też jest jedna,
   * konkretna sesja, a nie „żadna". Ten sam sentinel czyta rejestr strumienia. */
  const session = runFor(folder);
  const view = feedFor(folder ?? '');
  const lines = new Channel<unknown[]>();
  wireChannel(lines, (batch) => {
    const at = Date.now();
    const stamped = batch.map((line) => ({ ...line, id: nextStamp(), at }));
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
  const putBack = whatWasRunning(session);
  /* `?? null` ZAMIENIA BRAK POLA NA „NIE WIEMY", nie na pustą listę. Start bez planu z pliku —
   * ten spod wartości domyślnej `what` — nie ma prawa twierdzić, że ten bieg jest bez ani jednej
   * strzałki: to byłoby zdanie o kształcie pracy, a nikt go tu nie wypowiedział. */
  session.getState().nowRunning(what.name, what.steps, folder, workflow, what.links ?? null);

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
    /* Ten sam powód, co przy `task` i `folder`: sufit wydatku jedzie kluczem także wtedy, gdy
     * nikt go nie postawił. `null` znaczy „bez limitu"; pominięcie klucza znaczy „odrzuć to
     * wywołanie". */
    budgetUsd: ceiling,
    /* Explicit even at the default: Tauri matches named arguments before entering Rust. */
    reflectionEnabled,
    /* Present even for a manual Start. Tauri matches arguments by name before entering Rust,
     * so omitting this optional Rust value is not equivalent to sending `null`. */
    claim,
    lines,
  }).finally(() => {
    letTheRunGo();
    /* Bieg zszedł — także wtedy, gdy zszedł odmową Rusta. Bez tego Stop zostaje na ekranie na
     * zawsze i jest kontrolką bez roboty (niezmiennik 16), a pasek loadoutu opisuje bieg,
     * którego nie ma. `finally`, nie `then`: odmowa jest zejściem tak samo jak koniec.
     *
     * ODTWARZAMY, nie zerujemy — powód w całości stoi przy [`whatWasRunning`]. Dla startu,
     * który naprawdę ruszył, to jest to samo zerowanie, co wcześniej. */
    putBack();
    /* Bieg zszedł, więc nie stoi już na niczyim pytaniu: kontrolka „dalej" ma zniknąć razem
     * z nim, także wtedy, gdy człowiek odpowiedział na punkt kontrolny i biegu nie puścił.
     * Bez tej linii zostawałaby na ekranie po biegu, którego nie ma (niezmiennik 16).
     *
     * W SESJI TEGO BIEGU, nie w tej, którą widać: bieg zakresu A kończący się wtedy, kiedy
     * człowiek patrzy na zakres B, zdejmowałby przypięte pytanie z cudzej sesji. */
    view.runEnded();
  });
  holdTheRun(run);
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
  /** Sufit wydatku tego biegu, albo `null`. Ten sam sufit i tą samą drogą, co przy biegu
   * z pliku: `/ask` jest zwykłym biegiem, więc obowiązuje go ta sama kwota — razem z tym, że
   * nadpisanie z paska starcza na JEDEN bieg ([`theCeilingFor`]). */
  budgetUsd?: number | null,
): Promise<void> {
  /* Zawsze ręczny: `/ask` wychodzi z wiersza wejścia, przy którym siedzi człowiek, więc bierze
   * jego nadpisanie i je zjada — tak samo, jak zrobiłby to przycisk Start. */
  const ceiling = theCeilingFor(budgetUsd, null);
  /* TE DZIEWIĘĆ LINII SĄ TRZECIĄ KOPIĄ (`start`, `openChat`, tutaj) I TO JEST ZGŁOSZENIE, NIE
   * WYGODA. Wyciągnięcie ich do jednej funkcji jest oczywiste i należy do właściciela tego
   * pliku: mandat T-62 na `io.ts` pozwala DOPISAĆ jedną krawędź i mówi wprost, że żadna
   * istniejąca sygnatura nie jest przy tym zmieniana (TASK.md, „Wąskie mandaty na cudze
   * pliki"). Wspólny szew ruszyłby ciała `start` i `openChat`, czyli dokładnie to, przed czym
   * ten mandat stoi.
   *
   * 2026-09-02 — JEDNEJ RZECZY JUŻ TUTAJ NIE MA: własnego licznika stempla. Był czwartym
   * liczącym od 1, a wszystkie cztery pompy piszą do tej samej historii terminalu, więc numery
   * się w niej powtarzały. Wydaje je dziś `nextStamp` (`src/state/run.ts`) i to jest jedyna
   * kopia, która z tej czwórki zniknęła. */
  const session = runFor(folder);
  const view = feedFor(folder ?? '');
  const lines = new Channel<unknown[]>();
  wireChannel(lines, (batch) => {
    const at = Date.now();
    const stamped = batch.map((line) => ({ ...line, id: nextStamp(), at }));
    view.appendLines(stamped);
    session.getState().appendLines(stamped);
  });

  /* PLAN JEST JEDEN I OKNO GO ZNA, zanim Rust cokolwiek powie — tak samo jak przy biegu
   * z pliku. Klucz kroku to IDENTYFIKATOR AGENTA i musi nim być: pasek dopasowuje linie stanu
   * do bloków po tym kluczu (`state/run.ts`, `withStepStates`), a po tamtej stronie ten sam
   * klucz nosi kafelek jednokrokowego planu (`commands::run::plan_ask`). Uuid kroku powstaje
   * w Ruście, więc okno nigdy go nie widziało — pasek stałby na „waiting" do końca biegu. */
  const putBack = whatWasRunning(session);
  session
    .getState()
    .nowRunning(who.name, [{ id: who.id, name: who.name, state: 'pending' }], folder);

  return invoke<void>('run_agent', {
    agent: who.id,
    task,
    howManyAtOnce,
    folder,
    budgetUsd: ceiling,
    lines,
  }).finally(() => {
    /* Bieg zszedł — także wtedy, gdy zszedł odmową Rusta. Bez tego Stop zostaje na ekranie na
     * zawsze i jest kontrolką bez roboty (niezmiennik 16). Powód w całości stoi przy [`start`],
     * razem z tym, dlaczego to jest `finally`, a nie `then`. */
    putBack();
    view.runEnded();
  });
}

/**
 * Stop: zatrzymuje bieg TEGO folderu.
 *
 * Rozwiązuje się dopiero z **dowodem**, że po biegu nic nie żyje — `stop_run` po tamtej stronie
 * wraca po `kill(-pgid, 0) == ESRCH`, nie po wysłaniu sygnału (niezmiennik 6). Ekran, który
 * powie „zatrzymane" wcześniej, kłamie o agencie, który dalej pisze i dalej płaci.
 *
 * 2026-09 (Z-35) — FOLDER DOSZEDŁ I JEST CAŁĄ NAPRAWĄ „karta zabiera pracę sąsiadce". Do tego
 * dnia ta krawędź wołała komendę bez ani jednego argumentu, a tamta strona zatrzymywała wtedy
 * bieg w KAŻDYM żywym workspace. Okno zna ten folder, bo samo je wysłało do `run_workflow`
 * (patrz `invoke` w [`start`]) i trzyma je w `RunState.folder`.
 *
 * @param folder katalog karty, na której naciśnięto Stop, albo `null`. Klucz jedzie ZAWSZE,
 *   także jako `null` — Tauri dopasowuje argumenty po nazwie, a `null` znaczy po tamtej stronie
 *   „sesja bez zakresu", czyli katalog, pod którym wstała aplikacja. Wartość domyślna zostaje,
 *   bo cudze kryteria wołają tę krawędź bez argumentów i nie wolno ich tknąć.
 */
export function stop(folder: string | null = null): Promise<boolean> {
  /* ODDAJE ODPOWIEDŹ, NIE NIC. `false` znaczy „w tym folderze nie było czego zatrzymać"
   * i przychodzi z Rusta, bo tam mieszka zapadka biegu. Okno miało tę odpowiedź u siebie
   * (`workflow !== ''` w sesji zakresu) i bywała nieprawdziwa: gubi ją przeładowanie strony.
   * Powód w całości stoi przy `stop_run` w `src-tauri/src/ipc.rs`. */
  return invoke<boolean>('stop_run', { folder });
}

/**
 * Karta zamknięta: rozmowa tego terminalu schodzi, rozmowy pozostałych kart zostają.
 */
export function closeTerminal(terminal: string): Promise<void> {
  return invoke<void>('close_terminal', { terminal });
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
 * 2026-09 (Z-35) — FOLDER DOSZEDŁ DRUGIM ARGUMENTEM, i to nie jest wygoda. Bez niego tamta
 * strona brała „uchwyt, który ruszył ostatni", więc odpowiedź na punkt kontrolny widoczny na
 * karcie A puszczała dalej bieg z karty B: pytanie z ekranu zostawało bez odpowiedzi, a cudzy
 * bieg ruszał. DRUGIM, nie pierwszym, bo pierwszy jest zajęty przez cudze kryterium.
 *
 * Rozwiązuje się dopiero wtedy, kiedy bieg NAPRAWDĘ ruszył (`wait_until_moving` po tamtej
 * stronie) — tak samo jak Stop wraca dopiero z dowodem. Ekran, który wróci wcześniej, pokazuje
 * człowiekowi dalej stojący bieg tuż po tym, jak ten człowiek go puścił.
 */
export function continueRun(
  answer: string | null = null,
  folder: string | null = null,
): Promise<void> {
  return invoke<void>('continue_run', { folder, answer });
}

/**
 * Powtarza JEDEN krok ostatniego biegu tego workflow — jako nowy bieg, z wejściem tamtego.
 *
 * 2026-08-23 — ZE ZGŁOSZENIA WŁAŚCICIELA: „możemy zrobić restart/re-run danego kroku dowolnego
 * agenta, tego teraz nie ma". Powód jest z rachunku: jego bieg trwał 48 minut i padł na ostatnim
 * sprawdzeniu z przyczyny środowiskowej, a jedynym sposobem poprawienia tego jednego kroku było
 * puszczenie całej dziesiątki od zera.
 *
 * Katalogu biegu NIE podajemy: powstaje w środku planowania i okno nigdy go nie poznaje, więc
 * proszenie go o tę ścieżkę byłoby proszeniem o rzecz, której nie ma. Rust znajduje najnowszy
 * bieg tego workflow w tym workspace sam (`commands::rerun`).
 *
 * Oddaje zdanie do pokazania, kiedy dzisiejszy plik workflow różni się od tego, który wtedy
 * biegł — albo `null`, kiedy graf jest ten sam. „To samo jeszcze raz" i „to samo z twoją
 * poprawką" nie mogą wyglądać identycznie.
 */
export function rerunStep(
  fileName: string,
  step: string,
  howManyAtOnce: number,
  folder: string | null = null,
): Promise<string | null> {
  return asARun(fileName, fileName, folder, (lines) =>
    invoke<string | null>('rerun_step', {
      fileName,
      step,
      howManyAtOnce,
      /* KLUCZ OBECNY ZAWSZE, TAKŻE JAKO `null`: Tauri dopasowuje argumenty `invoke` po nazwie,
       * a klucz pominięty i klucz pusty to dla tamtej strony dwie różne rzeczy. */
      folder,
      lines,
    }),
  );
}

/**
 * Wznów wskazany bieg z historii od wskazanego kroku — on i wszystko, co graf stawia po nim.
 *
 * 2026-08-23, pytanie właściciela nad ekranem historii: „a z history możemy kontynuować?".
 * Różnica wobec [`rerunStep`] jest z życia, nie z symetrii: bieg, który padł na siódmym kroku
 * z dziesięciu, ma sześć skończonych, których nikt nie chce powtarzać, i trzy, które nigdy nie
 * ruszyły. Tamta krawędź powtarza JEDEN kafelek, ta wznawia RESZTĘ GRAFU.
 *
 * `run` jest nazwą katalogu — dokładnie tą, którą historia rysuje w wierszu. Ścieżki tu nie ma
 * i być nie może: bieg czyta przekazania z katalogu, który dostanie, a ścieżka z okna byłaby
 * drogą do czytania cudzych.
 *
 * Nazwy pliku workflow NIE podajemy: wiersz historii mówi, co biegło, a nie w którym pliku ten
 * graf dziś leży — plik można było przemianować. Rust idzie po identyfikatorze zapisanym
 * w `run.json` (`commands::rerun::onward`).
 *
 * Oddaje zdanie do pokazania, kiedy dzisiejszy plik różni się od tego, który wtedy biegł.
 */
export function resumeRun(
  run: string,
  step: string,
  howManyAtOnce: number,
  folder: string | null = null,
  /** Nazwa dla paska — tytuł biegu, który wznawiamy. */
  name = '',
  /** Plik workflow, jeżeli okno go zna. Patrz [`asARun`]. */
  fileName = '',
): Promise<string | null> {
  return asARun(name, fileName, folder, (lines) =>
    invoke<string | null>('resume_run', {
      run,
      step,
      howManyAtOnce,
      /* KLUCZ OBECNY ZAWSZE, TAKŻE JAKO `null`: Tauri dopasowuje argumenty `invoke` po nazwie,
       * a klucz pominięty i klucz pusty to dla tamtej strony dwie różne rzeczy. */
      folder,
      lines,
    }),
  );
}

/**
 * Puszcza cały zestaw z sekcji Lab — i jest to **zwykły bieg**.
 *
 * Ta sama zapadka, ten sam strumień linii, ten sam pasek żywego biegu i ten sam Stop, co przy
 * każdym innym starcie: wchodzi tą samą drogą, co wznowienie i powtórzenie kroku
 * (`asARun`). Własne okablowanie kanału w sekcji Lab byłoby trzecią kopią odpowiedzi na
 * pytanie „co robi okno, kiedy bieg rusza" — a ta odpowiedź rozjechała się już raz i skończyła
 * się biegiem, którego nie dało się zatrzymać.
 *
 * Nazwy pliku workflow nie podajemy: plan powstaje przy każdym uruchomieniu na nowo, obok
 * zestawu, i nie jest workflow, który człowiek otwiera z listy. Pusty napis znaczy „uruchom
 * ten krok jeszcze raz" odmówi przy tym biegu — i tak ma być, bo powtórzenie jednej komórki
 * należy do tabeli, nie do paska.
 *
 * @param set identyfikator zestawu — nazwa jego pliku bez rozszerzenia.
 * @param name nazwa dla paska: to, co człowiek zobaczy nad blokami kroków.
 */
export function runEvalSet(
  set: string,
  howManyAtOnce: number,
  folder: string | null = null,
  name = '',
  budgetUsd?: number | null,
): Promise<string | null> {
  /* Sufit wydatku jedzie tą samą drogą, co przy Starcie: jest faktem CAŁEJ aplikacji, a nie
   * ustawieniem tej jednej sekcji. Macierz jest zresztą tym miejscem, które najbardziej go
   * potrzebuje — dziewięć przypadków razy trzy kolumny to dwadzieścia siedem tur. */
  const ceiling = theCeilingFor(budgetUsd, null);
  return asARun(name, '', folder, (lines) =>
    invoke<void>('run_eval_set', {
      /* KLUCZ OBECNY ZAWSZE, TAKŻE JAKO `null`: Tauri dopasowuje argumenty `invoke` po nazwie,
       * a klucz pominięty i klucz pusty to dla tamtej strony dwie różne rzeczy. */
      folder,
      set,
      howManyAtOnce,
      budgetUsd: ceiling,
      lines,
    }).then(() => null),
  );
}

/**
 * Co zrobić z paskiem żywego biegu, kiedy start, który go nadpisał, **nigdy nie ruszył**. already going… Press Stop first"), a zaraz pod spodem `/stop` odpowiada
 * **„Nothing is running."** — o biegu, który w tej chwili pracował już czterdzieści minut.
 * Odmowa nazywa następny ruch, a ten ruch nie istnieje: z tego wiersza nie dało się już
 * zatrzymać niczego.
 *
 * PRZYCZYNA NIE JEST W STOPIE. Każdy start pisze do sesji „teraz biegnie to" **przed** `invoke`,
 * bo komenda po tamtej stronie trwa tyle, co bieg. `/ask` nie ma przy tym zapadki `going`
 * i ma jej nie mieć (powód stoi przy [`askOneAgent`]) — więc dochodzi do Rusta, dostaje odmowę,
 * a jego `finally` gasi wpis biegu. Zdanie „bieg zszedł" jest wtedy prawdziwe
 * o biegu, który nie ruszył, i **fałszywe o tym, który pracuje**: obu dotyczy jeden wpis
 * w jednej sesji zakresu. Od tej chwili okno uważa, że nic nie biegnie, a Stop znika.
 *
 * # Skąd wiemy, czy ODTWORZYĆ, czy ZAKOŃCZYĆ
 *
 * `finally` nie odróżnia odmowy od porażki w połowie biegu, a rozdzielanie tego na `then`/`catch`
 * dawałoby dwie drogi do jednej odpowiedzi. Rozstrzyga migawka sprzed próby: niepuste
 * `workflow` znaczy, że próba dostała odmowę nad cudzym biegiem i musi go odtworzyć; puste
 * znaczy, że próba była właścicielem sesji i jej plan ma zostać oznaczony jako skończony.
 * Rust nie wpuści dwóch biegów w jeden folder, więc trzeciego przypadku nie ma.
 */
function whatWasRunning(session: RunStore): () => void {
  const before = session.getState();
  const kept = {
    workflow: before.workflow,
    steps: before.steps,
    folder: before.folder,
    fileName: before.fileName,
    /* 2026-08-31 — STRZAŁKI WRACAJĄ RAZEM Z RESZTĄ. Odtworzenie, które by je pominęło,
     * zostawiałoby po odmowie `/ask` pasek opisujący ŻYWY bieg jako listę kroków bez ani jednej
     * relacji — czyli ten sam bieg pokazany jako coś innego, niż jest. */
    links: before.links,
    /* Flaga jedzie z całą migawką: odtworzenie ma być dokładne także po dołożeniu stanu
     * skończonego biegu, a nie opierać się na założeniu, że dziś przy żywym jest zawsze pusta. */
    ended: before.ended,
  };
  return () => {
    if (kept.workflow === '') {
      /* Ten start był właścicielem sesji: Rust mógł go wpuścić wyłącznie wtedy, gdy wcześniej
       * nic nie biegło. Zdejmujemy żywość, ale zostawiamy jego końcowy plan (Z-37). */
      session.getState().runEnded();
      return;
    }
    /* Odmówiony `/ask` nad cudzym żywym biegiem nie kończy go. Cała migawka wraca jednym
     * tyknięciem, więc Stop i kafelki nie widzą pośredniego pustego stanu (niezmiennik 13). */
    session.setState(kept);
  };
}

/**
 * Bieg, który NIE zaczyna się od Startu — a poza tym jest biegiem jak każdy inny.
 *
 * 2026-08-23 — POWSTAŁO Z DEFEKTU ZE ZRZUTU WŁAŚCICIELA: nacisnął `/stop` nad pracującym
 * agentem i dostał **„Nothing is running."**, a krok pracował dalej. Przyczyna: `rerunStep`
 * i `resumeRun` wpinały kanał linii i nic poza tym. Nie mówiły magazynowi, że bieg ruszył —
 * a „czy coś biegnie" to w całej aplikacji dokładnie `workflow !== ''` (`state/run.ts`), z czego
 * żyje przycisk Stop, `/stop` w wierszu i pasek żywych biegów. Bieg, którego nie da się
 * zatrzymać, jest gorszy od biegu, który padnie.
 *
 * Nie brały też ZAPADKI, więc drugi bieg dało się na nie położyć — czyli dwa biegi w jednym
 * folderze, dokładnie to, przed czym stoi niezmiennik 12.
 *
 * Jedna funkcja na obie drogi, i to jest ten sam powód, dla którego stoi w tym pliku: „co robi
 * okno, kiedy bieg rusza" jest jednym faktem, a trzy kopie tego faktu rozjechały się już raz.
 * Start ma własne ciało tylko dlatego, że wysyła inne argumenty i ma inne zdanie odmowy.
 *
 * @param name nazwa dla paska — ta, którą człowiek zobaczy nad blokami kroków.
 * @param fileName plik workflow, albo `''`, kiedy okno go nie zna. Puste znaczy, że „uruchom ten
 *   krok jeszcze raz" odmówi przy tym biegu po nazwie, zamiast zgadywać plik.
 */
function asARun(
  name: string,
  fileName: string,
  folder: string | null,
  send: (lines: Channel<unknown[]>) => Promise<string | null>,
): Promise<string | null> {
  if (aRunIsGoing()) return Promise.reject(ONE_RUN_AT_A_TIME);

  const session = runFor(folder);
  const view = feedFor(folder ?? '');
  const lines = new Channel<unknown[]>();
  wireChannel(lines, (batch) => {
    const at = Date.now();
    /* JEDEN LICZNIK NA CAŁE OKNO, ten sam, co przy Starcie — powód stoi przy `nextStamp`
     * (`src/state/run.ts`): wznowienie i powtórzenie kroku wchodzą do historii, w której stoi
     * już rozmowa tego terminalu. */
    const stamped = batch.map((line) => ({ ...line, id: nextStamp(), at }));
    view.appendLines(stamped);
    session.getState().appendLines(stamped);
  });

  /* PRZED `invoke`, nie po nim — ten sam powód, co przy Starcie: komenda po tamtej stronie trwa
   * tyle, co bieg, więc zapis po jej powrocie ogłaszałby start w chwili, w której bieg się
   * właśnie skończył. Kroków nie podajemy: przy wznowieniu okno nie wie z góry, które węzły
   * wejdą do wycinka, a wypełniacz byłby paskiem rysującym bloki, których nie ma
   * (niezmiennik 17). Nadejdą ze strumienia. */
  const putBack = whatWasRunning(session);
  session.getState().nowRunning(name, [], folder, fileName);

  const run = send(lines).finally(() => {
    letTheRunGo();
    // Odtworzenie, nie zerowanie; powód stoi przy [`whatWasRunning`].
    putBack();
    view.runEnded();
  });
  holdTheRun(run);
  return run;
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
 * 2026-09 (Z-35) — FOLDER DOSZEDŁ TRZECIM ARGUMENTEM, bo bez niego „ten jeden, który pracuje"
 * znaczyło „gdziekolwiek". Tamta strona brała uchwyt biegu, który ruszył ostatni, więc zdanie
 * wpisane na karcie A szło do agenta z karty B — tura, za którą ktoś płaci, trafiała do kogoś
 * innego, niż widać na ekranie.
 *
 * @param text co człowiek napisał. Puste odmawia po tamtej stronie — nie zgadujemy tu, co znaczy
 *   pusty Enter.
 * @param agent nazwa kroku, do którego mówimy, albo `null`. `null` znaczy „ten jeden, który
 *   pracuje **w tym folderze**": przy dwóch i więcej Rust odmawia z listą nazw, zamiast wysyłać
 *   do losowego.
 * @param folder katalog karty, z której to zdanie wyszło, albo `null`. Klucz jedzie ZAWSZE —
 *   powód w całości stoi przy `invoke` w [`start`].
 */
export function sayToAgent(
  text: string,
  agent: string | null = null,
  folder: string | null = null,
): Promise<void> {
  return invoke<void>('say_to_agent', { folder, agent, text });
}

/**
 * Tożsamość terminalu, do której należy ta rozmowa — z tego, co przysłał wołający.
 *
 * FOLDER NAZYWA DOMYŚLNY TERMINAL SWOJEGO ZAKRESU, i ta jedna reguła stoi w trzech miejscach
 * naraz, w każdym po swojej stronie tej samej granicy: tutaj, w rejestrze strumienia
 * (`./feed/live.ts`, `shown`) i po stronie Rusta (`commands::chat::key_of`). Nie jest to trzy razy
 * przepisana polityka, a jedna wartość policzona tam, gdzie ją widać — okno zna kartę, Rust dostaje
 * jej nazwę gotową. Wersja bez tej reguły oddawałaby historię do sesji `''`, kiedy zakres jest
 * wybrany, a karty jeszcze nie ma — czyli wiersze rozmowy trafiałyby do widoku, na który nikt nie
 * patrzy.
 *
 * Pusty napis znaczy „ani karty, ani zakresu": Rust bierze wtedy katalog, pod którym wstała
 * aplikacja (`AppState::project_for`), i to też jest jedna, konkretna rozmowa.
 */
function terminalOf(terminal: string | null, folder: string | null): string {
  return terminal ?? folder ?? '';
}

/**
 * Otwiera strumień rozmowy z liderem TEGO terminalu — bez uruchamiania programu.
 *
 * # Po co osobne otwarcie, a nie jedno wywołanie z tekstem
 *
 * Bo kanał do okna umie zbudować **tylko okno** (`docs/ARCHITECTURE.md` §3, §4), więc musi wejść
 * argumentem — a rozmowy u dostawcy nie wolno tu wstawiać: tura wystartowana przy montażu ekranu
 * jest turą, za którą ktoś płaci, choć nikt o nic nie zapytał. Ta krawędź zakłada więc pompę,
 * a lider wstaje dopiero przy pierwszym zdaniu (`say_to_orchestrator`).
 *
 * # Gdzie lądują te wiersze
 *
 * W TYM SAMYM strumieniu, co bieg: rozmowa o tym, co ma się stać, i praca, która się dzieje, są
 * jedną historią tego miejsca. Dlatego zapis idzie przez `feedFor(...)` i `runFor(folder)` —
 * tą samą drogą i tym samym stemplem, co paczki biegu (patrz `start`), bo dwie drogi do jednego
 * widoku dałyby dwa porządki wierszy i pierwszy sklejony wiersz by je rozjechał.
 *
 * @param folder katalog, w którym rozmowa ma patrzeć, albo `null`.
 * @param terminal karta, do której ta rozmowa należy, albo `null` — wtedy odpowiada folder
 *   ([`terminalOf`]). Argument opcjonalny, bo lustro komend (`src/sections/commands-wired.test.ts`)
 *   woła tę krawędź jednym argumentem i nie wolno go tknąć; klucz jedzie jednak ZAWSZE, bo
 *   pominięty klucz to wywołanie odrzucone, nie mniejsze.
 */
export function openChat(
  folder: string | null = null,
  terminal: string | null = null,
): Promise<void> {
  const session = runFor(folder);
  const at = terminalOf(terminal, folder);
  const view = feedFor(at);
  const lines = new Channel<unknown[]>();
  wireChannel(lines, (batch) => {
    const now = Date.now();
    /* TEN SAM LICZNIK, CO PRZY BIEGU, i to jest cała naprawa z 2026-09-02: ekran Pracy woła tę
     * krawędź przy KAŻDYM montażu (`./index.tsx`), a licznik zerowany razem z pompą wydawał przy
     * każdym powrocie numery, które w tej historii już stały. Powód przy `nextStamp`. */
    const stamped = batch.map((line) => ({ ...line, id: nextStamp(), at: now }));
    view.appendLines(stamped);
    session.getState().appendLines(stamped);

    /* LIDER, KTÓRY POPROSIŁ O START, DOSTAJE START — rozstrzygnięcie właściciela 2026-08-30
     * („rusza samo"). Wiersz jest już NA EKRANIE, zanim cokolwiek ruszy, i to jest jedyna
     * ochrona człowieka przy tej decyzji: widzi, co się zaczyna, w tej samej sekundzie.
     *
     * Ta sama droga, co Enter i co przycisk propozycji (`runSuggestion` → `startFromLine`), bo
     * „który workflow, ile naraz, w którym folderze" ma jedną odpowiedź (niezmiennik 23). Odmowa
     * ląduje w strumieniu tego terminalu — porzucona byłaby biegiem, który nie ruszył, i ciszą
     * zamiast powodu. */
    for (const going of autoStarts(stamped)) {
      /* FOLDER TEJ ROZMOWY JEDZIE DALEJ (2026-09, Z-39), bo od dziś wierszem z mostu bywa `/stop`,
       * a on ADRESUJE bieg: bez adresu zatrzymanie idzie w katalog, pod którym wstała aplikacja,
       * czyli potrafi zdjąć cudzy bieg z sąsiedniej karty — dokładnie ta wada, którą Z-35
       * zamknęło po stronie przycisku Stop. */
      void runSuggestion(going.command, folder).then((refusal) => {
        if (refusal !== null) {
          view.appendLines([
            { kind: 'note', agent: going.agent, text: refusal, id: nextStamp(), at: Date.now() },
          ]);
        }
      });
    }
  });
  return invoke<void>('open_chat', { terminal: at, folder, lines });
}

/**
 * Człowiek odpowiedział na pytanie przypięte w tym terminalu.
 *
 * # Dlaczego okno woła to przy KAŻDEJ odpowiedzi
 *
 * Bo nie ma jak rozstrzygnąć, do kogo należy przypięte pytanie: w jednym strumieniu stoi pytanie
 * lidera (tura zablokowana na wywołaniu narzędzia) i pytanie kafelka kontrolnego (bieg stojący na
 * punkcie). Rozstrzyga strona, która wie — Rust porównuje PODPIS pytającego i oddaje `false`,
 * kiedy w tym terminalu nikt na to nie czekał.
 *
 * `false` jest więc **odpowiedzią, nie błędem**: okno idzie wtedy swoją dotychczasową drogą,
 * a odpowiedź na punkt kontrolny jedzie tak, jak jechała.
 *
 * @param terminal karta, w której stoi pytanie — ta sama tożsamość, co przy `open_chat`.
 * @param agent podpis, pod którym pytanie stanęło na ekranie. Bez niego odpowiedź na kafelek
 *   odblokowywałaby przy okazji cudze pytanie, zdaniem, które go nie dotyczy.
 */
export function answerTheLead(
  terminal: string | null,
  folder: string | null,
  agent: string,
  answer: string,
): Promise<boolean> {
  return invoke<boolean>('answer_the_lead', {
    terminal: terminalOf(terminal, folder),
    agent,
    answer,
  });
}

/**
 * Co się stało z prośbą o przerwanie tury — lustro `commands::chat::InterruptedTheLead`.
 *
 * DWA FAKTY, NIE GOTOWE ZDANIE, i to jest ten sam powód, co przy [`WhatTheLeadCanDo`] niżej:
 * zdanie jest po angielsku i mieszka w oknie (decyzja D5), a to, co się stało z prośbą, jest
 * faktem o protokole i mieszka tam, gdzie ten protokół powstaje.
 */
export interface Interrupted {
  /** Co się stało: prośba pojechała, to CLI tego nie umie, albo nie ma już czego przerywać. */
  readonly answer: 'sent' | 'notAnnounced' | 'noLongerListening';
  /** Aplikacja agenta, którą ta rozmowa prowadzi. Pusto, kiedy nie ma o kim mówić. */
  readonly agentApp: string;
}

/**
 * Poproś turę lidera tego terminalu, żeby stanęła. **Nie kończy rozmowy.**
 *
 * # Po co osobna krawędź, a nie `closeTerminal` (2026-09, Z-40)
 *
 * Bo to są dwie różne prośby i dwa różne skutki. Zamknięcie karty dowodzi śmierci grupy i zabiera
 * cały kontekst, który człowiek z liderem zbudował; tutaj staje jedna tura, a rozmowa zostaje
 * wznawialna — i wiadomość, która czekała za tą komendą, idzie następna.
 *
 * ODPOWIEDŹ WRACA I MA STANĄĆ NA EKRANIE: CLI, które przerwania nie ogłosiło, mówi to zdaniem
 * w miejscu przycisku (niezmiennik 29). Przycisk, po którym nic się nie dzieje i nic tego nie
 * tłumaczy, jest gorszy niż jego brak.
 *
 * @param terminal karta, w której stoi ta rozmowa, albo `null` — wtedy odpowiada folder
 *   ([`terminalOf`]).
 * @param folder katalog tej karty albo `null`; nazywa domyślny terminal zakresu.
 */
export function interruptTheLead(
  terminal: string | null,
  folder: string | null = null,
): Promise<Interrupted> {
  return invoke<Interrupted>('interrupt_the_lead', { terminal: terminalOf(terminal, folder) });
}

/**
 * Powiedz zdanie liderowi tego terminalu — rozmowa, nie praca.
 *
 * LIDER NIE URUCHAMIA BIEGU I NIE MA JAK. Rozstrzygnięcie właściciela 2026-08-19: „tylko
 * komendy determinują akcje workflow". Po tamtej stronie nie jest to prośba w promptcie
 * systemowym, a własność struktury — `commands::chat` nie zna ani biegu, ani jego bazy.
 *
 * @param folder katalog, w którym rozmowa ma patrzeć — ścieżka aktywnego zakresu albo `null`.
 *   Klucz jest obecny zawsze, także jako `null`: Tauri dopasowuje argumenty PO NAZWIE
 *   i deserializuje je PRZED wejściem w ciało komendy, więc brakujący klucz odrzuca wywołanie.
 * @param terminal karta, która to mówi, albo `null` — wtedy odpowiada folder ([`terminalOf`]).
 *   Bez tego klucza dwie karty jednego projektu dostałyby JEDNĄ rozmowę: człowiek pisze w lewej,
 *   a odpowiedź pojawia mu się w prawej.
 * @param lead identyfikator zapisanego agenta, którego człowiek wskazał na lidera, albo `null`.
 *   `null` jest po tamtej stronie **odmową nazywającą następny ruch**, nigdy cichym powrotem do
 *   zaszytego vendora: rozmowa, która idzie, płaci i odpowiada nie tym agentem, którego człowiek
 *   wybrał, nie ma ani jednego sygnału, po którym dałoby się to zauważyć.
 * @param images obrazy w kolejności podglądów, już bez nazw plików. Pusta tablica jedzie jawnie:
 *   Tauri dopasowuje argumenty przed wejściem do komendy, więc „brak obrazów" i brak klucza to
 *   nie są dwa zapisy tego samego wywołania.
 */
export function sayToOrchestrator(
  text: string,
  folder: string | null = null,
  terminal: string | null = null,
  lead: string | null = null,
  images: readonly ConversationImage[] = [],
): Promise<void> {
  return invoke<void>('say_to_orchestrator', {
    terminal: terminalOf(terminal, folder),
    folder,
    lead,
    text,
    images,
  });
}

/**
 * Co wskazany lider naprawdę może — lustro `commands::chat::WhatTheLeadCanDo`.
 *
 * TRZY FAKTY, NIE GOTOWE ZDANIE. Zdanie jest po angielsku i mieszka w oknie razem z resztą
 * tekstu (decyzja D5); to, co lider może, jest faktem o argv i mieszka tam, gdzie to argv
 * powstaje. Napis złożony po tamtej stronie granicy byłby drugim domem języka interfejsu.
 */
export interface WhatTheLeadCanDo {
  /** Czy pod ręką ma cokolwiek, czym zmienia się plik (`Edit`, `Write`). */
  readonly changesFiles: boolean;
  /** Czy może uruchomić komendę (`Bash`, a u Codeksa sama piaskownica). */
  readonly runsCommands: boolean;
  /** Czy to, co zmienia, kończy się na folderze, w którym człowiek pracuje. */
  readonly heldToTheFolder: boolean;
}

/**
 * Czego wolno się spodziewać po liderze wskazanym w pasku — zanim padnie pierwsze zdanie.
 *
 * # Po co osobna krawędź, a nie pole w odpowiedzi na pierwsze zdanie (2026-09, Z-50)
 *
 * Bo zdanie pod polem stoi na ekranie ZANIM ktokolwiek naciśnie Enter, a odpowiedź lidera
 * kosztuje turę. Ostrzeżenie przychodzące razem z pierwszą płatną odpowiedzią jest ostrzeżeniem
 * po fakcie: człowiek zdążył już napisać zdanie, nie wiedząc, komu je oddaje.
 *
 * @param lead identyfikator zapisanego agenta, na którego człowiek wskazał, albo `null`. `null`
 *   jest po tamtej stronie odmową nazywającą następny ruch — tą samą, którą dostanie przy
 *   pierwszym Enterze ([`sayToOrchestrator`]) — a nie cichym „nic nie może".
 */
export function whatTheLeadCanDo(lead: string | null = null): Promise<WhatTheLeadCanDo> {
  return invoke<WhatTheLeadCanDo>('what_the_lead_can_do', { lead });
}

/**
 * Jeden bieg z historii TEGO folderu, tak jak przyjeżdża z Rusta.
 *
 * Lustro `commands::history::RunWire`. Ręcznie, jak `src/ipc/types.ts` — powód i cena stoją
 * tam; tutaj dochodzi jeden fakt: kryterium szwu (`src/sections/commands-wired.test.ts`)
 * wykonuje tę krawędź naprawdę, więc klucz, który by się rozjechał, jest widoczny.
 */
export interface PastRunRow {
  /** Nazwa katalogu biegu — adres, którym prosi się o niego z powrotem. Nigdy napis na ekranie. */
  readonly folder: string;
  /** Kiedy ruszył, gotowe do przeczytania: `2026-08-16 19:48`. */
  readonly when: string;
  /** Jak workflow nazywa sam siebie. Pusty, kiedy Rust nie dał rady przeczytać opisu. */
  readonly title: string;
  /** Dzisiejsza nazwa pliku workflow, albo pusty napis, kiedy pliku nie ma już w bibliotece. */
  readonly workflowFile: string;
  /** Słowo z drutu (`succeeded`, `failed`, …). Tłumaczy je `./history-command.ts`. */
  readonly state: string;
  /** Ile kroków miał ten bieg. */
  readonly steps: number;
  /** Ile kosztował, albo `null` — a to jest inne zdanie niż zero (niezmiennik 17). */
  readonly costUsd: number | null;
  /** Uczciwe zdanie, kiedy opisu biegu nie dało się przeczytać. `null` znaczy „przeczytany". */
  readonly said: string | null;
  /**
   * Co prywatna tura Loadouta zrobiła z tym biegiem.
   *
   * KLUCZ OPCJONALNY (niezmiennik 5 na granicy), choć dzisiejszy Rust wysyła go zawsze: bieg
   * zapisany przed tym polem ma się dać wypisać, a nie wywrócić listy. Czyta go sekcja
   * Knowledge, żeby powiedzieć, dlaczego po ostatnim biegu nie przyszła ani jedna notatka
   * (2026-09, Z-38).
   */
  readonly reflection?: PastReflection | null;
}

/** Krok otwartego biegu. Lustro `commands::history::PastStepWire`. */
export interface PastStep {
  /** Identyfikator kroku w TYM biegu. Unikalny w biegu i tylko w nim — nie wskazuje kafelka. */
  readonly id: string;
  /** Klucz kafelka z pliku workflow. Tym wznawia się bieg od tego miejsca; pusty znaczy, że
   * `run.json` nie mówi, z którego kafelka ten krok powstał. */
  readonly tile: string;
  readonly name: string;
  readonly agent: string;
  readonly state: string;
  /** Jedno zdanie, które ten krok po sobie zostawił. Puste, kiedy żadnego nie zostawił. */
  readonly summary: string;
  /** Powód, jeśli coś poszło nie tak. */
  readonly error: string;
  /**
   * Czyjego wyniku ten krok nie miał, choć pojechał dalej — po jednym zdaniu na poprzednika.
   *
   * OSOBNE OD `error`, bo mówi o czym innym: tamto jest powodem, dla którego TEN krok nie
   * przeszedł, a to jest zdaniem o materiale, którego nie dostał. Krok, który pojechał dalej mimo
   * poprzednika ubitego z zewnątrz, ma `error` puste i to zdanie niepuste.
   *
   * KLUCZ OPCJONALNY, i to jest niezmiennik 5 postawiony na granicy: każdy `run.json` zapisany
   * przed 2026-09 go nie ma. Brak czyta się jak pusta lista — czyli „ten krok dostał wszystko,
   * po co przyszedł", co jest prawdą także wtedy, gdy nikt nie umiał zapytać.
   */
  readonly ranWithout?: readonly string[];
  readonly costUsd: number | null;
  /** Zdanie policzone przez Rust z tego samego słownika, który zasila widoczny wiersz.
   * Opcjonalne tylko dla starszego drutu i zapisanych fixture'ów. */
  readonly contextPerTurn?: string | null;
  /**
   * Zamrożone notatki przypięte przez Rust do fizycznego UUID tego kroku.
   *
   * Opcjonalne wyłącznie dla zgodności ze starszym drutem i lokalnymi fixture'ami; dzisiejszy
   * `read_run` wysyła zawsze listę, także pustą.
   */
  readonly memory?: readonly PastMemory[];
  /**
   * Co aplikacja agenta wczytała z folderu, zanim ten krok powiedział pierwsze słowo.
   *
   * KLUCZ OPCJONALNY, i to jest niezmiennik 5 postawiony na granicy: każdy `run.json` zapisany
   * przed 2026-09 go nie ma, tak samo jak krok bez agenta i krok, który nie zdążył się
   * przedstawić. Brak znaczy „nie wiemy", nigdy „nic nie wczytał" — i wtedy ekran nie mówi o tym
   * ani słowa (niezmiennik 16).
   */
  readonly loadedByTheApp?: LoadedByTheApp | null;
  /** Zapisany strumień tego kroku — te same wiersze, które widać było na żywo. */
  readonly lines: readonly Line[];
}

/**
 * Co aplikacja agenta wczytała z folderu kroku sama z siebie. Lustro
 * `commands::history::LoadedByTheAppWire`.
 *
 * NIE MYLIĆ Z `PastMemory`: tamto jest tym, co Loadout do kroku WŁOŻYŁ, a to jest tym, co
 * aplikacja agenta dobrała z folderu, w którym akurat stanęła. Dwa różne pytania i dwie różne
 * odpowiedzi — mieszanie ich znaczyłoby ekran, który mówi „Loadout dał", pokazując cudze pliki.
 *
 * KAŻDE POLE TUTAJ TO CYTAT Z APLIKACJI AGENTA, nigdy odczyt dysku po naszej stronie. Pliku
 * instrukcji projektu nie ma na tej liście i nie ma go tam z rozmysłu: granica nie niesie ani
 * jednego pola, po którym dałoby się poznać, że został wczytany (2026-09-04, Z-16).
 */
export interface LoadedByTheApp {
  /** Nazwa folderu, z którego to przyszło. Sama nazwa, nigdy cała ścieżka na dysku człowieka. */
  readonly folder: string;
  readonly plugins: readonly string[];
  readonly slashCommands: readonly string[];
  readonly skills: readonly string[];
  readonly mcpServers: readonly string[];
  readonly memoryPaths: readonly string[];
  readonly agents: readonly string[];
}

/** Jedna pozycja zamrożonego rachunku pamięci kroku. */
export interface PastMemory {
  readonly reference: string;
  readonly hash: string;
  readonly bytes: number;
  readonly address: {
    /** Surowa wartość drutu; ekran jej nie pokazuje ani nie wyprowadza z niej pochodzenia. */
    readonly place: string;
    readonly id: string;
  };
  /** Projekt importu i bieg refleksji są rozdzielonymi, niezgadywanymi faktami. */
  readonly project: string | null;
  readonly from: string | null;
  /** `true` znaczy: pasowała do kroku, lecz ówczesny limit ją odłożył. */
  readonly leftOut: boolean;
}

/** Przekazanie, tak jak widzi je okno. Lustro `commands::handoffs::HandoffWire`. */
export interface PastHandoff {
  readonly from: string;
  readonly to: readonly string[];
  readonly title: string;
  readonly kind: string;
}

/**
 * Gałąź, którą ten bieg zostawił. Lustro `commands::history::BranchWire`.
 *
 * DWA POLA, BO CZŁOWIEK POTRZEBUJE OBU. Nazwa jest tym, co wpisze w gita, żeby znaleźć pracę;
 * krok jest tym, po czym pozna, o którą pracę chodzi — nazwy gałęzi jednego biegu różnią się
 * ostatnim członem i czyta się je jak jedną kolumnę tego samego napisu.
 */
export interface PastBranch {
  /** Pełna nazwa gałęzi: `loadout/<bieg>/<kafelek>`. */
  readonly name: string;
  /** Nazwa kroku, który ją zostawił — ta z kafelka. Pusta, kiedy `run.json` już go nie zna. */
  readonly step: string;
}

/**
 * Co prywatna tura Loadouta zrobiła z tym biegiem. Lustro `commands::history::ReflectionWire`.
 *
 * CZTERY LICZNIKI, BO TYLE ZAPISUJE `run.json` (`commands::run::ReflectionReceipt`). Ceny tej
 * tury tu nie ma i jest to zapisany dług: chip na pasku sumuje wyłącznie koszty kroków, więc
 * opłacona tura refleksji jest dziś niewidoczna na każdym ekranie.
 */
export interface PastReflection {
  /** Czy tura naprawdę poszła i wróciła użyteczną odpowiedzią. `false` znaczy „nie pytano". */
  readonly ran: boolean;
  /** Kod powodu, dla którego nie pytano. Brak zachowuje czytelność starszych plików. */
  readonly why?: string | null;
  /**
   * Sufit ceny, który tę turę obowiązywał — w dolarach.
   *
   * KLUCZ OPCJONALNY (niezmiennik 5 na granicy): bieg zapisany przed 2026-09 nie niesie ani
   * tej liczby, ani powodu, który ją cytuje, a panel historii ma się otworzyć tak samo.
   */
  readonly budgetUsd?: number | null;
  /** Ile notatek z niej powstało — te czekają w Memory na decyzję człowieka. */
  readonly kept: number;
  /** Ile wróciło takich, które człowiek już raz odrzucił. */
  readonly discardedAgain: number;
  /** Ile reguł przyszło bez uzasadnienia — takich nie zapisujemy [T6 §10.3]. */
  readonly droppedWithoutReason: number;
}

/** Otwarty bieg z historii. Lustro `commands::history::PastRunWire`. */
export interface PastRun {
  /** Nazwa dzisiejszego pliku workflow tego biegu — pusta, kiedy nie ma go już w bibliotece. */
  readonly workflowFile: string;
  readonly folder: string;
  readonly when: string;
  readonly title: string;
  readonly state: string;
  readonly steps: readonly PastStep[];
  readonly handoffs: readonly PastHandoff[];
  /**
   * Dlaczego przekazań nie dało się przeczytać, albo `null`, kiedy pusta lista jest prawdziwa.
   *
   * Klucz opcjonalny jak `branches`: starszy Rust go nie wysyła, a historia z takiego builda
   * nadal ma się otworzyć i zachować dotychczasowe znaczenie pustej listy (niezmiennik 5).
   */
  readonly handoffsSaid?: string | null;
  /**
   * Gałęzie, które ten bieg zostawił w repozytorium projektu.
   *
   * KLUCZ OPCJONALNY, choć dzisiejszy Rust wysyła go zawsze, i to jest niezmiennik 5 postawiony
   * na granicy: opis przysłany przez Loadouta, który o gałęziach jeszcze nie wie, ma się dać
   * przeczytać, a nie wywrócić panel historii. Brak klucza czyta się jak pusta lista — czyli
   * „ten bieg nic nie zostawił", co jest prawdą także wtedy, gdy nikt nie umiał zapytać.
   */
  readonly branches?: readonly PastBranch[];
  /**
   * Co prywatna tura Loadouta zrobiła z tym biegiem, albo `null` — kiedy jego opis o tym milczy.
   *
   * KLUCZ OPCJONALNY z dokładnie tego samego powodu, co `branches` wyżej (niezmiennik 5 na
   * granicy), ale `null` znaczy tu co innego niż brak klucza w `branches`: bieg zapisany przed
   * tym polem NIE JEST biegiem, którego nie pytano — i ekran ma te dwa stany rozróżniać
   * (`./reflection/said.ts`).
   */
  readonly reflection?: PastReflection | null;
  readonly said: string | null;
}

/**
 * Co ten folder do tej pory uruchomił — od najnowszego.
 *
 * FOLDER JEST JEDYNYM ZAKRESEM i to jest cały warunek właściciela („wszystko ma być per
 * workspace ta historia"). `null` zostaje jawne, żeby Rust mógł wziąć katalog, pod którym
 * wstała aplikacja (`AppState::project_for`), zamiast żeby okno podstawiało własną domyślną
 * ścieżkę — druga odpowiedź na pytanie „gdzie pracujemy" jest tą, która się rozjedzie.
 *
 * Nie odmawia z powodu jednego nieczytelnego biegu: taki wraca jako wiersz z uczciwym zdaniem
 * (`commands::history`, nagłówek modułu).
 */
export function listRuns(folder: string | null): Promise<readonly PastRunRow[]> {
  return invoke<readonly PastRunRow[]>('list_runs', { folder });
}

/**
 * Jeden bieg z historii, otwarty DO ODCZYTU.
 *
 * @param folder zakres, w którym ten bieg leży — ta sama ścieżka, którą dostało [`listRuns`].
 * @param run nazwa katalogu z `PastRunRow.folder`. Sprawdza ją Rust, zanim dotknie dysku:
 *   ten napis potrafi przyjechać z linii, którą wpisał człowiek.
 */
export function readRun(folder: string | null, run: string): Promise<PastRun> {
  return invoke<PastRun>('read_run', { folder, run });
}

/**
 * Zdejmuje gałęzie, które ten bieg zostawił — i **tylko** jego.
 *
 * Oddaje nazwy tych, których już nie ma. Rust odmawia całości, kiedy którakolwiek z nich jest
 * w tej chwili otwarta do pracy w innym folderze: zdjęcie jej spod czyjejś ręki jest jedyną
 * rzeczą, którą ta droga mogłaby zepsuć nieodwracalnie.
 *
 * @param folder zakres, w którym ten bieg leży — ta sama ścieżka, którą dostało [`readRun`].
 * @param run nazwa katalogu z `PastRunRow.folder`.
 */
export function forgetRunBranches(folder: string | null, run: string): Promise<readonly string[]> {
  return invoke<readonly string[]>('forget_run_branches', { folder, run });
}

/**
 * Zdejmuje CAŁY bieg: jego gałęzie i jego folder. Oddaje nazwy zdjętych gałęzi.
 *
 * 2026-09 (Z-9) — POWSTAŁO Z DRUGIEJ POŁOWY TEGO, CO ZOSTAWAŁO NA ZAWSZE. Gałęzie biegu dało się
 * zdjąć od 2026-08-23 ([`forgetRunBranches`]); jego folder — ze wszystkim, co bieg w nim zapisał —
 * nie schodził niczym. Zmierzone u właściciela 2026-09-02: 87 folderów biegów w jednym projekcie,
 * 3,8 GB, a jedyną drogą był terminal.
 *
 * Ta sama ostrożność, co przy gałęziach, i ta sama całościowość: kiedy którakolwiek gałąź tego
 * biegu jest w tej chwili otwarta do pracy w innym folderze, Rust nie zdejmuje ANI JEDNEJ rzeczy —
 * ani gałęzi, ani folderu — i mówi, która to gałąź.
 *
 * @param folder zakres, w którym ten bieg leży — ta sama ścieżka, którą dostało [`readRun`].
 * @param run nazwa katalogu z `PastRunRow.folder`.
 */
export function forgetRun(folder: string | null, run: string): Promise<readonly string[]> {
  return invoke<readonly string[]>('forget_run', { folder, run });
}

/** Co zejdzie razem z biegami starszymi niż tyle dni, ile człowiek podał. */
export interface OlderRuns {
  readonly runs: number;
  readonly branches: number;
  readonly workFolders: number;
  /** Jedno zdanie o tym, co zejdzie. Nigdy puste: „nic" też jest odpowiedzią na to pytanie. */
  readonly said: string;
}

/** Co ten folder mógłby zapomnieć. Lustro `commands::sweep::CouldForgetWire`. */
export interface CouldForget {
  /** Ile katalogów roboczych stoi po biegach, których Loadout nie zamknął. */
  readonly workFolders: number;
  /** Ile gałęzi zostało po biegach, których katalogu już nie ma. */
  readonly branches: number;
  /** Zdanie o obu liczbach. **Pusty napis znaczy „nie ma o czym mówić"** — i wtedy nie ma
   * kontrolki (niezmiennik 16). */
  readonly said: string;
  readonly older: OlderRuns;
}

/** Co naprawdę zeszło — i co nie. Lustro `commands::sweep::ForgottenWire`. */
export interface Forgotten {
  readonly workFolders: number;
  readonly branches: number;
  readonly runs: number;
  /** Co się stało i co ZOSTAŁO — po imieniu i ze ścieżką. To jedyne, co człowiek przeczyta. */
  readonly said: string;
}

/**
 * Co ten folder mógłby zapomnieć: leżaki po starych biegach i biegi starsze niż tyle dni.
 *
 * 2026-09 (Z-46) — POWSTAŁO, BO ZAMIATACZ MILCZAŁ. Sprzątanie przy otwarciu folderu domyka
 * wyłącznie katalogi, o których bieg zostawił notatkę; bieg sprzed tej notatki nie ma jej wcale,
 * więc jego katalog stoi dalej i nic o nim nie mówi. Zmierzone u właściciela 2026-09-03 na jednym
 * monorepo: dziennik zameldował 75 zamkniętych folderów, a dwanaście dalej stało, po 264 MB.
 *
 * **Nic nie kasuje.** To jest wyłącznie liczenie — kasują dwie krawędzie niżej, każda po
 * kliknięciu i każda po zdaniu, które człowiek przeczytał.
 *
 * @param folder zakres, o który pytamy — ta sama ścieżka, którą dostało [`listRuns`].
 * @param olderThanDays ile dni ma mieć bieg, żeby wejść do drugiej liczby.
 */
export function whatThisFolderCouldForget(
  folder: string | null,
  olderThanDays: number,
): Promise<CouldForget> {
  return invoke<CouldForget>('what_this_folder_could_forget', { folder, olderThanDays });
}

/**
 * Zdejmuje to, co zostawiły biegi, których Loadout nie zamknął — i **tylko** to.
 *
 * Katalog z niezapisaną zmianą zostaje, bo jest jedyną kopią tego, co ktoś w nim napisał; gałąź
 * z commitem, którego nie ma reszta projektu, zostaje z tego samego powodu. Odpowiedź nazywa oba
 * po imieniu i ze ścieżką, więc jest jedyną rzeczą, jaką trzeba pokazać po naciśnięciu.
 */
export function forgetWhatTheOldRunsLeft(folder: string | null): Promise<Forgotten> {
  return invoke<Forgotten>('forget_what_the_old_runs_left', { folder });
}

/**
 * Zapomina biegi starsze niż tyle dni — razem z ich gałęziami i katalogami roboczymi.
 *
 * TA SAMA DROGA, CO [`forgetRun`] przy jednym biegu, tylko zamówiona datą: folder, który biega raz
 * w tygodniu, i folder, który biega dziesięć razy dziennie, mają po tygodniu zupełnie inną
 * historię przy tej samej liczbie „zostaw N ostatnich".
 */
export function forgetRunsOlderThan(folder: string | null, days: number): Promise<Forgotten> {
  return invoke<Forgotten>('forget_runs_older_than', { folder, days });
}

/** Licznikowy paragon kopiowania; raport nigdy nie wraca do JavaScriptu. */
export interface DiagnosticsReceipt {
  readonly runs: number;
  readonly conversations: number;
  readonly artifacts: number;
}

/**
 * Każe Rustowi zbudować allowlistowany raport aktywnego workspace i zapisać go do schowka.
 *
 * Folder jest jedynym zakresem. `null` zostaje jawne, żeby Rust mógł odmówić bez pożyczania
 * katalogu procesu; przycisk nie ma prawa skopiować danych sąsiedniego projektu.
 */
export function copyDiagnostics(folder: string | null): Promise<DiagnosticsReceipt> {
  return invoke<DiagnosticsReceipt>('copy_diagnostics', { folder });
}

/**
 * `/start <komenda>`: uruchamia rzecz, która ma **zostać**, i oddaje jej grupę procesów.
 *
 * ROZWIĄZUJE SIĘ NATYCHMIAST, i to jest cała różnica wobec [`start`] i [`ask`]. Tamte trwają tyle,
 * co bieg, bo komenda po tamtej stronie czeka na jego koniec. Tutaj po tamtej stronie zostaje
 * UCHWYT (`engine::drivers::command::Staying`), więc wywołanie wraca, kiedy rzecz WSTAŁA, a nie
 * kiedy zeszła. Wołający, który zdejmie kafelek w `finally` — tak, jak te dwie drogi zdejmują
 * pasek biegu — zgasi go w tym samym tyknięciu, w którym go postawił.
 *
 * @param command wiersz powłoki, co do znaku. Rust odmawia pustego zdaniem, które mówi, co wpisać.
 * @param folder katalog, w którym ta rzecz ma stanąć, albo `null` — wtedy Rust bierze ten, pod
 *   którym wstała aplikacja (`AppState::project_for`). Klucz jedzie ZAWSZE, także jako `null`:
 *   powód w całości stoi przy `invoke` w [`start`].
 */
export function startProcess(command: string, folder: string | null = null): Promise<number> {
  return invoke<number>('start_process', { command, folder });
}

/**
 * „Stop" na kafelku: kończy tę jedną grupę.
 *
 * Rozwiązuje się dopiero z **dowodem**, że w grupie nie ma nikogo — `stop_process` po tamtej
 * stronie wraca po `kill(-pgid, 0) == ESRCH`, nie po wysłaniu sygnału (niezmiennik 6). Odmawia
 * dokładnie w jednym przypadku: grupa po pełnej eskalacji dalej odpowiada.
 *
 * @param pgid grupa z odpowiedzi [`startProcess`]. Jedyna liczba, którą tę rzecz da się
 *   zaadresować — okno jej nie wylicza i nie ma jak.
 */
export function stopProcess(pgid: number): Promise<void> {
  return invoke<void>('stop_process', { pgid });
}

/**
 * Wszystko, co Loadout uruchomił dla człowieka — razem z tym, co zeszło.
 *
 * Rzeczy zeszłe SĄ w tej odpowiedzi z rozmysłu: to jedyna droga, którą okno dowiaduje się
 * o śmierci czegoś, czego nie zatrzymało samo. Kafelka takiemu wpisowi nie rysuje widok
 * (`./rail/processes.ts`), więc lista może być uczciwa, a ekran mimo to nie kłamie.
 *
 * `Promise<unknown>`, a nie zadeklarowany kształt, i to jest wybór, nie lenistwo: ta krawędź
 * czyta się także pod atrapą granicy (`e2e/harness.ts` odpowiada KSZTAŁTEM, nie stanem), więc typ
 * obiecujący listę obiecywałby coś, czego nie ma czym dowieźć. Sprawdzenie pól należy do tego,
 * kto z tej odpowiedzi robi kafelki — i tam stoi, w jednym miejscu.
 *
 * @param opened `pgid` rzeczy, której panel jest otwarty, albo `null`. Wyjście jedzie tylko dla
 *   niej; powód i pomiar stoją przy `StartedWire::said` w `src-tauri/src/ipc.rs`.
 */
export function listProcesses(opened: number | null = null): Promise<unknown> {
  return invoke<unknown>('list_processes', { opened });
}
