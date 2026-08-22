/* Rysowanie strzałek: co wolno połączyć i co się dzieje po upuszczeniu na pustym płótnie.
 *
 * Wszystkie trzy funkcje są CZYSTE i biorą dokument ostatnim argumentem. Powód jest testowy
 * i architektoniczny naraz: gest — `pointerdown` na uchwycie, ruch, `pointerup` — nie jest
 * odtwarzalny bez przeglądarki [T3 §2.3, ryzyko 7], więc kryteria wołają dokładnie te funkcje,
 * które gest woła, z syntetycznym stanem połączenia. Wersja domknięta na `getNodes()/getEdges()`
 * (T3 §5.1) byłaby nie do zawołania bez `<ReactFlow>` w drzewie.
 *
 * Odchylenie od prozy TASK.md, świadome: kryterium pisze `isValidConnection({ source, target })`
 * z jednym argumentem, bo w React Flow ta funkcja jest domknięciem nad żywym grafem. Tutaj graf
 * przychodzi jawnie — płótno robi `isValidConnection={(c) => isValidConnection(c, file)}`.
 */
import type { Link, Point, Step, WorkflowFile } from '../../../state/workflows';
import { GRID } from '../../../state/workflows';
import { snap } from './map';

/** Tyle z `Connection` z `@xyflow/react`, ile ta warstwa czyta. Portów nie mamy (T3 §3.1). */
export interface Connection {
  source: string;
  target: string;
}

/** Tyle ze stanu, który React Flow oddaje w `onConnectEnd`, ile ta warstwa czyta.
 *
 * `isValid: true` znaczy „upuszczono nad istniejącym kafelkiem" — strzałkę robi wtedy
 * `onConnect` i tworzenie kroku byłoby kafelkiem-widmem przy każdym udanym połączeniu. */
export interface ConnectionEnd {
  isValid: boolean;
  fromNode: { id: string } | null;
  /**
   * Czy wskaźnik puszczono NAD ISTNIEJĄCYM KAFELKIEM — `toNode` z `FinalConnectionState`.
   *
   * 2026-08-22 — TO JEST NAPRAWA ZGŁOSZONA PRZEZ WŁAŚCICIELA, a nie zabezpieczenie na zapas.
   * `isValid` mówi wyłącznie „upuszczono na UCHWYCIE, i wolno tam było". Powrót ciągnie się
   * z dolnej kropki sędziego na GÓRNĄ kropkę kroku, do którego wraca praca — czyli w bok
   * i do góry, przez pół płótna. Kto minie tę kropkę o kilka pikseli i puści nad korpusem
   * kafelka, dostawał `isValid: false` i tę funkcję, która robiła mu WTEDY NOWY KROK: dokładnie
   * ten kafelek-widmo, na który właściciel zgłosił „nowy step się robi, jak próbuję tak zrobić".
   *
   * Widmo nie kończyło się na jednym zbędnym kafelku. Skasowanie go zostawiało strzałkę, która
   * już nie miała gdzie celować, a walidator meldował z tego `"Backend check" points at a step
   * that is not in this workflow (s_11)` — uwagę o kroku, którego człowiek nigdy nie chciał.
   * Drugą połowę tej samej awarii zamyka `toFile` (`./map.ts`).
   */
  overTile: boolean;
}

/** Zdarzenie wskaźnika obcięte do jedynej rzeczy, której potrzebujemy: punktu upuszczenia
 * W UKŁADZIE PŁÓTNA. Przeliczenie z ekranu robi `screenToFlowPosition` w komponencie — tu
 * przychodzi już gotowy punkt, żeby ta funkcja nie potrzebowała ani okna, ani viewportu. */
export interface DropEvent {
  at: Point;
}

