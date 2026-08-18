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
import type { Point, Step, WorkflowFile } from '../../../state/workflows';
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
}

/** Zdarzenie wskaźnika obcięte do jedynej rzeczy, której potrzebujemy: punktu upuszczenia
 * W UKŁADZIE PŁÓTNA. Przeliczenie z ekranu robi `screenToFlowPosition` w komponencie — tu
 * przychodzi już gotowy punkt, żeby ta funkcja nie potrzebowała ani okna, ani viewportu. */
export interface DropEvent {
  at: Point;
}

/** „Czy da się narysować tę strzałkę?" — jeden bool, jedyne sprawdzenie po stronie TypeScriptu.
 *
 * Cykl jest UNIEMOŻLIWIONY, nie zgłoszony: strzałka po prostu nie ląduje, uchwyt szarzeje
 * i nie ma żadnego komunikatu, bo użytkownik nie zrobił nic złego [T3 §5.1]. Reszta pytań
 * („czy to da się uruchomić?") należy do Rusta i wraca jako `Note[]`. */
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

  /* Czy z celu da się DOJŚĆ do źródła? Jeśli tak, ta strzałka domknęłaby koło — czyli pracę,
   * która nigdy się nie kończy. Obchód w przód, `seen` przeciwko kołu, które już jest w pliku
   * (plik bywa poprawiony ręcznie, a wtedy obchód bez `seen` nie wraca) [T3 §5.1]. */
  const seen = new Set<string>();
  const ahead = [target];
  for (let next = ahead.pop(); next !== undefined; next = ahead.pop()) {
    if (next === source) return false;
    if (seen.has(next)) continue;
    seen.add(next);
    for (const link of file.links) {
      if (link.from === next) ahead.push(link.to);
    }
  }
  return true;
}

/** Dokłada strzałkę, jeżeli [`isValidConnection`] ją przepuszcza; inaczej oddaje dokument
 * bez zmiany. Odmowa jest cicha — tu nie powstaje żadna uwaga. */
export function onConnect(connection: Connection, file: WorkflowFile): WorkflowFile {
  if (!isValidConnection(connection, file)) return file;
  return {
    ...file,
    links: [...file.links, { from: connection.source, to: connection.target }],
  };
}

/** Upuszczenie końca strzałki.
 *
 * Na PUSTYM płótnie (`isValid: false`) powstaje jeden krok rodzaju `agent` w przyciągniętym
 * punkcie upuszczenia i jedna strzałka do niego — „utwórz i połącz jednym ruchem"
 * [T3 §9, „MVP ships" punkt 2]. Nad istniejącym kafelkiem (`isValid: true`) nie powstaje nic.
 *
 * Identyfikator nowego kroku wyprowadzamy z dokumentu, a nie z zegara ani z losowości: funkcja
 * ma być czysta, a plik ma się dać porównać gitem po dwóch takich samych gestach. */
export function onConnectEnd(
  event: DropEvent,
  connection: ConnectionEnd,
  file: WorkflowFile,
): WorkflowFile {
  /* Upuszczenie nad istniejącym kafelkiem albo strzałka znikąd: nie powstaje NIC. Strzałkę
   * rysuje w tym wypadku `onConnect`, a krok dorobiony tutaj byłby kafelkiem-widmem przy
   * każdym udanym połączeniu. */
  if (connection.isValid || connection.fromNode === null) return file;

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
