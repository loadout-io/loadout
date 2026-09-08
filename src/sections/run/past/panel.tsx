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
import { why } from '../../../ipc/why';

import { costText, openOneRun, stateWord } from '../history-command';
import type {
  CouldForget,
  PastBranch,
  PastHandoff,
  PastMemory,
  PastResultFolder,
  PastRun,
  PastRunRow,
  PastStep,
} from '../io';
import { openResultFolder } from '../io';
import { Line } from '../feed/line';
import type { HistoryRow } from '../feed/model';
import { identityToken, statusToken } from '../rail/colour';
import { reflectionText } from '../reflection/said';
import { PICK_UP_HERE, pickUpFrom } from './pick-up';
import { rowsOf } from './rows';
import { Replay } from './replay';
import { SavedResults } from './result-restore';
import {
  askAboutRunsOlderThan,
  backToTheList,
  closeHistory,
  forgetTheBranches,
  forgetTheLeftovers,
  forgetTheOldRuns,
  forgetThisRun,
  pastNow,
  subscribeToPast,
} from './store';

/** Nazwa tego ekranu. Jedno słowo, to samo, którym człowiek go wywołał (`/history`). */
export const HEADING = 'History';

/** Co stoi zamiast strumienia kroku, którego nikt nie zapisał. */
export const NOTHING_KEPT_FOR_THIS_STEP = 'Nothing of what this step said was kept on disk.';

/** Nagłówek listy przekazań — po ludzku, nie nazwą pliku (niezmiennik 14). */
export const PASSED_ON = 'What the steps passed on';

/** Co stoi zamiast listy przekazań, kiedy żaden krok niczego nie oddał. */
export const PASSED_NOTHING = 'No step passed anything on in this run.';

/** Nagłówek listy gałęzi — po ludzku, nie nazwą polecenia gita (niezmiennik 14). */
export const BRANCHES_LEFT = 'Branches this run left';

/** Napis na kontrolce, która je zdejmuje. */
export const FORGET_THE_BRANCHES = 'Forget the branches';

/**
 * Napis na kontrolce, która zdejmuje CAŁY bieg — jego folder razem z gałęziami.
 *
 * „this run", nie „everything": człowiek stoi na opisie jednego biegu i to jego dotyczy ta
 * kontrolka. Napis mówiący więcej, niż kontrolka robi, jest przy kasowaniu najgorszym rodzajem
 * pomyłki.
 */
export const FORGET_THIS_RUN = 'Forget this run';

/**
 * Co stoi tam, gdzie stała lista gałęzi.
 *
 * JEDNO ZDANIE NA DWA STANY, i to jest wybór: bieg, po którym nie zostało nic, i bieg, którego
 * gałęzie właśnie zdjęto, są dla patrzącego tym samym faktem — nie ma tu żadnej gałęzi. Dwa
 * zdania o jednym stanie to dwa miejsca, w których mieszka jedna odpowiedź (niezmiennik 13).
 */
export const NO_BRANCHES_LEFT = 'No branches left';

/**
 * Napis na kontrolce, która zdejmuje to, co zostawiły biegi sprzed sprzątania.
 *
 * „them", bo zdanie tuż nad nią właśnie powiedziało, co to jest i ile tego jest. Napis
 * powtarzający liczby byłby drugim miejscem, w którym mieszka jedna odpowiedź (niezmiennik 13).
 */
export const FORGET_THE_LEFTOVERS = 'Forget them';

/** Początek zdania kontrolki daty. Liczba dni stoi w polu obok, bo to ona jest wyborem. */
export const FORGET_RUNS_OLDER_THAN = 'Forget runs older than';

/** Reszta tego zdania, za polem. */
export const DAYS = 'days';

/** Napis na kontrolce, która zdejmuje te biegi. Mówi dokładnie tyle, ile robi. */
export const FORGET_THESE_RUNS = 'Forget these runs';

/** Nagłówek zamrożonego rachunku pod każdym fizycznym krokiem. */
export const WHAT_THIS_STEP_KNEW = 'What this step knew';