/** „Czy da się narysować tę strzałkę?" — jeden bool, jedyne sprawdzenie po stronie TypeScriptu.
 *
 * Cykl NIEOZNACZONY jest uniemożliwiony, nie zgłoszony: strzałka po prostu nie ląduje, uchwyt
 * szarzeje i nie ma żadnego komunikatu, bo użytkownik nie zrobił nic złego [T3 §5.1]. Reszta pytań
 * („czy to da się uruchomić?") należy do Rusta i wraca jako `Note[]`.
 *
 * 2026-08-19 — KRAWĘDŹ DOMYKAJĄCA KOŁO LĄDUJE, TYLKO JAKO POWRÓT. Do tego dnia ta funkcja mówiła
 * „nie" każdemu kołu, i było to słuszne, dopóki koło znaczyło wyłącznie pracę, która się nie
 * kończy. Właściciel poprosił o kształt, którego bez powrotu nie da się wyrazić: implementer
 * wysyła do testera, tester zdaje raport, `fail` wraca do implementera, `pass` puszcza bieg dalej.
 * Powrót niesie sufit tur (`Link.max_turns`), więc pętla bez końca dalej jest niewyrażalna —
 * zmieniło się to, CO odmawiamy, a nie to, przed czym bronimy. */
export function isValidConnection(connection: Connection, file: WorkflowFile): boolean {
  const { source, target } = connection;

  /* Krok nie czeka na siebie samego, a strzałka do kroku, którego w pliku nie ma, nie ma
   * gdzie wylądować. */
  if (source === target) return false;
  if (!file.steps.some((step) => step.id === target)) return false;

  /* Ta sama strzałka drugi raz. Płótno nadaje krawędzi identyfikator `from->to`, więc duplikat
   * to dwie krawędzie pod jednym identyfikatorem — React Flow rysuje wtedy jedną i gubi drugą,
   * a plik niesie obie. „Już jest" wygląda dla użytkownika tak samo jak „narysowano". */
  if (file.links.some((link) => link.from === source && link.to === target)) return false;

  /* POWRÓT PRZECINAJĄCY CUDZĄ PĘTLĘ ODMAWIAMY TUTAJ, w chwili gestu. Rust daje na niego
   * `Problem` (`check::loops_that_cross`), czyli po narysowaniu plik przestałby się zapisywać —
   * a płótno, które pozwala narysować rzecz blokującą zapis, jest gorsze od takiego, które mówi
   * „nie" od razu: pierwsze kasuje pracę po cichu, drugie kosztuje jeden nieudany gest.
   *
   * 2026-08-22 — DO TEGO DNIA ODMAWIALIŚMY KAŻDEGO DRUGIEGO POWROTU, i było to za szerokie.
   * Graf z dwiema gałęziami (front i backend), z których każda ma własne sprawdzenie, potrzebuje
   * dwóch pętli, które nie mają ze sobą nic wspólnego — a odmowa kazała wybrać jedną gałąź.
   * Granica biegnie dziś tam, gdzie naprawdę leży: pętle ROZŁĄCZNE rozwijają się niezależnie
   * (`workflow::unroll`), a przecinające się i zagnieżdżone dalej nie, bo dla nich nie wiadomo,
   * która runda wychodzi na zewnątrz. */
  if (wouldCloseACircle(source, target, file)) {
    const body = loopBody(source, target, file);
    const crossed = file.links
      .filter(isAWayBack)
      .some((link) => shareAStep(body, loopBody(link.from, link.to, file)));
    if (crossed) return false;
  }

  return true;
}

/** Czy te dwa ciała pętli mają choć jeden wspólny krok. */
function shareAStep(one: ReadonlySet<string>, other: ReadonlySet<string>): boolean {
  for (const id of one) {
    if (other.has(id)) return true;
  }
  return false;
}

/** Kroki, do których da się dojść W PRZÓD od `start`, wliczając `start`.
 *
 * Powroty się nie liczą — kolejność pracy wyznaczają strzałki BEZ powrotów, bo tylko one znaczą
 * „po". Ta sama reguła i ten sam powód, co przy liczeniu rzędów w `tidy.ts` i przy `forward`
 * w walidatorze Rusta. */
function ahead(start: string, file: WorkflowFile): Set<string> {
  const seen = new Set<string>();
  const stack = [start];
  for (let at = stack.pop(); at !== undefined; at = stack.pop()) {
    if (seen.has(at)) continue;
    seen.add(at);
    for (const link of file.links) {
      if (!isAWayBack(link) && link.from === at) stack.push(link.to);
    }
  }
  return seen;
}

