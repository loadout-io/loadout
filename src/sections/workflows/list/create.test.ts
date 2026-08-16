/* Kryterium 1 dla T-14: dwie różne nazwy nigdy nie trafiają do jednego pliku.
 *
 * `Ship a feature` i `Ship a Feature` dają ten sam slug. Cicha porażka wygląda tak, że drugi
 * zapis KOŃCZY SIĘ SUKCESEM — bo zapis pliku po prostu nadpisuje — lista pokazuje dwie
 * pozycje, a na dysku jest jedna. Użytkownik traci workflow, którego nigdy nie usuwał,
 * i dowiaduje się o tym po restarcie aplikacji, nie po ekranie.
 *
 * Słaba wersja tego kryterium to `expect(io.write).toHaveBeenCalledTimes(2)`. Przechodzi
 * dokładnie dla tej implementacji: dwa wywołania, jedna ścieżka, jeden workflow mniej.
 * Rozróżniają to dwie asercje: ZBIÓR ścieżek przekazanych do zapisu (dwa różne wpisy) oraz
 * odczyt pierwszego pliku PO drugim zapisie, z asercją, że dalej ma swoją nazwę. Druga jest
 * ważniejsza, bo liczbę wywołań da się mieć poprawną przy jednym nadpisanym pliku.
 *
 * Atrapa dysku jest tu przepisana w całości, a nie wyjęta do wspólnego pliku pomocniczego,
 * i to jest decyzja: pięć kryteriów dzielących jedną atrapę to jedno miejsce, w którym da się
 * osłabić naraz wszystkie pięć, i wygląda to wtedy jak sprzątanie.
 */
import { describe, expect, it } from 'vitest';
import type { WorkflowEntry, WorkflowFile, WorkflowListIo } from './store';
import { createWorkflowListStore } from './store';

interface Disk extends WorkflowListIo {
  /** Zawartość katalogu: nazwa pliku → treść. Test zagląda tu wprost. */
  files: Map<string, WorkflowFile>;
  /** Nazwy plików przekazane do zapisu, w kolejności zapisywania. */
  writes: string[];
}

function disk(seed: readonly WorkflowEntry[]): Disk {
  const files = new Map<string, WorkflowFile>();
  for (const entry of seed) {
    files.set(entry.path, structuredClone(entry.workflow));
  }
  const writes: string[] = [];
  let minted = 0;

  return {
    files,
    writes,
    list: () =>
      Promise.resolve(
        [...files].map(([path, workflow]) => ({ path, workflow: structuredClone(workflow) })),
      ),
    newId: () => {
      minted += 1;
      return Promise.resolve('wf-' + String(minted));
    },
    write: (path, workflow) => {
      writes.push(path);
      /* Prawdziwy zapis nadpisuje po cichu i w tym jest cała pułapka tego kryterium.
       * Atrapa, która odmawia nadpisania, sprawdzałaby samą siebie. */
      files.set(path, structuredClone(workflow));
      return Promise.resolve();
    },
    remove: (path) => {
      files.delete(path);
      return Promise.resolve();
    },
  };
}

function entry(path: string, name: string): WorkflowEntry {
  return { path, workflow: { format: 1, id: 'seed-' + path, name, steps: [], links: [] } };
}

function fileAt(io: Disk, path: string): WorkflowFile {
  const hit = io.files.get(path);
  if (hit === undefined) {
    throw new Error('nothing sits at ' + path + ', so this test has nothing to read back');
  }
  return hit;
}

function names(io: Disk): string[] {
  return [...io.files.values()].map((workflow) => workflow.name).sort();
}

