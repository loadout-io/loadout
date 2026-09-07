/* Lista źródeł zestawu: co w nim leży, w jakim jest stanie i co można z tym zrobić.
 *
 * KOMPONENT JEST STEROWANY — źródła i czynności przychodzą propsami, tak jak w `editor.tsx`.
 *
 * JEDEN STAN NA WIERSZ (PLAN §12). Wiersz mówi, czy źródło jest gotowe, czy czeka na
 * przygotowanie, albo powtarza zdanie, którym Rust nazwał porażkę — trzy różne fakty w trzech
 * miejscach wiersza byłyby trzema odpowiedziami na jedno pytanie (niezmiennik 13).
 */
import type { ReactElement } from 'react';

import type { ContextSource } from '../../state/context';

export interface SourceListProps {
  readonly sources: readonly ContextSource[];
  /** Źródło, które właśnie się przygotowuje — jego `Prepare` jest wtedy wyłączone. */
  readonly preparing: string | null;
  readonly onPreview: (sourceId: string) => void;
  readonly onPrepare: (sourceId: string) => void;
  readonly onRemove: (sourceId: string) => void;
}

/** Ile znaków cytatu wystarczy, żeby człowiek rozpoznał SWÓJ tekst i nie zgubił wiersza. */
const ENOUGH_TO_RECOGNISE = 60;

/**
 * Tekst, przy którym stanął ten obraz — `null`, kiedy stanął sam.
 *
 * CYTAT, NIE NAZWA. Oba źródła z jednego wklejenia nazywają się tak samo („Pasted material"),
 * bo schowek nie podaje nazwy, której ktokolwiek by chciał; nazwa obok nazwy nie mówiłaby więc
 * nic. Pierwsze zdanie tekstu jest jedyną rzeczą, po której człowiek pozna, KTÓREGO podpisu
 * dotyczy ten obraz.
 */
export function pastedWith(
  sources: readonly ContextSource[],
  source: ContextSource,
): string | null {
  const id = source.companionOf ?? null;
  if (id === null) return null;
  const text = sources.find((one) => one.id === id)?.text.trim() ?? '';
  if (text === '') return null;
  return text.length > ENOUGH_TO_RECOGNISE ? text.slice(0, ENOUGH_TO_RECOGNISE) + '…' : text;
}

/** Jedno zdanie o stanie tego źródła — to samo, które czyta człowiek. */
export function stateOf(source: ContextSource): string {
  const preparation = source.preparation;
  if (preparation === undefined || preparation.state === 'notNeeded') return 'Ready';
  if (preparation.state === 'ready') return 'Ready';
  if (preparation.state === 'failed') return preparation.said;
  if (preparation.state === 'needs') {
    const total = source.file?.pages ?? null;
    /* Dopóki nikt nie otworzył pliku, nie wiemy, ile ma stron — i wtedy zdanie o „0 z 0"
     * byłoby liczbą, której nikt nie policzył. */
    return total === null
      ? 'Needs preparation'
      : 'Needs preparation — ' + String(preparation.pagesDone) + ' of ' + String(total) + ' pages';
  }
  /* Stan z nowszego Loadouta. Milczymy o nim uczciwie zamiast zgadywać (niezmiennik 5). */
  return 'Saved';
}

export default function SourceList({
  sources,
  preparing,
  onPreview,
  onPrepare,
  onRemove,
}: SourceListProps): ReactElement {
  if (sources.length === 0) {
    return <p className="lead">Nothing has been added to this set yet.</p>;
  }
  return (
    <ul className="flex flex-col gap-2">
      {sources.map((source) => {
        const waiting = source.preparation?.state === 'needs';
        const caption = pastedWith(sources, source);
        return (
          <li key={source.id} data-source-row={source.id} className="card flex flex-col gap-1">
            <span className="text-ink">{source.name}</span>
            <span className="value">{stateOf(source)}</span>
            {/* POWIĄZANIE JEST WIDOCZNE, nie tylko zapisane. Mieszane wklejenie zostawia dwa
                wiersze, a bez tej linii nikt już nie wie, że były jednym materiałem — a to jest
                dokładnie ta połowa, której `companionOf` w pliku sam z siebie nie dowozi. */}
            {caption === null ? null : (
              <span data-pasted-with={source.companionOf} className="lead">
                Pasted with: {caption}
              </span>
            )}
            {(source.notes ?? []).map((note) => (
              <span key={note} data-source-note className="lead">
                {note}
              </span>
            ))}
            <div className="flex flex-wrap items-center gap-2">
              <button
                data-preview={source.id}
                type="button"
                className="btn"
                onClick={() => {
                  onPreview(source.id);
                }}
              >
                Preview
              </button>
              {waiting ? (
                <button
                  data-prepare={source.id}
                  type="button"
                  className="btn"
                  disabled={preparing !== null}
                  onClick={() => {
                    onPrepare(source.id);
                  }}
                >
                  {preparing === source.id ? 'Preparing…' : 'Prepare'}
                </button>
              ) : null}
              <button
                data-remove={source.id}
                type="button"
                className="btn-quiet"
                onClick={() => {
                  onRemove(source.id);
                }}
              >
                Remove
              </button>
            </div>
          </li>
        );
      })}
    </ul>
  );
}
