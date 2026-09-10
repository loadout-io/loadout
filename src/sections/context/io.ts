/* Jedyne miejsce w sekcji Context, które zna nazwy komend po stronie Rusta
 * (niezmiennik 23: polityka w jednym rdzeniu, adapter po pięć linii).
 *
 * KAŻDA ODPOWIEDŹ JEST SPRAWDZANA, a nie rzutowana. `invoke<T>()` jest rzutowaniem: mówi
 * kompilatorowi, czego się spodziewamy, i nie pyta granicy o nic. Zmierzone na tej aplikacji —
 * `null` oddane przez atrapę na komendę, która miała zwrócić obiekt, przewraca render, a wtedy
 * `ScreenBoundary` zdejmuje CAŁĄ sekcję, nie jeden wiersz. Odpowiedź o złym kształcie ma tu
 * dostać nazwane zdanie po angielsku i zostać odmową, którą magazyn umie pokazać.
 */
import { invoke } from '@tauri-apps/api/core';
import { activeWorkspace } from '../../state/workspaces';

import type {
  ContextApp,
  ContextBuild,
  ContextBuildRead,
  ContextDraft,
  DeleteContextSet,
  ContextFinding,
  ContextRevision,
  ContextSet,
  ContextSetRead,
  DraftEdit,
  ImportItem,
  ImportReport,
  ImportResult,
  PreparedPage,
  PreviewImage,
  RevisionEdit,
  SourcePart,
} from '../../state/context';
import { emptyDraft } from '../../state/context';

