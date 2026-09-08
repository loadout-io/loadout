/* Odpowiedź drutu o wyborze rozmowy jest SPRAWDZANA, nie rzutowana.
 *
 * `invoke<T>` nie ogląda tego, co wróciło: `T` znika przy kompilacji. Zanim to sprawdzenie
 * powstało, `null` z drutu wjeżdżał do stanu Reacta jako „wybór, o którym nic nie wiadomo",
 * a picker mówił w kółko `Context choices are being read.` i MILCZAŁ o starych wiadomościach.
 * Ekran wyglądał na wczytujący się zamiast powiedzieć, że nie umie odczytać — a to jest ta
 * różnica, której nie widać z żadnego logu.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: sprawdzić samo `null`. Przechodzi ją implementacja pilnująca
 * wyłącznie `null`, a odpowiedź o WŁAŚCIWYM typie i NIEWŁAŚCIWYM kształcie — obiekt bez `pins`,
 * tablica, `pins` jako napis — dalej wjeżdża do stanu i gasi sekcję przez `ScreenBoundary`.
 * Dlatego każdy z tych kształtów ma tu własny przypadek, a poprawna odpowiedź musi przejść:
 * sprawdzenie odrzucające wszystko jest nieodróżnialne od zepsutego ekranu.
 */
import { describe, expect, it, vi } from 'vitest';

const { invoked, answer } = vi.hoisted(() => {
  const answer = { value: null as unknown };
  return {
    answer,
    invoked: vi.fn(() => Promise.resolve(answer.value)),
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invoked,
  Channel: class {
    public onmessage: ((batch: unknown) => void) | null = null;
  },
}));

const { whatThisChatPinned } = await import('./io');

const GOOD = {
  pins: { folder: 'p', sets: [], generation: 0, transcriptKeepsPreviousContext: false },
  view: { catalog: [], workflow: [], steps: [], warnings: [] },
};

describe('what this chat pinned', () => {
  it('refuses every answer that is not a conversation selection', async () => {
    const refused: unknown[] = [
      null,
      undefined,
      'Context choices are being read.',
      42,
      [],
      [GOOD],
      {},
      { view: GOOD.view },
      { pins: null, view: GOOD.view },
      { pins: 'none', view: GOOD.view },
      { pins: [], view: GOOD.view },
      { pins: GOOD.pins },
    ];
    for (const value of refused) {
      answer.value = value;
      await expect(whatThisChatPinned('terminal', 'folder')).rejects.toThrow(
        'could not read this conversation Context',
      );
    }
  });

  it('still returns a real selection, so the check is not a broken screen', async () => {
    answer.value = GOOD;
    await expect(whatThisChatPinned('terminal', 'folder')).resolves.toEqual(GOOD);
  });
});
