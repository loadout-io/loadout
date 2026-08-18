/* Workspace jako zakres: magazyn idzie DYSK-PIERWSZY, a przełącznik w bocznym menu ma skutek.
 *
 * SŁABA WERSJA TEGO PLIKU, i ona jest naprawdę łatwa do napisania:
 * `expect(markup).toContain('data-workspace-new')`. Przechodzi na przełączniku, który rysuje się
 * ładnie i nic nie robi — czyli dokładnie na defekcie, z którego wzięło się to zadanie
 * (niezmiennik 16: kontrolka bez skutku jest gorsza niż jej brak). Przechodzi też na magazynie,
 * który zmienia stan PRZED potwierdzeniem z dysku, a to jest defekt, który już raz w tym repo
 * wystąpił: agent zniknięty z listy przy nieudanym usunięciu wracał po restarcie.
 *
 * ODRÓŻNIAJĄ JE TRZY RZECZY, i każda ma tu swoją grupę:
 *   1. adapter jest PODSTAWIONY, więc widać, CO poszło na dysk i w jakiej kolejności — a przy
 *      odmowie widać, że stan się NIE ruszył;
 *   2. handlery przełącznika są wołane wprost, tak jak je woła przycisk, i pytamy magazyn, co się
 *      zmieniło. To repo nie ma jsdom, więc `renderToStaticMarkup` nigdy nie odpala `onClick`
 *      i „klikam i coś się dzieje" nie da się tu napisać inaczej;
 *   3. wartości oczekiwane są CZYTANE Z PLIKÓW w tym samym biegu: nazwy trzech komend
 *      z `src-tauri/commands.golden.txt`, a zdanie odmowy z `#[error(...)]`
 *      w `src-tauri/src/commands/workspaces.rs`. Zdanie przepisane z palca przechodzi także
 *      wtedy, gdy Rust mówi co innego — a wtedy człowiek czyta nasze zdanie zamiast jego.
 *
 * DLACZEGO PUNKT 3 O ODMOWIE JEST OSOBNY. Tauri odrzuca NAPISEM, nie `Error`
 * (`src-tauri/src/ipc.rs` robi `.map_err(|e| e.to_string())`), więc warunek `error instanceof
 * Error` jest zawsze fałszywy. Stał w siedmiu miejscach produkcyjnych i kasował każdą precyzyjną
 * odmowę: człowiek czytał zdanie zapasowe przy KAŻDEJ przyczynie. Test poniżej odrzuca właśnie
 * napisem i żąda, żeby to zdanie dotarło słowo w słowo.
 */
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Workspace } from '../../state/workspaces';

/* Zapis wywołań i nastawiona odpowiedź żyją w `vi.hoisted`, bo fabryka `vi.mock` jest podnoszona
 * nad importy i nie widzi zwykłej stałej modułu. */
const { disk } = vi.hoisted(() => ({
  disk: {
    /** Nazwy wywołanych funkcji adaptera, w kolejności. */
    calls: [] as string[],
    /** Argumenty tych wywołań, w tej samej kolejności. */
    args: [] as unknown[],
    /** Co adapter odpowie następnym razem: lista po zapisie albo odmowa. */
    answer: { kind: 'ok', all: [] } as
      { kind: 'ok'; all: readonly Workspace[] } | { kind: 'no'; said: unknown },
  },
}));

vi.mock('../../state/workspaces-io', () => {
  function reply(name: string, args: unknown): Promise<readonly Workspace[]> {
    disk.calls.push(name);
    disk.args.push(args);
    const answer = disk.answer;
    return answer.kind === 'ok' ? Promise.resolve(answer.all) : Promise.reject(answer.said);
  }
  return {
    listWorkspaces: () => reply('list', undefined),
    saveWorkspace: (args: { name: string; folder: string }) => reply('save', args),
    deleteWorkspace: (args: { id: string }) => reply('delete', args),
  };
});