/** Kroki, z których da się dojść do `goal`, wliczając `goal`. Lustro [`ahead`]. */
function behind(goal: string, file: WorkflowFile): Set<string> {
  const seen = new Set<string>();
  const stack = [goal];
  for (let at = stack.pop(); at !== undefined; at = stack.pop()) {
    if (seen.has(at)) continue;
    seen.add(at);
    for (const link of file.links) {
      if (!isAWayBack(link) && link.to === at) stack.push(link.from);
    }
  }
  return seen;
}

/** Ciało pętli domkniętej powrotem `judge → entry`: kroki, które ta pętla powtarza.
 *
 * Dokładnie ta sama definicja, co po stronie Rusta (`workflow::unroll`): krok należy do ciała,
 * jeżeli da się do niego dojść w przód z `entry` I da się z niego dojść do `judge`. Oba końce
 * powrotu należą do ciała. Dwie definicje tego zbioru rozjechałyby się przy pierwszej poprawce,
 * a rozjazd znaczyłby, że płótno przepuszcza plik, którego Rust odmawia. */
export function loopBody(judge: string, entry: string, file: WorkflowFile): Set<string> {
  const back = behind(judge, file);
  return new Set([...ahead(entry, file)].filter((id) => back.has(id)));
}

/** Czy ta krawędź jest POWROTEM — czyli czy wolno jej domknąć koło. Lustro `Link::is_a_way_back`. */
export function isAWayBack(link: Link): boolean {
  return link.max_turns !== undefined;
}

/** Ile rund dostaje powrót narysowany gestem.
 *
 * Trzy, nie jeden i nie dziesięć. Jedna runda to pętla, która wykonuje się raz i niczego nie
 * powtarza — czyli gest bez skutku. Dziesięć to sufit (`MOST_TURNS` po stronie Rusta) i noc bez
 * nadzoru dla kogoś, kto tylko pociągnął strzałkę. Liczbę zmienia się w panelu kroku. */
export const TURNS_BY_DEFAULT = 3;

/** Czy strzałka `source → target` domknęłaby koło.
 *
 * Obchód w przód od celu: jeżeli da się z niego wrócić do źródła, ta strzałka zamyka pętlę.
 * `seen` broni przed kołem, które JUŻ jest w pliku — plik bywa poprawiony ręcznie albo zmergowany
 * gitem, a obchód bez tego zbioru nie wraca [T3 §5.1].
 *
 * Osobna funkcja, bo odpowiedź jest potrzebna dwa razy i w dwóch różnych celach: `isValidConnection`
 * pyta „czy odmówić", a `onConnect` pyta „czy oznaczyć jako powrót". Dwie kopie tego obchodu
 * rozjechałyby się przy pierwszej poprawce. */
export function wouldCloseACircle(source: string, target: string, file: WorkflowFile): boolean {
  const seen = new Set<string>();
  const ahead = [target];
  for (let next = ahead.pop(); next !== undefined; next = ahead.pop()) {
    if (next === source) return true;
    if (seen.has(next)) continue;
    seen.add(next);
    for (const link of file.links) {
      if (link.from === next) ahead.push(link.to);
    }
  }
  return false;
}

/** Dokłada strzałkę, jeżeli [`isValidConnection`] ją przepuszcza; inaczej oddaje dokument
 * bez zmiany. Odmowa jest cicha — tu nie powstaje żadna uwaga. */
export function onConnect(connection: Connection, file: WorkflowFile): WorkflowFile {
  if (!isValidConnection(connection, file)) return file;
  /* Krawędź domykająca koło powstaje JAKO POWRÓT, z suficiem tur od razu. Wpuszczenie jej bez tej
   * liczby dałoby plik z nieoznaczonym cyklem, którego walidator odmawia — czyli gest, po którym
   * workflow przestaje się zapisywać. Liczba jest tu, a nie w osobnym kroku „a teraz ustaw tury",
   * bo strzałka bez niej nie jest poprawnym dokumentem ani przez chwilę. */
  const link = wouldCloseACircle(connection.source, connection.target, file)
    ? { from: connection.source, to: connection.target, max_turns: TURNS_BY_DEFAULT }
    : { from: connection.source, to: connection.target };
  return { ...file, links: [...file.links, link] };
}