/** Dostarczenie i pominięcie są rozłączne: odłożona notatka nie udaje wiedzy kroku. */
export const GIVEN_TO_THIS_STEP = 'Given to this step';
export const LEFT_OUT_FOR_LENGTH = "Left out because it exceeded this run's length limit.";
export const NO_FROZEN_MEMORY = 'No frozen memory was recorded for this step.';

/* `.btn-quiet` z `theme.css` — ta sama nazwa, co na ekranie agenta i w szynie. Wysokosc,
 * obrys i stany cichego przycisku mieszkaja od 2026-08-31 w jednym miejscu. */
const QUIET = 'btn-quiet';

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
  // 2026-09 (Z-33) — `not run` istnieje tylko w historii. Dostaje ten sam przygaszony token co
  // inne zakończone stany, bez dopisywania fikcyjnego stanu do żywej szyny agentów.
  const colour = word === 'not run' ? '--color-muted' : statusToken(word);
  return (
    <span
      data-run-state
      className="font-mono text-label whitespace-nowrap"
      style={{ color: `var(${colour})` }}
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
      /* `.card[data-interactive]`: obrys, promien i wypelnienie karty plus — czego ten wiersz
         dotad nie mial — myjka pod kursorem, wcisniecie i pierscien skupienia. Wiersz, ktory
         jest przyciskiem na calej szerokosci i milczy przy najechaniu, wyglada jak akapit. */
      data-interactive=""
      className="card grid w-full grid-cols-[112px_minmax(0,1fr)_auto_auto] items-baseline gap-3 px-3 py-[9px] text-left"
    >
      {/* Kiedy — wartość maszynowa, więc `.value`: kroj wchodzi razem ze stopniem (DESIGN §4). */}
      <span className="value">{row.when}</span>
      <span className="min-w-0 truncate text-body text-ink">{rowText(row)}</span>
      <StateWord state={row.state} />
      <span className="value whitespace-nowrap">{tallyText(row)}</span>
    </button>
  );
}

