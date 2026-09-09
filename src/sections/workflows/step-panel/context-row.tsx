/* CT-05: jeden zwijany wiersz pokazuje wynik rustowego resolvera i trzyma oba pickery.
 *
 * 2026-09-08 — panel nie liczy dziedziczenia sam. Gdyby React składał wybór drugi raz, mógłby
 * pokazać `From workflow` dla materiału, którego Start właśnie odmówił albo wykluczył. */
import { useState } from 'react';
import type { ReactElement } from 'react';

import { matching } from '../../../state/context';
import type {
  ContextChoice,
  ContextPin,
  ContextTopics,
  SelectedContext,
  StepContext,
  StepContextView,
  WorkflowContext,
  WorkflowContextView,
} from '../../../state/context';

const EMPTY_STEP: StepContext = { schema: 1, sets: [], exclude: [] };

export function contextRowStands(): boolean {
  return true;
}

function withPin(pins: readonly ContextPin[], pin: ContextPin): ContextPin[] {
  return [...pins.filter((one) => one.id !== pin.id), pin];
}

function withoutPin(pins: readonly ContextPin[], id: string): ContextPin[] {
  return pins.filter((one) => one.id !== id);
}

/* 2026-09-08 (CT-05): `context` pochodzi z rustowego `extra`, więc niepoprawny ręczny JSON
 * przekracza typ TypeScript. Panel ma pokazać odmowę resolvera, nie zgasnąć przed jej tekstem. */
function readablePins(value: unknown): ContextPin[] {
  if (!Array.isArray(value)) return [];
  return value.filter((pin): pin is ContextPin => {
    if (typeof pin !== 'object' || pin === null) return false;
    const candidate = pin as Partial<ContextPin>;
    return (
      typeof candidate.id === 'string' &&
      typeof candidate.revision === 'string' &&
      (candidate.topics === 'all' ||
        (Array.isArray(candidate.topics) &&
          candidate.topics.every((topic) => typeof topic === 'string')))
    );
  });
}

function readableIds(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((id): id is string => typeof id === 'string') : [];
}

function stepChoice(
  value: StepContext | undefined,
  change: Partial<StepContext>,
): StepContext | undefined {
  const known =
    typeof value === 'object' && value !== null && !Array.isArray(value) ? value : undefined;
  const next = {
    ...EMPTY_STEP,
    ...known,
    exclude: readableIds(known?.exclude),
    sets: readablePins(known?.sets),
    ...change,
  };
  /* 2026-09-08 (CT-05): pusty wybór wraca do braku klucza, bo sam klucz podnosi format do 2;
   * wyczyszczenie Context nie może zostawić dokumentu wyglądającego na używający tej funkcji. */
  return next.inheritWorkflow === undefined &&
    (next.sets?.length ?? 0) === 0 &&
    (next.exclude?.length ?? 0) === 0
    ? undefined
    : next;
}

function selectionSays(selected: ContextTopics): string {
  return selected === 'all'
    ? 'All topics'
    : `${String(selected.length)} topic${selected.length === 1 ? '' : 's'}`;
}

function TopicChoices({
  selected,
  topics,
  onChoose,
}: {
  selected: ContextTopics;
  topics: readonly { id: string; title: string }[];
  onChoose: (topics: ContextTopics) => void;
}): ReactElement {
  const picked = new Set(selected === 'all' ? topics.map((topic) => topic.id) : selected);
  return (
    <details className="rounded-sm border border-line p-2">
      <summary className="caption cursor-pointer">{selectionSays(selected)}</summary>
      <div className="stack pt-2" data-gap="2">
        <label className="flex items-baseline gap-2 text-body text-ink">
          <input
            type="radio"
            checked={selected === 'all'}
            onChange={() => {
              onChoose('all');
            }}
          />
          All topics
        </label>
        {topics.map((topic) => (
          <label key={topic.id} className="flex items-baseline gap-2 text-body text-ink">
            <input
              type="checkbox"
              checked={picked.has(topic.id)}
              onChange={() => {
                const next = topics
                  .map((one) => one.id)
                  .filter((id) => (id === topic.id ? !picked.has(id) : picked.has(id)));
                if (next.length > 0) onChoose(next);
              }}
            />
            {topic.title}
          </label>
        ))}
      </div>
    </details>
  );
}

