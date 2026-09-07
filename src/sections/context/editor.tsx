/* Edytor jednego zestawu: co w nim leży, po co, i jak ma zostać opracowany.
 *
 * KOMPONENT JEST STEROWANY — zestaw i czynności przychodzą propsami, więc każde kryterium da się
 * postawić bez zdarzenia myszy i bez DOM-u (w repo nie ma `jsdom`). To ta sama umowa, co
 * w `workflows/list/workflow-list.tsx`.
 *
 * JEDNO POLE MATERIAŁU, i to jest zakres tego etapu, nie uproszczenie na zapas: CT-01 zapisuje
 * wyłącznie materiał wpisany w polu, a pliki, obrazy i PDF dokłada CT-02. Źródło ma przez to
 * STAŁY identyfikator [`TYPED`] — przy jednym polu tekstowym na zestaw nie ma czego wybijać,
 * a wybity przy każdym zapisie zrywałby powiązania, które CT-04 buduje na odwołaniach do źródeł.
 *
 * ODMOWA STOI PRZY SWOIM PRZYCISKU. Zdanie z Rusta („ten plik zmienił się na dysku") pokazujemy
 * tam, gdzie człowiek przed chwilą kliknął — odmowa na górze ekranu, pod paskiem nagłówka, jest
 * odmową, której nikt nie zauważy.
 */
import type { ReactElement } from 'react';
import { useState } from 'react';

import type { ContextDraft, ContextSetRead, DraftEdit } from '../../state/context';

/** Identyfikator jedynego źródła tekstowego zestawu — powód stoi w nagłówku pliku. */
const TYPED = 'typed';

/** Nazwa, pod którą to źródło stoi na liście źródeł. */
const TYPED_NAME = 'Typed material';

export interface ContextEditorProps {
  /** Zestaw odczytany z dysku, razem z rewizją, którą ten zapis odda z powrotem. */
  open: ContextSetRead;
  /** Zdanie po odmowie — `null`, kiedy nie ma o czym mówić. */
  refusal: string | null;
  /** `true`, kiedy zapis naprawdę wszedł. Ekran zostaje otwarty, cokolwiek wróci. */
  onSave: (edit: DraftEdit) => Promise<boolean>;
  onClose: () => void;
}

/** Materiał wpisany w polu — pusty napis, kiedy w zestawie nie ma jeszcze ani jednego źródła. */
function typedText(draft: ContextDraft): string {
  return draft.sources.find((source) => source.id === TYPED)?.text ?? '';
}

/**
 * Szkic po edycji: jedno źródło tekstowe albo żadne.
 *
 * Pusty materiał nie zostawia pustego źródła: wiersz bez treści na liście źródeł byłby kształtem
 * materiału, którego nikt nie wpisał — a to jest ta sama wada, co pole z napisem `null`.
 */
function draftFrom(
  was: ContextDraft,
  text: string,
  howToPrepare: string,
  requirements: string,
): ContextDraft {
  const lines = requirements
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '');
  return {
    ...was,
    sources:
      text === ''
        ? []
        : [
            {
              id: TYPED,
              kind: 'text',
              name: TYPED_NAME,
              description: was.sources.find((source) => source.id === TYPED)?.description ?? '',
              text,
            },
          ],
    howToPrepare,
    requirements: lines,
  };
}