/** Kroki i przekazania jednego biegu — wszystko, co po nim zostało na dysku. */
function OneRun({ run, project }: { run: PastRun; project: string | null }): ReactElement {
  /* KTÓRE WIERSZE SĄ ROZWINIĘTE — stan tego ekranu i tylko jego. Rozwinięcie jest sprawą
   * patrzącego, nie pliku: dopisane do magazynu przeżyłoby zamknięcie panelu i wróciłoby
   * z cudzą decyzją sprzed dwóch dni. */
  const [opened, setOpened] = useState<readonly number[]>([]);
  const [foldersToForget, setFoldersToForget] = useState<readonly string[] | null>(null);

  function toggle(rowId: number): void {
    setOpened((now) =>
      now.includes(rowId) ? now.filter((one) => one !== rowId) : [...now, rowId],
    );
  }

  return (
    <div data-past-run={run.folder}>
      <div className="mb-4 flex items-baseline gap-3">
        <h3 className="text-title text-ink">{run.title === '' ? run.when : run.title}</h3>
        <span className="value">{run.when}</span>
        <StateWord state={run.state} />
      </div>

      {run.said === null ? null : (
        <p data-past-said className="lead mb-4">
          {run.said}
        </p>
      )}

      {/* CO REFLEKSJA ZROBIŁA Z TYM BIEGIEM — ZAWSZE, nigdy warunkowo (2026-08-29, T-165).
          Wiersz, który znika przy pustym wyniku, mówi „nic nie znalazłem" dokładnie tym samym
          pustym miejscem, którym ekran mówi „nie dorysowałem się" — a za tę turę ktoś zapłacił.
          Cisza jest tu wadą, nie stanem, i jest całą przyczyną, dla której to zdanie powstało.
          Który to z czterech stanów, rozstrzyga `../reflection/said.ts`; tutaj zostaje markup. */}
      <p data-reflection className="lead mb-4">
        {reflectionText(run.reflection ?? null)}
      </p>

      {run.steps.map((step) => (
        <Step key={step.id} step={step} opened={opened} onToggle={toggle} />
      ))}

      <section data-passed-on className="mt-4">
        <h4 className="border-b border-line px-[18px] py-[9px] font-mono text-eyebrow text-muted">
          {PASSED_ON}
        </h4>
        {run.handoffs.length === 0 ? (
          <p data-empty className="lead px-[18px] py-2">
            {run.handoffsSaid ?? PASSED_NOTHING}
          </p>
        ) : (
          run.handoffs.map((handoff, index) => (
            <Handed key={handoff.title + String(index)} handoff={handoff} />
          ))
        )}
      </section>

      <Branches run={run} />
      {project !== null && (
        <SavedResults key={project + ':files:' + run.folder} project={project} run={run} />
      )}
      {project !== null && run.state !== 'running' && run.state !== 'paused' && (
        <Replay key={project + ':' + run.folder} project={project} runFolder={run.folder} />
      )}
      {(run.resultFolders?.length ?? 0) > 0 && (
        <section data-result-folders className="mt-4">
          <h4 className="border-b border-line px-[18px] py-[9px] font-mono text-eyebrow text-muted">
            Folders this run kept
          </h4>
          {run.resultFolders?.map((folder) => (
            <ResultFolder key={folder.workKey} folder={folder} run={run.folder} project={project} />
          ))}
        </section>
      )}

      {/* JEDNO WYJŚCIE Z CAŁEGO BIEGU, i stoi ostatnie na ekranie — pod wszystkim, co ten bieg
          zostawił, bo dopiero po przeczytaniu tego człowiek wie, czy chce to stracić.
          POZA sekcją gałęzi, choć zdejmuje także je: tamta kontrolka nie istnieje, kiedy nie ma
          czego zdejmować (niezmiennik 16), a folder biegu jest zawsze. */}
      <div className="mt-4 px-[18px]">
        <button
          type="button"
          data-forget-run
          onClick={() => {
            const paths = (run.resultFolders ?? []).map((one) => one.path);
            if (paths.length > 0) setFoldersToForget(paths);
            else void forgetThisRun();
          }}
          className={QUIET}
        >
          {FORGET_THIS_RUN}
        </button>
        {foldersToForget !== null && (
          <section aria-label="Confirm removing saved results" className="mt-3">
            <p className="lead">
              This will remove these folders and the results they contain. This cannot be undone.
            </p>
            <ul>
              {foldersToForget.map((path) => (
                <li className="value break-all" key={path}>
                  {path}
                </li>
              ))}
            </ul>
            <button
              type="button"
              className="text-ink"
              onClick={() => {
                const confirmed = foldersToForget;
                setFoldersToForget(null);
                void forgetThisRun(confirmed);
              }}
            >
              Forget these folders and this run
            </button>
            <button
              type="button"
              className="ml-3 text-muted"
              onClick={() => setFoldersToForget(null)}
            >
              Keep folders
            </button>
          </section>
        )}
      </div>
    </div>
  );
}

/** WF-06: zachowany folder ma czynne wyjście; częściowe wejście nie udaje gotowego wyniku. */
function ResultFolder({
  folder,
  run,
  project,
}: {
  folder: PastResultFolder;
  run: string;
  project: string | null;
}): ReactElement {
  const [trouble, setTrouble] = useState<string | null>(null);
  const incomplete = folder.state === 'incomplete';
  const said = incomplete
    ? 'This folder is incomplete and cannot be used to resume the run.'
    : folder.state === 'changed'
      ? 'Changes kept in this folder.'
      : 'This folder was kept because its contents could not be checked.';
  return (
    <div data-result-folder={folder.workKey} className="px-[18px] py-2">
      <p className="value">{folder.step}</p>
      <p className="lead">{said}</p>
      <p className="value break-all">{folder.path}</p>
      <button
        type="button"
        className={QUIET}
        onClick={() => {
          setTrouble(null);
          void openResultFolder(project, run, folder.workKey).catch((error: unknown) => {
            setTrouble(
              why(
                error,
                'Loadout could not open this folder. It is still available at the path above.',
              ),
            );
          });
        }}
      >
        {incomplete ? 'Open partial folder' : 'Open result folder'}
      </button>
      {trouble !== null && (
        <p role="alert" className="lead">
          {trouble}
        </p>
      )}
    </div>
  );
}

