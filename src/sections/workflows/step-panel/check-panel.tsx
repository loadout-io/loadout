/* Panel kafelka „sprawdź" — cztery pola, bo ten kafelek ma cztery rzeczy do rozstrzygnięcia.
 *
 * Istnieje z tego samego powodu, co `serve-panel.tsx` i `checkpoint-panel.tsx`, i ten powód
 * jest niezmiennikiem 16: kafelek, którego nie da się wypełnić, jest kafelkiem, którego nie da
 * się uruchomić — a człowiek dowiaduje się o tym dopiero w środku biegu. Do 2026-08-23 klik
 * w ten kafelek wpadał w „wybierz agenta", czyli w formularz pytający o rzecz, której ten
 * kafelek nie ma.
 *
 * DWA POLA TEKSTOWE, NIE JEDNO, i to jest cała różnica między tym kafelkiem a „uruchom
 * i zostaw". Tamten podnosi komendę i idzie dalej; ten na nią CZEKA i sam orzeka wynik —
 * z tego, czy komenda wróciła bez błędu, ORAZ z tego, czy w wyjściu stoi to, co człowiek tu
 * wpisał. Bez drugiego pola werdykt liczyłby się z samego powrotu komendy, a suita, która nie
 * uruchomiła ani jednego testu, wraca szczęśliwa (niezmiennik 19). Dlatego Rust odmawia ZAPISU
 * pliku z pustym wzorcem (`check::a_command_step_left_empty`), a nie dopiero uruchomienia.
 *
 * To NIE jest `StepPanel` z siódemką wierszy: tu nie ma agenta, więc nie ma dziedziczenia,
 * nadpisań ani wiersza Skills — i nie ma po czym pokazywać wartości efektywnych.
 *
 * Ramki tu nie ma: rysuje ją `PanelForStep`, jedną, dla wszystkich paneli.
 */
import type { ReactElement } from 'react';
import type { CheckStep, WhenItFails } from '../../../state/workflows';

/** Cztery pola kafelka sprawdzenia. Ten kafelek nie ma agenta, więc nie dziedziczy niczego —
 * nie ma tu nadpisań ani wartości efektywnych, są same pola kroku. */
export type CheckFields = Partial<
  Pick<CheckStep, 'name' | 'command' | 'proof' | 'folder' | 'whenItFails'>
>;

export interface CheckPanelProps {
  step: CheckStep;
  onEditStep: (fields: CheckFields) => void;
}

/* GDZIE TO BIEGNIE — trzy wyjścia, bo dla sprawdzenia wszystkie trzy mają sens i wszystkie trzy
 * niesie plik.
 *
 * Dla tego kafelka to nie jest szczegół, tylko treść: `cargo test` w folderze projektu patrzy na
 * kod BEZ pracy, którą krok przed nim właśnie napisał, i przechodzi na starej wersji. Zielone
 * sprawdzenie, które sprawdziło nie tę pracę, jest gorsze niż jego brak — bo wygląda na dowód.
 *
 * Własna kopia zostaje na liście, w odróżnieniu od kafelka „uruchom i zostaw": sprawdzenie
 * biegnące obok innego kroku pisze po `target/` i po `node_modules/`, więc reguła kolizji
 * z niezmiennika 12 obowiązuje je tak samo jak agenta, a to jest jedyne wyjście z takiej kolizji. */
const WHERE = [
  {
    use: 'same-copy' as const,
    name: 'In the same copy as the step before it',
    why: 'Looks at the work that step just wrote. This is what a check is usually for.',
  },
  {
    use: 'project' as const,
    name: 'In your project folder',
    why: 'Looks at your project as it stands right now, without the work of the steps before it.',
  },
  {
    use: 'fresh-copy' as const,
    name: 'In a copy of its own',
    why: 'Its own copy, so it cannot write over a step running beside it. It sees none of their work.',
  },
];

/* Trzy odpowiedzi na porażkę, brzmienie w brzmienie z panelu kroku agenta
 * (`panel.tsx`, `WhenItFailsRow`). Druga kopia, świadoma: tamten wiersz jest napisany na polach
 * kroku agenta i nie da się go tu zamontować bez rozmontowania tamtego panelu. Rozjazd łapie
 * recenzja; wspólny moduł brzmień jest właściwym domem dla obu, kiedy ktoś będzie posiadał
 * oba pliki. */
const WHEN_IT_FAILS = [
  { value: 'stop' as const, label: 'Stop here' },
  { value: 'carry-on' as const, label: 'Carry on anyway' },
  { value: 'ask-me' as const, label: 'Ask me what to do' },
];

