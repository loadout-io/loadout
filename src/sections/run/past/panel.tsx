/* Panel historii: co już tutaj biegło, i jeden z tych biegów otwarty do odczytu.
 *
 * TEN PLIK NIE PODEJMUJE ANI JEDNEJ DECYZJI O TREŚCI — dokładnie jak `../session/session.tsx`.
 * Co pokazać, mówi magazyn (`./store.ts`); jak nazwać stan i ile to kosztowało, mówi polityka
 * (`../history-command.ts`); jak wygląda wiersz zapisanego strumienia, mówi `./rows.ts` razem
 * z `../feed/line.tsx`. Tutaj zostaje markup i dwie kontrolki, które ten ekran naprawdę ma.
 *
 * ZAKRYWA WIDOK PRACY, a nie przestawia go — `fixed inset-0`, ta sama decyzja co przy ekranie
 * agenta: bieg pod spodem idzie dalej, strumień dalej przyjmuje linie, a zamknięcie panelu nie
 * kosztuje ani jednego odczytu.
 *
 * DWA WYJŚCIA I OBA CZYNNE. „Close" jest zawsze, „←" pojawia się dopiero wtedy, gdy jest dokąd
 * wrócić. Ekran bez wyjścia jest pułapką, nie ekranem — i to jest ta sama wada, którą zamknięto
 * w T-77: modal przykrywający jedyne wyjście z sekcji.
 *
 * TYLKO DO ODCZYTU. Nie ma tu kontrolki „uruchom to jeszcze raz" i to jest decyzja, nie
 * przeoczenie: wznowienie musi rozstrzygnąć, co z folderem, z limitem i z krokami, które już
 * przeszły. Przycisk bez tych rozstrzygnięć robiłby coś innego, niż mówi (niezmiennik 16).
 */
import type { ReactElement } from 'react';
import { useState, useSyncExternalStore } from 'react';

import { costText, openOneRun, stateWord } from '../history-command';
import type { PastHandoff, PastRun, PastRunRow, PastStep } from '../io';
import { Line } from '../feed/line';
import type { HistoryRow } from '../feed/model';
import { identityToken, statusToken } from '../rail/colour';
import { PICK_UP_HERE, pickUpFrom } from './pick-up';
import { rowsOf } from './rows';
import { backToTheList, closeHistory, pastNow, subscribeToPast } from './store';

/** Nazwa tego ekranu. Jedno słowo, to samo, którym człowiek go wywołał (`/history`). */
export const HEADING = 'History';

/** Co stoi zamiast strumienia kroku, którego nikt nie zapisał. */
export const NOTHING_KEPT_FOR_THIS_STEP = 'Nothing of what this step said was kept on disk.';

/** Nagłówek listy przekazań — po ludzku, nie nazwą pliku (niezmiennik 14). */
export const PASSED_ON = 'What the steps passed on';

/** Co stoi zamiast listy przekazań, kiedy żaden krok niczego nie oddał. */
export const PASSED_NOTHING = 'No step passed anything on in this run.';

/** `button-quiet` z DESIGN §6, ta sama fraza co na ekranie agenta (`../session/session.tsx`). */
const QUIET = 'h-7 rounded-sm border border-line px-3 text-ui text-body';

/** Ile kroków — zdanie, nie liczba obok słowa, żeby jeden krok nie czytał się jak „1 steps". */
export function stepsText(steps: number): string {
  return steps === 1 ? '1 step' : String(steps) + ' steps';
}

/** Prawa kolumna wiersza listy: ile kroków i ile to kosztowało, kiedy ktokolwiek to zmierzył. */
export function tallyText(row: PastRunRow): string {
  const cost = costText(row.costUsd);
  if (row.steps === 0) return cost;
  return cost === '' ? stepsText(row.steps) : stepsText(row.steps) + ' · ' + cost;
}

/**
 * Co stoi w środkowej kolumnie wiersza.
 *
 * Tytuł, kiedy Loadout przeczytał opis biegu; **uczciwe zdanie**, kiedy nie przeczytał. Nigdy
 * pustka: wiersz bez ani jednego napisu w środku wygląda dokładnie jak wiersz, który się nie
 * dorysował, a to jest bieg, który naprawdę tu był (niezmiennik 4 — katalog jest faktem).
 */
export function rowText(row: PastRunRow): string {
  if (row.title !== '') return row.title;
  return row.said ?? '';
}