/**
 * Co ten bieg zostawił w repozytorium — i jedno wyjście z tego stanu.
 *
 * PO CO TO JEST NA EKRANIE. Katalog roboczy kroku znika zaraz po biegu, bo praca jest osiągalna
 * z gałęzi. Gałęzie zostawały natomiast na zawsze: nic ich nie listowało i nic nie umiało ich
 * zdjąć poza ręcznym poleceniem gita na każdą z osobna, a po tygodniu pracy jest ich
 * kilkadziesiąt.
 *
 * NAZWA I KROK, bo dopiero razem coś znaczą: nazwy gałęzi jednego biegu różnią się ostatnim
 * członem i czyta się je jak jedną kolumnę tego samego napisu.
 *
 * KONTROLKI NIE MA, KIEDY NIE MA CZEGO ZDEJMOWAĆ. Przycisk, który umie odpowiedzieć wyłącznie
 * „nie było czego", jest przyciskiem bez skutku (niezmiennik 16).
 */
function Branches({ run }: { run: PastRun }): ReactElement {
  const branches: readonly PastBranch[] = run.branches ?? [];
  return (
    <section data-branches className="mt-4">
      <h4 className="border-b border-line px-[18px] py-[9px] font-mono text-eyebrow text-muted">
        {BRANCHES_LEFT}
      </h4>
      {branches.length === 0 ? (
        <p data-empty className="lead px-[18px] py-2">
          {NO_BRANCHES_LEFT}
        </p>
      ) : (
        <>
          {branches.map((branch) => (
            <Branch key={branch.name} branch={branch} />
          ))}
          <div className="px-[18px] py-2">
            <button
              type="button"
              data-forget-branches
              onClick={() => {
                void forgetTheBranches();
              }}
              className={QUIET}
            >
              {FORGET_THE_BRANCHES}
            </button>
          </div>
        </>
      )}
    </section>
  );
}

/** Jedna gałąź: jak się nazywa i który krok ją zostawił. */
function Branch({ branch }: { branch: PastBranch }): ReactElement {
  return (
    <p
      data-branch={branch.name}
      className="grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-3 px-[18px] py-[5px]"
    >
      {/* Nazwa jest wartością maszynową — do przepisania znak w znak, więc mono (DESIGN §4). */}
      <span className="value min-w-0 truncate" data-tone="ink">
        {branch.name}
      </span>
      <span className="value whitespace-nowrap">{branch.step}</span>
    </p>
  );
}