/* Okno wyboru folderu podstawiamy CZĘŚCIOWO: `folderName` zostaje prawdziwe, bo asercja o nazwie
 * podpowiedzianej z folderu ma mówić o funkcji, która stoi w repo, a nie o kopii z tego pliku. */
const { dialog } = vi.hoisted(() => ({
  dialog: {
    answer: { kind: 'ok', path: null } as
      { kind: 'ok'; path: string | null } | { kind: 'no'; said: unknown },
  },
}));

vi.mock('../../sections/run/folders', async (importOriginal) => {
  const real = await importOriginal<typeof import('../../sections/run/folders')>();
  return {
    ...real,
    chooseWorkingFolder: (): Promise<string | null> => {
      const answer = dialog.answer;
      return answer.kind === 'ok' ? Promise.resolve(answer.path) : Promise.reject(answer.said);
    },
  };
});

const { activeWorkspace, useWorkspaces } = await import('../../state/workspaces');
const { NO_FOLDER_YET, NOTHING_PICKED, WorkspaceSwitcher, refusal, useSwitcher } =
  await import('./workspace-switcher');
const { SideNav } = await import('./titlebar');

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const GOLDEN = resolve(ROOT, 'src-tauri/commands.golden.txt');
const RUST = resolve(ROOT, 'src-tauri/src/commands/workspaces.rs');
const IO = resolve(ROOT, 'src/state/workspaces-io.ts');

function fileText(path: string): string {
  return existsSync(path) ? readFileSync(path, 'utf8') : '';
}

/** Nazwy komend o workspace'ach, przeczytane z rejestru granicy. */
function workspaceCommands(): readonly string[] {
  return fileText(GOLDEN)
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '' && !line.startsWith('#') && line.includes('workspace'));
}

/**
 * Jedno zdanie odmowy Rusta, przeczytane z `#[error("…")]`.
 *
 * Bierzemy wariant BEZ pola do podstawienia (`{folder}`), bo tylko takie zdanie jedzie na front
 * słowo w słowo i tylko takie da się porównać znak w znak.
 */
function rustRefusal(): string {
  return /#\[error\("([^"\\{}]+)"\)\]/.exec(fileText(RUST))?.[1] ?? '';
}

function occurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

const FIRST: Workspace = {
  id: '/Users/x/meetnotes',
  name: 'Meet notes',
  folder: '/Users/x/meetnotes',
};
const SECOND: Workspace = { id: '/Users/x/roster', name: 'Roster', folder: '/Users/x/roster' };

/** Stan okienka w spoczynku — to, co przełącznik dostaje propsem, kiedy nic nie jest rozwinięte. */
const SHUT = { open: false, adding: false, name: '', folder: null, troubled: null } as const;

beforeEach(() => {
  disk.calls = [];
  disk.args = [];
  disk.answer = { kind: 'ok', all: [] };
  dialog.answer = { kind: 'ok', path: null };
  useWorkspaces.setState({ all: [], activeId: null, said: null });
  useSwitcher.setState({ open: false, adding: false, name: '', folder: null, troubled: null });
});

describe('the three command names live in exactly one file', () => {
  it('reads three of them out of the boundary register', () => {
    expect(
      workspaceCommands(),
      'src-tauri/commands.golden.txt has to register three workspace commands. Without them the ' +
        'assertions below would run on an empty list and pass on nothing.',
    ).toHaveLength(3);
  });

  it('names every one of them in workspaces-io.ts and nowhere else under src/', () => {
    const commands = workspaceCommands();
    const io = fileText(IO);
    expect(
      commands.filter((name) => !io.includes(name)),
      'src/state/workspaces-io.ts has to call every workspace command the ' +
        'boundary registers. A command nobody calls is a boundary that exists and does nothing.',
    ).toEqual([]);

    /* Niezmiennik 23: nazwy komend Rusta mieszkają WYŁĄCZNIE w `io.ts` swojej sekcji. Druga
     * droga do Rusta znaczy, że kolejność „dysk pierwszy" pilnuje jednej z nich, a stan zmienia
     * się drugą — i nikt tego nie zauważy, bo licznik dalej pokazuje swoje. */
    const elsewhere: string[] = [];
    for (const path of ['src/state/workspaces.ts', 'src/ui/shell/workspace-switcher.tsx']) {
      const text = fileText(resolve(ROOT, path));
      for (const name of commands) {
        if (text.includes(name)) elsewhere.push(path + ' names ' + name);
      }
    }
    expect(
      elsewhere,
      'a file other than src/state/workspaces-io.ts spells a Rust command name. Invariant 23 keeps ' +
        'those names in one file so that there is exactly one edge through which a write can go.',
    ).toEqual([]);
  });
});

