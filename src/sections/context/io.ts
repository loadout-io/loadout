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

import type {
  ContextDraft,
  ContextSet,
  ContextSetRead,
  DraftEdit,
  ImportItem,
  ImportReport,
  ImportResult,
  PreparedPage,
  PreviewImage,
  SourcePart,
} from '../../state/context';
import { emptyDraft } from '../../state/context';

/** Wszystkie gotowe zestawy tej biblioteki. */
export async function list(): Promise<ContextSet[]> {
  const answer = await invoke<unknown>('list_context_sets');
  if (!Array.isArray(answer)) {
    throw new Error('Loadout could not read the context sets you have saved.');
  }
  return answer.filter(isSet);
}

/** Jeden zestaw w całości: manifest, szkic i rewizja, którą okno odda przy zapisie. */
export async function read(id: string): Promise<ContextSetRead> {
  return asRead(await invoke<unknown>('read_context_set', { id }), 'open');
}

/** Nowy zestaw pod nazwą, którą wpisał człowiek. */
export async function create(title: string): Promise<ContextSetRead> {
  return asRead(await invoke<unknown>('create_context_set', { title }), 'make');
}

/**
 * Zapisuje tytuł, opis i szkic — z rewizją, którą to okno przeczytało.
 *
 * Bez `expectedRevision` zapis z okna otwartego pięć minut temu kasuje pracę zapisaną minutę
 * temu i wygląda przy tym na udany. Odmowa wraca gotowym zdaniem z Rusta.
 */
export async function saveDraft(edit: DraftEdit): Promise<ContextSetRead> {
  const answer = await invoke<unknown>('save_context_draft', {
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
export async function importSources(
  setId: string,
  operationId: string,
  items: ImportItem[],
  expectedRevision: string | null,
): Promise<ImportReport> {
  const answer = await invoke<unknown>('import_context_sources', {
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
export async function completePreparation(
  setId: string,
  sourceId: string,
  page: PreparedPage,
  expectedRevision: string | null,
): Promise<ContextSetRead> {
  const answer = await invoke<unknown>('complete_context_source_preparation', {
    setId,
    sourceId,
    page,
    expectedRevision,
  });
  return asRead(answer, 'save');
}

/** Kawałek zatwierdzonego źródła. `page === null` przy dokumencie znaczy „daj cały plik". */
export async function readSource(
  setId: string,
  sourceId: string,
  page: number | null,
): Promise<SourcePart> {
  const answer = await invoke<unknown>('read_context_source', { setId, sourceId, page });
  return asPart(answer);
}

/** Zdejmuje źródło z zestawu. */
export async function removeSource(
  setId: string,
  sourceId: string,
  expectedRevision: string | null,
): Promise<ContextSetRead> {
  const answer = await invoke<unknown>('remove_context_source', {
    setId,
    sourceId,
    expectedRevision,
  });
  return asRead(answer, 'save');
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