/** Jedno przekazanie: kto komu i o czym. */
function Handed({ handoff }: { handoff: PastHandoff }): ReactElement {
  return (
    <p
      data-handed
      className="grid grid-cols-[minmax(0,auto)_minmax(0,1fr)] items-baseline gap-3 px-[18px] py-[5px]"
    >
      <span className="value whitespace-nowrap">
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
        <span className="value ml-auto">
          {[step.contextPerTurn, costText(step.costUsd)].filter(Boolean).join(' · ')}
        </span>
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

      {/* ZDANIE SKŁADA RUST, ekran je stawia (2026-09, Z-47). Które rzeczy w tym rekordzie są
          własnością biegu, wie wyłącznie ta strona granicy, która je krokowi wkłada — nazwy
          pluginu z umiejętnościami i przekierowanego katalogu pamięci mieszkają tam i tylko tam
          (niezmiennik 23). Liczenie tego tutaj byłoby drugą kopią tej wiedzy, po tej stronie,
          po której nie da się jej sprawdzić. */}
      <StepMemory
        memory={step.memory ?? []}
        alsoLoaded={step.whatLoadoutDidNotGive ?? null}
        instructions={step.projectInstructions ?? []}
        referenceMaterials={step.referenceMaterials ?? []}
        workPlan={step.workPlan ?? []}
      />

      {/* KROK JEST PUDEŁKIEM O SKOŃCZONEJ WYSOKOŚCI, i to jest cała naprawa tego ekranu.
          Zgłoszenie właściciela 2026-08-23: „ten UI od razu ogarnij bo mnie wkurwia".

          Do dziś panel rysował KAŻDY krok w całości, jeden pod drugim. Bieg `20260823-011240`
          to 22 kroki, a jeden z nich niósł w strumieniu wypowiedź agenta długości raportu —
          więc otwarcie historii dawało kilkadziesiąt ekranów przewijania, w których nie dało
          się znaleźć niczego. Nagłówki kroków, po które człowiek tu przychodzi, były od siebie
          oddalone o tysiące wierszy.

          SUFIT, NIE CIĘCIE. Nie skracamy ani jednego zdania: cały tekst zostaje w dokumencie,
          tylko przestaje rozpychać stronę. To jest różnica między „nie mieści się na ekranie"
          a „zostało utracone" — a ta druga rzecz jest dokładnie tym, o co ten ekran właśnie
          został oskarżony przy krokach codeksa.

          `overscroll-contain`: dojechanie do końca kroku nie ma prawa pociągnąć całej listy
          biegu. Bez tego przewijanie jednego kroku wyrzuca człowieka z miejsca, którego szukał. */}
      <div className="max-h-96 overflow-y-auto overscroll-contain py-[7px]">
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
        {/* CZEGO TEN KROK NIE DOSTAŁ, choć pojechał (2026-09, Z-39). Osobno od powodu wyżej i
            CICHYM tonem, nie `text-fail`: ten krok nie padł — pojechał bez cudzego wyniku, bo
            człowiek tak ustawił „co, kiedy nie przejdzie". Czerwień mówiłaby, że to on zawiódł,
            a wtedy szuka się wady u niego zamiast u poprzednika. */}
        {/* Klucz z indeksem, ta sama decyzja co przy `Handed` niżej: dwie rundy pętli noszą tę
            samą nazwę kafelka, więc dwa zdania o nich są co do znaku identyczne. */}
        {(step.ranWithout ?? []).map((said, index) => (
          <p
            key={said + String(index)}
            data-step-ran-without
            className="px-[18px] py-[3px] text-body text-muted"
          >
            {said}
          </p>
        ))}
        {rows.length === 0 ? (
          <p data-empty className="lead px-[18px] py-[3px]">
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
 * Zamrożony receipt — wyłącznie z `PastStep`, nigdy z dzisiejszego katalogu pamięci.
 *
 * JEDEN NAGŁÓWEK, i to jest niezmiennik 13: „co ten krok wiedział" jest jednym pytaniem.
 * Instrukcje, notatki i materiały referencyjne są tym, co Loadout dał; zdanie obok mówi, co
 * aplikacja agenta dobrała sama z folderu. Drugi region gdzie indziej na karcie kazałby czytać
 * dwa miejsca, żeby poznać jedną odpowiedź.
 */
function StepMemory({
  memory,
  alsoLoaded,
  instructions,
  referenceMaterials,
  workPlan,
}: {
  memory: readonly PastMemory[];
  alsoLoaded: string | null;
  instructions: NonNullable<PastStep['projectInstructions']>;
  referenceMaterials: NonNullable<PastStep['referenceMaterials']>;
  workPlan: NonNullable<PastStep['workPlan']>;
}): ReactElement {
  return (
    <section data-step-memory className="border-b border-line px-[18px] py-[9px]">
      <h5 className="mb-1 font-mono text-eyebrow text-muted">{WHAT_THIS_STEP_KNEW}</h5>
      {instructions.length === 0 ? null : (
        <details data-project-instructions>
          <summary className="label">
            Project instructions supplied by Loadout · {instructions.length} sources
          </summary>
          <p className="caption">
            Frozen when this workflow started. These are separate from anything the agent app loaded
            itself.
          </p>
          <ul className="stack" data-gap="1">
            {instructions.map((source, index) => (
              <li key={`${source.path}:${index}`} className="caption">
                <span className="break-all">{source.path}</span> — {source.bytes} bytes ·{' '}
                {source.digest?.slice(0, 8)}; folder {source.directory || '.'}
                {source.paths.length === 0 ? null : `; paths ${source.paths.join(', ')}`}
                {source.digest === undefined ? null : (
                  <details>
                    <summary>Full fingerprint</summary>
                    <span className="break-all">{source.digest}</span>
                  </details>
                )}
              </li>
            ))}
          </ul>
        </details>
      )}
      {alsoLoaded === null ? null : (
        <p data-also-loaded className="label">
          {alsoLoaded}
        </p>
      )}
      {referenceMaterials.length === 0 ? null : (
        <ul data-reference-materials className="stack" data-gap="1">
          {referenceMaterials.map((said, index) => (
            <li key={`${index}:${said}`} className="caption">
              {said}
            </li>
          ))}
        </ul>
      )}
      {workPlan.length === 0 ? null : (
        <ul data-work-plan className="stack" data-gap="1">
          {workPlan.map((said, index) => (
            <li key={`${index}:${said}`} className="caption">
              {said}
            </li>
          ))}
        </ul>
      )}
      {memory.length === 0 ? (
        <p className="lead">{NO_FROZEN_MEMORY}</p>
      ) : (
        <div className="grid gap-2">
          {memory.map((record) => (
            <MemoryRecord
              key={`${record.address.place}:${record.address.id}:${record.leftOut ? 'left-out' : 'given'}`}
              record={record}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function MemoryRecord({ record }: { record: PastMemory }): ReactElement {
  const origins = [
    record.project === null ? null : 'Imported from ' + record.project,
    record.from === null ? null : 'Suggested after run ' + record.from,
  ].filter((one): one is string => one !== null);

  return (
    <div data-past-memory={record.reference}>
      <div className="flex items-baseline gap-3">
        <span className="value min-w-0 truncate" data-tone="ink">
          {record.reference}
        </span>
        <span className="value ml-auto whitespace-nowrap">
          {String(record.bytes) + ' bytes · ' + record.hash.slice(0, 8)}
        </span>
      </div>
      <p className="label">{record.leftOut ? LEFT_OUT_FOR_LENGTH : GIVEN_TO_THIS_STEP}</p>
      {origins.map((origin) => (
        <p key={origin} className="label">
          {origin}
        </p>
      ))}
    </div>
  );
}

/**
 * Co ten folder mógłby zapomnieć — nad listą biegów, bo dotyczy całej listy, nie żadnego wiersza.
 *
 * # 2026-09 (Z-46) — PO CO TO JEST NA EKRANIE
 *
 * Sprzątanie przy otwarciu folderu domyka wyłącznie te katalogi robocze, o których bieg zostawił
 * notatkę. Bieg sprzed tej notatki nie ma jej wcale, więc jego katalog stoi dalej — i do dziś nic
 * o nim nie mówiło. Zmierzone u właściciela 2026-09-03 na jednym monorepo: dziennik zameldował
 * 75 zamkniętych folderów, `git worktree list` pokazywał dwanaście stojących po 264 MB, a gałęzi
 * po biegach, których katalogów już nie ma, stało tam 99 przy czternastu biegach.
 *
 * # DWIE KONTROLKI, BO TO SĄ DWIE RÓŻNE CZYNNOŚCI
 *
 * Pierwsza zdejmuje **leżaki po biegach, których Loadout nie zamknął** — same biegi zostają.
 * Druga zdejmuje **biegi po dacie**, razem ze wszystkim, co po nich zostało. Jedna kontrolka na
 * oba znaczyłaby, że człowiek, który chce odzyskać miejsce, traci przy okazji historię.
 *
 * # CZEGO TU NIE MA, KIEDY NIE MA CZEGO ZDEJMOWAĆ
 *
 * Zdania o leżakach nie ma, kiedy ich nie ma (Rust oddaje wtedy pusty napis), a całej kontrolki
 * daty nie ma w folderze bez ani jednego biegu. Przycisk, który umie odpowiedzieć wyłącznie „nie
 * było czego", jest przyciskiem bez skutku (niezmiennik 16). Samo POLE dni zostaje, dopóki jest
 * jakikolwiek bieg: to ono decyduje, o co pytamy, więc schowane za własną odpowiedzią nie dałoby
 * się nigdy zmienić.
 *
 * # 2026-09 (Z-46) — ZDANIE O LEŻAKACH NIE ZALEŻY OD DŁUGOŚCI LISTY BIEGÓW
 *
 * Stało tu `runs === 0 → null`, czyli cały ten blok znikał razem z pustą historią. To jest
 * dokładnie ten folder, w którym on jest najbardziej potrzebny: „Forget runs older than …"
 * kasuje KATALOGI biegów, a gałąź biegu, którego katalogu już nie ma, jest tym, co zamiatacz ma
 * sprzątać. Po dwóch takich czyszczeniach folder miał zero wierszy historii, komplet osieroconych
 * gałęzi i ani jednego zdania o nich.
 */
function WhatCouldGo({
  could,
  days,
  runs,
}: {
  could: CouldForget | null;
  days: number;
  runs: number;
}): ReactElement | null {
  if (could === null) return null;
  return (
    <section data-what-could-go className="mb-2 grid gap-2 border-b border-line pb-3">
      {could.said === '' ? null : (
        <div className="flex items-baseline gap-3">
          <p data-leftovers className="lead min-w-0">
            {could.said}
          </p>
          <button
            type="button"
            data-forget-leftovers
            onClick={() => {
              void forgetTheLeftovers();
            }}
            className={QUIET + ' ml-auto'}
          >
            {FORGET_THE_LEFTOVERS}
          </button>
        </div>
      )}

      {runs === 0 ? null : (
        <>
          <div className="flex items-baseline gap-2">
            <label className="label" htmlFor="older-than-days">
              {FORGET_RUNS_OLDER_THAN}
            </label>
            {/* Liczba jest wartością maszynową, więc `.field` (DESIGN §4) — ta sama kontrolka, co
            przy „ile ostatnich biegów zostaje" w Settings. */}
            <input
              id="older-than-days"
              data-older-than-days
              type="number"
              inputMode="numeric"
              min={1}
              step={1}
              className="field w-20 text-right"
              value={days}
              onChange={(event) => {
                askAboutRunsOlderThan(Number(event.target.value));
              }}
            />
            <span className="label">{DAYS}</span>
            {could.older.runs === 0 ? null : (
              <button
                type="button"
                data-forget-old-runs
                onClick={() => {
                  void forgetTheOldRuns();
                }}
                className={QUIET + ' ml-auto'}
              >
                {FORGET_THESE_RUNS}
              </button>
            )}
          </div>
          {/* CO ZEJDZIE — ZAWSZE, także kiedy nic. Cisza nad kontrolką, która kasuje, jest tym
              samym, co obietnica bez treści: człowiek naciska i dowiaduje się po fakcie. */}
          <p data-older-said className="lead">
            {could.older.said}
          </p>
        </>
      )}
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
    /* `.enter`: panel POJAWIA sie po `/history`, wiec wchodzi sprezyna — jedyne miejsce, gdzie
       DESIGN §7 na nia pozwala. Jeden region na to zdarzenie: wiersze w srodku nie ruszaja sie,
       bo lista, ktora wjezdza wierszami, kaze czekac na tresc, po ktora sie przyszlo. */
    <div
      data-history
      className="enter fixed inset-0 z-20 grid grid-rows-[auto_minmax(0,1fr)] bg-bg"
    >
      {/* `.screen-head` niesie 52 px z ARCHITECTURE §7; material bierze z `.glass` obok. */}
      <div className="screen-head glass">
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
        <span className="value">{runsHereText(now.rows.length)}</span>
        <button type="button" onClick={closeHistory} className={QUIET + ' ml-auto'}>
          Close
        </button>
      </div>

      <div className="screen-body">
        {now.said === null ? null : (
          <p data-history-said className="lead fade-in mb-3" data-tone="fail">
            {now.said}
          </p>
        )}
        {now.opened === null ? (
          <div className="grid gap-[6px]">
            <WhatCouldGo could={now.could} days={now.olderThanDays} runs={now.rows.length} />
            {now.rows.map((row) => (
              <Row key={row.folder} row={row} folder={now.folder} />
            ))}
          </div>
        ) : (
          <OneRun run={now.opened} project={now.folder} />
        )}
      </div>
    </div>
  );
}

/** Ile biegów jest na tej liście. Zdanie, nie liczba — „1 runs" czyta się jak usterka. */
export function runsHereText(runs: number): string {
  return runs === 1 ? '1 run in this folder' : String(runs) + ' runs in this folder';
}