describe('the store changes only after the disk confirms', () => {
  it('treats an empty list as a normal state, not as a failure', async () => {
    disk.answer = { kind: 'ok', all: [] };
    await useWorkspaces.getState().load();
    const state = useWorkspaces.getState();
    expect(state.all, 'a fresh machine has no workspaces file, and Rust answers []').toEqual([]);
    expect(state.activeId, 'nothing can be active when there is nothing').toBeNull();
    expect(
      state.said,
      'an empty list is not a refusal. Saying something here is how "No such file or directory ' +
        '(os error 2)" ended up on the screen of every fresh install.',
    ).toBeNull();
  });

  it('picks the first workspace when the list arrives and nothing is active', async () => {
    disk.answer = { kind: 'ok', all: [FIRST, SECOND] };
    await useWorkspaces.getState().load();
    expect(
      useWorkspaces.getState().activeId,
      'activeId is not written to disk (picking a view is not durable state), so after a restart ' +
        'nothing would be active and the Run screen would have no folder while three workspaces ' +
        'sat in the switcher.',
    ).toBe(FIRST.id);
    expect(
      activeWorkspace()?.folder,
      'activeWorkspace() is the one answer to "where do we work"',
    ).toBe(FIRST.folder);
  });

  it('carries the sentence Rust wrote, word for word, when the disk refuses', async () => {
    const said = rustRefusal();
    expect(
      said,
      'no placeholder-free #[error("…")] sentence could be read out of ' +
        'src-tauri/src/commands/workspaces.rs, so the comparison below would run against an ' +
        'empty string and pass on nothing.',
    ).not.toBe('');

    /* ODRZUCENIE NAPISEM, nie `Error` — tak odrzuca Tauri. Wersja z `instanceof Error` stała
     * w siedmiu miejscach i była zawsze fałszywa, więc każda precyzyjna odmowa ginęła. */
    disk.answer = { kind: 'no', said };
    const done = await useWorkspaces.getState().add('', '/Users/x/nope');
    expect(done, 'add returns false when the disk refused, so the form stays open').toBe(false);
    expect(
      useWorkspaces.getState().said,
      'the sentence Rust wrote did not reach the store. A fallback sentence here means the human ' +
        'reads "Loadout could not add that workspace." for every possible cause.',
    ).toBe(said);
  });

  it('leaves the list untouched when adding is refused', async () => {
    useWorkspaces.setState({ all: [FIRST], activeId: FIRST.id });
    disk.answer = { kind: 'no', said: 'no' };
    await useWorkspaces.getState().add('Roster', SECOND.folder);
    expect(
      useWorkspaces.getState().all,
      'the window put a row in the list that is not in the file. Disk first is the whole rule: ' +
        'the reverse order is how an agent removed from the list came back after a restart.',
    ).toEqual([FIRST]);
  });

  it('takes the new list from the answer, never from its own arguments', async () => {
    /* Rust przycina nazwę i przy DRUGIM zapisie tego samego folderu ZMIENIA NAZWĘ zamiast
     * dokładać wiersz. Lista złożona w oknie z `[...all, { name, folder }]` pokazywałaby wtedy
     * duplikat, którego w pliku nie ma — dlatego odpowiedź jest inna niż argumenty. */
    const trimmed: Workspace = { ...SECOND, name: 'Roster' };
    disk.answer = { kind: 'ok', all: [FIRST, trimmed] };
    const done = await useWorkspaces.getState().add('  Roster  ', SECOND.folder);
    expect(done).toBe(true);
    expect(
      useWorkspaces.getState().all,
      'the store believed its own arguments instead of the list the disk answered with',
    ).toEqual([FIRST, trimmed]);
    expect(
      useWorkspaces.getState().activeId,
      'a workspace just added is where the human means to work — adding it and not switching to ' +
        'it is a choice with no effect',
    ).toBe(SECOND.id);
  });

  it('renames through the folder of the entry it was given', async () => {
    useWorkspaces.setState({ all: [FIRST], activeId: FIRST.id });
    disk.answer = { kind: 'ok', all: [{ ...FIRST, name: 'Notes' }] };
    const done = await useWorkspaces.getState().rename(FIRST.id, 'Notes');
    expect(done).toBe(true);
    expect(
      disk.args[0],
      'renaming rides the same command as adding, and the key is the folder — so the folder has ' +
        'to travel with the new name or the write lands on nothing',
    ).toEqual({ name: 'Notes', folder: FIRST.folder });
    expect(useWorkspaces.getState().all[0]?.name).toBe('Notes');
  });

  it('refuses to rename an entry that is no longer on the list, without touching the disk', async () => {
    const done = await useWorkspaces.getState().rename(FIRST.id, 'Notes');
    expect(done).toBe(false);
    expect(
      disk.calls,
      'there is no folder to write to, so there is nothing to ask the disk about',
    ).toEqual([]);
    expect(
      useWorkspaces.getState().said,
      'the human has to be told the list moved under their hand, and told it in a sentence that ' +
        'is not about the disk',
    ).not.toBeNull();
  });

  it('keeps the removed workspace when removing is refused', async () => {
    useWorkspaces.setState({ all: [FIRST, SECOND], activeId: FIRST.id });
    disk.answer = { kind: 'no', said: 'no' };
    const done = await useWorkspaces.getState().remove(FIRST.id);
    expect(done).toBe(false);
    expect(
      useWorkspaces.getState().all,
      'a workspace vanished from the switcher while it is still in the file — it would come back ' +
        'at the next restart, which is exactly the defect this order exists to stop',
    ).toEqual([FIRST, SECOND]);
    expect(useWorkspaces.getState().activeId).toBe(FIRST.id);
  });

  it('falls back to the first workspace after removing the active one', async () => {
    useWorkspaces.setState({ all: [FIRST, SECOND], activeId: FIRST.id });
    disk.answer = { kind: 'ok', all: [SECOND] };
    const done = await useWorkspaces.getState().remove(FIRST.id);
    expect(done).toBe(true);
    expect(disk.args[0], 'remove is keyed by id and touches nothing else').toEqual({
      id: FIRST.id,
    });
    expect(
      useWorkspaces.getState().activeId,
      'activeId has to point at a workspace that exists, or be null. Left pointing at the ' +
        'removed one, activeWorkspace() answers null while the switcher shows two entries.',
    ).toBe(SECOND.id);
  });

  it('is left with nothing active after the last workspace goes', async () => {
    useWorkspaces.setState({ all: [FIRST], activeId: FIRST.id });
    disk.answer = { kind: 'ok', all: [] };
    await useWorkspaces.getState().remove(FIRST.id);
    expect(useWorkspaces.getState().activeId).toBeNull();
    expect(activeWorkspace()).toBeNull();
  });

  it('does not move the view when a workspace in the background is removed', async () => {
    useWorkspaces.setState({ all: [FIRST, SECOND], activeId: SECOND.id });
    disk.answer = { kind: 'ok', all: [SECOND] };
    await useWorkspaces.getState().remove(FIRST.id);
    expect(
      useWorkspaces.getState().activeId,
      'removing something the human was not looking at has no business moving the view — that is ' +
        'the kind of self-will that makes Remove a control you press with your heart in your mouth',
    ).toBe(SECOND.id);
  });

  it('switches the view without touching the disk at all', () => {
    useWorkspaces.setState({ all: [FIRST, SECOND], activeId: FIRST.id });
    useWorkspaces.getState().activate(SECOND.id);
    expect(useWorkspaces.getState().activeId).toBe(SECOND.id);
    expect(
      disk.calls,
      'activate is a change of view and nothing else. A write here would mean switching a ' +
        'workspace can fail, and the owner asked for switching that never loses the live work.',
    ).toEqual([]);
  });

  it('ignores a switch to a workspace nobody saved', () => {
    useWorkspaces.setState({ all: [FIRST], activeId: FIRST.id });
    useWorkspaces.getState().activate(SECOND.id);
    expect(
      useWorkspaces.getState().activeId,
      'activeWorkspace() would otherwise answer with an entry that is not in the list at all',
    ).toBe(FIRST.id);
  });

  it('clears the sentence when the human has read it', async () => {
    disk.answer = { kind: 'no', said: 'no' };
    await useWorkspaces.getState().load();
    expect(useWorkspaces.getState().said).not.toBeNull();
    useWorkspaces.getState().dismiss();
    expect(useWorkspaces.getState().said).toBeNull();
  });
});

