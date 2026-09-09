/* Panel kafelka „uruchom i zostaw" — dwa wiersze, bo ten kafelek ma dwa pola.
 *
 * Istnieje z tego samego powodu, co `checkpoint-panel.tsx`, i ten powód jest niezmiennikiem 16:
 * płótno ma przycisk `＋ Start something`, a przycisk, który stawia kafelek bez sposobu na wpisanie
 * komendy, jest kontrolką prowadzącą donikąd. Kafelek z pustą komendą wygląda na płótnie prawie
 * tak samo jak wypełniony, a odmawia dopiero w środku biegu.
 *
 * To NIE jest `StepPanel` z siódemką wierszy ani panel kroku „sprawdź": tu nie ma agenta, więc
 * nie ma dziedziczenia, nadpisań ani wiersza Skills — i nie ma pola „Proof that it ran", bo ten
 * kafelek niczego nie orzeka. Wspólny formularz z połową wierszy schowanych warunkiem jest tą
 * samą konstrukcją, którą DESIGN §6 nazywa zakładkami w panelu.
 *
 * Ramki tu nie ma: rysuje ją `PanelForStep`, jedną, dla wszystkich paneli.
 */
import type { ReactElement } from 'react';
import { fieldNameFor } from './hands-over-the-command';
import type { CommandProducer } from './hands-over-the-command';
import { WhereItWorks } from './where-it-works';
import type { ServeStep } from '../../../state/workflows';
import { Tick } from '../../../ui/primitives/tick';

export interface ServePanelProps {
  step: ServeStep;
  onEditStep: (
    fields: Partial<
      Pick<
        ServeStep,
        | 'name'
        | 'command'
        | 'folder'
        | 'commandFrom'
        | 'readiness'
        | 'endpoints'
        | 'lifetime'
        | 'startWhen'
        | 'targetKind'
        | 'testDataEnv'
      >
    >,
  ) => void;
  /**
   * Nazwa kroku, który po strzałce stoi przed tym kafelkiem, albo `null`.
   *
   * Liczona przez EDYTOR, nie tutaj: panel nie zna strzałek, więc nie ma jak pomylić się co do
   * tego, który krok jest tym przed (ten sam ruch i ten sam powód, co przy `wayBack`).
   */
  stepBefore?: string | null | undefined;
  /** Czy ten krok jest już proszony o pole, na które ten kafelek czeka. */
  handsItOver?: boolean | undefined;
  /** Człowiek prosi krok przed tym o to pole — jawnym kliknięciem, nie efektem ubocznym. */
  onAskTheStepBefore?: (() => void) | undefined;
  commandProducers?: readonly CommandProducer[] | undefined;
}

/* GDZIE TO WSTAJE — DWA wyjścia, bo tylko dwa mają dla serwera sens.
 *
 * 2026-08-23 — WYBÓR DOSZEDŁ PO PIERWSZYM PRAWDZIWYM UŻYCIU. Kafelek wychodził z przycisku
 * z folderem projektu i nie dało się tego zmienić, a to jest dla serwera zły domyślny:
 * sprawdzenie, które ma na niego patrzeć, pracuje w kopii kroku, który właśnie pisał kod — więc
 * serwer z folderu projektu podaje kod BEZ tej pracy. Strona, która się otwiera i pokazuje starą
 * wersję, wygląda na działającą, a to jest gorsze niż serwer, którego nie ma.
 *
 * Własnej kopii tu nie ma i nie powinno być: świeży checkout serwowałby kod, którego nikt w tym
 * biegu nie tknął — czyli ten sam błąd, tylko drożej. Dlatego ten jeden panel podaje wspólnej
 * kontrolce listę odpowiedzi; brzmienia i pytanie bierze takie, jak wszyscy
 * (`./where-it-works.tsx`).
 *
 * 2026-08-31 — WŁASNEJ LISTY TU JUŻ NIE MA, z tego samego powodu, co w `check-panel.tsx`. */
const OFFERS = ['project', 'same-copy'] as const;

/* `ROW` i `LABEL` zniknęły 2026-08-31 — patrz `checkpoint-panel.tsx`: rolę niosą teraz
 * `.stack` (etykieta nad kontrolką) i `.label` (etykieta pola), a zdanie pod kontrolką ma
 * własną, inną rolę (`.lead`). */
/* Klasa domu, nie własny opis — ten sam powód, co w `checkpoint-panel.tsx`. */
const FIELD = 'field';