/** Nazwa kroku, tak jak stoi na kafelku. To ona jedzie w zdaniu do człowieka — `s_4` jest
 * identyfikatorem z pliku i na ekranie nie ma czego szukać (niezmiennik 14). */
function named(id: string, file: WorkflowFile): string {
  return file.steps.find((step) => step.id === id)?.name ?? id;
}

/** Wynik przycisku „Add loop": dokument albo zdanie, dlaczego tej pętli nie da się zrobić. */
export interface LoopAdded {
  file: WorkflowFile;
  /** Zdanie dla człowieka. `null` znaczy „powstała". */
  refused: string | null;
}

/** Pętla zrobiona WSKAZANIEM DWÓCH KAFELKÓW, a nie pociągnięciem strzałki przez pół płótna.
 *
 * 2026-08-22 — ZGŁOSZENIE WŁAŚCICIELA. Powrót dawał się dotąd narysować wyłącznie gestem z dolnej
 * kropki sędziego na górną kropkę kroku, do którego wraca praca — czyli w bok i do góry, obok
 * dwóch innych kafelków. Kto minął kropkę, dostawał kafelek-widmo (`onConnectEnd`), a po jego
 * skasowaniu — uwagę o strzałce celującej w krok, którego nie ma. Gest, który tak łatwo kończy
 * się czymś innym, niż chciał człowiek, nie jest jedynym wejściem do funkcji, na której stoi
 * cały kształt „implementer → sprawdzenie → poprawka".
 *
 * `judge` to krok, z którego powrót WYCHODZI — ten, który orzeka. `entry` to krok, do którego
 * wraca praca. Ta sama para i ta sama kolejność, co w `Link { from, to }`, żeby nie było dwóch
 * odpowiedzi na pytanie, który koniec jest który.
 *
 * ODMOWA JEST ZDANIEM, NIE CISZĄ, i to jest różnica wobec `isValidConnection`. Tam człowiek
 * ciągnie strzałkę i widzi, że nie łapie; tutaj klika dwa kafelki i bez zdania nie wiedziałby,
 * czy pętla powstała, czy nie. */
export function addLoop(judge: string, entry: string, file: WorkflowFile): LoopAdded {
  if (judge === entry) {
    return { file, refused: 'Pick two different steps.' };
  }
  if (file.links.some((link) => isAWayBack(link) && link.from === judge)) {
    return { file, refused: `"${named(judge, file)}" already sends the work back.` };
  }
  /* Powrót ma dokąd wracać tylko wtedy, gdy sędzia naprawdę biegnie PO tamtym kroku. Bez tego
   * warunku dwa kliknięcia w przypadkowe kafelki dałyby strzałkę, którą walidator Rusta i tak
   * odrzuci jako nieoznaczone koło — czyli plik, który przestaje się zapisywać. */
  if (!ahead(entry, file).has(judge)) {
    return {
      file,
      refused: `"${named(judge, file)}" does not run after "${named(entry, file)}", so there is nothing to send the work back to.`,
    };
  }
  const body = loopBody(judge, entry, file);
  if (
    file.links
      .filter(isAWayBack)
      .some((link) => shareAStep(body, loopBody(link.from, link.to, file)))
  ) {
    return {
      file,
      refused:
        'This loop would cross another one. Loadout runs loops side by side, never one inside another.',
    };
  }

  /* Strzałka `judge → entry` MOŻE JUŻ ISTNIEĆ jako zwykłe „po" — wtedy ją podnosimy, zamiast
   * dokładać drugą o tym samym znaczeniu. Dwie strzałki między tą samą parą kroków dają na
   * płótnie jeden identyfikator krawędzi i React gubi jedną z nich (`map.ts`, `eachArrowOnce`). */
  const standing = file.links.some((link) => link.from === judge && link.to === entry);
  return {
    file: {
      ...file,
      links: standing
        ? file.links.map((link) =>
            link.from === judge && link.to === entry
              ? { ...link, max_turns: TURNS_BY_DEFAULT }
              : link,
          )
        : [...file.links, { from: judge, to: entry, max_turns: TURNS_BY_DEFAULT }],
    },
    refused: null,
  };
}

