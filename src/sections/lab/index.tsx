import type { ReactElement } from 'react';
import { useEffect, useState, useSyncExternalStore } from 'react';

import type { LabState } from '../../state/lab';
import { useLab } from '../../state/lab';
import { sectionEntry } from '../../ui/sections';
import type { Section } from '../../ui/sections';
import { isDirty, nextColumn, typedOf, withTyped } from './columns';
import type { Typed } from './columns';
import type { EvalCell, EvalSet, EvalVariant, PastEval } from './io';
import { Matrix } from './matrix';
import {
  count,
  howItEnded,
  howManyCells,
  runningCases,
  scoreOf,
  spendOf,
  suggestedCases,
  tableFor,
  theNextMoveIs,
  trendOf,
} from './model';
import { Suggestion } from './suggestion';
import { Trend } from './trend';

/* Ekran Lab: lista zestawów po lewej, jeden bohater i tabela po prawej.
 *
 * TABELA MÓWI „JAK JEST TERAZ", TREND MÓWI „CZY SIĘ POPRAWIA", a to są dwa różne pytania
 * i dlatego stoją osobno. Jedna kontrolka odpowiadająca na oba byłaby wykresem, na którym
 * nie widać, który wiersz się zepsuł.
 *
 * ŻADNEJ KONTROLKI BEZ SKUTKU (niezmiennik 16). Wybór agenta rysuje się wyłącznie wtedy, gdy
 * biblioteka kogoś ma; Run rysuje się wyłącznie przy otwartym zestawie; Stop wyłącznie wtedy,
 * gdy jest co zatrzymać. Każda z tych trzech rzeczy wygaszona „na wszelki wypadek" byłaby
 * kontrolką, która na kliknięcie nie ma odpowiedzi.
 *
 * ── PRZEBUDOWA 2026-08-31, ZARZUT WŁAŚCICIELA SŁOWO W SŁOWO ────────────────────────────────
 *
 * „nie spełnia naszych standardów ux/ui, nie ma żadnych informacji jak klikniesz co się
 * dzieje / stanów". Dwie wady, obie zmierzone w kodzie:
 *
 * (a) NIE WIDAĆ, CO SIĘ STANIE PO KLIKNIĘCIU. `Run`, `Write cases` i `Stop` stały jako trzy
 *     gołe czasowniki. Jedyne zdanie o którymkolwiek z nich pojawiało się wtedy, gdy
 *     naciśnięcie było NIEMOŻLIWE (`cannotRun`) — czyli dokładnie wtedy, gdy było bezużyteczne.
 *     Dziś każda kontrolka niesie zdanie o tym, co zrobi, ZANIM się ją naciśnie.
 *
 * (b) NIE MA STANÓW. Cztery prace tej sekcji przechodzą granicę i trwają minuty; ekran nie
 *     zmieniał na to ani jednego piksela poza wygaszeniem kontrolek. Naciśnięty `Run` wyglądał
 *     identycznie jak `Run`, który nie doszedł — a drugie naciśnięcie jest wtedy winą
 *     interfejsu, nie człowieka. Cztery stany mają dziś nośnik: [`Working`].
 *
 * BOHATER JEST JEDEN i jest nim NAZWA ZESTAWU (`text-display`, 40 px). Nazwa sekcji zeszła
 * do `h2` w pasku — ten sam ruch i ten sam powód, co na ekranie biegu (`run/strip/head.tsx`):
 * oko czyta 40 px jako rzecz ważniejszą, czytnik ekranu czyta `h1` jako rzecz ważniejszą,
 * więc jeśli to dwa różne napisy, ekran mówi dwie różne rzeczy zależnie od tego, czym się go
 * czyta.
 */

/** Kontrolka, która prowadzi: akcent i pełna waga. Na ekranie jest zawsze dokładnie jedna. */
const LEADS = 'h-9 rounded-sm bg-accent px-4 text-ui text-bg';

/** Kontrolka, która jest do dyspozycji, ale nie jest następnym ruchem. */
const FOLLOWS = 'h-9 rounded-sm border border-line px-3 text-ui text-body';

/** Co zrobi `Run`, powiedziane zanim ktokolwiek go naciśnie. */
const RUN_WILL =
  'Measure every case in this set, in every column. It goes out as an ordinary run, so you can ' +
  'watch it over in Run.';