/** Wariant z listy albo dotychczasowy. Rzutowanie napisu z DOM-u na wariant enuma byłoby
 * obietnicą, której ten napis nie składa. */
function failureFrom(raw: string, now: WhenItFails): WhenItFails {
  return WHEN_IT_FAILS.find((one) => one.value === raw)?.value ?? now;
}

const ROW = 'flex flex-col gap-1';
const LABEL = 'text-label text-muted';
/* Klasa domu, nie własny opis — ten sam powód, co w `serve-panel.tsx`. */
const FIELD = 'field';

export function CheckPanel({ step, onEditStep }: CheckPanelProps): ReactElement {
  /* Brak klucza w pliku znaczy `carry-on` (`state/workflows.ts`), więc lista pokazuje to samo,
   * co zrobi bieg. Kafelek postawiony przyciskiem niesie tu `stop` i to jest jego wartość, nie
   * domyślna wartość pola. */
  const whenItFails = step.whenItFails ?? 'carry-on';

  return (
    <>
      <div className={ROW}>
        <label htmlFor="check-name" className={LABEL}>
          Name
        </label>
        <input
          id="check-name"
          className={FIELD}
          value={step.name}
          onChange={(event) => {
            onEditStep({ name: event.target.value });
          }}
        />
      </div>

      <div className={ROW}>
        <label htmlFor="check-command" className={LABEL}>
          Command to run
        </label>
        <input
          id="check-command"
          className={FIELD}
          placeholder="npm test"
          value={step.command}
          onChange={(event) => {
            onEditStep({ command: event.target.value });
          }}
        />
        {/* Zdanie o CZEKANIU stoi tam, gdzie człowiek wybiera między tym kafelkiem a „uruchom
            i zostaw" (niezmiennik 29). Bez niego oba wyglądają jak jedno pole na wiersz powłoki,
            a różnią się jedyną rzeczą, która ma tu znaczenie. */}
        <span className={LABEL}>
          The steps after this one wait for it to finish, and what it says here decides whether they
          get the work.
        </span>
      </div>

      <div className={ROW}>
        <label htmlFor="check-passes-when" className={LABEL}>
          Counts as passed when the output contains
        </label>
        <input
          id="check-passes-when"
          className={FIELD}
          placeholder="(\d+) passed"
          value={step.proof}
          onChange={(event) => {
            onEditStep({ proof: event.target.value });
          }}
        />
        {/* JEDYNY ZNAK SPECJALNY, JAKI TO POLE ZNA, i jedyne miejsce, w którym człowiek może się
            o nim dowiedzieć. Bez tego zdania wpisze liczbę, którą akurat widzi („12 passed"),
            a wzorzec przestanie pasować przy trzynastym teście — czyli sprawdzenie zacznie
            mówić „nie" o pracy, która jest w porządku. Ta sama notacja, co w linii `expect:`
            naszej własnej bramki: jedna notacja, jedno znaczenie. */}
        <span className={LABEL}>
          Plain text, with (\d+) standing for a number: write (\d+) passed and the count can be
          anything. Left empty, this step cannot be saved — a command that ran nothing at all comes
          back happy.
        </span>
      </div>

      <div className={ROW}>
        <span className={LABEL}>Where it runs</span>
        {WHERE.map((one) => (
          <label key={one.use} className="flex items-baseline gap-2 text-body text-ink">
            <input
              type="radio"
              name="check-where"
              value={one.use}
              /* `pick` nie ma tu kontrolki, więc plik z ręcznie wpisaną ścieżką nie zaznaczy ani
               * jednego pola — i to jest uczciwsze niż zaznaczenie czegoś, czego w pliku nie ma
               * (niezmiennik 17). Człowiek widzi wtedy, że musi wybrać. */
              checked={step.folder.use === one.use}
              onChange={() => {
                onEditStep({ folder: { use: one.use } });
              }}
            />
            <span className="min-w-0">
              {one.name}
              <span className={'block ' + LABEL}>{one.why}</span>
            </span>
          </label>
        ))}
      </div>

      <div className={ROW}>
        <label htmlFor="check-when-it-fails" className={LABEL}>
          If this check does not pass
        </label>
        <select
          id="check-when-it-fails"
          className={FIELD}
          value={whenItFails}
          onChange={(event) => {
            onEditStep({ whenItFails: failureFrom(event.target.value, whenItFails) });
          }}
        >
          {WHEN_IT_FAILS.map((one) => (
            <option key={one.value} value={one.value}>
              {one.label}
            </option>
          ))}
        </select>
        <span className={LABEL}>
          Stopping keeps the steps after this one from starting. Carrying on hands them the work
          anyway, and tells them it did not pass.
        </span>
      </div>
    </>
  );
}