/** Słowo stanu w jego kolorze, albo nic — bo nie każdy bieg da się o to zapytać. */
function StateWord({ state }: { state: string }): ReactElement | null {
  const word = stateWord(state);
  if (word === '') return null;
  return (
    <span
      data-run-state
      className="font-mono text-label whitespace-nowrap"
      style={{ color: `var(${statusToken(word)})` }}
    >
      {word}
    </span>
  );
}

/** Jeden bieg na liście — wiersz, który się klika. */
function Row({ row, folder }: { row: PastRunRow; folder: string | null }): ReactElement {
  return (
    <button
      type="button"
      data-history-row={row.folder}
      /* CAŁY WIERSZ JEST KONTROLKĄ, nie odnośnik w środku: człowiek celuje w bieg, a nie
         w jedno słowo w nim. Zakres jedzie z magazynu panelu — powód przy `openOneRun`. */
      onClick={() => {
        void openOneRun(folder, row.folder);
      }}
      className="grid w-full grid-cols-[112px_minmax(0,1fr)_auto_auto] items-baseline gap-3 rounded-md border border-line bg-panel px-3 py-[9px] text-left"
    >
      {/* Kiedy — wartość maszynowa, więc mono, do przepisania znak w znak (DESIGN §4). */}
      <span className="font-mono text-mono text-muted">{row.when}</span>
      <span className="min-w-0 truncate text-body text-ink">{rowText(row)}</span>
      <StateWord state={row.state} />
      <span className="font-mono text-mono whitespace-nowrap text-muted">{tallyText(row)}</span>
    </button>
  );
}

/** Kroki i przekazania jednego biegu — wszystko, co po nim zostało na dysku. */
function OneRun({ run }: { run: PastRun }): ReactElement {
  /* KTÓRE WIERSZE SĄ ROZWINIĘTE — stan tego ekranu i tylko jego. Rozwinięcie jest sprawą
   * patrzącego, nie pliku: dopisane do magazynu przeżyłoby zamknięcie panelu i wróciłoby
   * z cudzą decyzją sprzed dwóch dni. */
  const [opened, setOpened] = useState<readonly number[]>([]);

  function toggle(rowId: number): void {
    setOpened((now) =>
      now.includes(rowId) ? now.filter((one) => one !== rowId) : [...now, rowId],
    );
  }

  return (
    <div data-past-run={run.folder}>
      <div className="mb-4 flex items-baseline gap-3">
        <h3 className="text-title text-ink">{run.title === '' ? run.when : run.title}</h3>
        <span className="font-mono text-mono text-muted">{run.when}</span>
        <StateWord state={run.state} />
      </div>

      {run.said === null ? null : (
        <p data-past-said className="mb-4 text-body text-muted">
          {run.said}
        </p>
      )}

      {run.steps.map((step) => (
        <Step key={step.id} step={step} opened={opened} onToggle={toggle} />
      ))}

      <section data-passed-on className="mt-4">
        <h4 className="border-b border-line px-[18px] py-[9px] font-mono text-eyebrow text-muted">
          {PASSED_ON}
        </h4>
        {run.handoffs.length === 0 ? (
          <p data-empty className="px-[18px] py-2 text-body text-muted">
            {PASSED_NOTHING}
          </p>
        ) : (
          run.handoffs.map((handoff, index) => (
            <Handed key={handoff.title + String(index)} handoff={handoff} />
          ))
        )}
      </section>
    </div>
  );
}

/** Jedno przekazanie: kto komu i o czym. */
function Handed({ handoff }: { handoff: PastHandoff }): ReactElement {
  return (
    <p
      data-handed
      className="grid grid-cols-[minmax(0,auto)_minmax(0,1fr)] items-baseline gap-3 px-[18px] py-[5px]"
    >
      <span className="font-mono text-mono whitespace-nowrap text-muted">
        {handoff.from + ' → ' + (handoff.to.length === 0 ? '—' : handoff.to.join(', '))}
      </span>
      <span className="min-w-0 text-body text-ink">{handoff.title}</span>
    </p>
  );
}

