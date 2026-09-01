import type { ReactElement } from 'react';
import { useEffect, useSyncExternalStore } from 'react';

import type { LabState } from '../../state/lab';
import { useLab } from '../../state/lab';
import { sectionEntry } from '../../ui/sections';
import type { Section } from '../../ui/sections';
import { modelOf, nextColumn, withModel, withName } from './columns';
import type { EvalCell, EvalSet, EvalVariant, PastEval } from './io';
import { Matrix } from './matrix';
import { scoreOf, spendOf, suggestedCases, tableFor, trendOf } from './model';
import { Suggestion } from './suggestion';
import { Trend } from './trend';

/* Ekran Lab: lista zestawów po lewej, tabela po prawej.
 *
 * TABELA MÓWI „JAK JEST TERAZ", TREND MÓWI „CZY SIĘ POPRAWIA", a to są dwa różne pytania
 * i dlatego stoją osobno. Jedna kontrolka odpowiadająca na oba byłaby wykresem, na którym
 * nie widać, który wiersz się zepsuł.
 *
 * ŻADNEJ KONTROLKI BEZ SKUTKU (niezmiennik 16). Wybór agenta rysuje się wyłącznie wtedy, gdy
 * biblioteka kogoś ma; Run rysuje się wyłącznie przy otwartym zestawie; Stop wyłącznie wtedy,
 * gdy jest co zatrzymać. Każda z tych trzech rzeczy wygaszona „na wszelki wypadek" byłaby
 * kontrolką, która na kliknięcie nie ma odpowiedzi.
 */

export interface LabScreenProps {
  /** Magazyn wchodzi propsem, żeby kryteria mogły podać własny — produkcja używa jednego. */
  readonly store?: typeof useLab;
}

export default function LabScreen({ store = useLab }: LabScreenProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  const empty = sectionEntry('lab' as Section).empty;

  useEffect(() => {
    void store.getState().load();
  }, [store]);

  /* Kandydatki pisze PIERWSZY agent biblioteki, dopóki człowiek nie powie inaczej — a mówi to
   * dziś tam, gdzie stoi ten agent (przycisk „Evaluate" w sekcji Agents). Pole wyboru w tym
   * nagłówku byłoby drugą odpowiedzią na pytanie „kto to robi", a pierwszą zadaje się przy
   * zakładaniu zestawu. */
  const chosenWriter = state.agents[0]?.id ?? '';

  return (
    <section data-lab-screen className="flex h-full flex-col">
      <header className="flex h-13 items-center gap-3 border-b border-line bg-panel px-4">
        <h1 className="text-title text-ink">Lab</h1>
        {state.board === null ? null : (
          <span className="text-ui text-muted">{scoreOf(state.board)}</span>
        )}
        <div className="ml-auto flex items-center gap-2">
          {state.busy === 'proposing' ? (
            <button
              data-lab-stop
              type="button"
              className="h-9 rounded-sm border border-line px-3 text-ui text-body"
              onClick={() => {
                void store.getState().stopProposing();
              }}
            >
              Stop
            </button>
          ) : null}
          {state.board === null || state.agents.length === 0 ? null : (
            <button
              data-lab-propose
              type="button"
              disabled={state.busy !== 'idle'}
              className="h-9 rounded-sm border border-line px-3 text-ui text-body"
              onClick={() => {
                void store.getState().propose(chosenWriter);
              }}
            >
              Write cases
            </button>
          )}
          {state.board === null ? null : (
            <button
              data-lab-run
              type="button"
              /* WYGASZONY, KIEDY NIE MA CZEGO URUCHOMIC. Powod stoi na ekranie zdaniem
                 `cannotRun` — a przycisk, ktory po klikniecu tylko powtarza to zdanie, jest
                 kontrolka bez skutku i drugim miejscem, w ktorym mieszka ten sam fakt
                 (niezmienniki 16 i 13). */
              disabled={state.busy !== 'idle' || state.board.cannotRun !== null}
              className="h-9 rounded-sm bg-accent px-4 text-ui text-bg"
              onClick={() => {
                void store.getState().run();
              }}
            >
              Run
            </button>
          )}
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        {state.sets.length === 0 ? null : (
          <nav className="min-h-0 w-60 overflow-auto border-r border-line bg-panel p-2">
            <ul>
              {state.sets.map((one) => (
                <li key={one.id}>
                  <button
                    data-lab-set={one.id}
                    type="button"
                    aria-current={one.id === state.openId ? 'true' : undefined}
                    className={
                      'w-full rounded-sm px-2 py-1.5 text-left text-body ' +
                      (one.id === state.openId ? 'bg-hover text-ink' : 'text-body')
                    }
                    onClick={() => {
                      void store.getState().open(one.id);
                    }}
                  >
                    {one.name}
                  </button>
                </li>
              ))}
            </ul>
          </nav>
        )}

        <div className="min-h-0 flex-1 overflow-auto p-4">
          {state.said === null ? null : (
            <p className="mb-3 max-w-160 text-body text-attend">{state.said}</p>
          )}
          {state.sets.length === 0 ? (
            <FirstSet empty={empty} state={state} store={store} />
          ) : state.board === null ? (
            <p className="text-body text-muted">Pick a set on the left to see how it is doing.</p>
          ) : (
            <Board state={state} store={store} />
          )}
        </div>
      </div>
    </section>
  );
}

