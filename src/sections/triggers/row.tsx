import type { ReactElement } from 'react';

import type { TriggerVisibleStatus, TriggerView } from '../../state/triggers';
import type { TriggerWorkspaceOption } from './form';

export interface TriggerRowProps {
  readonly trigger: TriggerView;
  readonly workspaces: readonly TriggerWorkspaceOption[];
  readonly onToggle: (slug: string, enabled: boolean) => Promise<void>;
  readonly onRunAgain: (slug: string) => Promise<void>;
  readonly onOpen: (slug: string) => void;
}

interface SaidStatus {
  readonly sentence: string;
  readonly machineTime?: string;
}

function utcStartTime(milliseconds: number): { readonly label: string; readonly iso: string } {
  const started = new Date(milliseconds);
  if (Number.isNaN(started.getTime())) {
    return { label: 'an unknown start time', iso: '' };
  }
  const iso = started.toISOString();
  return { label: `${iso.slice(0, 19).replace('T', ' ')} UTC`, iso };
}

function workspaceName(
  folder: string | null,
  workspaces: readonly TriggerWorkspaceOption[],
): string {
  if (folder === null) return 'an unspecified workspace';
  return workspaces.find((workspace) => workspace.folder === folder)?.name ?? folder;
}

function says(
  status: TriggerVisibleStatus,
  workspaces: readonly TriggerWorkspaceOption[],
): SaidStatus {
  if (status.kind === 'unchecked') return { sentence: 'Not checked yet.' };
  if (status.kind === 'armed') {
    return { sentence: 'Watching for new issues. Nothing has started yet.' };
  }
  if (status.kind === 'busy') {
    const workspace = status.delivery.claim.workspace;
    return {
      sentence: `${status.delivery.issue.identifier} is saved for ${workspaceName(workspace, workspaces)} while Loadout handles the run.`,
    };
  }
  if (status.kind === 'refused') return { sentence: status.sentence };
  /* Zdanie o wstrzymaniu ułożył Rust i to ono ma dotrzeć na ekran: mówi, że trigger przestał
   * pytać, dlaczego przestał i co z tym zrobić. Kontrolka obok jest tą jedną drogą powrotu. */
  if (status.kind === 'paused') return { sentence: status.sentence };
  if (status.retryRefusal !== undefined) return { sentence: status.retryRefusal };

  const started = utcStartTime(status.receiptAt);
  return {
    sentence: `Started ${status.workflow} in ${workspaceName(status.workspace, workspaces)} at ${started.label}.`,
    machineTime: started.iso,
  };
}

function sourceName(source: string): string {
  return source.toLowerCase() === 'linear' ? 'Linear' : 'Issue tracker';
}

/**
 * Nazwa warunku, którą czyta człowiek — i nigdy pustka.
 *
 * 2026-08-31, DWIE WADY NAPRAWIONE JEDNYM KSZTAŁTEM.
 *
 * PIERWSZA BYŁA WIDOCZNA. Normalizacja zamieniała myślniki na spacje, a pustkę łapał osobny
 * warunek `words.length === 0` — liczący długość PO tej zamianie. Warunek zapisany jako `-`
 * albo `_` schodził więc do pojedynczej SPACJI, miał długość 1, przechodził obok tamtego
 * warunku i lądował na ekranie jako pusta komórka: wiersz nie mówił wtedy nic o tym, kiedy
 * ten trigger w ogóle strzela. Przycięcie na KOŃCU normalizacji jest całą naprawą.
 *
 * DRUGA BYŁA O JEDEN KROK OD EKRANU. Napis powstawał z `words[0]?.toUpperCase() + words.slice(1)`,
 * czyli z dodawania czegoś, co MOŻE nie istnieć, do napisu — a `undefined + ''` w JavaScripcie
 * daje dosłowny napis „undefined". Nie dało się w to wejść wyłącznie dzięki warunkowi stojącemu
 * OBOK; wystarczyło, żeby ktoś kiedyś przeniósł tamten warunek albo zmienił normalizację, i na
 * ekranie stanęłoby „undefined". `slice(0, 1)` nie zna wartości nieistniejącej: na pustym napisie
 * oddaje pusty napis, więc pustka i wielka litera są tu JEDNYM pytaniem, a nie dwoma.
 */
function conditionName(condition: string): string {
  const words = condition.replace(/[-_\s]+/g, ' ').trim();
  if (words.toLowerCase() === 'assigned to me') return 'Assigned to you';
  const first = words.slice(0, 1).toUpperCase();
  return first === '' ? 'No condition saved' : first + words.slice(1);
}

