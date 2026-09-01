/* Kto oddaje komendę kafelkowi „uruchom i zostaw", i czy naprawdę o nią poproszono.
 *
 * # Po co to istnieje
 *
 * Zamówienie właściciela 2026-08-30: „dajmy taki step o nazwie run preview app, tylko że agent
 * sam ma rozkminić jakie komendy użyć do odpalenia, my nie ingerujemy bo nie chcę w każdym
 * projekcie osobno wpisywać na front i backend command".
 *
 * Mechanizm istnieje (`ServeStep.commandFrom`), ale sam z siebie nie wystarcza: kafelek czeka na
 * NAZWANE pole, a krok przed nim musi zostać o nie poproszony. Bez tego bieg dochodzi do tego
 * kafelka wyłącznie po to, żeby odmówić — po tym, jak człowiek odczekał swoje na krokach przed nim.
 *
 * # Dlaczego to jest czysty moduł, a nie warunek w panelu
 *
 * Bo to jest polityka („kto komu co oddaje"), a to repo nie ma jsdom — reguła zamknięta
 * w komponencie byłaby kodem, którego nie dotyka żadne kryterium. Panel dostaje gotowe
 * odpowiedzi i nie zna strzałek, dokładnie jak przy `wayBack` w edytorze (niezmiennik 13).
 */
import type {
  AgentStep,
  HandoverField,
  ServeStep,
  Step,
  WorkflowFile,
} from '../../../state/workflows';
import { typable } from '../../run/run-command';

/** Opis, który dostaje agent proszony o wiersz powłoki. Jego słowami, nie naszym żargonem. */
export const DESCRIBE_THE_COMMAND =
  'the one shell line that starts this, ready to run in this project';

/**
 * Nazwa pola, o które ten kafelek prosi — **liczona RAZ, przy zaznaczeniu, i zapisana**.
 *
 * Wyprowadzona z nazwy kafelka, żeby dwa kafelki („Run frontend", „Run backend") składały się
 * bez kolizji i bez wpisywania czegokolwiek. To jest cała treść zamówienia „front i backend":
 * jedna stała nazwa dawałaby obu kafelkom tę samą komendę.
 *
 * PRZELICZANIE PRZY KAŻDYM RENDERZE BYŁOBY WADĄ, i dlatego wołający ma to zapisać: zmiana nazwy
 * kafelka po okablowaniu zmieniałaby nazwę pola, a krok przed nim dalej oddawałby starą — czyli
 * przemianowanie kafelka po cichu rozłączałoby graf. Zapisana nazwa co najwyżej rozjeżdża się
 * WIZUALNIE z nazwą kafelka, a to widać w obu panelach.
 */
export function fieldNameFor(tile: Pick<ServeStep, 'id' | 'name'>): string {
  const fromName = typable(tile.name);
  /* Kafelek bez nazwy (albo nazwany samymi znakami, których `typable` nie przepuszcza) dostaje
   * swój identyfikator: pole o pustej nazwie nie ma jak zostać oddane ani poproszone. */
  return fromName === '' ? typable(tile.id) : fromName;
}

/** Krok, który po strzałce stoi PRZED tym kafelkiem i jest agentem — albo `null`. */
export function theStepBefore(document: WorkflowFile, serveId: string): AgentStep | null {
  const before = document.links
    .filter((link) => link.to === serveId)
    .map((link) => document.steps.find((step: Step) => step.id === link.from))
    .find((step): step is AgentStep => step?.kind === 'agent');
  return before ?? null;
}

/** Czy ten krok jest już proszony o to pole. */
export function handsOver(step: AgentStep | null, field: string): boolean {
  if (step === null || step.handover === 'notes') return false;
  return step.handover.fields.some((one) => one.name.trim() === field);
}

/**
 * Dokument, w którym krok przed tym kafelkiem jest poproszony o to pole.
 *
 * Dokłada, nigdy nie nadpisuje: krok może już oddawać inne rzeczy i skasowanie ich przy okazji
 * byłoby zabraniem pracy, o którą nikt nie prosił. Wołanie tego drugi raz nic nie zmienia.
 */
export function askTheStepBefore(
  document: WorkflowFile,
  serveId: string,
  field: string,
): WorkflowFile {
  const before = theStepBefore(document, serveId);
  if (before === null || handsOver(before, field)) return document;

  const asked: HandoverField = {
    name: field,
    describe: DESCRIBE_THE_COMMAND,
    /* POTRZEBNE, a nie „miło mieć": bez tego pola kafelek za nim ODMAWIA startu, więc pole
     * nieobowiązkowe byłoby prośbą, której zignorowanie kosztuje bieg. */
    required: true,
  };
  const already = before.handover === 'notes' ? [] : before.handover.fields;

  return {
    ...document,
    steps: document.steps.map((step) =>
      step.id === before.id ? { ...step, handover: { fields: [...already, asked] } } : step,
    ),
  };
}

/**
 * Nazwa pola, na które ten kafelek czeka — pusta, kiedy nie czeka na żadne.
 *
 * Z PLIKU, nie przeliczana z nazwy kafelka: nazwa powstała raz, przy zaznaczeniu, i od tej chwili
 * jest zapisana. Powód w całości stoi przy [`fieldNameFor`].
 */
export function fieldWaitedFor(document: WorkflowFile, serveId: string): string {
  const tile = document.steps.find((step: Step) => step.id === serveId);
  if (tile?.kind !== 'serve') return '';
  return tile.commandFrom?.field ?? '';
}