/** Operacja jest przypisana do projektu przez całe życie edytora/budowania. */
export function forProject(folder: string | null) {
  return {
    list,
    archive,
    deleteSet,
    read,
    create,
    saveDraft,
    importSources,
    completePreparation,
    readSource,
    removeSource,
    buildContext,
    readBuild,
    stopBuild,
    saveRevision,
  };

  /** Wszystkie gotowe zestawy tej biblioteki. */
  async function list(archived: boolean): Promise<ContextSet[]> {
    const answer = await invoke<unknown>('list_context_sets', { folder, archived });
    if (!Array.isArray(answer)) {
      throw new Error('Loadout could not read the context sets you have saved.');
    }
    return answer.filter(isSet);
  }

  /** Przenosi zestaw między dwiema półkami bez ruszania jego przypięć. */
  async function archive(setId: string, archived: boolean): Promise<ContextSet> {
    const answer = await invoke<unknown>('archive_context_set', { folder, id: setId, archived });
    if (!isSet(answer)) throw new Error('Loadout could not move that context set.');
    return answer;
  }

  /** Pierwszy obrót zwraca pytanie z użyciami, drugi wykonuje tę samą rozstrzygniętą czynność. */
  async function deleteSet(setId: string, confirmed: boolean): Promise<DeleteContextSet> {
    const answer = await invoke<unknown>('delete_context_set', { folder, id: setId, confirmed });
    if (typeof answer !== 'object' || answer === null) {
      throw new Error('Loadout could not check that context set before deleting it.');
    }
    const row = answer as Partial<DeleteContextSet>;
    if (typeof row.setId !== 'string' || typeof row.said !== 'string') {
      throw new Error('Loadout could not check that context set before deleting it.');
    }
    return {
      setId: row.setId,
      title: typeof row.title === 'string' ? row.title : '',
      uses: Array.isArray(row.uses)
        ? row.uses.filter((use): use is string => typeof use === 'string')
        : [],
      deleted: row.deleted === true,
      said: row.said,
    };
  }

  /** Jeden zestaw w całości: manifest, szkic i rewizja, którą okno odda przy zapisie. */
  async function read(id: string): Promise<ContextSetRead> {
    return asRead(await invoke<unknown>('read_context_set', { folder, id }), 'open');
  }

  /** Nowy zestaw pod nazwą, którą wpisał człowiek. */
  async function create(title: string): Promise<ContextSetRead> {
    return asRead(await invoke<unknown>('create_context_set', { folder, title }), 'make');
  }

  /**
   * Zapisuje tytuł, opis i szkic — z rewizją, którą to okno przeczytało.
   *
   * Bez `expectedRevision` zapis z okna otwartego pięć minut temu kasuje pracę zapisaną minutę
   * temu i wygląda przy tym na udany. Odmowa wraca gotowym zdaniem z Rusta.
   */
  async function saveDraft(edit: DraftEdit): Promise<ContextSetRead> {
    const answer = await invoke<unknown>('save_context_draft', {
      folder,
      id: edit.id,
      title: edit.title,
      description: edit.description,
      draft: edit.draft,
      expectedRevision: edit.expectedRevision,
    });
    return asRead(answer, 'save');
  }

  /**
   * Kładzie w zestawie wszystko, co człowiek wybrał albo wkleił.
   *
   * Plik jedzie ŚCIEŻKĄ, nie bajtami: okno nie ma po co czytać pliku, którego i tak nie pokaże,
   * a 50 MiB przepchnięte base64 przez granicę byłoby jego kopią w pamięci karty. Bajty jadą tylko
   * ze schowka, bo schowka nie da się otworzyć z Rusta.
   */
  async function importSources(
    setId: string,
    operationId: string,
    items: ImportItem[],
    expectedRevision: string | null,
  ): Promise<ImportReport> {
    const answer = await invoke<unknown>('import_context_sources', {
      folder,
      setId,
      operationId,
      items,
      expectedRevision,
    });
    if (typeof answer !== 'object' || answer === null) {
      throw new Error('Loadout could not add those files to this context set.');
    }
    const row = answer as Partial<ImportReport>;
    return {
      operationId: typeof row.operationId === 'string' ? row.operationId : operationId,
      results: Array.isArray(row.results) ? row.results.filter(isResult) : [],
      read: asRead(row.read, 'save'),
    };
  }

  /** Zatwierdza jedną przygotowaną stronę dokumentu. */
  async function completePreparation(
    setId: string,
    sourceId: string,
    page: PreparedPage,
    expectedRevision: string | null,
  ): Promise<ContextSetRead> {
    const answer = await invoke<unknown>('complete_context_source_preparation', {
      folder,
      setId,
      sourceId,
      page,
      expectedRevision,
    });
    return asRead(answer, 'save');
  }

  /** Kawałek zatwierdzonego źródła. `page === null` przy dokumencie znaczy „daj cały plik". */
  async function readSource(
    setId: string,
    sourceId: string,
    page: number | null,
  ): Promise<SourcePart> {
    const answer = await invoke<unknown>('read_context_source', { folder, setId, sourceId, page });
    return asPart(answer);
  }

  /** Zdejmuje źródło z zestawu. */
  async function removeSource(
    setId: string,
    sourceId: string,
    expectedRevision: string | null,
  ): Promise<ContextSetRead> {
    const answer = await invoke<unknown>('remove_context_source', {
      folder,
      setId,
      sourceId,
      expectedRevision,
    });
    return asRead(answer, 'save');
  }

  /** Uruchamia budowanie; postęp podczas pracy czyta osobna krawędź poniżej. */
  async function buildContext(
    setId: string,
    operationId: string,
    app: ContextApp,
    model: string | null,
  ): Promise<ContextBuildRead> {
    const answer = await invoke<unknown>('build_context', {
      folder,
      setId,
      operationId,
      app,
      model,
    });
    return asBuildRead(answer, 'build');
  }

  /** Odczyt trwałego postępu po wejściu albo powrocie do zestawu. */
  async function readBuild(setId: string): Promise<ContextBuildRead> {
    return asBuildRead(await invoke<unknown>('read_context_build', { folder, setId }), 'read');
  }

  /** Prosi o zatrzymanie i wraca dopiero z zapisanym końcem. */
  async function stopBuild(setId: string, operationId: string): Promise<ContextBuild> {
    return asBuild(
      await invoke<unknown>('stop_context_build', { folder, setId, operationId }),
      'Loadout could not stop this context build.',
    );
  }

  /** Publikuje zmianę człowieka jako następną, niezmienną wersję. */
  async function saveRevision(setId: string, edit: RevisionEdit): Promise<ContextBuildRead> {
    return asBuildRead(
      await invoke<unknown>('save_context_revision', { folder, setId, edit }),
      'save',
    );
  }
}