/** Pusty ekran: zdanie, zaproszenie i jedna czynna kontrolka. */
function FirstSet({
  empty,
  state,
  store,
}: {
  readonly empty: string;
  readonly state: LabState;
  readonly store: typeof useLab;
}): ReactElement {
  const first = state.agents[0];
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3">
      <span className="flex size-8 items-center justify-center rounded-md border border-dashed border-line-strong text-muted">
        ◇
      </span>
      <p data-empty className="text-body text-ink">
        {empty}
      </p>
      <p className="max-w-120 text-center text-body text-muted">
        Pick one of your agents and Loadout will draft cases from this project, so you can see
        whether a change to it made the work better.
      </p>
      {first === undefined ? (
        <p className="text-ui text-muted">Save an agent first, over in Agents.</p>
      ) : (
        <button
          data-lab-first
          type="button"
          disabled={state.busy !== 'idle'}
          className="h-9 rounded-sm bg-accent px-4 text-ui text-bg"
          onClick={() => {
            void store.getState().create(first.name, { kind: 'agent', id: first.id }, first.id);
          }}
        >
          Evaluate {first.name}
        </button>
      )}
    </div>
  );
}

/** Otwarty zestaw: tabela, trend, to co nie przeszło, i kandydatki. */
function Board({
  state,
  store,
}: {
  readonly state: LabState;
  readonly store: typeof useLab;
}): ReactElement | null {
  const board = state.board;
  if (board === null) return null;
  const newest: PastEval | null = board.runs[0] ?? null;
  const table = tableFor(board.set.set, newest);
  const waiting = suggestedCases(board.set.set);
  const trend = trendOf(board.runs);

  return (
    <div className="flex flex-col gap-4">
      {board.cannotRun === null ? null : (
        <p className="max-w-160 text-body text-attend">{board.cannotRun}</p>
      )}

      {/* TABELA WYLACZNIE Z WIERSZAMI. Sam naglowek nad zerem wierszy nie jest pusta
          tabela — jest obietnica, ze cos tam jest, i czlowiek szuka wzrokiem czegos, czego
          nikt nie napisal. Czego brakuje, mowi zdanie `cannotRun` nad tym miejscem. */}
      {table.rows.length === 0 ? null : <Matrix table={table} />}

      <Columns state={state} store={store} />

      {trend.length < 2 ? null : <Trend shares={trend} />}

      <WhatDidNotPass set={board.set.set} run={newest} state={state} store={store} />

      {state.fix === null ? null : <ProposedFix state={state} store={store} />}

      {waiting.length === 0 ? null : (
        <section data-lab-waiting>
          <h2 className="mb-2 text-ui text-muted">Waiting for you</h2>
          <ul className="overflow-hidden rounded-md border border-line bg-panel">
            {waiting.map((one) => (
              <Suggestion
                key={one.id}
                one={one}
                busy={state.busy !== 'idle'}
                onKeep={(id) => {
                  void store.getState().decide(id, true);
                }}
                onDiscard={(id) => {
                  void store.getState().decide(id, false);
                }}
                onEdit={(edited) => {
                  void store.getState().putCase(edited);
                }}
              />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

/** Kolumny: co je od siebie różni i jak dołożyć następną. */
function Columns({
  state,
  store,
}: {
  readonly state: LabState;
  readonly store: typeof useLab;
}): ReactElement | null {
  const board = state.board;
  if (board === null) return null;
  const columns = board.set.set.variants;
  const put = (variant: EvalVariant): void => {
    void store.getState().putVariant(variant);
  };
  return (
    <section data-lab-columns>
      <h2 className="mb-2 text-ui text-muted">What tells the columns apart</h2>
      <ul className="overflow-hidden rounded-md border border-line bg-panel">
        {columns.map((one) => (
          <li
            key={one.id}
            className="flex items-center gap-2 border-b border-line-subtle p-2 last:border-b-0"
          >
            {/* ETYKIETA WIDOCZNA, nie sam `aria-label`. Zmierzone na zywym ekranie
                2026-08-31: czlowiek wpisal nazwe modelu w pole obok, bo placeholder znika po
                pierwszym znaku i nic juz nie mowi, czym jest to, co wpisal. Napis w `<label>`
                staje sie przy okazji nazwa dostepna kontrolki, wiec `aria-label` schodzi —
                dwa zrodla tej samej nazwy to jedno, ktore kiedys sie rozjedzie. */}
            <label className="text-ui text-muted">
              Name
              <input
                data-lab-column-name={one.id}
                defaultValue={one.name}
                disabled={state.busy !== 'idle'}
                className="ml-2 h-8 w-40 rounded-sm border border-line bg-well px-2 text-ui text-ink"
                onBlur={(event) => {
                  put(withName(one, event.target.value));
                }}
              />
            </label>
            {/* MODEL I NIC POZA NIM. Dial, narzędzia i limit czasu mieszkają w formularzu
                agenta i mają tam zostać: drugi formularz agenta wewnątrz tabeli byłby drugim
                miejscem, w którym mieszka odpowiedź „czym ten agent jest". */}
            <label className="flex flex-1 items-center gap-2 text-ui text-muted">
              Model
              <input
                data-lab-column-model={one.id}
                placeholder="the one this agent already has"
                defaultValue={modelOf(one)}
                disabled={state.busy !== 'idle'}
                className="h-8 flex-1 rounded-sm border border-line bg-well px-2 text-ui text-ink"
                onBlur={(event) => {
                  put(withModel(one, event.target.value));
                }}
              />
            </label>
            {/* Ostatniej kolumny nie da się zdjąć: zestaw bez ani jednej nie ma czego uruchomić,
                a odmowa po kliknięciu jest kontrolką, która kłamie. */}
            {columns.length < 2 ? null : (
              <button
                data-lab-drop-column={one.id}
                type="button"
                aria-label={'Remove this column'}
                disabled={state.busy !== 'idle'}
                className="h-8 rounded-sm border border-line px-2 text-ui text-muted"
                onClick={() => {
                  void store.getState().dropVariant(one.id);
                }}
              >
                ×
              </button>
            )}
          </li>
        ))}
      </ul>
      <button
        data-lab-add-column
        type="button"
        disabled={state.busy !== 'idle'}
        className="mt-2 h-8 rounded-sm border border-line px-3 text-ui text-body"
        onClick={() => {
          const made = nextColumn(columns, state.agents[0]?.id ?? '');
          if (made !== null) put(made);
        }}
      >
        Add column
      </button>
    </section>
  );
}

/** Poprawka czekająca na człowieka: powód, oba teksty, Apply i Discard. */
function ProposedFix({
  state,
  store,
}: {
  readonly state: LabState;
  readonly store: typeof useLab;
}): ReactElement | null {
  const fix = state.fix;
  if (fix === null) return null;
  return (
    <section data-lab-fix className="rounded-md border border-line bg-panel p-3">
      <h2 className="text-ui text-muted">What {fix.name} would be told instead</h2>
      {/* POWÓD STOI NAD TEKSTEM, nie pod nim. Ściana znaków bez zdania o tym, co ma naprawić,
          jest ścianą, którą akceptuje się albo odrzuca bez czytania — a ten przycisk zmienia
          zachowanie agenta w każdym jego przyszłym biegu. */}
      <p className="mt-1 max-w-160 text-body text-ink">{fix.because}</p>
      <pre className="mt-2 max-h-80 overflow-auto rounded-sm border border-line-subtle p-2 text-note text-body">
        {fix.instructions}
      </pre>
      <div className="mt-2 flex gap-2">
        <button
          data-lab-apply-fix
          type="button"
          disabled={state.busy !== 'idle'}
          className="h-8 rounded-sm bg-accent px-3 text-ui text-bg"
          onClick={() => {
            void store.getState().applyFix();
          }}
        >
          Apply
        </button>
        <button
          data-lab-drop-fix
          type="button"
          className="h-8 rounded-sm border border-line px-3 text-ui text-body"
          onClick={() => {
            store.getState().dropFix();
          }}
        >
          Discard
        </button>
      </div>
    </section>
  );
}

/** Lista tego, co nie przeszło — jedno zdanie na komórkę, w kolejności tabeli. */
function WhatDidNotPass({
  set,
  run,
  state,
  store,
}: {
  readonly set: EvalSet;
  readonly run: PastEval | null;
  readonly state: LabState;
  readonly store: typeof useLab;
}): ReactElement | null {
  if (run === null) return null;
  const failed = run.cells.filter((cell) => cell.outcome === 'did-not-pass');
  if (failed.length === 0) return null;
  const nameOf = (cell: EvalCell): string => {
    const row = set.cases.find((one) => one.id === cell.case)?.name ?? cell.case;
    const column = set.variants.find((one) => one.id === cell.variant)?.name ?? cell.variant;
    return row + ' · ' + column;
  };
  /* PRZYCISK WYŁĄCZNIE PRZY AGENCIE, i to nie jest niedokończona gałąź. Zmiana `SKILL.md`
   * musi przejść przez ten sam skaner wstrzyknięć, co umiejętność wciągnięta z linku
   * (`skills::ingest`), a jedyną drogą tamtędy jest sekcja Skills. Przycisk odmawiający po
   * kliknięciu byłby kontrolką, która kłamie; zdanie obok mówi, gdzie iść. */
  const forAnAgent = set.subject.kind === 'agent';
  const writer = state.agents[0]?.id ?? '';
  return (
    <section data-lab-failures>
      <div className="mb-2 flex items-center gap-3">
        <h2 className="text-ui text-muted">What did not pass</h2>
        {forAnAgent && writer !== '' ? (
          <button
            data-lab-ask-fix
            type="button"
            disabled={state.busy !== 'idle'}
            className="h-8 rounded-sm border border-line px-3 text-ui text-body"
            onClick={() => {
              void store.getState().askForAFix(writer);
            }}
          >
            Propose a fix
          </button>
        ) : null}
        {forAnAgent ? null : (
          <p className="text-ui text-muted">
            A change to a skill goes through the same check as one pasted from a link, so it is
            written over in Skills.
          </p>
        )}
      </div>
      <ul className="overflow-hidden rounded-md border border-line bg-panel">
        {failed.map((cell) => (
          <li
            key={cell.case + cell.variant}
            className="border-b border-line-subtle p-3 last:border-b-0"
          >
            <p className="text-body text-ink">{nameOf(cell)}</p>
            <p className="mt-1 max-w-160 text-ui text-body">{cell.said}</p>
            {cell.costUsd === null ? null : (
              <p className="mt-1 text-ui text-muted">{spendOf(cell.costUsd)}</p>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
