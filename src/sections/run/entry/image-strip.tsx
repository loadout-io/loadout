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
    /* WCHODZI SPRĘŻYNĄ, bo to jest pasek, który POJAWIA SIĘ nad wierszem wejścia w chwili
       wklejenia i którego przed nim nie było (DESIGN §7). Wklejenie obrazu jest jedyną
       czynnością w tym wierszu, po której człowiek nie widzi ani jednego znaku w polu —
       pasek wskakujący skokiem czyta się wtedy jak przeskok układu, a nie jak odpowiedź
       na to, co się właśnie zrobiło.

       KLASA JEST NA PASKU, NIE NA KAŻDYM KAFELKU. Jedno wklejenie potrafi przynieść trzy
       obrazy, a trzy kafelki animujące się od jednego zdarzenia przekroczyłyby sufit dwóch
       regionów [ARCHITECTURE §7]. Pasek jest jednym regionem i pojawia się dokładnie raz:
       przy pierwszym obrazie, bo bez obrazów ten komponent nie renderuje ani jednego węzła. */
    <div data-image-strip className="enter mb-2 flex min-w-0 gap-2 overflow-x-auto">
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
              /* `.btn` (przycisk drugoplanowy) daje tu DOKŁADNIE ten sam wygląd, który stał
                 tu z palca — `--raised` i `--panel` to dziś ta sama wartość co do bajta, obrys
                 `--line-strong` i promień kontrolki też się zgadzają — a dokłada cztery stany,
                 z których ten przycisk nie miał ani jednego. Kwadrat 24 px i zerowy padding
                 zostają klasami narzędziowymi, bo to jest rozmieszczenie, a nie rola: kontrolka
                 siedzi w rogu podglądu 56 px i wysokość prymitywu by go przykryła. Warstwa
                 `utilities` stoi nad `components`, więc obie znoszą się bez `!important`. */
              className="btn absolute top-1 right-1 size-6 p-0"
            >
              ×
            </button>
          </figure>
        );
      })}
    </div>
  );
}
