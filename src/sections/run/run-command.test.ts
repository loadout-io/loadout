/* `/run` z wiersza wejścia: który workflow i co ma zbudować.
 *
 * PO CO TO ISTNIEJE. Zgłoszenie właściciela 2026-08-19: „ten terminal nie ma sensu teraz xD, no bo
 * jak ja mam np puścić jakieś workflow i przekazać prompta?". Do tego dnia wiersz rozumiał `/open`
 * i `/stop`, a makieta obiecuje w tym samym miejscu `/plan · /run · or just say what you want` —
 * czyli kontrolka wyglądająca na główny sposób pracy nie umiała uruchomić pracy.
 *
 * DLACZEGO KRYTERIUM STOI NA CZYSTEJ FUNKCJI. To repo nie ma jsdom, więc `onClick` ani Enter nie
 * odpalają się w teście, a `renderToStaticMarkup` nie uruchamia efektów. Rozbiór linii zamknięty
 * w komponencie byłby kodem, którego żadne kryterium nie umie dotknąć — dokładnie ta rodzina,
 * z której wzięło się siedemnaście kłamiących kontrolek. `readRunLine` bierze napis i oddaje
 * decyzję, więc test woła to, co woła Enter.
 *
 * SŁABA WERSJA CAŁEGO TEGO PLIKU: sprawdzić, że `/run cokolwiek` czegoś nie odmawia. Przechodzi
 * dla implementacji, która zgaduje — bierze pierwszy workflow z listy i wysyła resztę linii jako
 * zadanie. Rozstrzygają dwie rzeczy naraz: WYBÓR (pierwszy z krokami, nie pierwszy bajtowo)
 * i ODMOWA przy nazwie, której nikt nie ma (bo cichy domysł uruchamia cudzy workflow z twoim
 * promptem i wygląda przy tym na sukces).
 */
import { describe, expect, it } from 'vitest';

import type { Choice } from './choices';
import { NOTHING_SAVED, readRunLine, typable } from './run-command';

/** Krok planu w kształcie, którego chce `Choice`. */
function step(id: string, name: string) {
  return { id, name, state: 'pending' as const };
}

/**
 * Świeży szkic BEZ kroków, pierwszy bajtowo.
 *
 * `new-workflow-2.json` wypada przed `new-workflow.json`, bo `-` (0x2D) jest przed `.` (0x2E),
 * a lista przychodzi z Rusta posortowana bajtowo. Ta pułapka jest udokumentowana w `choices.ts`
 * i kosztowała już jedno „There are no steps yet." powiedziane o workflow z dwoma krokami.
 */
const DRAFT: Choice = { path: 'new-workflow-2.json', name: 'New workflow 2', steps: [] };

const SHIP: Choice = {
  path: 'ship-a-feature.json',
  name: 'Ship a feature',
  steps: [step('s_plan', 'Plan'), step('s_build', 'Build')],
};

const TODO: Choice = {
  path: 'todo-list.json',
  name: 'Todo list',
  steps: [step('s_lead', 'Lead')],
};

const ALL: readonly Choice[] = [DRAFT, SHIP, TODO];

describe('/run picks a workflow and carries what to build', () => {
  it('with nothing after it, starts the first workflow that HAS steps', () => {
    const read = readRunLine(ALL, '');

    /* NIE `choices[0]`, i to jest cała treść tego kryterium. Pierwszy bajtowo jest tu szkicem
     * bez kroków, czyli biegiem, który odmówi — a domyślny wybór gwarantujący odmowę jest gorszy
     * niż brak domyślnego wyboru. */
    expect(
      'go' in read ? read.go.path : read.refusal,
      'a bare /run has to start the same thing the Start button starts without picking: the ' +
        'first workflow with steps in it. Choosing the first one on a byte-sorted list means ' +
        'choosing a fresh empty draft, which refuses on the Rust side.',
    ).toBe(SHIP.path);
    expect('go' in read ? read.task : 'refused', 'nobody said what to build').toBe(null);
  });

  it('takes the first word as the workflow and everything after it as the task', () => {
    const read = readRunLine(ALL, 'todo-list build me a pretty todo list');

    expect('go' in read ? read.go.path : read.refusal, 'the named workflow has to win').toBe(
      TODO.path,
    );
    expect(
      'go' in read ? read.task : 'refused',
      'the rest of the line is what the agents are being asked to build, word for word — this is ' +
        'the argument that was missing entirely, so six agents could only ever build the one ' +
        'thing somebody had typed into the file beforehand',
    ).toBe('build me a pretty todo list');
  });

  it('accepts the name a person can actually type, and the file name too', () => {
    /* Workflow nazywa sam siebie zdaniem („Ship a feature"), a wiersz przyjmuje słowa. Obie drogi
     * muszą prowadzić do jednego workflow, bo inaczej „jak to się nazywa" ma dwie odpowiedzi. */
    const bySlug = readRunLine(ALL, 'ship-a-feature');
    const byFile = readRunLine(ALL, 'ship-a-feature.json');

    expect('go' in bySlug ? bySlug.go.path : bySlug.refusal, 'the typable name has to work').toBe(
      SHIP.path,
    );
    expect('go' in byFile ? byFile.go.path : byFile.refusal, 'so does the file name').toBe(
      SHIP.path,
    );
    expect(typable('Ship a feature'), 'spaces are not typable as one word').toBe('ship-a-feature');
  });

  it('refuses a name nobody has, and says which names exist', () => {
    const read = readRunLine(ALL, 'shipp build the thing');

    const refusal = 'refusal' in read ? read.refusal : 'nothing was refused at all';
    /* ODMAWIA, nie zgaduje. Implementacja, która przy nietrafionym pierwszym słowie bierze całość
     * jako zadanie, uruchomiłaby tu `Ship a feature` z promptem „shipp build the thing" — czyli
     * cudzy workflow z twoim tekstem, i wyglądałaby przy tym na sukces. */
    expect(
      refusal,
      'a typo in the workflow name must not silently become part of the task',
    ).toContain('shipp');
    /* WYMIENIA NAZWY. Odmowa bez listy zostawia człowieka tam, gdzie był: nazw, których nie
     * widzi, nie ma jak zgadnąć (DESIGN §8). */
    expect(refusal, 'the refusal has to show a name that can be typed').toContain('ship-a-feature');
    expect(refusal, 'and the other one as well').toContain('todo-list');
  });

  it('leaves the task empty rather than blank when only a name was typed', () => {
    const read = readRunLine(ALL, 'todo-list   ');

    /* `null`, nie `''`. Po tamtej stronie `None` znaczy „prompt kroku co do bajtu", a `Some("")`
     * byłoby zadaniem, które istnieje i nic nie mówi — czyli nagłówkiem nad pustką w promptcie,
     * za który ktoś płaci długością. */
    expect(
      'go' in read ? read.task : 'refused',
      'a name with no task after it means "run the file as it is", and that is `null` on the wire',
    ).toBe(null);
  });

  it('says there is nothing to run when no workflow has steps', () => {
    const read = readRunLine([DRAFT], '');

    expect(
      'refusal' in read ? read.refusal : 'nothing was refused at all',
      'a folder holding only empty drafts has nothing to run, and the sentence has to say what ' +
        'to do about it',
    ).toBe(NOTHING_SAVED);
  });
});