export function list(
  ...args: Parameters<ReturnType<typeof forProject>['list']>
): ReturnType<ReturnType<typeof forProject>['list']> {
  return forProject(activeWorkspace()?.folder ?? null).list(...args);
}
export function archive(
  ...args: Parameters<ReturnType<typeof forProject>['archive']>
): ReturnType<ReturnType<typeof forProject>['archive']> {
  return forProject(activeWorkspace()?.folder ?? null).archive(...args);
}
export function deleteSet(
  ...args: Parameters<ReturnType<typeof forProject>['deleteSet']>
): ReturnType<ReturnType<typeof forProject>['deleteSet']> {
  return forProject(activeWorkspace()?.folder ?? null).deleteSet(...args);
}
export function read(
  ...args: Parameters<ReturnType<typeof forProject>['read']>
): ReturnType<ReturnType<typeof forProject>['read']> {
  return forProject(activeWorkspace()?.folder ?? null).read(...args);
}
export function create(
  ...args: Parameters<ReturnType<typeof forProject>['create']>
): ReturnType<ReturnType<typeof forProject>['create']> {
  return forProject(activeWorkspace()?.folder ?? null).create(...args);
}
export function saveDraft(
  ...args: Parameters<ReturnType<typeof forProject>['saveDraft']>
): ReturnType<ReturnType<typeof forProject>['saveDraft']> {
  return forProject(activeWorkspace()?.folder ?? null).saveDraft(...args);
}
export function importSources(
  ...args: Parameters<ReturnType<typeof forProject>['importSources']>
): ReturnType<ReturnType<typeof forProject>['importSources']> {
  return forProject(activeWorkspace()?.folder ?? null).importSources(...args);
}
export function completePreparation(
  ...args: Parameters<ReturnType<typeof forProject>['completePreparation']>
): ReturnType<ReturnType<typeof forProject>['completePreparation']> {
  return forProject(activeWorkspace()?.folder ?? null).completePreparation(...args);
}
export function readSource(
  ...args: Parameters<ReturnType<typeof forProject>['readSource']>
): ReturnType<ReturnType<typeof forProject>['readSource']> {
  return forProject(activeWorkspace()?.folder ?? null).readSource(...args);
}
export function removeSource(
  ...args: Parameters<ReturnType<typeof forProject>['removeSource']>
): ReturnType<ReturnType<typeof forProject>['removeSource']> {
  return forProject(activeWorkspace()?.folder ?? null).removeSource(...args);
}
export function buildContext(
  ...args: Parameters<ReturnType<typeof forProject>['buildContext']>
): ReturnType<ReturnType<typeof forProject>['buildContext']> {
  return forProject(activeWorkspace()?.folder ?? null).buildContext(...args);
}
export function readBuild(
  ...args: Parameters<ReturnType<typeof forProject>['readBuild']>
): ReturnType<ReturnType<typeof forProject>['readBuild']> {
  return forProject(activeWorkspace()?.folder ?? null).readBuild(...args);
}
export function stopBuild(
  ...args: Parameters<ReturnType<typeof forProject>['stopBuild']>
): ReturnType<ReturnType<typeof forProject>['stopBuild']> {
  return forProject(activeWorkspace()?.folder ?? null).stopBuild(...args);
}
export function saveRevision(
  ...args: Parameters<ReturnType<typeof forProject>['saveRevision']>
): ReturnType<ReturnType<typeof forProject>['saveRevision']> {
  return forProject(activeWorkspace()?.folder ?? null).saveRevision(...args);
}

function asBuildRead(answer: unknown, doing: 'build' | 'read' | 'save'): ContextBuildRead {
  if (typeof answer !== 'object' || answer === null) {
    throw new Error('Loadout could not ' + doing + ' this context.');
  }
  const row = answer as Record<string, unknown>;
  const app = asApp(row['buildWith']);
  return {
    build:
      row['build'] === null || row['build'] === undefined
        ? null
        : asBuild(row['build'], 'Loadout could not read this context build.'),
    revision:
      row['revision'] === null || row['revision'] === undefined
        ? null
        : asRevision(row['revision']),
    buildWith: app,
  };
}