function UpdateChoice({
  selected,
  latest,
  onChoose,
}: {
  selected: SelectedContext;
  latest: ContextChoice;
  onChoose: (pin: ContextPin) => void;
}): ReactElement | null {
  const [replacementTopics, setReplacementTopics] = useState<ContextTopics>(() =>
    selected.selectedTopics === 'all'
      ? 'all'
      : selected.selectedTopics.filter((topic) => latest.topics.some((one) => one.id === topic)),
  );
  if (selected.update === null || latest.revision === null) return null;
  const oldIds = new Set(selected.topics.map((topic) => topic.id));
  const newIds = new Set(latest.topics.map((topic) => topic.id));
  const added = latest.topics.filter((topic) => !oldIds.has(topic.id));
  const removed = selected.topics.filter((topic) => !newIds.has(topic.id));
  const changed = latest.topics.flatMap((topic) => {
    const old = selected.topics.find((one) => one.id === topic.id);
    return old === undefined || old.title === topic.title ? [] : [`${old.title} → ${topic.title}`];
  });
  const chosenMissing =
    selected.selectedTopics === 'all'
      ? []
      : selected.selectedTopics.filter((topic) => !newIds.has(topic));
  /* 2026-09-08 (CT-05): przy utracie wybranego ID nie ma bezpiecznego domysłu. Przycisk wraca
   * dopiero po nowym wyborze; rozszerzenie na `all` ujawniłoby materiał, którego nie wybrano. */
  return (
    <details data-context-update className="rounded-sm border border-warn-edge bg-warn-soft p-2">
      <summary className="label cursor-pointer">{selected.update}</summary>
      <div className="stack pt-2" data-gap="2">
        <p className="caption">
          {added.length === 0
            ? 'No topics were added.'
            : `Added: ${added.map((one) => one.title).join(', ')}.`}
        </p>
        <p className="caption">
          {removed.length === 0
            ? 'No topics were removed.'
            : `Removed: ${removed.map((one) => one.title).join(', ')}.`}
        </p>
        <p className="caption">
          {changed.length === 0 ? 'No topics changed.' : `Changed: ${changed.join(', ')}.`}
        </p>
        {chosenMissing.length > 0 ? (
          <div className="stack" data-gap="2">
            <p className="text-body text-warn">
              A chosen topic was removed. Choose topics for the new version before updating.
            </p>
            <TopicChoices
              selected={replacementTopics}
              topics={latest.topics}
              onChoose={setReplacementTopics}
            />
            {replacementTopics === 'all' || replacementTopics.length > 0 ? (
              <button
                type="button"
                className="btn-quiet"
                onClick={() => {
                  onChoose({
                    id: selected.id,
                    revision: latest.revision ?? selected.revision,
                    topics: replacementTopics,
                  });
                }}
              >
                Update
              </button>
            ) : null}
          </div>
        ) : (
          <button
            type="button"
            className="btn-quiet"
            onClick={() => {
              onChoose({
                id: selected.id,
                revision: latest.revision ?? selected.revision,
                topics: selected.selectedTopics,
              });
            }}
          >
            Update
          </button>
        )}
      </div>
    </details>
  );
}

