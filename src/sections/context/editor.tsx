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
import { useRef, useState } from 'react';
import { why } from '../../ipc/why';

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
import { whatIsNextForTheSet } from '../../state/context';
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
  onAdd: (items: ImportItem[]) => Promise<void> | void;
  onPrepare: (sourceId: string) => Promise<void> | void;
  onPreview: (sourceId: string, page: number | null) => void;
  onHidePreview: () => void;
  onRemove: (sourceId: string) => Promise<void> | void;
  onChooseBuildWith: (app: ContextApp) => void;
  onBuildModel: (model: string) => void;
  onBuild: () => Promise<void> | void;
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

/** Co powiedzieć po UDANYM zapisie szkicu.
 *
 * 2026-09-09 (CT-09, znalezisko natywne) — właściciel kliknął `Save` osiem razy i za każdym
 * razem zapis SIĘ UDAŁ (`draftRevision` doszedł do 8), a ekran nie powiedział ani słowa: handler
 * ustawiał zdanie tylko przy PORAŻCE. Zapis, który udaje się bez śladu, jest nieodróżnialny od
 * zapisu, który nic nie zrobił — i wniosek „Save nie działa" był racjonalny.
 *
 * Zdanie nie brzmi „Saved", bo to nie jest pytanie, które człowiek naprawdę zadaje. Zadaje
 * „czy mogę tego już użyć", a odpowiedź zależy od tego, czy zestaw ma gotową wersję. */