describe('every control in the switcher has a handler with an effect', () => {
  it('opens the add form', () => {
    useSwitcher.getState().startAdd();
    expect(useSwitcher.getState().adding).toBe(true);
  });

  it('keeps what was typed', () => {
    useSwitcher.getState().typeName('Roster');
    expect(useSwitcher.getState().name).toBe('Roster');
  });

  it('drops the draft when the add form is cancelled', () => {
    useSwitcher.setState({ adding: true, name: 'Roster', folder: SECOND.folder });
    useSwitcher.getState().cancelAdd();
    expect(useSwitcher.getState()).toMatchObject({ adding: false, name: '', folder: null });
  });

  it('rolls the list open and shut', () => {
    useSwitcher.getState().toggle();
    expect(useSwitcher.getState().open).toBe(true);
    useSwitcher.getState().toggle();
    expect(useSwitcher.getState().open).toBe(false);
  });

  it('picks the folder through the one door to the system chooser, and suggests a name', async () => {
    dialog.answer = { kind: 'ok', path: SECOND.folder };
    await useSwitcher.getState().pickFolder();
    expect(useSwitcher.getState().folder).toBe(SECOND.folder);
    expect(
      useSwitcher.getState().name,
      'the name is suggested from the folder, so adding a workspace is one pick and one Save. ' +
        'Read through the real folderName() from src/sections/run/folders.ts, never a copy.',
    ).toBe('roster');
    expect(
      disk.calls,
      'choosing a folder writes nothing. Nothing is saved until the human presses Save.',
    ).toEqual([]);
  });

  it('keeps a name the human typed instead of overwriting it with the folder', async () => {
    useSwitcher.setState({ name: 'My roster' });
    dialog.answer = { kind: 'ok', path: SECOND.folder };
    await useSwitcher.getState().pickFolder();
    expect(useSwitcher.getState().name).toBe('My roster');
  });

  it('says nothing when the human closes the folder chooser', async () => {
    dialog.answer = { kind: 'ok', path: null };
    await useSwitcher.getState().pickFolder();
    expect(useSwitcher.getState().folder).toBeNull();
    expect(
      useSwitcher.getState().troubled,
      'cancelling is a value, not an error (invariant 7). Silence after closing your own dialog ' +
        'is exactly what the human expects.',
    ).toBeNull();
  });

  it('speaks when the folder chooser itself fails', async () => {
    dialog.answer = { kind: 'no', said: 'the chooser is not available' };
    await useSwitcher.getState().pickFolder();
    expect(useSwitcher.getState().troubled).toBe('the chooser is not available');
  });

  it('will not save before a folder is chosen, and asks for one', async () => {
    useSwitcher.setState({ adding: true, name: 'Roster', folder: null });
    await useSwitcher.getState().save();
    expect(useSwitcher.getState().troubled).toBe(NO_FOLDER_YET);
    expect(disk.calls, 'there is nothing to save yet, so nothing is asked of the disk').toEqual([]);
    expect(
      useSwitcher.getState().adding,
      'the form stays open, because it is what needs fixing',
    ).toBe(true);
  });

  it('keeps the form open when the disk refuses the save', async () => {
    useSwitcher.setState({ adding: true, name: 'Roster', folder: SECOND.folder });
    disk.answer = { kind: 'no', said: 'nope' };
    await useSwitcher.getState().save();
    expect(
      useSwitcher.getState().adding,
      'closing the form on a refusal throws away what the human typed and leaves them with a ' +
        'sentence about work they can no longer see',
    ).toBe(true);
    expect(useSwitcher.getState().name).toBe('Roster');
    expect(useWorkspaces.getState().said).toBe('nope');
  });

  it('closes the form only once the disk has confirmed', async () => {
    useSwitcher.setState({ adding: true, open: true, name: 'Roster', folder: SECOND.folder });
    disk.answer = { kind: 'ok', all: [SECOND] };
    await useSwitcher.getState().save();
    expect(disk.args[0]).toEqual({ name: 'Roster', folder: SECOND.folder });
    expect(useSwitcher.getState()).toMatchObject({
      adding: false,
      open: false,
      name: '',
      folder: null,
    });
    expect(useWorkspaces.getState().activeId).toBe(SECOND.id);
  });

  it('switches and shuts the list when a workspace is picked', () => {
    useWorkspaces.setState({ all: [FIRST, SECOND], activeId: FIRST.id });
    useSwitcher.setState({ open: true });
    useSwitcher.getState().choose(SECOND.id);
    expect(useWorkspaces.getState().activeId).toBe(SECOND.id);
    expect(useSwitcher.getState().open).toBe(false);
    expect(disk.calls, 'switching workspaces writes nothing and can never fail').toEqual([]);
  });

  it('clears both refusals with the one Dismiss the human sees', () => {
    useWorkspaces.setState({ said: 'from the disk' });
    useSwitcher.setState({ troubled: 'from the window' });
    useSwitcher.getState().dismiss();
    expect(useWorkspaces.getState().said).toBeNull();
    expect(useSwitcher.getState().troubled).toBeNull();
  });

  it('shows the disk sentence first when both have something to say', () => {
    expect(
      refusal('from the disk', 'from the window'),
      'the disk refusal is about the save that just finished; the window one is about a step ' +
        'before it and is stale by then',
    ).toBe('from the disk');
    expect(refusal(null, 'from the window')).toBe('from the window');
    expect(refusal(null, null)).toBeNull();
  });
});