describe('two different names never land in one file', () => {
  it('writes one file, named after the workflow, holding the name the person typed', async () => {
    const io = disk([]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    await store.getState().create('Ship a feature');

    expect(io.writes, 'the file is named after the workflow, in lower case and hyphenated').toEqual(
      ['ship-a-feature.json'],
    );

    const saved = fileAt(io, 'ship-a-feature.json');
    expect(
      saved.name,
      'the name is what the human typed, letter for letter and case for case. The file name is ' +
        'derived from it once; the name itself is never squashed to match',
    ).toBe('Ship a feature');
    expect(saved.format, 'a new file carries the current format').toBe(1);
    expect(saved.steps, 'a new workflow starts empty').toEqual([]);
    expect(saved.links, 'and unwired').toEqual([]);
  });

  it('gives a second name that slugs the same a file of its own', async () => {
    const io = disk([]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    await store.getState().create('Ship a feature');
    await store.getState().create('Ship a Feature');

    expect(
      new Set(io.writes).size,
      'two creations have to reach two different files. Counting the writes instead of the ' +
        'paths passes for the version that wrote twice to one file — two calls, one file, ' +
        'one workflow gone',
    ).toBe(2);
    expect(
      io.writes.filter((path) => path !== 'ship-a-feature.json'),
      'the free name is found by suffixing, so the second one lands next to the first',
    ).toEqual(['ship-a-feature-2.json']);
    expect(io.files.size, 'and both are on disk afterwards').toBe(2);

    expect(
      fileAt(io, 'ship-a-feature.json').name,
      'the first file still holds the first workflow after the second create. This is the ' +
        'assertion the silent failure cannot survive: overwriting leaves a file that reads ' +
        'Ship a Feature, and the workflow the user had is gone with nobody having deleted it',
    ).toBe('Ship a feature');
    expect(names(io), 'both names survive, differing only in case').toEqual([
      'Ship a Feature',
      'Ship a feature',
    ]);

    const paths = store.getState().workflows.map((listed) => listed.path);
    expect(
      new Set(paths).size,
      'and the list agrees with the disk: two entries, two files. A list of two entries over ' +
        'one file is the state that looks right and is not',
    ).toBe(2);
  });

  it('never lets a name of punctuation alone produce an empty file name', async () => {
    const io = disk([]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    await store.getState().create('???');
    await store.getState().create('???');

    expect(io.writes, 'two creations, two writes').toHaveLength(2);
    expect(
      new Set(io.writes).size,
      'and two different files: a name that leaves nothing sluggable still has to be unique, ' +
        'or the second one silently eats the first',
    ).toBe(2);

    for (const path of io.writes) {
      expect(
        path,
        'the file name has to be a name a file system can carry: never a bare .json, never ' +
          'the punctuation itself. Got ' +
          path,
      ).toMatch(/^[a-z0-9][a-z0-9-]*\.json$/);
      expect(
        fileAt(io, path).name,
        'and the name the person typed is kept as it was typed, however unhelpful it is',
      ).toBe('???');
    }
  });

  it('sorts what it created the way a reader reads, not the way ASCII sorts', async () => {
    const io = disk([]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    await store.getState().create('Banana');
    await store.getState().create('apple');

    expect(
      store.getState().workflows.map((listed) => listed.workflow.name),
      'apple stands before Banana. A plain Array.sort() puts Banana first, because capitals ' +
        'have the lower code points — and a list sorted that way reads as unsorted',
    ).toEqual(['apple', 'Banana']);
  });

  it('sorts what it found on disk too, whatever order the folder handed it over in', async () => {
    const io = disk([entry('banana.json', 'Banana'), entry('apple.json', 'apple')]);
    const store = createWorkflowListStore(io);

    await store.getState().load();

    expect(
      store.getState().workflows.map((listed) => listed.workflow.name),
      'sorting belongs to the list, not to the create path. A folder hands its files over in ' +
        'whatever order it likes',
    ).toEqual(['apple', 'Banana']);
  });

  /* Dopisane w fazie implementacji (2026-08-16), nie osłabia niczego powyżej. Ta sama cicha
   * porażka, co w całej reszcie tego pliku — dwie nazwy, jeden plik — tylko wywołana szybkim
   * palcem zamiast wielkością liter. „Przeczytaj katalog, wybierz wolną nazwę, zapisz" ma
   * w środku dwa `await`, a przez to okno wchodzi drugie kliknięcie. */
  it('keeps two creations fired at once in two files, not in a race for one', async () => {
    const io = disk([]);
    const store = createWorkflowListStore(io);
    await store.getState().load();

    // Dwa kliknięcia `＋ Create` w tej samej sekundzie: drugie nie czeka na pierwsze.
    await Promise.all([store.getState().create('Ship it'), store.getState().create('Ship it')]);

    expect(
      new Set(io.writes).size,
      'both creations read the folder before either of them wrote to it, so both find the ' +
        'same free name and the second write lands on the first',
    ).toBe(2);
    expect(io.files.size, 'and both are on disk afterwards').toBe(2);
    expect(
      store
        .getState()
        .workflows.map((listed) => listed.path)
        .sort(),
      'and the list holds both files. A creation that rebuilds the list out of the folder it ' +
        'read BEFORE the other one wrote drops the other one off the screen, and the row comes ' +
        'back only after a reload — which is the same lie as the lost file, told by the list',
    ).toEqual([...io.files.keys()].sort());
  });
});
