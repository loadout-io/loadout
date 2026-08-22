/* Widoczny szkic obrazów pod wierszem wejścia — T-34 AC-6.
 *
 * Nazwy są wyłącznie porządkowe („Pasted image 1"), nigdy z `File.name`: oryginalna nazwa może
 * sama być sekretem i zgodnie z kontraktem T-34 nie opuszcza zdarzenia paste. Remove dostaje
 * numer z tej samej mapy co podgląd, więc czytnik ekranu wskazuje dokładnie ten obraz.
 */
import type { ReactElement } from 'react';

import type { PastedImage } from './images';

export interface ImageStripProps {
  readonly images: readonly PastedImage[];
  readonly onRemove: (id: string) => void;
}

export function ImageStrip({ images, onRemove }: ImageStripProps): ReactElement {
  if (images.length === 0) return <></>;

  return (
    <div data-image-strip className="mb-2 flex min-w-0 gap-2 overflow-x-auto">
      {images.map((image, index) => {
        const number = index + 1;
        const label = `Pasted image ${String(number)}`;
        return (
          <figure
            key={image.id}
            data-image-preview
            className="relative size-14 shrink-0 overflow-hidden rounded-sm border border-line-strong bg-well"
          >
            <img src={image.previewUrl} alt={label} className="size-full object-cover" />
            <button
              type="button"
              aria-label={`Remove pasted image ${String(number)}`}
              title={`Remove ${label.toLowerCase()}`}
              onClick={() => {
                onRemove(image.id);
              }}
              className="absolute top-1 right-1 grid size-6 place-items-center rounded-sm border border-line-strong bg-panel text-ui text-ink"
            >
              ×
            </button>
          </figure>
        );
      })}
    </div>
  );
}
