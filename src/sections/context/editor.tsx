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

import type {
  ContextApp,
  ContextBuild,
  ContextDraft,
  ContextRevision,
  ContextSetRead,
  DraftEdit,
  ImportItem,
  ImportResult,
  RevisionEdit,
  SourcePart,
} from '../../state/context';
import type { AgentAppStatus } from '../../state/agent-apps';
import BuildControls from './build-controls';
import ContextOverview from './overview';
import { carriesAPicture, pastedIntoMaterial } from './paste';
import { chooseFilesToAdd } from './pick-files';
import SourceList from './source-list';
import SourcePreview from './source-preview';

/** Identyfikator jedynego źródła tekstowego zestawu — powód stoi w nagłówku pliku. */
const TYPED = 'typed';

/** Nazwa, pod którą to źródło stoi na liście źródeł. */
const TYPED_NAME = 'Typed material';

export interface ContextEditorProps {
  /** Zestaw odczytany z dysku, razem z rewizją, którą ten zapis odda z powrotem. */
  open: ContextSetRead;
  /** Zdanie po odmowie — `null`, kiedy nie ma o czym mówić. */
  refusal: string | null;
  /** Wynik ostatniego importu: jeden wiersz na każdą pozycję, którą człowiek wybrał. */
  imported: readonly ImportResult[];
  /** Otwarty podgląd — `null`, kiedy nikt o niego nie prosił. */
  preview: { readonly sourceId: string; readonly part: SourcePart } | null;
  /** Źródło, które właśnie się przygotowuje. */
  preparing: string | null;
  build: ContextBuild | null;
  version: ContextRevision | null;
  buildWith: ContextApp;
  buildModel: string;
  claudeCode: AgentAppStatus;
  codex: AgentAppStatus;
  /** `true`, kiedy zapis naprawdę wszedł. Ekran zostaje otwarty, cokolwiek wróci. */
  onSave: (edit: DraftEdit) => Promise<boolean>;
  /** Kładzie w zestawie to, co człowiek wybrał albo wkleił. */
  onAdd: (items: ImportItem[]) => void;
  onPrepare: (sourceId: string) => void;
  onPreview: (sourceId: string, page: number | null) => void;
  onHidePreview: () => void;
  onRemove: (sourceId: string) => void;
  onChooseBuildWith: (app: ContextApp) => void;
  onBuildModel: (model: string) => void;
  onBuild: () => void;
  onStopBuild: () => void;
  onSaveRevision: (edit: RevisionEdit) => void;
  onClose: () => void;
}

/** Materiał wpisany w polu — pusty napis, kiedy w zestawie nie ma jeszcze ani jednego źródła. */
function typedText(draft: ContextDraft): string {
  return draft.sources.find((source) => source.id === TYPED)?.text ?? '';
}