/** Upuszczenie końca strzałki.
 *
 * Na PUSTYM płótnie powstaje jeden krok rodzaju `agent` w przyciągniętym punkcie upuszczenia
 * i jedna strzałka do niego — „utwórz i połącz jednym ruchem" [T3 §9, „MVP ships" punkt 2].
 * „Puste" znaczy od 2026-08-22 dwie rzeczy naraz: ani uchwyt (`isValid`), ani korpus cudzego
 * kafelka (`overTile`). Nad kafelkiem nie powstaje NIC.
 *
 * Identyfikator nowego kroku wyprowadzamy z dokumentu, a nie z zegara ani z losowości: funkcja
 * ma być czysta, a plik ma się dać porównać gitem po dwóch takich samych gestach. */
export function onConnectEnd(
  event: DropEvent,
  connection: ConnectionEnd,
  file: WorkflowFile,
): WorkflowFile {
  /* Upuszczenie nad istniejącym kafelkiem — na uchwycie albo obok niego, na korpusie — albo
   * strzałka znikąd: nie powstaje NIC. Udane połączenie rysuje `onConnect`, a nieudane jest
   * gestem, który się nie udał, i tyle. Krok dorobiony tutaj byłby kafelkiem-widmem. */
  if (connection.isValid || connection.overTile || connection.fromNode === null) return file;

  const step = freshStep('agent', freshId(file), snap(event.at));
  return {
    ...file,
    /* Dopisujemy na KOŃCU: kolejność w `steps` jest kolejnością wstawiania i nigdy nie jest
     * sortowana [T3 §8.2 reguła 2]. Wstawienie w środek przepisuje ogon pliku w gicie. */
    steps: [...file.steps, step],
    /* To jest cały gest: utwórz I połącz. Bez tej linii użytkownik za każdym razem domyka
     * strzałkę ręcznie, a gest jest połową gestu [T3 §9, „MVP ships" punkt 2]. */
    links: [...file.links, { from: connection.fromNode.id, to: step.id }],
  };
}

/** Identyfikator, którego w tym dokumencie jeszcze nie ma.
 *
 * Wyprowadzony z dokumentu, nie z zegara ani z losowości: ta funkcja jest czysta, a dwa takie
 * same gesty mają dać plik, który da się porównać gitem. Pętla kończy się zawsze — kandydatów
 * jest o jeden więcej niż zajętych identyfikatorów. */
export function freshId(file: WorkflowFile): string {
  const taken = new Set(file.steps.map((step) => step.id));
  for (let n = file.steps.length + 1; ; n += 1) {
    const candidate = `s_${String(n)}`;
    if (!taken.has(candidate)) return candidate;
  }
}

/** Świeży krok jednego z dwóch rodzajów — i jedyne miejsce, w którym powstaje nowy krok.
 *
 * Upuszczenie strzałki i oba przyciski płótna wołają TĘ funkcję. Druga lista wartości
 * domyślnych, wpisana przy przycisku, rozjechałaby się z tą przy pierwszym polu dopisanym do
 * schematu — i rozjechałaby się po cichu, bo krok z brakującym polem wygląda jak każdy inny.
 *
 * `agent: ''` znaczy „jeszcze nie wybrano" i jest widoczne od razu: walidator (T-12) zgłasza
 * to jako problem. Zgadywanie pierwszego agenta z biblioteki byłoby decyzją podjętą za
 * użytkownika w miejscu, w którym on jej nie widzi. */
/** Wolne miejsce pod najniższym kafelkiem — tam ląduje krok dodany przyciskiem.
 *
 * Nowy kafelek dokładnie na innym wygląda jak zgubiony, a przycisk nie niesie punktu, w którym
 * użytkownik go chciał (w odróżnieniu od upuszczenia strzałki). */
