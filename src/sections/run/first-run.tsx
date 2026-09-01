/* PIERWSZE URUCHOMIENIE: trzy kroki, w kolejności, z widocznym stanem każdego.
 *
 * ZMIERZONE 2026-08-31. Nowy człowiek lądował na ekranie Run, który rysuje pełny układ
 * produkcyjny — pasek kart, pasek loadoutu, pustą strefę pracy, wiersz wejścia — a niemal
 * wszystko na nim jest wygaszone albo puste. Do pierwszego działającego biegu było osiem do
 * jedenastu ruchów i aplikacja ANI RAZU nie mówiła, gdzie je zrobić: strefa pracy mówiła
 * „Nothing here yet: the work shows up line by line.", czyli komunikat o braku danych, którego
 * DESIGN §6 zakazuje wprost („Pusty ekran to zaproszenie do działania, nie komunikat o braku
 * danych"). Jedyne wskazówki wisiały w atrybutach `title` wygaszonych kontrolek — tekst, po
 * który trzeba najechać myszą i który nie mówi, dokąd iść.
 *
 * TRZY KROKI, NIE WIĘCEJ, bo tyle ich naprawdę jest: bieg potrzebuje folderu (zakres), kogoś,
 * kto pracuje (agent), i kolejności, w jakiej pracują (workflow). Każdy z nich ma odpowiedź
 * z DYSKU, nie z domysłu — liczby wchodzą do [`firstRunSteps`] z tych samych trzech list, które
 * czytają sekcje.
 *
 * DLACZEGO STAN KROKU JEST WYLICZANY, A NIE ZAPAMIĘTYWANY. Zapamiętane „już to zrobiłeś"
 * rozjeżdża się z rzeczywistością przy pierwszym skasowanym pliku i wtedy przewodnik mówi
 * o świecie, którego nie ma. Kroki są funkcją tego, co LEŻY, więc skasowanie ostatniego agenta
 * cofa krok sam z siebie.
 *
 * ── DLACZEGO TEN BLOK JEST TAK CIASNY, CO DO ELEMENTU ──────────────────────────────────────
 *
 * Niezmiennik 18: sufit gęstości jest mierzony, a zapadka może tylko maleć. Zmierzone tego dnia
 * kolektorem (`node scripts/density-collect.mjs`) na widoku domyślnym: `textElements` = 25 przy
 * zapadce 26 — czyli JEDEN element zapasu na całą aplikację. Strefa pracy oddaje dwa (znak `◇`
 * i zdanie o braku danych), więc na cały przewodnik przypadają TRZY elementy niosące tekst.
 *
 * Stąd kształt, który wygląda na oszczędny, a jest wymuszony: jeden wiersz = jeden element,
 * a stan kroku mieszka w JEGO WŁASNYM tekście („— done"), nie w osobnej pigułce obok. Wiersz
 * kroku bieżącego JEST przyciskiem — osobny przycisk pod zdaniem byłby czwartym elementem
 * i podniósłby zapadkę, czyli byłby regresem w rozumieniu reguły 18.
 *
 * `data-empty` SIEDZI NA TYM PRZYCISKU, i to jest ta sama arytmetyka. Ekran Run ma nieść
 * dokładnie jeden taki znacznik (`src/sections/empty-screen-invites.test.tsx`), a osobne zdanie
 * nad listą byłoby elementem, którego nie ma z czego opłacić. Napis kroku bieżącego JEST tym
 * zdaniem: `FIRST_INVITE` jest w `workspace-switcher.tsx` opisany dosłownie jako „zaproszenie
 * z pustego stanu, jedno zdanie, tryb rozkazujący (DESIGN §6)". Kiedy zapadka gęstości kiedyś
 * spadnie o dwa, zdanie wolno wyprowadzić do własnego `<p data-empty>` nad listą i wtedy ten
 * akapit znika razem ze znacznikiem na przycisku.
 */
import type { ReactElement } from 'react';

import { useSectionStore } from '../../ui/shell/section-store';
/* NAPIS PIERWSZEGO KROKU JEDZIE ZE STAŁEJ PRZEŁĄCZNIKA, nie z literału tutaj: „dodaj zakres" ma
 * w całej aplikacji jedno brzmienie, a dwie kopie tego samego zdania rozjeżdżają się przy
 * pierwszej zmianie i wtedy odmowa odsyła do przycisku o innej nazwie (niezmiennik 13). */
import { FIRST_INVITE } from '../../ui/shell/workspace-switcher';

/** Krok zrobiony, krok do zrobienia teraz, krok, który poczeka. */
export type FirstRunState = 'done' | 'now' | 'later';

export interface FirstRunStep {
  /** Czego ten krok dotyczy — po tym poznaje go kryterium i po tym wybiera się czynność. */
  readonly id: 'workspace' | 'agent' | 'workflow';
  /** Zdanie na ekranie: tryb rozkazujący, bez kropki (DESIGN §6). */
  readonly title: string;
  readonly state: FirstRunState;
}

/** Co naprawdę leży na dysku — trzy liczby, każda z listy, którą czyta jakaś sekcja. */
export interface WhatIsThere {
  readonly workspaces: number;
  readonly agents: number;
  readonly workflows: number;
}

