/* Podgląd jednego źródła: miniatura obrazu, strona dokumentu albo początek tekstu.
 *
 * ADRESEM JEST ZATWIERDZONE ŹRÓDŁO, nie ścieżka (PLAN §12) — i widać to w propsach: ten
 * komponent nie zna ani jednej ścieżki, dostaje kawałek, który już wrócił z granicy.
 *
 * OBRAZ JEDZIE JAKO `data:`, bo `img-src` w CSP aplikacji dopuszcza `data:`, a `blob:` nie.
 * Podgląd zbudowany na `blob:` byłby pusty w prawdziwym oknie i pełny w przeglądarce testowej,
 * czyli działałby dokładnie tam, gdzie nikt na niego nie patrzy.
 */
import type { ReactElement } from 'react';

import type { SourcePart } from '../../state/context';

export interface SourcePreviewProps {
  readonly part: SourcePart;
  /** Nazwa źródła, żeby podgląd mówił, na co człowiek patrzy. */
  readonly name: string;
  /** Sąsiednia strona dokumentu — `null`, kiedy nie ma dokąd iść. */
  readonly onPage: (number: number) => void;
  readonly onClose: () => void;
}

export default function SourcePreview({
  part,
  name,
  onPage,
  onClose,
}: SourcePreviewProps): ReactElement {
  return (
    <div data-source-preview className="card flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <span className="text-ink">{name}</span>
        <button data-close-preview type="button" className="btn-quiet ml-auto" onClick={onClose}>
          Close
        </button>
      </div>
      <Inside part={part} onPage={onPage} />
    </div>
  );
}

function Inside({
  part,
  onPage,
}: {
  readonly part: SourcePart;
  readonly onPage: (number: number) => void;
}): ReactElement {
  if (part.kind === 'image') {
    return (
      <img
        data-preview-image
        className="max-w-160"
        alt="What this picture holds, at the size it was saved."
        src={'data:' + part.image.mime + ';base64,' + part.image.base64}
      />
    );
  }
  if (part.kind === 'page') {
    return (
      <div className="flex flex-col gap-2">
        {/* NUMER STRONY STOI PRZY TREŚCI, a nie w nagłówku podglądu: odwołanie „to jest na
            stronie 3" ma dać się przeczytać razem z tym, co na niej stoi. */}
        <span data-preview-page className="value">
          Page {part.number} of {part.pagesTotal}
        </span>
        {part.image === null ? null : (
          <img
            data-preview-image
            className="max-w-160"
            alt={'How page ' + String(part.number) + ' looks.'}
            src={'data:' + part.image.mime + ';base64,' + part.image.base64}
          />
        )}
        <p className="lead whitespace-pre-wrap">{part.text}</p>
        <div className="flex items-center gap-2">
          <button
            data-page-back
            type="button"
            className="btn"
            disabled={part.number <= 1}
            onClick={() => {
              onPage(part.number - 1);
            }}
          >
            ← Previous page
          </button>
          <button
            data-page-on
            type="button"
            className="btn"
            disabled={part.number >= part.pagesTotal}
            onClick={() => {
              onPage(part.number + 1);
            }}
          >
            Next page →
          </button>
        </div>
      </div>
    );
  }
  if (part.kind === 'text') {
    return (
      <div className="flex flex-col gap-1">
        <p data-preview-text className="lead whitespace-pre-wrap">
          {part.text}
        </p>
        {/* „Jest tego więcej" jest FAKTEM, nie ozdobą: bez niego początek pliku czyta się jak
            cały plik, a człowiek nie wie, że reszta gdzieś jest. */}
        {part.more ? <span className="value">This is the beginning of a longer file.</span> : null}
      </div>
    );
  }
  /* `whole` wraca wyłącznie do tego, kto ma ten plik przygotować — pokazanie go tutaj byłoby
     wyświetleniem pliku w miejscu, w którym człowiek prosił o jego treść. */
  return <p className="lead">This file is not prepared yet.</p>;
}