/** Jeden krok: co o nim wiadomo, a pod spodem to, co po sobie zostawił. */
function Step({
  step,
  opened,
  onToggle,
}: {
  step: PastStep;
  opened: readonly number[];
  onToggle: (rowId: number) => void;
}): ReactElement {
  const rows: readonly HistoryRow[] = rowsOf(step.lines);
  return (
    <section data-past-step={step.id} className="mb-4 border border-line bg-panel">
      <h4 className="flex items-baseline gap-3 border-b border-line px-3 py-[9px]">
        {/* Nazwa kroku w kolorze tożsamości — ta sama mapa, z której żyje kwadrat na kafelku
            agenta i podpis w strumieniu (`../rail/colour.ts`). */}
        <span
          className="font-mono text-mono-strong"
          style={{ color: `var(${identityToken(step.name)})` }}
        >
          {step.name}
        </span>
        <StateWord state={step.state} />
        <span className="ml-auto font-mono text-mono text-muted">{costText(step.costUsd)}</span>
        {/* KONTYNUACJA STOI PRZY KROKU, a nie w nagłówku biegu, bo to jest wybór KROKU:
            „od którego miejsca ciągniemy dalej". Jeden przycisk nad całym biegiem musiałby ten
            krok zgadnąć — a zgadnięcie źle znaczy albo powtórzenie pracy, która się udała, albo
            pominięcie tej, która nie. Powód całej tej drogi stoi przy `./pick-up.ts`.

            `step.tile`, NIE `step.id` — i to jest naprawa ze zrzutu właściciela z 2026-08-23.
            `id` jest identyfikatorem kroku W TYM BIEGU (UUID nadany przy planowaniu), a wznowienie
            szuka kroku po kluczu Z PLIKU: przycisk odmawiał zdaniem-zagadką *„01a02b3c-… is not a
            step in that workflow any more"* o kroku, który stoi na płótnie.

            Pusty klucz znaczy, że `run.json` tego biegu nie mówi, z którego kafelka ten krok
            powstał — wtedy nie ma czego wskazać, więc przycisku nie ma. Kontrolka, która na pewno
            odmówi, jest kontrolką bez skutku (niezmiennik 16). */}
        {step.tile === '' ? null : (
          <button
            type="button"
            data-pick-up={step.tile}
            onClick={() => {
              pickUpFrom(step.tile);
            }}
            className={QUIET}
          >
            {PICK_UP_HERE}
          </button>
        )}
      </h4>

      <div className="py-[7px]">
        {step.summary === '' ? null : (
          <p data-step-said className="px-[18px] py-[3px] text-body text-ink">
            {step.summary}
          </p>
        )}
        {step.error === '' ? null : (
          <p data-step-problem className="px-[18px] py-[3px] text-body text-fail">
            {step.error}
          </p>
        )}
        {rows.length === 0 ? (
          <p data-empty className="px-[18px] py-[3px] text-body text-muted">
            {NOTHING_KEPT_FOR_THIS_STEP}
          </p>
        ) : (
          rows.map((row) => (
            <Line
              key={row.id}
              row={{ ...row, expanded: row.expanded || opened.includes(row.id) }}
              onToggle={onToggle}
              command={row.command}
            />
          ))
        )}
      </div>
    </section>
  );
}

/**
 * Panel historii — albo `null`, kiedy nikt o nią nie poprosił.
 *
 * Montuje go ekran pracy (`../index.tsx`), tuż obok pytania o zamknięcie karty: obie te rzeczy
 * stoją NAD widokiem pracy i obie stawia magazyn, więc ekran ma je w jednym miejscu.
 */
export function PastRuns(): ReactElement | null {
  const now = useSyncExternalStore(subscribeToPast, pastNow, pastNow);
  if (!now.open) return null;

  return (
    <div data-history className="fixed inset-0 z-20 grid grid-rows-[auto_minmax(0,1fr)] bg-bg">
      <div className="flex h-13 shrink-0 items-center gap-3 border-b border-line bg-panel px-[18px]">
        {now.opened === null ? null : (
          <button
            type="button"
            aria-label="Back to the list"
            onClick={backToTheList}
            className={QUIET}
          >
            ←
          </button>
        )}
        <h2 className="text-title text-ink">{HEADING}</h2>
        <span className="font-mono text-mono text-muted">{runsHereText(now.rows.length)}</span>
        <button type="button" onClick={closeHistory} className={QUIET + ' ml-auto'}>
          Close
        </button>
      </div>

      <div className="min-h-0 overflow-auto p-[18px]">
        {now.said === null ? null : (
          <p data-history-said className="mb-3 text-body text-fail">
            {now.said}
          </p>
        )}
        {now.opened === null ? (
          <div className="grid gap-[6px]">
            {now.rows.map((row) => (
              <Row key={row.folder} row={row} folder={now.folder} />
            ))}
          </div>
        ) : (
          <OneRun run={now.opened} />
        )}
      </div>
    </div>
  );
}

/** Ile biegów jest na tej liście. Zdanie, nie liczba — „1 runs" czyta się jak usterka. */
export function runsHereText(runs: number): string {
  return runs === 1 ? '1 run in this folder' : String(runs) + ' runs in this folder';
}