function roomBelow(file: WorkflowFile): Point {
  const lowest = file.steps.reduce((deepest, step) => Math.max(deepest, step.at.y), 0);
  return snap({ x: GRID, y: file.steps.length === 0 ? GRID : lowest + 6 * GRID });
}

/** Oba przyciski płótna, jednym ruchem: nowy krok stawiany LUZEM, bez żadnej strzałki.
 *
 * 2026-08-19 — ROZSTRZYGNIĘCIE WŁAŚCICIELA. Do tego dnia przycisk doklejał strzałkę od
 * OSTATNIEGO kroku w pliku, więc płótno umiało zbudować wyłącznie łańcuch: żeby dostać trzy
 * gałęzie wchodzące do jednego kroku, człowiek musiał najpierw skasować strzałkę, której
 * nie prosił. Kafelki dokłada się luzem i łączy się je samemu — to jest cały gest tego edytora.
 *
 * DLACZEGO TO DOPIERO TERAZ, i dlaczego samo skreślenie strzałki wcześniej by nie wystarczyło.
 * Nowy krok niesie `folder: { use: 'project' }`, a `one_folder_two_steps`
 * (`src-tauri/src/workflow/check.rs`) mówi o dwóch krokach bez strzałki „mogą biec równocześnie
 * i piszą po tych samych plikach". Dopóki była to `Level::Problem` przy zapisie, DRUGI dołożony
 * kafelek robił z dokumentu plik, którego `workflow::file::save` odmawiał przed `fs::write` —
 * autosave dostawał odmowę, a na dysku zostawał plik o jeden kafelek do tyłu. Zmierzone na dysku
 * właściciela: plik z `s_1` i `s_3`, bez `s_2`. Ta sama reguła jest dziś ostrzeżeniem przy
 * zapisie i problemem przy Run, więc szkic w budowie ZAPISUJE SIĘ, a bieg dalej nie ruszy
 * z dwiema gałęziami w jednym folderze.
 *
 * `fresh-copy` na nowym kroku nie jest tu alternatywą i nigdy nie był: kasuje odmowę, ale
 * podejmuje za człowieka decyzję o IZOLACJI, której on nie widzi — krok w osobnej kopii plików
 * nie oddaje zmian do projektu, więc „agent poprawił, a w repo nic nie ma" byłoby następnym
 * zgłoszeniem.
 *
 * Krok, który ma iść PO innym, robi się jednym gestem: pociągnij z dolnej kropki kafelka
 * i upuść na pustym płótnie (`onConnectEnd`). Ten przycisk jest od stawiania, tamten gest od
 * stawiania i łączenia naraz — dwie różne czynności, dwa różne wejścia. */
export function addStep(
  kind: Step['kind'],
  file: WorkflowFile,
): { file: WorkflowFile; step: Step } {
  const step = freshStep(kind, freshId(file), roomBelow(file));

  return {
    /* `links` idzie NIETKNIĘTE, nie przepisane: strzałki są decyzją człowieka i ta funkcja
     * nie ma o nich zdania. */
    file: { ...file, steps: [...file.steps, step] },
    step,
  };
}

export function freshStep(kind: Step['kind'], id: string, at: Point): Step {
  if (kind === 'checkpoint') return { kind, id, name: 'Ask me first', at };
  /* Pusta komenda, nie przykład w rodzaju `npm run dev`. Wypełniacz wygląda na płótnie dokładnie
   * tak samo jak decyzja człowieka — a ten kafelek URUCHAMIA to, co w nim stoi. */
  if (kind === 'serve')
    return {
      kind,
      id,
      name: 'Start and leave running',
      command: '',
      folder: { use: 'project' },
      at,
    };

  return {
    kind,
    id,
    name: 'New step',
    agent: '',
    overrides: {},
    copies: 1,
    instructions: '',
    skills: 'all',
    folder: { use: 'project' },
    handover: 'notes',
    at,
  };
}
