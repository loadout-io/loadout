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
 *
 * 2026-08-18 — UKŁAD Z MAKIETY, cztery rzeczy, których tu nie było. Do tego dnia kafelek był
 * `<b>{step.name}</b>`, akapitem i stopką bez linii, o szerokości `w-70` (280 px). Makieta
 * (`.node`, `docs/mockup/index.html:243`) ma 246 px, uchwyt przeciągania po lewej stronie
 * nazwy, chip agenta z kolorową kostką tożsamości po prawej i stopkę NAD linią. U właściciela
 * kafelki były przy tym rysowane w powiększeniu 2× (`fitView` bez `maxZoom`), więc różnica
 * 280 do 246 była najmniejszym z problemów tego ekranu.
 *
 * CHIP AGENTA JEST OPCJONALNY i to jest treść niezmiennika 17 w tym pliku: kafelek rysuje go
 * WYŁĄCZNIE wtedy, gdy dostał agenta, którego `step.agent` naprawdę wskazuje. Wypisanie tam
 * identyfikatora albo słowa zastępczego byłoby relacją, której w danych nie ma — a krok bez
 * agenta ma o tym mówić brakiem chipu, nie wypełniaczem.
 */
import type { ReactElement } from 'react';
import type { Agent, Color } from '../../../state/agents';
import type { Link, Step } from '../../../state/workflows';

export interface StepTileProps {
  step: Step;
  /** Wszystkie kroki — stopka nazywa poprzednika po nazwie, nie po identyfikatorze. */
  steps: Step[];
  /** Wszystkie strzałki. Stopka jest z nich wyliczana (niezmiennik 17). */
  links: Link[];
  /**
   * Agent, którego ten krok nazywa — jeżeli jest w bibliotece.
   *
   * Bez niego chip nie powstaje. Rozwiązanie `step.agent` należy do płótna, bo tam mieszka
   * lista agentów; kafelek, który rozwiązywałby je drugi raz u siebie, mógłby rozwiązać
   * inaczej niż panel i pokazać przy kroku innego agenta niż ten, którego panel nazywa.
   */
  agent?: Agent;
  /** Zaznaczenie jest stanem płótna, nie polem pliku. */
  selected?: boolean;
}

/* `node-card` z makiety (`.node`): 246 px, `--raised`, obrys `--line-strong`, zaznaczony
 * `--accent`. Szerokość jest stała, bo płótno układa kafelki w kolumny — kafelek, który rośnie
 * z treścią, przesuwa sąsiadów przy każdej zmianie nazwy.
 *
 * `w-61.5` to `calc(var(--spacing) * 61.5)`, czyli 246 px przy bazie 4 px. Nie `w-[246px]`:
 * liczba w klasie jest literałem rozmiaru, a mnożnik siatki nią nie jest — i przeżyje zmianę
 * bazy. ROZJAZD, świadomy i zgłoszony: DESIGN §6 mówi w tym miejscu 280 px, makieta 246,
 * a przy rozbieżności wygrywa makieta. */
const CARD = 'w-61.5 rounded-md border bg-raised p-3 text-body';
const CARD_LINE = 'border-line-strong';
const CARD_SELECTED = 'border-accent';

/** Kostka tożsamości agenta → nazwa klasy tła.
 *
 * `Agent.color` jest polem, które formularz agenta zapisuje od T-11, a do 2026-08-18 NIC w całym
 * repo go nie czytało: pięć kolorów do wyboru i ani jednego miejsca, w którym wybór był widoczny.
 * Tu jest pierwsze, więc to mapowanie nie ma jeszcze drugiej kopii, z którą mogłoby się rozjechać.
 *
 * `Record`, nie `switch` z gałęzią domyślną: szósty kolor dopisany do `Color` przestaje TU się
 * kompilować, zamiast po cichu wpaść w „reszta" i dostać kolor stanu — a mieszanie tożsamości ze
 * stanem jest dokładnie tym błędem, przez który obie palety są rozdzielone (DESIGN §3). */
const IDENTITY: Readonly<Record<Color, string>> = {
  slate: 'bg-id-1',
  plum: 'bg-id-2',
  clay: 'bg-id-3',
  moss: 'bg-id-4',
  rose: 'bg-id-5',
};

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

export function StepTile({
  step,
  steps,
  links,
  agent,
  selected = false,
}: StepTileProps): ReactElement {
  const copies = copiesOf(step);
  const waits = waitsFor(step, steps, links);
  const handsOn = links.some((link) => link.from === step.id);

  return (
    <div className={`${CARD} ${selected ? CARD_SELECTED : CARD_LINE}`} data-step={step.id}>
      <div className="flex items-center gap-2">
        {/* Uchwyt przeciągania z makiety (`.node .grab`). Nie jest `<button>` i nie ma
            handlera z rozmysłem: ciągnie CAŁY kafelek, a to robi React Flow na samym
            kafelku. Przycisk z własnym `onClick` byłby drugą, cichszą drogą do tego samego
            gestu — i tą, która nie działa. */}
        <span
          aria-hidden
          className="grid size-4.5 shrink-0 cursor-grab place-items-center rounded-sm border border-line bg-well font-mono text-label text-muted"
        >
          ⠿
        </span>
        <b className="min-w-0 flex-1 truncate text-heading text-ink">{step.name}</b>
        {step.kind === 'checkpoint' ? (
          <span className="shrink-0 text-label text-muted">asks you</span>
        ) : null}
        {agent === undefined ? null : (
          <span className="flex shrink-0 items-center gap-1 font-mono text-label text-muted">
            <i className={`block size-2.75 ${IDENTITY[agent.color]}`} />
            {agent.name}
          </span>
        )}
      </div>

      {/* Dwie linie, obcięte. Czwarta linia tekstu na kafelku jest błędem projektowym,
          nie ciasnotą (DESIGN §6), a `line-clamp` jest jedynym miejscem, w którym to widać. */}
      <p className="mt-1 line-clamp-2 text-body text-ink">{saysWhat(step)}</p>

      {/* Stopka NAD linią (`.node .bot`: `padding-top:7px; border-top:1px solid var(--line)`)
          i w kroju maszynowym, bo to są wartości wyliczone, nie zdania. */}
      <div className="mt-2 flex items-baseline justify-between gap-2 border-t border-line pt-2 font-mono text-label text-muted">
        <span>{copies === null ? waits : `${copies} · ${waits}`}</span>
        {handsOn ? <span>runs before ▸</span> : null}
      </div>
    </div>
  );
}
