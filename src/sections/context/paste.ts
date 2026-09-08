/* Cmd+V w polu materiałów: jedno wklejenie, jedna pozycja importu.
 *
 * JEDNA POZYCJA, NIE DWIE. Część schowków niesie screenshot i podpis jednocześnie, a te dwie
 * rzeczy są jednym materiałem człowieka — rozdzielone na dwa żądania przestają wiedzieć o sobie
 * nawzajem. Rust rozkłada to na dwa źródła i zapisuje szew między nimi (`context::sources`),
 * bo tam mieszka reguła, a nie tutaj.
 *
 * NIE POŻYCZAMY `run/entry/images.ts`. Tamten moduł jest bramą ROZMOWY i nosi jej limity
 * (cztery obrazy, 5 MiB każdy, 12 MiB razem) — słuszne dla jednej wiadomości do vendora
 * i bez sensu dla biblioteki na dysku, której limity mieszkają w Ruście
 * (`context::limits`). Pożyczone jest stąd tylko czytanie bajtów, bo to jest wyłącznie
 * przepisanie `ArrayBuffer` na base64.
 *
 * PRZECHWYTUJEMY WYŁĄCZNIE WKLEJENIE Z PLIKIEM. Zwykły tekst zostaje zwykłym wklejeniem
 * przeglądarki: pole, w którym Cmd+V przestaje wstawiać zdanie, jest polem zepsutym.
 */
import type { ImportItem } from '../../state/context';

/** Trzy rodzaje obrazu, które biblioteka przyjmuje. Ta sama zamknięta lista, co po stronie Rusta. */
const ACCEPTED: ReadonlySet<string> = new Set(['image/png', 'image/jpeg', 'image/webp']);

/**
 * Czy to wklejenie w ogóle niesie obraz.
 *
 * Osobno od [`pastedIntoMaterial`], bo odpowiedź jest potrzebna **synchronicznie**: decyzja
 * o przejęciu wklejenia (`preventDefault`) zapada w ciele handlera, a czytanie bajtów jest
 * asynchroniczne. Przejęte za późno wklejenie zostawia przeglądarce zwykłą wstawkę tekstu
 * i dokłada do niej obraz drugą drogą.
 */
export function carriesAPicture(data: DataTransfer): boolean {
  return Array.from(data.files).some((file) => ACCEPTED.has(file.type));
}

/**
 * Co niesie to wklejenie — albo `null`, kiedy nie niesie pliku i ma zostać zwykłym wklejeniem.
 *
 * NAZWY PLIKU NIE WYSYŁAMY. Screenshot ze schowka nazywa się tak, jak nazwał go system, i nie
 * jest to nazwa, którą ktokolwiek wybrał; Rust nazywa taki wiersz sam. Ta sama zasada, co przy
 * obrazach rozmowy (T-34): oryginalna nazwa nie opuszcza okna, kiedy nie jest do niczego
 * potrzebna.
 */
export async function pastedIntoMaterial(data: DataTransfer): Promise<ImportItem | null> {
  const picture = Array.from(data.files).find((file) => ACCEPTED.has(file.type));
  if (picture === undefined) return null;
  const caption = data.getData('text/plain');
  return {
    name: '',
    path: null,
    text: caption === '' ? null : caption,
    image: { mime: picture.type, base64: base64Of(await picture.arrayBuffer()) },
  };
}

/**
 * Base64 bez przedrostka `data:` — Rust dostaje MIME osobno i nie musi rozbierać drugiego formatu.
 *
 * Po kawałku, bo `String.fromCharCode(...bytes)` na całym obrazie przekracza limit argumentów
 * wywołania i wywraca się na kilku megabajtach.
 */
function base64Of(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  const chunks: string[] = [];
  const chunkSize = 0x8000;
  for (let start = 0; start < bytes.length; start += chunkSize) {
    chunks.push(String.fromCharCode(...bytes.subarray(start, start + chunkSize)));
  }
  return btoa(chunks.join(''));
}