export default function ContextEditor({
  open,
  refusal,
  onSave,
  onClose,
}: ContextEditorProps): ReactElement {
  const [tab, setTab] = useState<'sources' | 'overview'>('sources');
  const [title, setTitle] = useState(open.set.title);
  const [description, setDescription] = useState(open.set.description);
  const [text, setText] = useState(typedText(open.draft));
  const [howToPrepare, setHowToPrepare] = useState(open.draft.howToPrepare);
  const [requirements, setRequirements] = useState(open.draft.requirements.join('\n'));

  const save = (): void => {
    void onSave({
      id: open.set.id,
      title: title.trim(),
      description,
      draft: draftFrom(open.draft, text, howToPrepare, requirements),
      /* Rewizja, którą to okno PRZECZYTAŁO. Bez niej zapis z okna otwartego pięć minut temu
         kasuje pracę zapisaną minutę temu i wygląda przy tym na udany. */
      expectedRevision: open.revision,
    });
  };

  return (
    <div data-context-editor={open.set.id} className="flex flex-col gap-4">
      <div className="flex items-center gap-2">
        <button data-back type="button" className="btn-quiet" onClick={onClose}>
          ← All sets
        </button>
        {/* Nazwa zestawu, tak jak człowiek ją widzi na liście. Stopień nagłówka, nie tytułu:
            tytułem tego ekranu jest `Context` w pasku nagłówka (niezmiennik 13). */}
        <h2 className="text-heading text-ink">{title === '' ? open.set.title : title}</h2>
      </div>

      {/* DWIE ZAKŁADKI Z PLAN §12. `aria-pressed` mówi czytnikowi ekranu, która jest wybrana —
          drugi napis o tym samym byłby drugim miejscem na jeden fakt. */}
      <div className="flex gap-2">
        <button
          data-tab="sources"
          type="button"
          className="btn"
          aria-pressed={tab === 'sources'}
          onClick={() => {
            setTab('sources');
          }}
        >
          Sources
        </button>
        <button
          data-tab="overview"
          type="button"
          className="btn"
          aria-pressed={tab === 'overview'}
          onClick={() => {
            setTab('overview');
          }}
        >
          Overview
        </button>
      </div>

      {tab === 'sources' ? (
        <div className="card flex flex-col gap-3">
          <div className="flex flex-col gap-1">
            <label className="label" htmlFor="context-title">
              Name
            </label>
            <input
              id="context-title"
              className="field"
              value={title}
              onChange={(event) => {
                setTitle(event.target.value);
              }}
            />
          </div>

          <div className="flex flex-col gap-1">
            <label className="label" htmlFor="context-description">
              What this set is for
            </label>
            <input
              id="context-description"
              className="field"
              value={description}
              onChange={(event) => {
                setDescription(event.target.value);
              }}
            />
          </div>

          <div className="flex flex-col gap-1">
            <label className="label" htmlFor="context-material">
              Material
            </label>
            <textarea
              id="context-material"
              className="field"
              value={text}
              onChange={(event) => {
                setText(event.target.value);
              }}
            />
          </div>

          <div className="flex flex-col gap-1">
            <label className="label" htmlFor="context-preparation">
              How should this context be prepared?
            </label>
            <textarea
              id="context-preparation"
              className="field"
              value={howToPrepare}
              onChange={(event) => {
                setHowToPrepare(event.target.value);
              }}
            />
          </div>

          <div className="flex flex-col gap-1">
            <label className="label" htmlFor="context-requirements">
              Requirements
            </label>
            {/* Jedno wymaganie w wierszu, i tak wracają na ekran. Brzmienie zostaje słowo
                w słowo: wymaganie skrócone albo sparafrazowane przestaje być tym, co człowiek
                napisał (PLAN §4). */}
            <textarea
              id="context-requirements"
              className="field"
              value={requirements}
              onChange={(event) => {
                setRequirements(event.target.value);
              }}
            />
          </div>

          <div className="flex items-center gap-3">
            <button data-save type="button" className="btn-primary mr-auto" onClick={save}>
              Save
            </button>
            {refusal === null ? null : (
              /* `text-fail` klasą, nie `data-tone`: ton maluje `.lead` i `.value`, a to zdanie
                 żadnej z tych ról nie nosi. */
              <p data-refusal role="alert" className="text-fail">
                {refusal}
              </p>
            )}
          </div>
        </div>
      ) : (
        <div className="card flex flex-col items-center gap-3 text-center">
          <span aria-hidden className="mark">
            ◇
          </span>
          <p data-nothing-built className="text-ink">
            Nothing has been prepared from this material yet.
          </p>
          <p className="lead max-w-160">
            Everything you write under Sources is kept exactly as you wrote it, and stays yours to
            edit.
          </p>
          <button
            type="button"
            className="btn-primary"
            onClick={() => {
              setTab('sources');
            }}
          >
            Go to Sources
          </button>
        </div>
      )}
    </div>
  );
}