function cadenceName(minutes: number): string {
  if (minutes === 1) return 'Every minute';
  if (minutes === 60) return 'Every hour';
  return `Every ${String(minutes)} minutes`;
}

/** One library row: at most four visible facts and bounded controls, with no invented broken config. */
export function TriggerRow({
  trigger,
  workspaces,
  onToggle,
  onRunAgain,
  onOpen,
}: TriggerRowProps): ReactElement {
  if (trigger.problem !== undefined) {
    return (
      <li data-trigger-row={trigger.slug} data-tone="attend" className="card stack" data-gap="2">
        <span data-trigger-text className="value">
          {trigger.slug}
        </span>
        <p data-trigger-text data-trigger-status className="lead" data-tone="attend">
          {trigger.problem}
        </p>
      </li>
    );
  }

  const status = says(trigger.status, workspaces);
  const statusWorkspace =
    trigger.status.kind === 'busy'
      ? trigger.status.delivery.claim.workspace
      : trigger.status.kind === 'accepted'
        ? trigger.status.workspace
        : null;
  const workflow = trigger.workflowName ?? `Workflow ${trigger.workflow} is missing.`;
  const workspace =
    trigger.workspace === null
      ? null
      : (workspaces.find((one) => one.folder === trigger.workspace) ?? null);
  const missingWorkspace = workspace === null;
  const workspaceLabel = workspace?.name ?? 'Workspace needs attention';
  const workspaceStatus =
    trigger.workspace === null
      ? 'Choose a workspace in Edit before this trigger can run.'
      : 'That workspace is no longer available. Choose another one in Edit before this trigger can run.';
  const toggleBlocked = missingWorkspace && !trigger.enabled;
  const retryLabel =
    !trigger.enabled || missingWorkspace
      ? null
      : trigger.status.kind === 'accepted'
        ? 'Run again'
        : trigger.status.kind === 'paused' ||
            (trigger.status.kind === 'refused' && trigger.status.retryable === true)
          ? 'Retry'
          : null;
  return (
    /* KARTA, NIE WIERSZ TABELI — 2026-08-31, przebudowa kompozycji.
     *
     * BYŁO: pięć komórek jednej wysokości w rzędzie, wszystkie tej samej głośności. Największym
     * napisem wiersza był warunek („Assigned to you"), czyli fakt IDENTYCZNY w każdym wierszu
     * biblioteki, a nazwa workflow — jedyna rzecz, która odpowiada na pytanie „co się odpali" —
     * stała w kolumnie `lead` z `truncate`, czyli w barwie drugoplanowej i z przycięciem.
     * Zmierzone na zrzucie 1512×950: trzy triggery zajmowały 135 px z 860 px ciała ekranu,
     * a metadana (źródło, warunek) była głośniejsza od treści.
     *
     * JEST: nadoczko z metadaną, pod nim JEDNA linia treści. Chip jest pigułką ODCZYTU
     * (DESIGN §6) i to jest dokładnie rola źródła i warunku — one nie są nazwą tej rzeczy,
     * tylko jej etykietami. Nazwa stoi pod nimi w stopniu `--text-subhead`, który istnieje
     * właśnie po to („tytuł kafelka na liście", theme.css:233) i jest w barwie `ink`.
     *
     * KOLEJNOŚĆ W DRZEWIE ZOSTAJE: źródło, warunek, wiersz o workflow, zdanie o stanie. Czyta
     * ją czytnik ekranu i czytają ją kryteria po pozycji (`status-is-text-not-a-button.test.tsx`,
     * `cells(row)[1]`), więc przestawienie jej CSS-em byłoby rozjazdem tego, co widać, z tym,
     * co słychać. Nadoczko nad tytułem jest zwykłą kolejnością czytania, nie obejściem. */
    <li data-trigger-row={trigger.slug} className="card stack" data-gap="2">
      {/* MYJKA POD KURSOREM, bo to jest wiersz listy, a nie napis: cały ten blok otwiera
          edytor, a do 2026-08-31 nie odpowiadał na najechanie ani jednym pikselem — kontrolka,
          która nie reaguje na kursor, czyta się jak etykieta (DESIGN §6, cztery stany).
          `.row` niesie myjkę, wciśnięcie i pierścień skupienia; `p-1 -m-1` daje jej oddech
          bez przesunięcia czegokolwiek, a `flex-col items-start` znosi jej `display:flex`
          w rzędzie, bo dwie linie są rozmieszczeniem i należą do miejsca, nie do roli. */}
      <button
        data-trigger-open
        type="button"
        aria-label={`Edit ${trigger.slug}`}
        className="row -m-1 flex min-w-0 flex-col items-start gap-2 p-1"
        onClick={() => {
          onOpen(trigger.slug);
        }}
      >
        <span className="flex min-w-0 flex-wrap items-center gap-2">
          <span data-trigger-text className="chip">
            {sourceName(trigger.source)}
          </span>
          <span data-trigger-text className="chip">
            {conditionName(trigger.condition)}
          </span>
        </span>
        {/* BEZ `truncate`, i to jest cała różnica między tym wierszem a poprzednim. Nazwa
            workflow jest treścią, a metadana ustępuje treści, nigdy odwrotnie: przy trzech
            kolumnach równej szerokości „Ship a fix" ginęło jako „Ship a…", żeby zmieściła się
            kadencja. Tutaj linia ma całą szerokość karty i zawija się na drugą, a kadencja
            i tak stoi na jej końcu. */}
        <span
          data-trigger-text
          data-trigger-workflow
          title={workspace?.folder ?? trigger.workspace ?? workspaceStatus}
          className="text-subhead text-ink"
        >
          {`${workflow} · ${workspaceLabel} · ${cadenceName(trigger.pollEveryMinutes)}`}
        </span>
      </button>
      {/* STOPKA KARTY: co się stało, i najwyżej dwie rzeczy, które można z tym zrobić.
          Kreska nad nią oddziela odczyt od czynności — bez niej przełącznik czytał się jak
          część zdania o stanie. */}
      <div className="flex items-center gap-3 border-t border-line pt-2">
        {/* CO SIĘ STAŁO — TEKST, ZAWSZE, NIEZALEŻNIE OD TEGO, CZY JEST CO KLIKNĄĆ.
            2026-08-31: do tego dnia przy wierszu z czynnością zdanie o stanie było ETYKIETĄ
            przycisku (`${status.sentence} · ${retryLabel}`). Żeby przeczytać, co się właściwie
            stało, trzeba było wodzić kursorem po żywym „Run again" — a przypadkowe kliknięcie
            ODPALA BIEG. Zdanie się czyta, czynność się robi; jedna kontrolka na oba znaczy, że
            czytanie kosztuje ryzyko (niezmiennik 16 od drugiej strony: kontrolka, która niesie
            treść niebędącą jej nazwą, kłamie o tym, co zrobi po kliknięciu). */}
        <span
          data-trigger-text
          data-trigger-status
          title={statusWorkspace ?? undefined}
          className="lead min-w-0 flex-1"
        >
          {missingWorkspace ? workspaceStatus : status.sentence}
        </span>
        {retryLabel === null ? null : (
          /* `.btn` drugoplanowy: obrys `--line-strong` i wypełnienie szkła to dokładnie ten
             prymityw, a razem z nim wchodzą cztery stany, których ta kontrolka nie miała.
             GOŁY PRYMITYW, bez ani jednej klasy obok: nazwa czynności jest teraz krótka i jedna,
             więc geometria, oddech i `white-space: nowrap` z `.btn` wystarczają. Miejscowa
             wysokość, sufit szerokości i łamanie wiersza zniknęły razem ze zdaniem, które
             musiało się w tej kolumnie łamać. */
          <button
            type="button"
            data-trigger-text
            data-trigger-run-again
            className="btn"
            onClick={() => {
              void onRunAgain(trigger.slug);
            }}
          >
            {retryLabel}
          </button>
        )}
        {status.machineTime === undefined ? null : (
          <time aria-hidden dateTime={status.machineTime} />
        )}
        {/* `.btn-bare` wnosi tu WYŁĄCZNIE cztery stany — wygaszenie i `not-allowed` przy
            `disabled`, wciśnięcie, pierścień skupienia. Geometria przełącznika zostaje
            miejscowa, bo to nie jest przycisk z etykietą, tylko tor z gałką. Ręczny bliźniak
            `opacity-50` znika: stan wyłączony jest regułą przy `:disabled`, a nie drugą
            klasą (DESIGN §6). */}
        <button
          type="button"
          data-trigger-toggle
          disabled={toggleBlocked}
          aria-pressed={trigger.enabled}
          aria-label={
            toggleBlocked
              ? `Choose a workspace before changing ${trigger.slug}`
              : `${trigger.enabled ? 'Turn off' : 'Turn on'} ${trigger.slug}`
          }
          className="btn-bare h-5 w-9 justify-start rounded-pill border border-line-strong bg-raised px-0.5"
          onClick={() => {
            void onToggle(trigger.slug, !trigger.enabled);
          }}
        >
          <span
            aria-hidden
            className={`size-3.5 rounded-pill ${trigger.enabled ? 'ml-auto bg-accent' : 'bg-line-strong'}`}
          />
        </button>
      </div>
    </li>
  );
}
