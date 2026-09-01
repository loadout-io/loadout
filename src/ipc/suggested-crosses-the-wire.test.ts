/* Nowy rodzaj wiersza przechodzi drut i ma miejsce w rejestrze — drugie kryterium T-61.
 *
 * DWIE STRONY JEDNEGO PLIKU. `line-wire.golden.json` czytają oba brzegi granicy: strona rustowa
 * pilnuje, że pompa wysyła dokładnie to, co w nim stoi (`src-tauri/tests/it/
 * ipc_line_wire_golden.rs`), a lustro w `./types.ts` — że okno to przyjmuje. Ten plik pyta o to
 * samo dla JEDNEGO rodzaju: tego, który to zadanie dokłada.
 *
 * SŁABA WERSJA TEGO KRYTERIUM: sprawdzić sam `kinds()`. Przechodzi dla rodzaju dopisanego
 * WYŁĄCZNIE w oknie — takiego, który z drutu nie przyjdzie nigdy, więc wpis w rejestrze opisuje
 * wiersz-widmo. Rozróżnia to pierwszy przypadek: rodzaj musi stać w złotym pliku i przejść przez
 * `parseLine`, czyli przez tę samą funkcję, którą karmi kanał z Rusta.
 *
 * DRUGA SŁABA WERSJA, groźniejsza: przepisać oczekiwania z rejestru do testu. Wtedy zielone
 * znaczy „ktoś to wpisał dwa razy tak samo". Dlatego lista rodzajów SPRZED zmiany jest tu
 * wpisana jawnie, jako migawka historyczna, a wszystko inne pochodzi z plików: ze złotego
 * pliku, z lustra i z rejestru.
 */
import { describe, expect, it } from 'vitest';

import golden from './line-wire.golden.json';
import { parseLine, WIRE_KINDS } from './types';
import type { Kind } from '../sections/run/feed/kinds';
import { kinds } from '../sections/run/feed/kinds';
import { authorityOf } from '../sections/run/rail/say';

/** Złoty plik jako zwykłe obiekty: tu chodzi o KLUCZE, więc typ z wnioskowania przeszkadza. */
const entries = golden as unknown as Array<Record<string, unknown>>;

/** Nazwa nowego rodzaju na drucie. */
const SUGGESTED = 'suggested';

/**
 * Rodzaje, jakie drut umiał wyprodukować PRZED tą zmianą — migawka z 2026-08-20.
 *
 * WPISANE Z PALCA I TO JEST TU JEDYNA POPRAWNA FORMA, choć wszędzie indziej byłaby wadą: to
 * jest twierdzenie o PRZESZŁOŚCI, a przeszłości nie da się odczytać z pliku, który właśnie się
 * zmienia. Bez tej listy „rejestr zna nowy rodzaj" przechodzi dla rodzaju, który istniał od
 * dawna — wystarczy pomylić się w nazwie i sprawdzać `note` jeszcze raz. Lista pilnuje przy tym
 * drugiej rzeczy, o którą nikt nie pyta: że nic nie zniknęło ani nie zostało przemianowane po
 * drodze.
 */
const BEFORE: readonly string[] = [
  'run',
  'step',
  'agent',
  'thinking',
  'stepState',
  'read',
  'search',
  'edit',
  'ran',
  'note',
  'told',
  'asked',
  'handoff',
  'memory',
  'problem',
  'done',
];

/**
 * Nazwa rodzaju jako `Kind`.
 *
 * Rzutowanie jest tu TREŚCIĄ, nie obejściem: `Kind` pochodzi z lustra drutu, więc dopóki lustro
 * tego rodzaju nie zna, nazwa do niego nie należy i ten plik nie skompilowałby się wcale —
 * a kryterium, które się nie kompiluje, nie uruchomiło niczego i nie jest czerwone (AGENTS.md
 * §2a p. 5). Po zamknięciu zadania rzutowanie jest bezczynne; asercje niżej nie.
 */