/** Kolejność jest częścią odpowiedzi: bez folderu nie ma gdzie pracować, bez agenta nie ma kto. */
const ORDER = [
  { id: 'workspace', title: FIRST_INVITE },
  { id: 'agent', title: 'Add an agent' },
  { id: 'workflow', title: 'Build a workflow' },
] as const satisfies readonly { id: FirstRunStep['id']; title: string }[];

/**
 * Trzy kroki ze stanami — pierwszy niezrobiony jest bieżący, reszta czeka.
 *
 * DOKŁADNIE JEDEN krok jest bieżący, dopóki cokolwiek zostało: dwa akcenty naraz znaczą, że
 * człowiek ma wybrać, od czego zacząć, a to jest pytanie, którego pierwsze uruchomienie zadawać
 * nie ma prawa. Kiedy wszystko już leży, żaden nie jest bieżący i wołający tej listy nie rysuje
 * — przewodnik nad kompletem to trzy odhaczone wiersze zajmujące strefę pracy.
 */
export function firstRunSteps(there: WhatIsThere): readonly FirstRunStep[] {
  const done: Record<FirstRunStep['id'], boolean> = {
    workspace: there.workspaces > 0,
    agent: there.agents > 0,
    workflow: there.workflows > 0,
  };
  let lit = false;
  return ORDER.map((step) => {
    if (done[step.id]) return { ...step, state: 'done' as const };
    if (lit) return { ...step, state: 'later' as const };
    lit = true;
    return { ...step, state: 'now' as const };
  });
}

/** Czy z tej listy zostało cokolwiek do zrobienia — czyli czy przewodnik ma się w ogóle pokazać. */
export function somethingIsLeft(steps: readonly FirstRunStep[]): boolean {
  return steps.some((step) => step.state === 'now');
}

/* DWIE CZYNNOŚCI STOJĄ W MODULE, NIE W KOMPONENCIE, i to jest ta sama decyzja, co
 * w `workspace-switcher.tsx`: repo nie ma jsdom, więc kliknięcia nie da się odpalić w teście,
 * a `renderToStaticMarkup` nigdy nie woła `onClick`. Handler zamknięty w komponencie byłby
 * kodem, którego żadne kryterium nie umie dotknąć — czyli tą samą martwą kontrolką, przed którą
 * stoi niezmiennik 16. Tutaj kryterium woła dokładnie to, co woła przycisk. */

/** Zabiera na ekran Agents — tam, gdzie agenta się dodaje. */
export function openAgents(): void {
  useSectionStore.getState().go('agents');
}

/** Zabiera na ekran Workflows — tam, gdzie składa się kolejność pracy. */
export function openWorkflows(): void {
  useSectionStore.getState().go('workflows');
}

export interface FirstRunProps {
  readonly steps: readonly FirstRunStep[];
  /**
   * Pytanie o pierwszy folder. Wchodzi propsem, bo droga do dysku i zdanie odmowy należą do
   * ekranu Run (`index.tsx`, `openFolder`), a nie do tego bloku — druga kopia tego wywołania
   * byłaby drugim miejscem, z którego bierze się zakres (niezmiennik 23).
   */
  readonly onAddWorkspace: () => void;
}

export function FirstRun({ steps, onAddWorkspace }: FirstRunProps): ReactElement {
  const act: Record<FirstRunStep['id'], () => void> = {
    workspace: onAddWorkspace,
    agent: openAgents,
    workflow: openWorkflows,
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center">
      {/* `<ol>`, bo to jest kolejność, a nie zbiór: krok drugi bez pierwszego nie ma sensu,
          i czytnik ekranu ma o tym powiedzieć tak samo jak oko. */}
      <ol data-first-run className="stack" data-gap="2">
        {steps.map((step) => (
          <li
            key={step.id}
            data-first-step={step.id}
            data-step-state={step.state}
            /* Klasa siedzi na `<li>` WYŁĄCZNIE wtedy, gdy to on niesie tekst. Przy kroku
               bieżącym tekst niesie przycisk i on ma własny stopień. */
            className={step.state === 'now' ? undefined : 'lead'}
          >
            {step.state === 'now' ? (
              <button
                type="button"
                /* Patrz akapit „`data-empty` SIEDZI NA TYM PRZYCISKU" w nagłówku pliku. */
                data-empty
                /* Znacznik pierwszego kroku jest tym samym, którego szuka
                   `e2e/tests/plus-opens-a-terminal.spec.ts` i kolektor gęstości: „ekran wciąż
                   prosi o pierwszy folder". Stoi więc na kontrolce, która o niego prosi,
                   i znika razem z nią po pierwszym wskazaniu. */
                {...(step.id === 'workspace' ? { 'data-add-workspace': true } : {})}
                onClick={act[step.id]}
                className="btn-primary"
              >
                {step.title}
              </button>
            ) : /* STAN MIESZKA W TEKŚCIE WIERSZA, nie w pigułce obok — powód (jeden element
                 zapasu w zapadce gęstości) stoi w nagłówku pliku. Krok zrobiony ma to
                 POWIEDZIEĆ: wiersz czytający się tak samo przed i po zostawia człowieka
                 robiącego drugi raz to, co już zrobił. */
            step.state === 'done' ? (
              step.title + ' — done'
            ) : (
              step.title
            )}
          </li>
        ))}
      </ol>
    </div>
  );
}