export function ServePanel({
  step,
  onEditStep,
  stepBefore = null,
  handsItOver = false,
  onAskTheStepBefore = () => undefined,
  commandProducers = [],
}: ServePanelProps): ReactElement {
  return (
    <>
      <div className="stack">
        <label htmlFor="serve-name" className="label">
          Name
        </label>
        <input
          id="serve-name"
          className={FIELD}
          value={step.name}
          onChange={(event) => {
            onEditStep({ name: event.target.value });
          }}
        />
      </div>

      <div className="stack">
        <label className="label" htmlFor="serve-start-when">
          When to start
        </label>
        <select
          id="serve-start-when"
          className={FIELD}
          value={step.startWhen ?? 'reached'}
          onChange={(event) => {
            if (event.target.value === 'reached' || event.target.value === 'asked') {
              onEditStep({ startWhen: event.target.value });
            }
          }}
        >
          <option value="reached">When the workflow reaches this step</option>
          <option value="asked">When an allowed agent asks to start it</option>
        </select>
        {step.startWhen === 'asked' ? (
          <span className="lead">
            This step prepares the app description but does not start the app. Only an agent
            explicitly allowed to use this app can start it.
          </span>
        ) : null}
      </div>

      {/* CO TO WŁAŚCIWIE WSTAJE — jedno pytanie, od którego zależą DWIE rzeczy naraz.
          Do 2026-09-07 nie było go wcale i każdy kafelek był traktowany jak serwer: gotowy port
          uchodził za dowód gotowości, a katalog danych zostawał ten sam, co twój. Dla aplikacji
          z własnym oknem oba te domyślne są złe — otwarty port ma ona, zanim cokolwiek narysuje,
          a scenariusz QA klika w PRAWDZIWE dane, jeśli nikt jej ich nie podmienił.

          Wiersz z ustawieniem pokazuje się tylko dla okna, bo tylko tam jego brak jest ODMOWĄ
          startu. Zdanie pod spodem mówi to zawczasu i tym samym słowem, którym odmówi bieg
          (niezmiennik 29): odmowę czyta się przy wypełnianiu kafelka, a nie w czwartej minucie
          biegu, który już zapłacił za trzy kroki przed tym. */}
      <div className="stack">
        <label className="label" htmlFor="serve-target-kind">
          What this starts
        </label>
        <select
          id="serve-target-kind"
          className={FIELD}
          value={step.targetKind ?? 'web'}
          onChange={(event) => {
            const chosen = event.target.value;
            if (chosen !== 'web' && chosen !== 'native' && chosen !== 'cli') return;
            onEditStep({ targetKind: chosen });
          }}
        >
          <option value="web">A web app or a server</option>
          <option value="native">An app with its own window</option>
          <option value="cli">A command-line program</option>
        </select>
        {step.targetKind === 'native' ? (
          <>
            <label className="label" htmlFor="serve-test-data-env">
              Setting it reads for a test data folder
            </label>
            <input
              id="serve-test-data-env"
              className={FIELD}
              placeholder="MURMUR_DATA_DIR"
              value={step.testDataEnv ?? ''}
              onChange={(event) => {
                onEditStep({ testDataEnv: event.target.value || undefined });
              }}
            />
            <span className="lead" data-field="testDataEnvState">
              {(step.testDataEnv ?? '').trim() === ''
                ? 'Without this, Loadout does not start the app: a test instance writing into ' +
                  'your real data folder is worse than no test at all. That is a missing setting ' +
                  'in the app, not a result about it.'
                : `Loadout starts this app with ${(step.testDataEnv ?? '').trim()} pointing at a ` +
                  'folder of its own inside this run, and waits for its window — not just an ' +
                  'open port — before the steps after it begin.'}
            </span>
          </>
        ) : null}
      </div>

      <div className="stack">
        <label htmlFor="serve-command" className="label">
          Command to run
        </label>
        <input
          id="serve-command"
          className={FIELD}
          placeholder="npm run dev"
          value={step.command}
          disabled={step.commandFrom !== undefined}
          onChange={(event) => {
            onEditStep({ command: event.target.value });
          }}
        />
        {/* PRZEŁĄCZNIK, NIE DRUGIE POLE OBOK. Komenda ma jedno źródło naraz — wpisana ręcznie
            albo oddana przez krok przed tym — a dwa wypełnione pola obok siebie każą człowiekowi
            zgadywać, które wygra. Pole wyżej gaśnie, kiedy wygrywa krok przed tym, więc widać to
            bez czytania czegokolwiek.

            Nazwa pola jest ustalona i nie ma kontrolki: to jest ta sama nazwa, o którą krok przed
            tym jest proszony w swoim „What it hands over", a dwie nazwy do uzgodnienia w dwóch
            miejscach są pierwszą rzeczą, która się rozjedzie — i rozjazd widać dopiero jako bieg,
            który dochodzi do tego kafelka po to, żeby odmówić. */}
        <Tick
          className="flex items-baseline gap-2 text-body text-ink"
          label="Let the step before this one work out the command"
          field="commandFrom"
          checked={step.commandFrom !== undefined}
          onChange={(event) => {
            onEditStep({
              /* NAZWA LICZONA RAZ, PRZY ZAZNACZENIU, i od tej chwili zapisana. Przeliczana
                 przy każdym renderze znaczyłaby, że przemianowanie kafelka po cichu rozłącza
                 graf — powód w całości stoi przy `fieldNameFor`. */
              commandFrom: event.target.checked ? { field: fieldNameFor(step) } : undefined,
            });
          }}
        />
        {step.commandFrom === undefined ? null : (
          <>
            <label className="label" htmlFor="serve-command-format">
              What the agent hands over
            </label>
            <select
              id="serve-command-format"
              className={FIELD}
              value={step.commandFrom.format ?? 'command'}
              onChange={(event) => {
                if (
                  step.commandFrom === undefined ||
                  (event.target.value !== 'command' && event.target.value !== 'launch-description')
                )
                  return;
                onEditStep({ commandFrom: { ...step.commandFrom, format: event.target.value } });
              }}
            >
              <option value="command">A command only</option>
              <option value="launch-description">
                An app description with folder, variables and addresses
              </option>
            </select>
            <label className="label" htmlFor="serve-command-producer">
              Use this earlier result
            </label>
            <select
              id="serve-command-producer"
              className={FIELD}
              value={step.commandFrom.producer ?? ''}
              onChange={(event) => {
                if (step.commandFrom === undefined) return;
                onEditStep({
                  commandFrom: { ...step.commandFrom, producer: event.target.value || undefined },
                });
              }}
            >
              <option value="">The only matching result — refuse if there is more than one</option>
              {commandProducers.map((choice) => (
                <option key={choice.key} value={choice.key}>
                  {choice.name}
                </option>
              ))}
            </select>
            <span className="lead">
              A missing result stops this step. The saved manual command is not used instead.
            </span>
          </>
        )}
        {step.commandFrom === undefined ? null : (
          /* TRZY STANY, NIE JEDEN NAPIS. „Poproś go" bez powiedzenia, czy już poproszono, każe
             człowiekowi sprawdzać drugi kafelek za każdym razem; a kafelek bez poprzednika
             odsyła go do kroku, którego nie ma. */
          <span className="lead" data-field="commandFromState">
            {stepBefore === null
              ? 'Nothing points at this tile yet, so there is nobody to work the command out. ' +
                'Draw an arrow from the step that should.'
              : handsItOver
                ? `${stepBefore} hands over “${step.commandFrom.field}”, and this tile runs it.`
                : `${stepBefore} does not hand over “${step.commandFrom.field}” yet, so this ` +
                  `tile would have nothing to run.`}
          </span>
        )}
        {step.commandFrom === undefined || handsItOver || stepBefore === null ? null : (
          /* JAWNY PRZYCISK, NIE EFEKT UBOCZNY ZAZNACZENIA. Kliknięcie w ten kafelek, które po
             cichu zmienia SĄSIEDNI, jest rodzajem magii, przez którą przestaje się ufać
             edytorowi — a tutaj widać, co się stanie, zanim się to zrobi. */
          <button
            type="button"
            data-field="askTheStepBefore"
            className="text-left text-label text-accent hover:underline"
            onClick={onAskTheStepBefore}
          >
            Ask {stepBefore} for it
          </button>
        )}
        {/* To zdanie jest CAŁĄ różnicą między tym kafelkiem a krokiem „sprawdź" i musi stać
            tam, gdzie człowiek podejmuje decyzję (niezmiennik 29). Druga połowa mówi, DOKĄD ta
            rzecz idzie i kiedy umiera: bez niej człowiek nie wie, czy zostanie mu w tle serwer
            trzymający port. „Started" jest nazwą TEJ SEKCJI z ekranu biegu (`rail.tsx`), nie
            naszym słowem — człowiek ma szukać tego, co widzi (niezmiennik 13). */}
        <span className="lead">
          {step.startWhen === 'asked'
            ? 'The workflow continues once this app is configured. Starting it later waits for the requested response. '
            : step.commandFrom?.format === 'launch-description'
              ? 'The app description chooses which response the next steps wait for. '
              : step.readiness === undefined
                ? 'The steps after this one start right away, without waiting for it to finish. '
                : 'The steps after this one wait until the app responds as requested. '}
          {step.lifetime === 'run'
            ? 'It stops when this workflow ends.'
            : 'It stays alive under Started on the right until you stop it there or close Loadout.'}
        </span>
      </div>

      {step.commandFrom?.format === 'launch-description' ? null : (
        <div className="stack">
          <label htmlFor="serve-readiness" className="label">
            Wait until the app responds
          </label>
          <select
            id="serve-readiness"
            className={FIELD}
            value={step.readiness?.kind ?? ''}
            onChange={(event) => {
              const kind = event.target.value;
              onEditStep({
                readiness:
                  kind === 'http' || kind === 'tcp'
                    ? {
                        kind,
                        endpoint: step.readiness?.endpoint ?? 'web',
                        path: step.readiness?.path ?? '/',
                        timeoutSeconds: step.readiness?.timeoutSeconds ?? 30,
                        expectedStatus: step.readiness?.expectedStatus ?? 200,
                      }
                    : undefined,
                ...(kind === '' || (step.endpoints?.length ?? 0) > 0
                  ? {}
                  : {
                      endpoints: [{ name: 'web', host: '127.0.0.1', port: 3000, portEnv: 'PORT' }],
                    }),
              });
            }}
          >
            <option value="">Do not check — only start it</option>
            <option value="http">An HTTP response</option>
            <option value="tcp">An open connection</option>
          </select>
          {step.readiness === undefined ? null : (
            <ReadinessFields step={step} onEditStep={onEditStep} />
          )}
        </div>
      )}

      <WhereItWorks
        group="serve-where"
        offers={OFFERS}
        value={step.folder}
        onChoose={(folder) => {
          onEditStep({ folder });
        }}
      />
    </>
  );
}