export function whatTheSaveChanged(hasReadyRevision: boolean): string {
  return hasReadyRevision
    ? 'Saved. Prepare this set again so steps receive the change.'
    : 'Saved. This set needs preparing before a step can use it.';
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
  const [title, setTitle] = useState(open.set.title);
  const [description, setDescription] = useState(open.set.description);
  const [text, setText] = useState(typedText(open.draft));
  const [howToPrepare, setHowToPrepare] = useState(open.draft.howToPrepare);
  const [requirements, setRequirements] = useState(open.draft.requirements.join('\n'));
  const [editing, setEditing] = useState(false);
  const [working, setWorking] = useState<string | null>(null);
  const [said, setSaid] = useState<string | null>(null);
  const [localRefusal, setLocalRefusal] = useState<string | null>(null);
  // 2026-09-09: dwa kliknięcia przed następnym renderem też są jednym zleceniem.
  const occupied = useRef(false);
  const running = build?.end === 'running' || build?.end === 'stillRunning';
  const busy = working !== null || running || preparing !== null;
  const draft = draftFrom(open.draft, text, howToPrepare, requirements);
  const dirty =
    title.trim() !== open.set.title ||
    description !== open.set.description ||
    text !== typedText(open.draft) ||
    howToPrepare !== open.draft.howToPrepare ||
    JSON.stringify(draft.requirements) !== JSON.stringify(open.draft.requirements);
  const needsRebuild =
    version !== null && (dirty || version.draftRevision !== open.set.draftRevision);
  const showMaterials = version === null || editing || needsRebuild;

  const perform = async (label: string, task: () => Promise<void>): Promise<void> => {
    if (occupied.current || running || preparing !== null) return;
    occupied.current = true;
    setWorking(label);
    setSaid(null);
    setLocalRefusal(null);
    try {
      await task();
    } catch (error) {
      setLocalRefusal(why(error, 'Loadout could not finish that action. Try again.'));
    } finally {
      occupied.current = false;
      setWorking(null);
    }
  };
  const save = async (): Promise<boolean> =>
    onSave({
      id: open.set.id,
      title: title.trim(),
      description,
      draft,
      expectedRevision: open.revision,
    });
  const buildCurrent = (): void => {
    void perform('Saving…', async () => {
      // Build czyta dysk. Klik ma najpierw zapisać widoczne pola, a odmowa nie może
      // uruchomić agenta na starym materiale. Niezmienionego szkicu nie wersjonujemy ponownie.
      if (dirty && !(await save())) return;
      setWorking('Building…');
      setEditing(false);
      await onBuild();
    });
  };
  const addFiles = (): void => {
    void perform('Adding files…', async () => {
      const paths = await chooseFilesToAdd();
      if (paths.length > 0)
        await onAdd(paths.map((path) => ({ name: '', path, text: null, image: null })));
    });
  };
  const options = (
    <>
      <label className="flex flex-col gap-1" htmlFor="context-title">
        <span className="label">Name</span>
        <input
          id="context-title"
          className="field"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
        />
      </label>
      <label className="flex flex-col gap-1" htmlFor="context-description">
        <span className="label">What is this context for? (optional)</span>
        <input
          id="context-description"
          className="field"
          value={description}
          placeholder="e.g. Designing screens for Murmur"
          onChange={(e) => setDescription(e.target.value)}
        />
      </label>
      <label className="flex flex-col gap-1" htmlFor="context-preparation">
        <span className="label">How should the agent organize it? (optional)</span>
        <textarea
          id="context-preparation"
          className="field"
          value={howToPrepare}
          placeholder="e.g. Group by screen and separate inspiration from requirements"
          onChange={(e) => setHowToPrepare(e.target.value)}
        />
      </label>
      <label className="flex flex-col gap-1" htmlFor="context-requirements">
        <span className="label">Rules to keep exactly (optional, one per line)</span>
        <textarea
          id="context-requirements"
          className="field"
          value={requirements}
          placeholder="e.g. All interface text must be in English"
          onChange={(e) => setRequirements(e.target.value)}
        />
      </label>
    </>
  );

  return (
    <div data-context-editor={open.set.id} className="mx-auto flex w-full max-w-240 flex-col gap-4">
      <div className="flex items-center gap-2">
        <button data-back type="button" className="btn-quiet" disabled={busy} onClick={onClose}>
          ← All sets
        </button>
        <h2 className="text-heading text-ink">{title === '' ? open.set.title : title}</h2>
        {version !== null && !needsRebuild ? (
          <button
            data-edit-material
            type="button"
            className="btn-quiet ml-auto"
            disabled={busy}
            onClick={() => setEditing(!editing)}
          >
            {showMaterials ? 'View context' : 'Edit material'}
          </button>
        ) : null}
      </div>
      <p data-set-next className="lead">
        {needsRebuild
          ? 'Build again to include your changes. Workflows keep the previous version until you update them.'
          : whatIsNextForTheSet(open.set, draft)}
      </p>

      <div className="card flex flex-col gap-4">
        {showMaterials ? (
          <fieldset disabled={busy} className="flex flex-col gap-3">
            <label className="flex flex-col gap-2" htmlFor="context-material">
              <span className="label">Material</span>
              <textarea
                id="context-material"
                className="field min-h-40"
                value={text}
                placeholder="Paste notes, describe what matters, or paste a screenshot here."
                onChange={(e) => setText(e.target.value)}
                onPaste={(event) => {
                  if (!carriesAPicture(event.clipboardData)) return;
                  event.preventDefault();
                  const clipboard = event.clipboardData;
                  void perform('Adding files…', async () => {
                    const item = await pastedIntoMaterial(clipboard);
                    if (item !== null) await onAdd([item]);
                  });
                }}
              />
            </label>
            <div className="flex items-center gap-3">
              <button data-add-files type="button" className="btn" onClick={addFiles}>
                Add files
              </button>
              <span className="caption">Images, PDFs and text files</span>
            </div>
            {imported.some((one) => one.refused !== null || one.added.length === 0) ? (
              <ul data-import-results className="flex flex-col gap-2">
                {imported
                  .filter((one) => one.refused !== null || one.added.length === 0)
                  .map((one, at) => (
                    <li
                      key={one.name + String(at)}
                      data-import-result
                      className="card flex flex-col gap-1"
                    >
                      <span className="text-ink">{one.name}</span>
                      {one.refused !== null ? (
                        <span role="alert" className="text-fail">
                          {one.refused}
                        </span>
                      ) : null}
                      {one.notes.map((note) => (
                        <span key={note} className="lead">
                          {note}
                        </span>
                      ))}
                    </li>
                  ))}
              </ul>
            ) : null}
            {open.draft.sources.some((source) => source.id !== TYPED) ? (
              <SourceList
                sources={open.draft.sources.filter((source) => source.id !== TYPED)}
                preparing={preparing}
                disabled={busy}
                onPreview={(sourceId) => {
                  const source = open.draft.sources.find((one) => one.id === sourceId);
                  onPreview(sourceId, source?.kind === 'pdf' ? 1 : null);
                }}
                onPrepare={(sourceId) => {
                  void perform('Preparing…', async () => {
                    await onPrepare(sourceId);
                  });
                }}
                onRemove={(sourceId) => {
                  void perform('Removing…', async () => {
                    await onRemove(sourceId);
                  });
                }}
              />
            ) : null}
            {preview !== null ? (
              <SourcePreview
                part={preview.part}
                name={
                  open.draft.sources.find((source) => source.id === preview.sourceId)?.name ?? ''
                }
                onPage={(number) => onPreview(preview.sourceId, number)}
                onClose={onHidePreview}
              />
            ) : null}
          </fieldset>
        ) : null}
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
          onBuild={buildCurrent}
          onStop={onStopBuild}
          options={options}
          preparing={preparing}
          pending={working === 'Building…' ? null : working}
          disabled={busy || title.trim() === '' || draft.sources.length === 0}
          secondaryAction={
            showMaterials ? (
              <button
                data-save
                type="button"
                className="btn-quiet"
                disabled={busy}
                onClick={() => {
                  void perform('Saving…', async () => {
                    if (await save())
                      setSaid(whatTheSaveChanged(open.set.latestReadyRevision !== null));
                  });
                }}
              >
                Save draft
              </button>
            ) : null
          }
        />
        {refusal !== null || localRefusal !== null ? (
          <p data-refusal role="alert" className="text-fail">
            {refusal ?? localRefusal}
          </p>
        ) : said !== null ? (
          <p data-saved role="status" className="caption">
            {said}
          </p>
        ) : null}
      </div>
      {version !== null ? (
        <ContextOverview key={version.id} revision={version} onSave={onSaveRevision} />
      ) : null}
    </div>
  );
}