function asKind(kind: string): Kind {
  return kind as unknown as Kind;
}

/** Wiersz złotego pliku tego rodzaju. */
function goldenRow(kind: string): Record<string, unknown> {
  const found = entries.find((entry) => entry['kind'] === kind);
  if (found === undefined) {
    throw new Error(
      'src/ipc/line-wire.golden.json has no line of kind ' +
        kind +
        ', so nothing describes what this row looks like on the wire. Both sides read that ' +
        'file: a kind with no line there is a kind nobody has ever looked at crossing the ' +
        'boundary, and the window would be free to expect whatever it liked.',
    );
  }
  return found;
}

describe('the run a lead suggested crosses the wire and has a place in the view', () => {
  it('is a kind the wire declares, and it is genuinely a new one', () => {
    expect(
      [...WIRE_KINDS].sort(),
      'the mirror in src/ipc/types.ts has to declare exactly the old kinds plus this one. ' +
        'Compared as a whole list, so the two failures nobody looks for both show up here: a ' +
        'kind added only in the view (which the wire will never send, leaving a row that cannot ' +
        'arrive) and a kind quietly dropped or renamed while the count still looked right.',
    ).toEqual([...BEFORE, SUGGESTED, 'stepCarriedOn'].sort());
    expect(
      BEFORE.includes(SUGGESTED),
      'and it was not there before, or this whole file is checking a row that already existed',
    ).toBe(false);
  });

  it('has a line in the golden file whose keys the mirror accepts one for one', () => {
    const row = goldenRow(SUGGESTED);

    expect(
      parseLine(row),
      'the mirror turned the golden line down. That is the shape a real line from Rust has, so ' +
        'a mirror that refuses it drops every one of these rows in silence — the view simply ' +
        'never shows what the lead suggested.',
    ).toEqual(row);
    expect(
      parseLine({ ...row, why: 'the cookie name is wrong' }),
      'a line carrying one key MORE than the kind declares has to be dropped. The set has to ' +
        'agree one for one, or a field added in Rust without camelCase renaming rides along ' +
        'under a name the window does not know, and the view falls over on undefined ' +
        '[FOUNDATIONS §3].',
    ).toBeNull();
    for (const key of Object.keys(row)) {
      const short: Record<string, unknown> = { ...row };
      delete short[key];
      expect(
        parseLine(short),
        'a line missing ' + key + ' has to be dropped as well — one for one goes both ways',
      ).toBeNull();
    }
    expect(
      typeof row['command'] === 'string' && (row['command'] as string).startsWith('/run'),
      'the line has to carry the command itself, in a field of its own. Without it the window ' +
        'has to cut the command back out of the prose to know what the button would run — and a ' +
        'window that reads `/run` out of an agent paragraph is the curation this design keeps in ' +
        'Rust (invariant 15). It carried ' +
        JSON.stringify(row),
    ).toBe(true);
  });

  it('goes to the history and stands open, because a proposal nobody sees is not one', () => {
    expect(
      kinds()[asKind(SUGGESTED)],
      'a kind with no entry in the registry is dropped by the model, so the row never reaches ' +
        'the screen at all. It belongs in the history (it happened, and it stays), and it is ' +
        'open: collapsed, the sentence and the button are both behind a click nobody knows to ' +
        'make.',
    ).toEqual({ route: 'history', expanded: true });
  });

  it('is signed as the words of the agent who said them', () => {
    expect(
      authorityOf(asKind(SUGGESTED)),
      'these are the lead own words, so the row is signed like prose. Signed as Loadout it ' +
        'would read as a message from the app — the same mistake as a tile quoting `3 of 40 ' +
        'checks failed` as if the agent had said it [FOUNDATIONS §2.2] — and signed as the ' +
        'person it would put the lead sentence in your mouth.',
    ).toBe('agent');
  });
});
