/* Kafelek na płótnie. Dwa rodzaje, cztery linie tekstu, stopka WYLICZANA ze strzałek.
 *
 * Niezmiennik 17 mieszka w kształcie propsów. Stopka (`first step` / `after Plan` /
 * `reads 3 handoffs` po lewej, `runs before ▸` po prawej) nie jest napisem podanym z zewnątrz
 * ani wpisanym na sztywno w komponent — jest liczona z `links`. Wpisana na sztywno wygląda
 * identycznie do chwili, w której ktoś przesunie strzałkę, i wtedy kłamie po cichu.
 *
 * Dlatego kafelek dostaje CAŁE `links` i `steps`, a nie gotowy podpis: `steps` są potrzebne,
 * bo stopka nazywa poprzednika NAZWĄ, a `s_plan` nie jest niczym, co użytkownik widzi.
 * Przy piętnastu kafelkach koszt jest zerowy, a alternatywa — policzenie podpisu w płótnie
 * i podanie propsem — przenosi ten sam kod o jedno piętro wyżej i wyjmuje go spod kryterium.
 *
 * Komponent jest STEROWANY: `selected` przychodzi propsem i NIE jest zapisywane do pliku
 * (T3 §3.3, kryterium `to-file`).
 */
import type { ReactElement } from 'react';
import type { Link, Step } from '../../../state/workflows';

export interface StepTileProps {
  step: Step;
  /** Wszystkie kroki — stopka nazywa poprzednika po nazwie, nie po identyfikatorze. */
  steps: Step[];
  /** Wszystkie strzałki. Stopka jest z nich wyliczana (niezmiennik 17). */
  links: Link[];
  /** Zaznaczenie jest stanem płótna, nie polem pliku. */
  selected?: boolean;
}

/* `node-card` z DESIGN §6: 280 px, `--raised`, obrys `--line-strong`, zaznaczony `--accent`.
 * Szerokość jest stała, bo płótno układa kafelki w kolumny — kafelek, który rośnie z treścią,
 * przesuwa sąsiadów przy każdej zmianie nazwy. */
const CARD = 'w-70 rounded-sq border bg-raised p-3 text-body';
const CARD_LINE = 'border-line-strong';
const CARD_SELECTED = 'border-accent';

/** Nazwa kroku o tym identyfikatorze.
 *
 * Kiedy strzałka wskazuje krok, którego w pliku nie ma, stopka mówi „another step" zamiast
 * pokazać `s_plan`: identyfikator jest nazwą z drutu i nie ma prawa trafić na ekran
 * (niezmiennik 14). Sam fakt wiszącej strzałki zgłasza walidator, nie kafelek. */
function nameOf(steps: Step[], id: string): string {
  return steps.find((step) => step.id === id)?.name ?? 'another step';
}

/** Lewa połowa stopki, WYLICZONA ze strzałek (niezmiennik 17).
 *
 * Trzy zdania, bo trzy różne fakty: nikt przede mną, jeden krok przede mną, kilka kroków
 * przede mną. Jedno zdanie „reads N handoffs" dla wszystkich trzech kłamie na pierwszym kroku
 * każdego workflow, a nazwanie trzech poprzedników po imieniu nie mieści się w czterech
 * liniach (DESIGN §6). */
function waitsFor(step: Step, steps: Step[], links: Link[]): string {
  const incoming = links.filter((link) => link.to === step.id);
  const first = incoming[0];

  if (first === undefined) return 'first step';
  if (incoming.length === 1) return `after ${nameOf(steps, first.from)}`;
  return `reads ${String(incoming.length)} handoffs`;
}

/** Ile kopii tego kroku biegnie naraz, jeżeli więcej niż jedna (makieta, linia 512). */
function copiesOf(step: Step): string | null {
  return step.kind === 'agent' && step.copies > 1 ? `×${String(step.copies)}` : null;
}

/** Jedno zdanie o tym, co ten kafelek robi. Punkt kontrolny pyta, krok pracuje. */
function saysWhat(step: Step): string {
  return step.kind === 'agent' ? step.instructions : (step.question ?? '');
}

export function StepTile({ step, steps, links, selected = false }: StepTileProps): ReactElement {
  const copies = copiesOf(step);
  const waits = waitsFor(step, steps, links);
  const handsOn = links.some((link) => link.from === step.id);

  return (
    <div className={`${CARD} ${selected ? CARD_SELECTED : CARD_LINE}`} data-step={step.id}>
      <div className="flex items-baseline gap-2">
        <b className="text-heading text-ink">{step.name}</b>
        {step.kind === 'checkpoint' ? (
          <span className="text-label text-muted">asks you</span>
        ) : null}
      </div>

      {/* Dwie linie, obcięte. Czwarta linia tekstu na kafelku jest błędem projektowym,
          nie ciasnotą (DESIGN §6), a `line-clamp` jest jedynym miejscem, w którym to widać. */}
      <p className="mt-1 line-clamp-2 text-body text-ink">{saysWhat(step)}</p>

      <div className="mt-2 flex items-baseline justify-between text-label text-muted">
        <span>{copies === null ? waits : `${copies} · ${waits}`}</span>
        {handsOn ? <span>runs before ▸</span> : null}
      </div>
    </div>
  );
}