describe('the switcher shows the scope, and an empty one invites', () => {
  it('greets an empty list with an invitation, not with an empty menu', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceSwitcher all={[]} activeId={null} said={null} ui={SHUT} />,
    );
    expect(
      occurrences(markup, 'data-workspace-new'),
      'with nothing saved, the switcher has to be an invitation to add the first workspace ' +
        '(DESIGN §6: an empty screen is an invitation to act, not a notice that data is missing). ' +
        'A menu with zero entries says "nothing here" and does not say what to do about it.',
    ).toBe(1);
    expect(
      occurrences(markup, 'data-workspace-open'),
      'there is nothing to roll open, so the disclosure control has no business being rendered',
    ).toBe(0);
    expect(occurrences(markup, 'data-workspace-pick')).toBe(0);
  });

  it('names the active workspace on the control that rolls the list open', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceSwitcher all={[FIRST, SECOND]} activeId={SECOND.id} said={null} ui={SHUT} />,
    );
    expect(markup).toContain(SECOND.name);
    expect(
      occurrences(markup, 'data-workspace-pick'),
      'the list is shut, so the entries are not in the tree at all — not merely invisible',
    ).toBe(0);
  });

  it('says "pick one" when the list is not empty and nothing is active', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceSwitcher all={[FIRST]} activeId={null} said={null} ui={SHUT} />,
    );
    expect(markup).toContain(NOTHING_PICKED);
  });

  it('offers one entry per workspace plus the way to add another, when rolled open', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceSwitcher
        all={[FIRST, SECOND]}
        activeId={SECOND.id}
        said={null}
        ui={{ ...SHUT, open: true }}
      />,
    );
    expect(occurrences(markup, 'data-workspace-pick="' + FIRST.id + '"')).toBe(1);
    expect(occurrences(markup, 'data-workspace-pick="' + SECOND.id + '"')).toBe(1);
    expect(
      occurrences(markup, 'data-workspace-new'),
      'the way to add another scope belongs in the open list, next to the scopes — that is the ' +
        'whole shape the owner asked for',
    ).toBe(1);
    expect(
      occurrences(markup, 'aria-checked="true"'),
      'exactly one entry says it is the active one (invariant 13). poprzedni prototyp showed the ' +
        'connection state in six places at once.',
    ).toBe(1);
    expect(
      occurrences(markup, 'aria-current'),
      'aria-current in the nav answers "which section is open" and is asserted to appear exactly ' +
        'once in the whole shell (sections.test.tsx). The switcher must not spend it on a second ' +
        'question.',
    ).toBe(0);
  });

  it('shows the refusal with a way to put it away', () => {
    const said = rustRefusal();
    const markup = renderToStaticMarkup(
      <WorkspaceSwitcher all={[]} activeId={null} said={said} ui={SHUT} />,
    );
    expect(markup, 'the sentence the disk wrote has to reach the screen unchanged').toContain(said);
    expect(occurrences(markup, 'data-workspace-dismiss')).toBe(1);
  });

  it('asks for a name and a folder, with a way out, while adding', () => {
    const markup = renderToStaticMarkup(
      <WorkspaceSwitcher
        all={[]}
        activeId={null}
        said={null}
        ui={{ ...SHUT, adding: true, name: 'Roster', folder: SECOND.folder }}
      />,
    );
    for (const marker of [
      'data-workspace-name',
      'data-workspace-folder',
      'data-workspace-save',
      'data-workspace-cancel',
    ]) {
      expect(occurrences(markup, marker), 'the add form has to carry ' + marker + ' once').toBe(1);
    }
    expect(markup, 'the chosen folder has to be shown, not merely remembered').toContain('roster');
  });
});