function asBuild(answer: unknown, refused: string): ContextBuild {
  if (typeof answer !== 'object' || answer === null) throw new Error(refused);
  const row = answer as Record<string, unknown>;
  if (typeof row['operationId'] !== 'string' || typeof row['said'] !== 'string') {
    throw new Error(refused);
  }
  return {
    operationId: row['operationId'],
    setId: text(row['setId']),
    generation: number(row['generation']),
    draftRevision: number(row['draftRevision']),
    stage: asStage(row['stage']),
    end: asEnd(row['end']),
    app: asApp(row['app']),
    requestedModel: optionalText(row['requestedModel']),
    model: optionalText(row['model']),
    batchesDone: number(row['batchesDone']),
    batchesTotal: number(row['batchesTotal']),
    sources: Array.isArray(row['sources']) ? row['sources'].map(asProgress) : [],
    said: row['said'],
    revisionId: optionalText(row['revisionId']),
    startedAt: text(row['startedAt']),
    changedAt: text(row['changedAt']),
  };
}

function asRevision(answer: unknown): ContextRevision {
  if (typeof answer !== 'object' || answer === null) {
    throw new Error('Loadout could not read the ready context version.');
  }
  const row = answer as Record<string, unknown>;
  if (typeof row['id'] !== 'string' || !Array.isArray(row['findings'])) {
    throw new Error('Loadout could not read the ready context version.');
  }
  return {
    id: row['id'],
    setId: text(row['setId']),
    draftRevision: number(row['draftRevision']),
    app: asApp(row['app']),
    requestedModel: optionalText(row['requestedModel']),
    model: optionalText(row['model']),
    createdAt: text(row['createdAt']),
    origin: row['origin'] === 'human' || row['origin'] === 'generated' ? row['origin'] : 'unknown',
    topics: Array.isArray(row['topics'])
      ? row['topics'].map((topic) => {
          const value = object(topic);
          return { id: text(value['id']), title: text(value['title']) };
        })
      : [],
    findings: row['findings'].map(asFinding),
    questions: strings(row['questions']),
    conflicts: strings(row['conflicts']),
    sources: Array.isArray(row['sources']) ? row['sources'].map(asProgress) : [],
  };
}

function asFinding(value: unknown): ContextFinding {
  const row = object(value);
  return {
    id: text(row['id']),
    kind:
      row['kind'] === 'requirement' ||
      row['kind'] === 'fact' ||
      row['kind'] === 'visual-reference' ||
      row['kind'] === 'assumption' ||
      row['kind'] === 'question' ||
      row['kind'] === 'conflict'
        ? row['kind']
        : 'unknown',
    text: text(row['text']),
    condition: text(row['condition']),
    sources: Array.isArray(row['sources'])
      ? row['sources'].map((source) => {
          const reference = object(source);
          return { sourceId: text(reference['sourceId']), part: text(reference['part']) };
        })
      : [],
    topic: text(row['topic']),
    conflictsWith: strings(row['conflictsWith']),
    origin: row['origin'] === 'human' || row['origin'] === 'generated' ? row['origin'] : 'unknown',
  };
}

function asProgress(value: unknown): ContextBuild['sources'][number] {
  const row = object(value);
  const outcome = row['outcome'];
  return {
    sourceId: text(row['sourceId']),
    part: text(row['part']),
    outcome:
      outcome === 'processed' || outcome === 'excluded' || outcome === 'failed'
        ? outcome
        : 'unknown',
    said: text(row['said']),
  };
}

function asApp(value: unknown): ContextApp {
  if (value === 'codex') return 'codex';
  if (value === 'claude-code') return 'claude-code';
  throw new Error('Loadout could not tell which app would read this material.');
}

function asStage(value: unknown): ContextBuild['stage'] {
  const known = [
    'freezing',
    'splitting',
    'extracting',
    'grouping',
    'publishing',
    'ready',
    'interrupted',
  ];
  return known.includes(String(value)) ? (value as ContextBuild['stage']) : 'unknown';
}

function asEnd(value: unknown): ContextBuild['end'] {
  const known = ['running', 'ready', 'cancelled', 'failed', 'stillRunning', 'interrupted'];
  return known.includes(String(value)) ? (value as ContextBuild['end']) : 'unknown';
}

function object(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
}