function ReadinessFields({
  step,
  onEditStep,
}: Pick<ServePanelProps, 'step' | 'onEditStep'>): ReactElement {
  const readiness = step.readiness;
  if (readiness === undefined) return <></>;
  return (
    <>
      {(step.endpoints ?? []).map((endpoint, index) => (
        <div className="stack" key={index}>
          <label className="label">
            Address name
            <input
              className={FIELD}
              value={endpoint.name}
              onChange={(event) => {
                const name = event.target.value;
                onEditStep({
                  endpoints: step.endpoints?.map((one, at) =>
                    at === index ? { ...one, name } : one,
                  ),
                  readiness:
                    readiness.endpoint === endpoint.name
                      ? { ...readiness, endpoint: name }
                      : readiness,
                });
              }}
            />
          </label>
          <label className="label">
            Port (0 chooses one)
            <input
              type="number"
              min={0}
              max={65535}
              className={FIELD}
              value={endpoint.port}
              onChange={(event) =>
                onEditStep({
                  endpoints: step.endpoints?.map((one, at) =>
                    at === index ? { ...one, port: Number(event.target.value) } : one,
                  ),
                })
              }
            />
          </label>
          <label className="label">
            Port variable
            <input
              className={FIELD}
              value={endpoint.portEnv ?? ''}
              onChange={(event) =>
                onEditStep({
                  endpoints: step.endpoints?.map((one, at) =>
                    at === index ? { ...one, portEnv: event.target.value } : one,
                  ),
                })
              }
            />
          </label>
        </div>
      ))}
      <button
        type="button"
        className="btn-quiet"
        onClick={() =>
          onEditStep({
            endpoints: [
              ...(step.endpoints ?? []),
              {
                name: `address${String((step.endpoints?.length ?? 0) + 1)}`,
                host: '127.0.0.1',
                port: 0,
                portEnv: `PORT_${String((step.endpoints?.length ?? 0) + 1)}`,
              },
            ],
          })
        }
      >
        Add another address
      </button>
      <label className="label">
        Address to check
        <select
          className={FIELD}
          value={readiness.endpoint}
          onChange={(event) =>
            onEditStep({ readiness: { ...readiness, endpoint: event.target.value } })
          }
        >
          {(step.endpoints ?? []).map((endpoint) => (
            <option key={endpoint.name} value={endpoint.name}>
              {endpoint.name}
            </option>
          ))}
        </select>
      </label>
      {readiness.kind !== 'http' ? null : (
        <>
          <label className="label">
            Path
            <input
              className={FIELD}
              value={readiness.path}
              onChange={(event) =>
                onEditStep({ readiness: { ...readiness, path: event.target.value } })
              }
            />
          </label>
          <label className="label">
            Expected response
            <input
              type="number"
              min={100}
              max={599}
              className={FIELD}
              value={readiness.expectedStatus}
              onChange={(event) =>
                onEditStep({
                  readiness: { ...readiness, expectedStatus: Number(event.target.value) },
                })
              }
            />
          </label>
        </>
      )}
      <label className="label">
        Wait at most (seconds)
        <input
          type="number"
          min={1}
          max={120}
          className={FIELD}
          value={readiness.timeoutSeconds}
          onChange={(event) =>
            onEditStep({ readiness: { ...readiness, timeoutSeconds: Number(event.target.value) } })
          }
        />
      </label>
    </>
  );
}