/**
 * Szkic po edycji: pole materiału plus WSZYSTKIE źródła plikowe, których to pole nie dotyczy.
 *
 * Pusty materiał nie zostawia pustego źródła: wiersz bez treści na liście źródeł byłby kształtem
 * materiału, którego nikt nie wpisał — a to jest ta sama wada, co pole z napisem `null`.
 *
 * 2026-09-07 (CT-02) — DRUGA POŁOWA TEJ FUNKCJI POWSTAŁA Z NAPRAWY. Do dziś wymieniała CAŁĄ
 * listę źródeł na jeden wpis `typed`, bo innych źródeł nie było. Od chwili, w której zestaw
 * może trzymać pliki, pierwszy `Save` po imporcie kasowałby każdy z nich — i wyglądałby przy
 * tym na udany.
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
  const typed = was.sources.find((source) => source.id === TYPED);
  return {
    ...was,
    sources: [
      ...(text === ''
        ? []
        : [
            {
              id: TYPED,
              kind: 'text' as const,
              name: TYPED_NAME,
              description: typed?.description ?? '',
              text,
            },
          ]),
      ...was.sources.filter((source) => source.id !== TYPED),
    ],
    howToPrepare,
    requirements: lines,
  };
}

export default function ContextEditor({
  open,
  refusal,
  imported,
  preview,
  preparing,
  build,
  version,
  buildWith,
  buildModel,
  claudeCode,
  codex,
  onSave,
  onAdd,
  onPrepare,
  onPreview,
  onHidePreview,
  onRemove,
  onChooseBuildWith,
  onBuildModel,
  onBuild,
  onStopBuild,
  onSaveRevision,
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

  /* Okno wyboru pliku oddaje ŚCIEŻKI, a Rust sam je otwiera i kopiuje. Anulowanie jest pustą
     listą, czyli wartością, nie błędem (niezmiennik 7) — i wtedy nie ma czego wysyłać. */
  const addFiles = (): void => {
    void chooseFilesToAdd().then((paths) => {
      onAdd(paths.map((path) => ({ name: '', path, text: null, image: null })));
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
              onPaste={(event) => {
                /* PRZEJMUJEMY WYŁĄCZNIE WKLEJENIE Z OBRAZEM. Sprawdzenie musi być tutaj
                   i synchronicznie: po pierwszym `await` jest już za późno na `preventDefault`,
                   a pole, w którym Cmd+V przestaje wstawiać zdanie, jest polem zepsutym. */
                if (!carriesAPicture(event.clipboardData)) return;
                event.preventDefault();
                void pastedIntoMaterial(event.clipboardData).then((item) => {
                  if (item !== null) onAdd([item]);
                });
              }}
            />
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <button data-add-files type="button" className="btn" onClick={addFiles}>
              Add files
            </button>
            <span className="lead">
              Pictures, PDF files, Markdown and plain text. Paste a screenshot into Material to keep
              it here too.
            </span>
          </div>

          {imported.length === 0 ? null : (
            /* JEDEN WIERSZ NA KAŻDY WYBRANY PLIK (PLAN §5). Jedno zdanie „część plików nie
               weszła" jest odpowiedzią, po której człowiek musi zgadywać, które to były. */
            <ul data-import-results className="flex flex-col gap-2">
              {imported.map((one, at) => (
                <li
                  key={one.name + String(at)}
                  data-import-result
                  className="card flex flex-col gap-1"
                >
                  <span className="text-ink">{one.name}</span>
                  {one.refused === null ? (
                    <span className="value">Added</span>
                  ) : (
                    <span role="alert" className="text-fail">
                      {one.refused}
                    </span>
                  )}
                  {one.notes.map((note) => (
                    <span key={note} className="lead">
                      {note}
                    </span>
                  ))}
                </li>
              ))}
            </ul>
          )}

          <SourceList
            sources={open.draft.sources.filter((source) => source.id !== TYPED)}
            preparing={preparing}
            onPreview={(sourceId) => {
              /* Dokument otwiera się na PIERWSZEJ stronie; obraz i tekst numeru strony nie mają,
                 więc idą bez niego. Strona, której jeszcze nie przygotowano, wraca z własnym
                 zdaniem — i tak ma być, bo to jest prawda o tym pliku. */
              const source = open.draft.sources.find((one) => one.id === sourceId);
              onPreview(sourceId, source?.kind === 'pdf' ? 1 : null);
            }}
            onPrepare={onPrepare}
            onRemove={onRemove}
          />

          {preview === null ? null : (
            <SourcePreview
              part={preview.part}
              name={open.draft.sources.find((source) => source.id === preview.sourceId)?.name ?? ''}
              onPage={(number) => {
                onPreview(preview.sourceId, number);
              }}
              onClose={onHidePreview}
            />
          )}

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
        <div className="flex flex-col gap-4">
          <BuildControls
            build={build}
            app={buildWith}
            model={buildModel}
            claudeCode={claudeCode}
            codex={codex}
            hasVersion={version !== null}
            sourceNames={Object.fromEntries(
              open.draft.sources.map((source) => [source.id, source.name]),
            )}
            onChooseApp={onChooseBuildWith}
            onModel={onBuildModel}
            onBuild={onBuild}
            onStop={onStopBuild}
          />
          {refusal === null ? null : (
            <p data-refusal role="alert" className="text-fail">
              {refusal}
            </p>
          )}
          {version === null ? (
            <div className="card flex flex-col items-center gap-3 text-center">
              <span aria-hidden className="mark">
                ◇
              </span>
              <p data-nothing-built className="text-ink">
                Nothing has been prepared from this material yet.
              </p>
              <p className="lead max-w-160">
                Everything you write under Sources is kept exactly as you wrote it, and stays yours
                to edit.
              </p>
              <button
                type="button"
                className="btn"
                onClick={() => {
                  setTab('sources');
                }}
              >
                Go to Sources
              </button>
            </div>
          ) : (
            <ContextOverview revision={version} onSave={onSaveRevision} />
          )}
        </div>
      )}
    </div>
  );
}