function text(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function optionalText(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function number(value: unknown): number {
  return typeof value === 'number' ? value : 0;
}

function strings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((one): one is string => typeof one === 'string') : [];
}

/** Czy to, co przyszło, jest wierszem wyniku importu. */
function isResult(value: unknown): value is ImportResult {
  if (typeof value !== 'object' || value === null) return false;
  const row = value as Partial<ImportResult>;
  return typeof row.name === 'string' && Array.isArray(row.added);
}

/**
 * Kawałek źródła, sprawdzony co do kształtu.
 *
 * Cztery rodzaje i nic poza nimi. Odpowiedź o rodzaju, którego ten build nie zna, wraca jako
 * pusty tekst — nowszy Loadout ma prawo dopisać piąty, a sekcja ma go MINĄĆ, nie przewrócić na
 * nim ekranu (niezmiennik 5).
 */
function asPart(answer: unknown): SourcePart {
  if (typeof answer !== 'object' || answer === null) {
    throw new Error('Loadout could not open that file.');
  }
  const row = answer as Record<string, unknown>;
  if (row['kind'] === 'image') {
    return { kind: 'image', image: asImage(row['image']) };
  }
  if (row['kind'] === 'page') {
    return {
      kind: 'page',
      number: typeof row['number'] === 'number' ? row['number'] : 1,
      pagesTotal: typeof row['pagesTotal'] === 'number' ? row['pagesTotal'] : 0,
      text: typeof row['text'] === 'string' ? row['text'] : '',
      image: row['image'] === null || row['image'] === undefined ? null : asImage(row['image']),
    };
  }
  if (row['kind'] === 'whole') {
    return {
      kind: 'whole',
      mime: typeof row['mime'] === 'string' ? row['mime'] : '',
      base64: typeof row['base64'] === 'string' ? row['base64'] : '',
      operationId: typeof row['operationId'] === 'string' ? row['operationId'] : '',
      fingerprint: typeof row['fingerprint'] === 'string' ? row['fingerprint'] : '',
    };
  }
  return {
    kind: 'text',
    text: typeof row['text'] === 'string' ? row['text'] : '',
    more: row['more'] === true,
  };
}

function asImage(value: unknown): PreviewImage {
  if (typeof value !== 'object' || value === null) {
    throw new Error('Loadout could not open that file.');
  }
  const row = value as Partial<PreviewImage>;
  return {
    mime: typeof row.mime === 'string' ? row.mime : 'image/png',
    base64: typeof row.base64 === 'string' ? row.base64 : '',
  };
}

/** Czy to, co przyszło, jest zestawem, czy tylko czymś o tym kształcie z nowszego Loadouta. */
function isSet(value: unknown): value is ContextSet {
  if (typeof value !== 'object' || value === null) return false;
  const row = value as Partial<ContextSet>;
  return typeof row.id === 'string' && typeof row.title === 'string';
}

/**
 * Odpowiedź o pełnym zestawie, sprawdzona co do kształtu.
 *
 * `doing` wchodzi w zdanie, bo trzy krawędzie mają trzy różne czynności, a „coś poszło nie tak"
 * w miejscu, w którym znamy czynność, jest gorsze niż brak zdania.
 */
function asRead(answer: unknown, doing: 'open' | 'make' | 'save'): ContextSetRead {
  if (typeof answer !== 'object' || answer === null) {
    throw new Error('Loadout could not ' + doing + ' that context set.');
  }
  const row = answer as Partial<ContextSetRead>;
  if (!isSet(row.set)) {
    throw new Error('Loadout could not ' + doing + ' that context set.');
  }
  return {
    set: row.set,
    /* Brakujący szkic znaczy pusty szkic, a nie przewrócony ekran: nowszy Loadout może dopisać
     * do tego pliku pola, których ten build nie zna, i sekcja ma je minąć (niezmiennik 5). */
    draft: asDraft(row.draft),
    revision: typeof row.revision === 'string' ? row.revision : '',
  };
}

function asDraft(value: unknown): ContextDraft {
  if (typeof value !== 'object' || value === null) return emptyDraft();
  const row = value as Partial<ContextDraft>;
  return {
    schema: typeof row.schema === 'number' ? row.schema : 1,
    sources: Array.isArray(row.sources) ? row.sources : [],
    excluded: Array.isArray(row.excluded) ? row.excluded : [],
    howToPrepare: typeof row.howToPrepare === 'string' ? row.howToPrepare : '',
    requirements: Array.isArray(row.requirements) ? row.requirements : [],
  };
}
