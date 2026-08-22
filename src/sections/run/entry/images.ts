/* Obraz wklejony do rozmowy — lokalny szkic i jedyna konwersja na drut T-34.
 *
 * DLACZEGO MODUŁ ISTNIEJE PRZED IMPLEMENTACJĄ. Kryterium AC-6 importuje prawdziwy ekran Run.
 * NA DRUT NIE JEDZIE NAZWA PLIKU. `File.name` istnieje w oknie, bo daje je przeglądarka, ale
 * nie ma pola w żadnym typie niżej. To konstrukcyjnie pilnuje decyzji T-34: oryginalna nazwa
 * nie opuszcza webview nawet wtedy, kiedy plik nazywa się sekretem.
 */

/** Limity są lustrami twardej odmowy Rusta, potrzebnymi do szybkiej odpowiedzi przed IPC. */
export const MAX_IMAGES = 4;
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024;
export const MAX_TOTAL_IMAGE_BYTES = 12 * 1024 * 1024;

/** Stałe zdania z UI — surowa odmowa FileReadera ani vendora nigdy ich nie zastępuje. */
export const IMAGE_PASTE_FAILED =
  'Loadout could not add that image. Use PNG, JPEG, GIF or WebP within the size limits.';
export const IMAGE_SEND_FAILED = 'Loadout could not send that message.';
export const IMAGES_TO_LEAD_ONLY = 'Images can only be sent to the Lead right now.';
export const IMAGES_WITH_COMMANDS =
  'Images cannot be sent with commands. Remove them or send them to the Lead first.';

/** Cztery formaty, które oba adaptery vendora mają obowiązek przyjąć. */
export type ConversationImageMime = 'image/png' | 'image/jpeg' | 'image/gif' | 'image/webp';

/** Jedyny kształt obrazu, który wolno wysłać przez IPC. */
export interface ConversationImage {
  readonly mime: ConversationImageMime;
  readonly base64: string;
}

/** Obraz żyjący wyłącznie w szkicu okna. */
export interface PastedImage extends ConversationImage {
  readonly id: string;
  readonly previewUrl: string;
  /** Rozmiar do lokalnego limitu 12 MiB; nie wchodzi do [`ConversationImage`]. */
  readonly bytes: number;
}

const ACCEPTED: ReadonlySet<string> = new Set<ConversationImageMime>([
  'image/png',
  'image/jpeg',
  'image/gif',
  'image/webp',
]);

function acceptedMime(mime: string): mime is ConversationImageMime {
  return ACCEPTED.has(mime);
}

/** Base64 bez `data:` prefixu — Rust dostaje MIME osobno i nie musi rozbierać drugiego formatu. */
function base64Of(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  const chunks: string[] = [];
  const chunkSize = 0x8000;
  for (let start = 0; start < bytes.length; start += chunkSize) {
    chunks.push(String.fromCharCode(...bytes.subarray(start, start + chunkSize)));
  }
  return btoa(chunks.join(''));
}

/**
 * Zamienia nowe pliki z natywnego paste w lokalne podglądy i payloady.
 *
 * Walidacja całej paczki jest PRZED pierwszym `createObjectURL`: odrzucony piąty obraz nie
 * zostawia czterech blobów bez właściciela. Magic bytes i zgodność MIME są ponownie, twardo
 * sądzone po stronie Rusta przed procesem; okno daje tylko szybką, stałą odmowę człowiekowi.
 */
export async function readPastedImages(
  files: readonly File[],
  alreadyThere: readonly PastedImage[],
): Promise<readonly PastedImage[]> {
  if (alreadyThere.length + files.length > MAX_IMAGES) throw new Error(IMAGE_PASTE_FAILED);

  let total = alreadyThere.reduce((sum, image) => sum + image.bytes, 0);
  const safeFiles: { readonly file: File; readonly mime: ConversationImageMime }[] = [];
  for (const file of files) {
    if (!acceptedMime(file.type) || file.size > MAX_IMAGE_BYTES) {
      throw new Error(IMAGE_PASTE_FAILED);
    }
    total += file.size;
    safeFiles.push({ file, mime: file.type });
  }
  if (total > MAX_TOTAL_IMAGE_BYTES) throw new Error(IMAGE_PASTE_FAILED);

  const read = await Promise.all(
    safeFiles.map(async ({ file, mime }) => ({
      mime,
      bytes: file.size,
      buffer: await file.arrayBuffer(),
    })),
  );
  const made: PastedImage[] = [];
  try {
    for (const image of read) {
      /* Blob, nie File: nawet lokalny URL podglądu nie zachowuje oryginalnej nazwy. */
      const blob = new Blob([image.buffer], { type: image.mime });
      made.push({
        id: crypto.randomUUID(),
        mime: image.mime,
        base64: base64Of(image.buffer),
        previewUrl: URL.createObjectURL(blob),
        bytes: image.bytes,
      });
    }
    return made;
  } catch (error) {
    revokePastedImages(made);
    throw error;
  }
}

/** Jedyne miejsce, które odcina lokalny blob od przeglądarki. Wielokrotne revoke jest bezpieczne. */
export function revokePastedImages(images: readonly PastedImage[]): void {
  for (const image of images) URL.revokeObjectURL(image.previewUrl);
}

/** Zdejmuje pola szkicu; nazwę pliku konstrukcyjnie nie ma skąd wziąć. */
export function conversationImages(images: readonly PastedImage[]): readonly ConversationImage[] {
  return images.map(({ mime, base64 }) => ({ mime, base64 }));
}