describe('the switcher stands at the top of the side nav', () => {
  const nav = renderToStaticMarkup(<SideNav section="run" />);

  it('comes after the brand and before the first section switch', () => {
    const brandAt = nav.indexOf('LOADOUT');
    const switcherAt = nav.indexOf('data-workspace-switcher');
    const firstSection = nav.indexOf('data-section-switch');

    expect(switcherAt, 'the side nav does not mount the workspace switcher at all').toBeGreaterThan(
      -1,
    );
    expect(brandAt, 'the brand comes first, as in the mockup').toBeLessThan(switcherAt);
    expect(
      switcherAt,
      'the scope frames all five sections, so it stands above them. Put below, it reads as a ' +
        'sixth place you go into rather than as the answer to "where am I working".',
    ).toBeLessThan(firstSection);
  });

  it('adds no second drag region and no second aria-current', () => {
    expect(
      occurrences(nav, 'data-tauri-drag-region'),
      'a window with more than one drag region drags from places the human reads as content',
    ).toBe(1);
    expect(occurrences(nav, 'aria-current="true"')).toBe(1);
  });

  it('carries no section switch of its own', () => {
    const switcher = nav.slice(
      nav.indexOf('data-workspace-switcher'),
      nav.indexOf('data-section-switch'),
    );
    expect(
      occurrences(switcher, 'data-section-switch'),
      'the switcher answers "where", the section list answers "what am I doing". Two axes, and ' +
        'the switcher has no business carrying the other one (ARCHITECTURE §7).',
    ).toBe(0);
  });
});
