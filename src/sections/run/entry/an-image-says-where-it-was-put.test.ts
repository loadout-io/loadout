/* Wklejony obraz zostawia w tekście ślad tam, GDZIE go wklejono.
 *
 * Zgłoszenie właściciela 2026-09-01: „tak samo jak zdjecie wysylam w terminal to spoko ze jest
 * preview ale chce miec w tekscie miejsce ze akurat tam zostalo dodane bo czesto odnosze sie do
 * danego miejsca". Do tego dnia `EntryDraft` trzymał `images` jako listę OBOK `text`, więc zdanie
 * „tu przycisk jest ucięty" docierało do agenta bez adresu słowa „tu".
 *
 * SŁABA WERSJA TEGO KRYTERIUM pytałaby, czy tekst zawiera `[image 1]`. Przeszłaby dla
 * implementacji doklejającej znacznik na KOŃCU wiadomości — czyli dla tej samej wady, tylko
 * napisanej inaczej. Dlatego każdy punkt niżej pyta o MIEJSCE, nie o obecność.
 */
import { describe, expect, it } from 'vitest';

import { markersIn, placedAt, renumbered, withoutMarker } from './image-marker';

describe('an image says where it was put', () => {
  it('leaves its mark at the caret, not at the end of the message', () => {
    const placed = placedAt('before after', 'before '.length);

    expect(
      placed.text,
      'the mark did not land where the person was typing. A mark appended at the end says only ' +
        '"there is an image somewhere in this message", which is what the image strip already ' +
        'said, and leaves the word "here" without an address.',
    ).toBe('before [image 1] after');
  });

  it('puts the caret after its own mark, so the next word is not written before the image', () => {
    const placed = placedAt('', 0);

    expect(placed.caret, 'the caret stayed before the mark').toBe('[image 1] '.length);
  });

  it('numbers by where the marks stand, not by the order they were pasted', () => {
    /* Wklejenie na końcu, potem cofnięcie kursora na początek i drugie wklejenie. Numerowanie po
     * kolejności wklejenia postawiłoby tu `[image 2]` PRZED `[image 1]`. */
    const first = placedAt('a b', 3);
    const second = placedAt(first.text, 0);

    expect(
      second.text,
      'the marks read out of order. The strip under the field numbers its thumbnails left to ' +
        'right, so a message whose marks count backwards points every sentence at the wrong ' +
        'picture.',
    ).toBe('[image 1] a b [image 2] ');
    expect(second.index, 'the new image did not take the place its mark took in the text').toBe(0);
  });

  it('renumbers what is left when one image is taken away', () => {
    const text = '[image 1] one [image 2] two [image 3] three';

    expect(
      withoutMarker(text, 1),
      'removing the middle image left a hole in the numbering, so the third thumbnail is ' +
        'labelled 3 while its mark still says 3 and nothing carries 2.',
    ).toBe('[image 1] one two [image 2] three');
  });

  it('does not double a space that was already there', () => {
    /* Złapane 2026-09-01 przez `e2e/tests/pasted-image-reaches-lead.spec.ts`, nie przez ten plik:
     * wklejenie w środek zdania z odstępem po kursorze dawało `[image 1]  tail`. Podwójna spacja
     * jest znakiem, którego człowiek nie napisał i nie ma jak sobie wytłumaczyć. */
    expect(placedAt('keep caption tail', 'keep caption '.length).text).toBe(
      'keep caption [image 1] tail',
    );
  });

  it('takes the space it added away with it', () => {
    expect(
      withoutMarker('[image 1] word', 0),
      'the mark went but its trailing space stayed, so every removal leaves a gap the person ' +
        'did not type and cannot explain.',
    ).toBe('word');
  });

  it('reads a message written by hand without renumbering it into nonsense', () => {
    expect(markersIn('no marks here'), 'found a mark where there is none').toHaveLength(0);
    expect(renumbered('[image 7] alone'), 'a lone mark did not become the first').toBe(
      '[image 1] alone',
    );
  });
});
