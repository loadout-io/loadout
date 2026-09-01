/* Wiersz „What it hands over" — czym ten krok płaci następnemu.
 *
 * # Po co to istnieje
 *
 * Pytanie właściciela 2026-08-30: „czy agenci są świadomi co po nich następuje… np że architekt
 * do dewelopera?". Odpowiedź z kodu brzmiała: wstecz wiedzą wszystko (indeks przekazań podpisany
 * NAZWĄ kafelka), w przód nie wiedzą nic poza tym, że ktoś jest.
 *
 * Rozważona i odrzucona odpowiedź: dopisać do promptu każdego agenta nazwy następników. Odrzucona,
 * bo sama nazwa roli jest CIENKA — „następny jest Tester" każe agentowi zgadywać, co z tego
 * wynika, a prompt rośnie wtedy monotonicznie u wszystkich i na zawsze (AGENTS.md, niezmiennik 28).
 *
 * Rzecz, która naprawdę zmienia wynik, była w produkcie od dawna i **nie miała ani jednej
 * kontrolki**: formularz przekazania. Tu człowiek mówi wprost „następny krok potrzebuje pola
 * `plan`, opisanego tak-a-tak", a agent dostaje konkretne zamówienie z jego własnym opisem —
 * zamiast nazwy roli do interpretacji.
 *
 * # Co ten wiersz obiecuje, a czego nie
 *
 * Pola **dokładają się** do trzech nagłówków, nie zastępują ich: po stronie Rusta prośba brzmi
 * „This step ALSO has to hand these back" (`commands::run`, `FIELDS_ASKED_FOR`). Napis w tym
 * wierszu musi to oddawać, bo kontrolka obiecująca zamianę byłaby kontrolką, która kłamie
 * o skutku.
 *
 * „Needed" też nie jest ozdobą: pole oznaczone jako potrzebne, którego agent nie odda, jest
 * ODMOWĄ kroku, a nie brakiem w odpowiedzi (`missing_a_required_field`). Dlatego stoi przy nim
 * zdanie o skutku, a nie samo słowo.
 */
import type { ReactElement } from 'react';

import type { Handover, HandoverField } from '../../../state/workflows';

export interface HandoverRowProps {
  /** Co ten krok oddaje dzisiaj. */
  value: Handover;
  onEditStep: (fields: { handover: Handover }) => void;
}

/* `ROW`, `LABEL` i `NOTE` zniknęły 2026-08-31: rolę niosą `.stack`, `.label` i `.lead`
 * z `theme.css`. `CHOICE` zostaje jako klej układu (pole wyboru obok zdania), a `FIELD` jest
 * już nazwą prymitywu, nie jego przepisaniem.
 *
 * `ADD` dostał REAKCJĘ NA NAJECHANIE. „Remove" i „+ Ask for one more" są kontrolkami, a do dziś
 * nie zmieniały się pod kursorem ani przy skupieniu — czyli czytały się jak podpis. Nie biorą
 * `.btn-bare`, bo ten ma 28 px wysokości i padding: postawiony w wierszu z polem wyboru
 * rozepchnąłby formularz, a to byłaby zmiana układu przemycona pod migracją. */
const CHOICE = 'flex items-baseline gap-2 text-body text-ink';
const FIELD = 'field';
const ADD = 'label text-left hover:text-ink';

/** Pola, które ten krok oddaje — pusta lista, kiedy oddaje samą prozę. */
function fieldsOf(value: Handover): readonly HandoverField[] {
  return value === 'notes' ? [] : value.fields;
}

/**
 * Wiersz formularza przekazania.
 *
 * Świeże pole powstaje z pustymi napisami i **z `required: true`**: człowiek, który dokłada pole,
 * chce je dostać — a pole nieobowiązkowe, o które nikt nie prosił, jest wierszem promptu bez
 * skutku. Odznaczenie zostaje, bo format je zna i wiersz ma być jego lustrem, nie zawężeniem.
 */
export function HandoverRow({ value, onEditStep }: HandoverRowProps): ReactElement {
  const fields = fieldsOf(value);
  const asFields = value !== 'notes';

  const write = (next: readonly HandoverField[]) => {
    onEditStep({ handover: { fields: [...next] } });
  };

  const edit = (at: number, change: Partial<HandoverField>) => {
    write(fields.map((one, index) => (index === at ? { ...one, ...change } : one)));
  };

  return (
    <div data-row="handover" className="stack">
      <span className="label">What it hands over</span>

      <label className={CHOICE}>
        <input
          type="radio"
          name="handover-shape"
          checked={!asFields}
          onChange={() => {
            onEditStep({ handover: 'notes' });
          }}
        />
        Its answer, in its own words
      </label>

      <label className={CHOICE}>
        <input
          type="radio"
          name="handover-shape"
          data-field="handover"
          checked={asFields}
          onChange={() => {
            /* Puste pole od razu, nie pusta lista: „wybrałem tryb i nic się nie stało" jest
               stanem, z którego człowiek nie wie, co zrobić dalej. */
            write(fields.length > 0 ? fields : [{ name: '', describe: '', required: true }]);
          }}
        />
        Its answer, plus named things
      </label>

      {!asFields ? null : (
        <>
          {/* ZDANIE O SKUTKU, nie sama nazwa trybu. Po stronie Rusta prośba brzmi „This step ALSO
              has to hand these back", a pole oznaczone jako potrzebne, którego agent nie odda,
              jest ODMOWĄ kroku — nie brakiem w odpowiedzi. */}
          <span className="lead">
            It still answers under the three headings. These are extra lines it has to write, one
            per line, starting with the name and a colon. A needed one it leaves out stops the step.
          </span>

          {fields.map((field, at) => (
            <div key={at} className="stack pl-4">
              <input
                className={FIELD}
                data-field="handover-name"
                placeholder="plan"
                value={field.name}
                onChange={(event) => {
                  edit(at, { name: event.target.value });
                }}
              />
              <input
                className={FIELD}
                data-field="handover-describe"
                placeholder="what it should contain, in your words"
                value={field.describe}
                onChange={(event) => {
                  edit(at, { describe: event.target.value });
                }}
              />
              <div className="flex items-baseline gap-3">
                <label className={CHOICE}>
                  <input
                    type="checkbox"
                    checked={field.required === true}
                    onChange={(event) => {
                      edit(at, { required: event.target.checked });
                    }}
                  />
                  Needed
                </label>
                <button
                  type="button"
                  className={ADD}
                  onClick={() => {
                    const left = fields.filter((_, index) => index !== at);
                    /* Skasowanie OSTATNIEGO pola wraca do prozy, a nie zostawia pustego
                       formularza: pusty formularz to tryb, który nic nie znaczy, a po stronie
                       Rusta i tak czyta się jak jego brak. */
                    if (left.length === 0) onEditStep({ handover: 'notes' });
                    else write(left);
                  }}
                >
                  Remove
                </button>
              </div>
            </div>
          ))}

          <button
            type="button"
            className={ADD}
            onClick={() => {
              write([...fields, { name: '', describe: '', required: true }]);
            }}
          >
            + Ask for one more
          </button>
        </>
      )}
    </div>
  );
}