/** To samo o `Stop`: mówi, co zatrzymuje, i czego NIE zatrzymuje. */
const STOP_WILL =
  'Stop the agent drafting cases. A run that is measuring the set is stopped over in Run.';

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
  const writer = state.agents[0];
  const chosenWriter = writer?.id ?? '';

  return (
    <section data-lab-screen className="flex h-full flex-col">
      {/* `.screen-head` niesie wysokość 52 px, odstępy i kreskę; tła nie niesie z rozmysłu,
          więc dokłada je `.glass`. Szkło jest chrome, papier jest treścią. */}
      <header className="screen-head glass">
        {/* `h2`, NIE `h1` — bohaterem tego ekranu jest nazwa zestawu, nie nazwa sekcji.
            Powód w całości w nagłówku pliku. */}
        <h2 className="text-heading text-ink">Lab</h2>

        <div className="ml-auto flex items-center gap-2">
          {state.busy === 'proposing' ? (
            <button
              data-lab-stop
              type="button"
              title={STOP_WILL}
              className={FOLLOWS}
              onClick={() => {
                void store.getState().stopProposing();
              }}
            >
              Stop
            </button>
          ) : null}
          {/* AKCENT NOSI TO, CO DA SIĘ ZROBIĆ — powód w całości przy `model.theNextMoveIs`.
              Dwie klasy, jedna decyzja: przy zestawie, którego nie ma czym uruchomić, największą
              rzeczą na ekranie jest `Write cases`, a `Run` schodzi do obrysu. */}
          {state.board === null || writer === undefined ? null : (
            <button
              data-lab-propose
              type="button"
              disabled={state.busy !== 'idle'}
              title={
                'Ask ' +
                writer.name +
                ' to read this project and draft cases for this set. You decide which of them ' +
                'count — nothing is measured until you accept one.'
              }
              className={theNextMoveIs(state.board.cannotRun) === 'write-cases' ? LEADS : FOLLOWS}
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
              title={RUN_WILL}
              className={theNextMoveIs(state.board.cannotRun) === 'run' ? LEADS : FOLLOWS}
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
          <nav
            aria-label="Sets in this project"
            className="min-h-0 w-60 overflow-auto border-r border-line bg-panel p-2"
          >
            <ul>
              {state.sets.map((one) => (
                <li key={one.id}>
                  <button
                    data-lab-set={one.id}
                    type="button"
                    aria-current={one.id === state.openId ? 'true' : undefined}
                    title={'Open ' + one.name + ' and read how its last run went.'}
                    className="row"
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

        <div className="screen-body">
          <Hero state={state} />
          <Working state={state} />
          {state.said === null ? null : (
            <p className="mt-3 max-w-160 lead" data-tone="attend">
              {state.said}
            </p>
          )}
          <div className="mt-4">
            {state.sets.length === 0 && state.busy !== 'loading' ? (
              <FirstSet empty={empty} state={state} store={store} />
            ) : state.board === null ? null : (
              <Board state={state} store={store} />
            )}
          </div>
        </div>
      </div>
    </section>
  );
}

/**
 * Bohater ekranu: nadoczko w akcencie, tytuł w stopniu display, jedno zdanie i wynik.
 *
 * JEDEN NA EKRAN. Do 2026-08-31 największą rzeczą w tej sekcji był napis „Lab" w 20 px, więc
 * ekran nie miał czym poprowadzić oka i każda próba hierarchii kończyła się szarym prostokątem
 * obok szarego prostokąta — to jest zmierzona przyczyna, dla której właściciel odrzucił dwie
 * poprzednie przebudowy interfejsu (DESIGN §4), a nie kwestia gustu.
 *
 * WYNIK STOI TUTAJ, a nie w pasku, bo jest faktem o TYM zestawie, nie o sekcji. Zdanie o tym,
 * jak skończył się przebieg, stoi zaraz pod nim, bo bez niego wynik bywa nie do przeczytania —
 * cały powód przy `model.howItEnded`.
 */
function Hero({ state }: { readonly state: LabState }): ReactElement {
  const board = state.board;
  const newest: PastEval | null = board?.runs[0] ?? null;
  const ending = newest === null ? '' : howItEnded(newest);

  const eyebrow =
    board === null ? 'Lab' : board.set.set.subject.kind === 'agent' ? 'Agent' : 'Skill';
  const title =
    board !== null
      ? board.set.set.name
      : state.busy === 'loading'
        ? 'Lab'
        : state.sets.length === 0
          ? 'Try an agent on your own code'
          : 'Nothing open yet';
  const lead =
    board !== null
      ? board.set.set.subject.kind === 'agent'
        ? 'Every case runs in every column, so you can see whether a change to this agent made ' +
          'its work better.'
        : 'Every case runs with this skill and without it, so you can see what the skill is worth.'
      : state.busy === 'loading'
        ? ''
        : state.sets.length === 0
          ? ''
          : 'Pick a set on the left to see how it is doing.';

  return (
    <div data-lab-hero className="stack" data-gap="2">
      <p className="text-eyebrow">{eyebrow}</p>
      <h1 className="text-display text-ink">{title}</h1>
      {lead === '' ? null : <p className="max-w-160 lead">{lead}</p>}
      {board === null ? null : <p className="value">{scoreOf(board)}</p>}
      {/* ZDANIE O CAŁYM PRZEBIEGU, nie o komórce. Powód komórki stoi w liście pod tabelą i jest
          tam po jednym na komórkę; ten fakt dotyczy biegu i nie da się go z tamtych złożyć. */}
      {ending === '' ? null : (
        <p data-lab-ending className="max-w-160 lead" data-tone="attend">
          {ending}
        </p>
      )}
    </div>
  );
}

/**
 * Cztery prace, które trwają — i to, czym ekran o nich mówi.
 *
 * # Dlaczego to w ogóle istnieje
 *
 * Cztery czynności tej sekcji przechodzą granicę i trwają od sekund do minut: czytanie listy,
 * pisanie kandydatek, mierzenie zestawu i zapis. Do 2026-08-31 ekran nie mówił o ŻADNEJ z nich
 * ani słowa — wygaszał kontrolki i tyle. Kliknięcie, po którym ekran milczy, czyta się jak
 * kliknięcie, które nie doszło.
 *
 * NAJGORSZY BYŁ PIERWSZY. `sets` startuje jako pusta lista, a ekran rozgałęział się po jej
 * długości, więc człowiek z trzema zestawami i dwudziestoma agentami czytał przez czas dwóch
 * odczytów granicy „Sets you build to test agents will be listed here." **plus** „Save an agent
 * first, over in Agents." Dwa zdania, oba nieprawdziwe, oba wyglądające jak spokojna odpowiedź.
 * To jest awaria, która wygląda jak działanie, i `src/ui/shell/what-you-have.ts` broni się przed
 * nią trzema stanami; Lab tej obrony nie miał.
 *
 * PASEK JEST NIEOKREŚLONY, i to jest uczciwość, nie lenistwo: żadna z tych czterech prac nie
 * melduje postępu, więc pasek rosnący do stu procent rysowałby liczbę, której nikt nie zmierzył
 * (niezmiennik 17).
 */
function Working({ state }: { readonly state: LabState }): ReactElement | null {
  if (state.busy === 'idle') return null;
  const set = state.board?.set.set ?? null;

  if (state.busy === 'loading') {
    return (
      <Doing marker="data-lab-loading">
        {state.sets.length === 0
          ? 'Reading the sets in this project…'
          : 'Reading how this set has been doing…'}
      </Doing>
    );
  }
  if (state.busy === 'proposing') {
    return (
      <Doing marker="data-lab-proposing">
        {(state.agents[0]?.name ?? 'An agent') +
          ' is reading this project and drafting cases. It reads only, changes nothing, and ' +
          'every case it writes waits for you to accept it.'}
      </Doing>
    );
  }
  if (state.busy === 'running' && set !== null) {
    return (
      <Doing marker="data-lab-running">
        {'Measuring ' +
          count(howManyCells(set), 'cell', 'cells') +
          ' — ' +
          count(runningCases(set).length, 'case', 'cases') +
          ' across ' +
          count(set.variants.length, 'column', 'columns') +
          '. It goes out as an ordinary run, so open Run to watch it work; the table here fills ' +
          'in when you pick this set again.'}
      </Doing>
    );
  }
  if (state.busy === 'saving') {
    return <Doing marker="data-lab-saving">Saving this to the set file…</Doing>;
  }
  return null;
}

/** Jedno pudełko na jedną trwającą pracę: pasek, zdanie, znacznik dla wyroczni. */
function Doing({
  marker,
  children,
}: {
  readonly marker:
    'data-lab-loading' | 'data-lab-proposing' | 'data-lab-running' | 'data-lab-saving';
  readonly children: string;
}): ReactElement {
  const marked = { [marker]: '' };
  return (
    /* JEDEN REGION ANIMOWANY, nie dwa. Sufit gęstości daje na jedno zdarzenie dwa
       (`docs/ARCHITECTURE.md` §7), a wejście karty przez `.fade-in` obok paska zjadałoby oba
       na fakt, który niesie sam pasek. */
    <div {...marked} className="card mt-4 max-w-160" data-tone="live">
      <div className="working" aria-hidden="true" />
      <p className="mt-2 text-body text-ink">{children}</p>
    </div>
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
    <div className="flex flex-col items-start gap-3">
      <span className="mark" aria-hidden="true">
        ◇
      </span>
      <p data-empty className="text-body text-ink">
        {empty}
      </p>
      <p className="max-w-120 lead">
        Pick one of your agents and Loadout will draft cases from this project, so you can see
        whether a change to it made the work better.
      </p>
      {first === undefined ? (
        <p className="lead">Save an agent first, over in Agents.</p>
      ) : (
        <button
          data-lab-first
          type="button"
          disabled={state.busy !== 'idle'}
          title={
            'Start a set for ' +
            first.name +
            '. Loadout writes it to this project and opens it here.'
          }
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
        <p className="max-w-160 lead" data-tone="attend">
          {board.cannotRun}
        </p>
      )}

      {/* TABELA WYLACZNIE Z WIERSZAMI. Sam naglowek nad zerem wierszy nie jest pusta
          tabela — jest obietnica, ze cos tam jest, i czlowiek szuka wzrokiem czegos, czego
          nikt nie napisal. Czego brakuje, mowi zdanie `cannotRun` nad tym miejscem. */}
      {table.rows.length === 0 ? null : <Matrix table={table} />}

      {/* TREND ZARAZ POD TABELĄ, bo to on odpowiada na pytanie, dla którego ta sekcja powstała.
          Krótszy niż dwa przebiegi nie jest linią — i wtedy mówi to zdaniem, zamiast znikać
          bez śladu dokładnie wtedy, gdy człowiek pyta pierwszy raz. */}
      {trend.length < 2 ? (
        newest === null ? null : (
          <p className="max-w-160 lead">
            This is the first run with something to measure. Run the set again after a change and a
            line here will say whether it is getting better.
          </p>
        )
      ) : (
        <Trend shares={trend} />
      )}

      <Columns state={state} store={store} />

      <WhatDidNotPass set={board.set.set} run={newest} state={state} store={store} />

      {state.fix === null ? null : <ProposedFix state={state} store={store} />}

      {waiting.length === 0 ? null : (
        <section data-lab-waiting>
          <h2 className="mb-2 text-eyebrow">Waiting for you</h2>
          <ul className="paper">
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

/** Kolumny: co je od siebie różni i jak dołożyć następną.
 *
 * POLA SĄ KONTROLOWANE, A NIEZAPISANE MÓWI TO WPROST. Do 2026-08-31 stały tu pola
 * niekontrolowane, zapisywane wyłącznie przy `onBlur` — wpisana wartość żyła w DOM i nigdzie
 * indziej, a ekran nie odróżniał jej niczym od zapisanej. Powód i pomiar stoją w całości przy
 * `columns.isDirty`.
 *
 * TRZY DROGI ZAPISU, wszystkie prowadzą w to samo miejsce: wyjście z pola, Enter i przycisk,
 * który pojawia się DOPIERO, gdy jest co zapisać. Przycisk stojący tam zawsze byłby kontrolką,
 * która przez większość czasu nie ma nic do zrobienia.
 */
function Columns({
  state,
  store,
}: {
  readonly state: LabState;
  readonly store: typeof useLab;
}): ReactElement | null {
  const [edited, setEdited] = useState<Record<string, Typed>>({});
  const board = state.board;
  if (board === null) return null;
  const columns = board.set.set.variants;

  const forget = (id: string): void => {
    setEdited((was) => {
      const next = { ...was };
      delete next[id];
      return next;
    });
  };
  const save = (variant: EvalVariant): void => {
    const typed = edited[variant.id];
    if (typed === undefined || !isDirty(variant, typed)) {
      forget(variant.id);
      return;
    }
    forget(variant.id);
    void store.getState().putVariant(withTyped(variant, typed));
  };
  const type = (id: string, typed: Typed): void => {
    setEdited((was) => ({ ...was, [id]: typed }));
  };

  return (
    <section data-lab-columns>
      <h2 className="mb-1 text-eyebrow">What tells the columns apart</h2>
      <p className="mb-2 max-w-160 lead">
        A column is this agent with one thing changed. Two columns that differ by two things cannot
        say which of them made the difference.
      </p>
      <ul className="paper">
        {columns.map((one) => {
          const typed = typedOf(one, edited[one.id]);
          const unsaved = isDirty(one, edited[one.id]);
          return (
            <li
              key={one.id}
              className="flex items-center gap-2 border-b border-line-subtle p-2 last:border-b-0"
            >
              <label className="label">
                Name
                <input
                  data-lab-column-name={one.id}
                  value={typed.name}
                  disabled={state.busy !== 'idle'}
                  className="ml-2 h-8 w-40 rounded-sm border border-line bg-well px-2 text-ui text-ink"
                  onChange={(event) => {
                    type(one.id, { ...typed, name: event.target.value });
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') save(one);
                  }}
                  onBlur={() => {
                    save(one);
                  }}
                />
              </label>
              <label className="flex flex-1 items-center gap-2 label">
                Model
                <input
                  data-lab-column-model={one.id}
                  placeholder="the one this agent already has"
                  value={typed.model}
                  disabled={state.busy !== 'idle'}
                  className="h-8 flex-1 rounded-sm border border-line bg-well px-2 text-ui text-ink"
                  onChange={(event) => {
                    type(one.id, { ...typed, model: event.target.value });
                  }}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') save(one);
                  }}
                  onBlur={() => {
                    save(one);
                  }}
                />
              </label>
              {/* POJAWIA SIĘ DOPIERO, GDY JEST CO ZAPISAĆ — i wtedy mówi to słowem, a nie
                  samym istnieniem. Bez tego wpisana wartość wygląda identycznie jak zapisana
                  i człowiek dowiaduje się o różnicy dopiero z biegu, który poszedł nie tym,
                  czego chciał. */}
              {unsaved ? (
                <button
                  data-lab-save-column={one.id}
                  type="button"
                  title="Write this column to the set file. Nothing here counts until you do."
                  className="h-8 rounded-sm bg-accent px-3 text-ui text-bg"
                  onClick={() => {
                    save(one);
                  }}
                >
                  Save
                </button>
              ) : null}
              {/* Ostatniej kolumny nie da się zdjąć: zestaw bez ani jednej nie ma czego
                  uruchomić, a odmowa po kliknięciu jest kontrolką, która kłamie. */}
              {columns.length < 2 ? null : (
                <button
                  data-lab-drop-column={one.id}
                  type="button"
                  aria-label={'Remove this column'}
                  disabled={state.busy !== 'idle'}
                  title={
                    'Remove ' + one.name + ' from this set. Its past results stay in the runs.'
                  }
                  className="h-8 rounded-sm border border-line px-2 text-ui text-muted"
                  onClick={() => {
                    void store.getState().dropVariant(one.id);
                  }}
                >
                  ×
                </button>
              )}
            </li>
          );
        })}
      </ul>
      <button
        data-lab-add-column
        type="button"
        disabled={state.busy !== 'idle'}
        title="Add a copy of the last column, so you can change one thing in it and compare."
        className="mt-2 h-8 rounded-sm border border-line px-3 text-ui text-body"
        onClick={() => {
          const made = nextColumn(columns, state.agents[0]?.id ?? '');
          if (made !== null) void store.getState().putVariant(made);
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
    <section data-lab-fix className="card">
      <h2 className="text-eyebrow">What {fix.name} would be told instead</h2>
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
          title={
            'Write this over what ' +
            fix.name +
            ' is told, everywhere — in this set and in every other run.'
          }
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
          title="Throw this suggestion away. Nothing about the agent changes."
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
        <h2 className="text-eyebrow">What did not pass</h2>
        {forAnAgent && writer !== '' ? (
          <button
            data-lab-ask-fix
            type="button"
            disabled={state.busy !== 'idle'}
            title="Have an agent read what did not pass and write new instructions. You read them before anything changes."
            className="h-8 rounded-sm border border-line px-3 text-ui text-body"
            onClick={() => {
              void store.getState().askForAFix(writer);
            }}
          >
            Propose a fix
          </button>
        ) : null}
        {forAnAgent ? null : (
          <p className="lead">
            A change to a skill goes through the same check as one pasted from a link, so it is
            written over in Skills.
          </p>
        )}
      </div>
      <ul className="paper">
        {failed.map((cell) => (
          <li
            key={cell.case + cell.variant}
            className="border-b border-line-subtle p-3 last:border-b-0"
          >
            <p className="text-body text-ink">{nameOf(cell)}</p>
            <p className="mt-1 max-w-160 lead" data-tone="body">
              {cell.said}
            </p>
            {cell.costUsd === null ? null : <p className="mt-1 value">{spendOf(cell.costUsd)}</p>}
          </li>
        ))}
      </ul>
    </section>
  );
}