function Catalog({
  catalog,
  query,
  pins,
  resolved,
  onChoose,
}: {
  catalog: readonly ContextChoice[];
  query: string;
  pins: readonly ContextPin[];
  resolved: readonly SelectedContext[];
  onChoose: (pins: ContextPin[]) => void;
}): ReactElement {
  return (
    <div className="stack" data-gap="2">
      {matching(catalog, query).map((set) => {
        const pin = pins.find((one) => one.id === set.id);
        const selected = resolved.find((one) => one.id === set.id);
        return (
          <div key={set.id} className="stack rounded-sm border border-line p-2" data-gap="2">
            <label className="flex items-baseline gap-2 text-body text-ink">
              <input
                type="checkbox"
                checked={pin !== undefined}
                disabled={pin === undefined && set.revision === null}
                onChange={(event) => {
                  if (!event.target.checked) {
                    onChoose(withoutPin(pins, set.id));
                  } else if (set.revision !== null) {
                    onChoose(withPin(pins, { id: set.id, revision: set.revision, topics: 'all' }));
                  }
                }}
              />
              {set.title}
            </label>
            {set.description === '' ? null : <p className="caption">{set.description}</p>}
            {set.said === null ? null : <p className="text-body text-warn">{set.said}</p>}
            {pin === undefined || selected === undefined ? null : (
              <TopicChoices
                selected={pin.topics}
                topics={selected.topics}
                onChoose={(topics) => onChoose(withPin(pins, { ...pin, topics }))}
              />
            )}
            {pin === undefined || selected === undefined ? null : (
              <UpdateChoice
                key={`${selected.id}:${selected.revision}:${set.revision ?? 'missing'}`}
                selected={selected}
                latest={set}
                onChoose={(next) => onChoose(withPin(pins, next))}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}

function ContextPickerBox({
  searchLabel,
  catalog,
  pins,
  resolved,
  onChoose,
}: {
  searchLabel: string;
  catalog: readonly ContextChoice[];
  pins: readonly ContextPin[];
  resolved: readonly SelectedContext[];
  onChoose: (pins: ContextPin[]) => void;
}): ReactElement {
  const [query, setQuery] = useState('');
  return (
    <>
      <input
        className="field"
        type="search"
        aria-label={searchLabel}
        placeholder="Search context"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      <Catalog
        catalog={catalog}
        query={query}
        pins={pins}
        resolved={resolved}
        onChoose={onChoose}
      />
    </>
  );
}

export function WorkflowContextPicker({
  value,
  view,
  onChoose,
}: {
  value: WorkflowContext | undefined;
  view: WorkflowContextView | null;
  onChoose: (value: WorkflowContext | undefined) => void;
}): ReactElement {
  const pins = readablePins(value?.sets);
  return (
    <details data-workflow-context className="mx-3 mt-3 rounded-md border border-line bg-panel p-2">
      <summary className="label cursor-pointer">
        Context · {pins.length === 0 ? 'None' : `${String(pins.length)} selected`}
      </summary>
      <div className="stack pt-2" data-gap="2">
        {view === null ? (
          <p className="caption">Context choices are being read.</p>
        ) : (
          <ContextPickerBox
            searchLabel="Search workflow context"
            catalog={view.catalog}
            pins={pins}
            resolved={view.workflow}
            onChoose={(sets) => onChoose(sets.length === 0 ? undefined : { schema: 1, sets })}
          />
        )}
        {view?.warnings.map((warning) => (
          <p key={warning} className="text-body text-warn">
            {warning}
          </p>
        ))}
      </div>
    </details>
  );
}

export function ChatContextPicker({
  value,
  view,
  refusal,
  onChoose,
}: {
  value: readonly ContextPin[];
  view: WorkflowContextView | null;
  refusal?: string | null;
  onChoose: (value: ContextPin[]) => void;
}): ReactElement {
  const pins = readablePins(value);
  /* 2026-09-08 (CT-07) — WNĘTRZE POWSTAJE DOPIERO PO ROZWINIĘCIU. Zwinięty `<details>`
   * trzyma swoje dzieci w drzewie, a kolektor gęstości JE LICZY — zmierzone: zdanie
   * schowane w środku podbijało `textElements` w widoku domyślnym. Bez tego całe pole
   * szukania, katalog i każdy zestaw wchodziłyby do pomiaru widoku, w którym człowiek
   * ich nawet nie widzi. */
  const [open, setOpen] = useState(false);
  return (
    <details
      data-chat-context
      className="mb-2 rounded-md border border-line bg-panel p-2"
      onToggle={(event) => {
        setOpen(event.currentTarget.open);
      }}
    >
      {/* 2026-09-08 (CT-07) — STAN NIEUDANEGO ODCZYTU STOI NA UCHWYCIE, nie tylko w środku.
          Picker jest zwinięty domyślnie, więc odmowa widoczna wyłącznie po rozwinięciu jest
          odmową, której nikt nie przeczyta: człowiek zobaczyłby „Context · None" i uznał, że
          po prostu nic nie wybrał. Pełne zdanie zostaje w środku, dla tego, kto rozwinie. */}
      <summary className="label cursor-pointer">
        Context ·{' '}
        {refusal === null || refusal === undefined
          ? pins.length === 0
            ? 'None'
            : `${String(pins.length)} selected`
          : 'could not be read'}
      </summary>
      {!open ? null : (
        <div className="stack pt-2" data-gap="2">
          {/* 2026-09-08 (CT-07) — JEDNO ZDANIE NARAZ. Stało tu „Context choices are being read."
            OBOK zdania odmowy, więc ekran mówił jednocześnie, że właśnie czyta wybór i że nie
            umiał go odczytać. Nieudany odczyt nie jest trwaniem odczytu: gdy jest odmowa, to
            ona jest całą prawdą o tym stanie. */}
          {refusal === null || refusal === undefined ? (
            view === null ? (
              <p className="caption">Context choices are being read.</p>
            ) : (
              <ContextPickerBox
                searchLabel="Search conversation context"
                catalog={view.catalog}
                pins={pins}
                resolved={view.workflow}
                onChoose={onChoose}
              />
            )
          ) : (
            <p className="text-body text-warn">{refusal}</p>
          )}
          {view?.warnings.map((warning) => (
            <p key={warning} className="text-body text-warn">
              {warning}
            </p>
          ))}
        </div>
      )}
    </details>
  );
}

export function ContextRow({
  value,
  view,
  catalog,
  refusal,
  onChoose,
}: {
  value: StepContext | undefined;
  view: StepContextView | null;
  catalog: readonly ContextChoice[];
  refusal: string | null;
  onChoose: (value: StepContext | undefined) => void;
}): ReactElement {
  const local = readablePins(value?.sets);
  const excluded = new Set(readableIds(value?.exclude));
  const chosen = view?.sets ?? [];
  /* 2026-09-08 (CT-05): wykluczony zestaw też musi tu stać, bo inaczej kontrolka, która go
   * wyłączyła, znikałaby dokładnie w chwili, w której jest potrzebna do ponownego włączenia. */
  const inheritedChoices = [
    ...(view?.sets.filter((set) => set.source === 'workflow') ?? []),
    ...(view?.omitted ?? []),
  ].filter((set, at, all) => all.findIndex((one) => one.id === set.id) === at);
  return (
    <div data-row="context" className="stack">
      <span className="label">Context</span>
      {/* 2026-09-09 — stan stoi przed pickerem, bo Context jest teraz jednym z pięciu faktów
          widocznych bez otwierania ustawień; same wybory nadal należą do ujawnienia niżej. */}
      <span className="lead">
        {chosen.length === 0 ? 'No context' : `${String(chosen.length)} selected`}
      </span>
      {chosen.map((set) => (
        <div key={set.id} className="stack rounded-sm border border-line p-2" data-gap="2">
          <div className="flex items-baseline justify-between gap-2">
            <span className="text-body text-ink">{set.title}</span>
            <span className="caption">
              {set.source === 'workflow' ? 'From workflow' : 'Added to this step'}
            </span>
          </div>
          <span className="caption">{selectionSays(set.selectedTopics)}</span>
          {set.update === null ? null : <span className="text-body text-warn">{set.update}</span>}
          {set.said === null ? null : <span className="text-body text-warn">{set.said}</span>}
        </div>
      ))}
      {view?.omitted.map((set) => (
        <p key={set.id} className="caption">
          {set.title}: {set.said}
        </p>
      ))}
      {view?.said === null || view?.said === undefined ? null : (
        <p className="caption">{view.said}</p>
      )}
      {refusal === null ? null : <p className="text-body text-warn">{refusal}</p>}

      <details data-step-context-picker className="rounded-sm border border-line p-2">
        <summary className="caption cursor-pointer">Choose context</summary>
        <div className="stack pt-2" data-gap="2">
          <label className="flex items-baseline gap-2 text-body text-ink">
            <input
              type="checkbox"
              checked={view?.inheritsWorkflow ?? value?.inheritWorkflow !== false}
              onChange={(event) => {
                onChoose(stepChoice(value, { inheritWorkflow: event.target.checked }));
              }}
            />
            Use workflow context
          </label>
          {inheritedChoices.map((set) => (
            <label key={set.id} className="flex items-baseline gap-2 text-body text-ink">
              <input
                type="checkbox"
                checked={!excluded.has(set.id)}
                onChange={(event) => {
                  const next = event.target.checked
                    ? [...excluded].filter((id) => id !== set.id)
                    : [...excluded, set.id];
                  onChoose(stepChoice(value, { exclude: next }));
                }}
              />
              Use {set.title} from workflow
            </label>
          ))}
          <ContextPickerBox
            searchLabel="Search step context"
            catalog={catalog}
            pins={local}
            resolved={chosen}
            onChoose={(sets) => onChoose(stepChoice(value, { sets }))}
          />
          {chosen
            .filter((set) => set.source === 'workflow')
            .map((set) => {
              const pin = local.find((one) => one.id === set.id) ?? {
                id: set.id,
                revision: set.revision,
                topics: set.selectedTopics,
              };
              return (
                <div key={`topics-${set.id}`} className="stack" data-gap="2">
                  <span className="caption">Topics for {set.title}</span>
                  <TopicChoices
                    selected={pin.topics}
                    topics={set.topics}
                    onChoose={(topics) =>
                      onChoose(stepChoice(value, { sets: withPin(local, { ...pin, topics }) }))
                    }
                  />
                </div>
              );
            })}
        </div>
      </details>
    </div>
  );
}
