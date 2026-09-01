/* Zgoda na wniesienie CUDZEJ umiejętności — jedyna rzecz, którą to okno może o niej powiedzieć
 * uczciwie, i warunek, bez którego jej nie wnosi.
 *
 * # Co tu jest naprawiane (2026-08-31)
 *
 * `SKILL.md` wchodzi do produktu dwiema drogami. Wklejony linkiem staje przed kartą przeglądu
 * (`src/sections/skills/review-card.tsx`): ukryty tekst, próba nadpisania instrukcji i linia
 * wysyłająca dane stoją na ekranie, cytowane dosłownie, a blokujące znaleziska trzeba odklikać
 * po jednym („I have read this"). Ten sam plik znaleziony w cudzym projekcie wchodził przez
 * „Import setup" jednym kliknięciem, bez ani jednego zdania o tym, co w nim jest.
 *
 * # Czego tu NIE MA i dlaczego
 *
 * Skanu. Skan mieszka po stronie Rusta i **biegnie także dla importu**:
 * `import::adapters::skill` woła `skills::ingest::from_folder`, a werdykt wchodzi w zgodność
 * pozycji — `Blocked` daje `Unsupported`, czyli pozycję, której to okno nie da się wnieść.
 * Przepisanie tej polityki tutaj byłoby drugim rdzeniem (niezmiennik 23).
 *
 * Listy znalezisk. Ona też ma jeden dom i jest nim `src/sections/skills/findings.tsx` — ten
 * sam, z którego czyta karta przeglądu przy wklejonym linku. Tutaj stoją wyłącznie zdania
 * o TEJ drodze: co ekran importu robi z pozycją, której przegląd coś znalazł.
 *
 * # Co się tu zmieniło 2026-08-31, po południu
 *
 * Do rana stała tu stała `NOT_SHOWN_HERE`: „Loadout read this one before import, and this
 * screen does not show you what it found." Zdanie było PRAWDZIWE, dopóki `ImportItem` niósł
 * status i jedno zdanie zamiast znalezisk — milczenie w tym miejscu czyta się jak „nic nie
 * znaleziono", więc lepiej było powiedzieć wprost, czego ten ekran nie umie.
 *
 * Drut powstał (`ImportItem::reviewed`, `src-tauri/src/import/mod.rs`) i zdanie przestało być
 * prawdziwe tego samego dnia. Zdanie, które było prawdziwe wczoraj, jest najgorszym rodzajem
 * copy: czyta się jak aktualne, a wysyła człowieka szukać znalezisk gdzie indziej — albo,
 * gorzej, uczy go, że ten ekran nigdy nic nie pokaże, więc przestaje na niego patrzeć.
 * Dlatego stała znikła RAZEM z powodem, a nie została „na wszelki wypadek".
 *
 * # Dlaczego zgoda dotyczy `adjusted`, a nie każdej umiejętności
 *
 * Bo `exact` znaczy dokładnie jedno: skan nie znalazł nic ORAZ tekst wchodzi bajt w bajt taki,
 * jaki leży w cudzym projekcie (`Verdict::Clean if body == content` w `import::adapters`).
 * Nad taką pozycją to okno niczego nie przemilcza, więc pytanie o zgodę miałoby jedną możliwą
 * odpowiedź — a pytanie zadane 17 razy z rzędu przestaje być czytane po trzecim.
 *
 * `adjusted` jest odwrotnością tego zdania: albo przegląd RUSZYŁ tekst (znaki niewidzialne
 * i komentarze HTML znikają z ciała — to jest technika ataku, nie formatowanie), albo znalazł
 * w nim coś wagi ostrzegawczej (`Verdict::Concerns`: umiejętność prosi we frontmatterze
 * o własne narzędzia, albo próbuje nadpisać reguły w bloku kodu), albo jest drugą kopią tej
 * samej rzeczy z innej aplikacji. Znaleziska rozróżniają dziś PIERWSZE DWA — lista pod
 * wierszem mówi, co i w której linii — ale trzeciego nie widać w nich wcale: kopia zgodna co
 * do bajtu z drugą aplikacją ma przegląd czysty i mimo to nie jest `exact`. Dlatego pytanie
 * o przeczytanie zostaje przy całym `adjusted`, a nie przy samych znaleziskach.
 */
import type { Compatibility, ImportItem, ImportPreview } from './setup';

/* TE SAME SŁOWA, co pod blokującym znaleziskiem w karcie przeglądu — bo to jest ta sama zgoda
 * i ten sam człowiek. Dwa napisy na jedną czynność uczą, że to dwie różne czynności, więc od
 * 2026-08-31 nie ma tu drugiego literału: jest przeniesienie tego jednego, który już istniał. */
export { READ_IT } from '../skills/findings';

export const BECOMES_INSTRUCTIONS = 'A skill becomes instructions your agents follow.';

/** Co blokujące znalezisko robi TUTAJ — i to jest inna odpowiedź niż w sekcji Umiejętności.
 *
 * Tam instalacja czeka, aż człowiek odklika każde blokujące znalezisko, więc nośnikiem jest
 * przycisk. Tu nie czeka nic: umiejętność z takim znaleziskiem jest po stronie Rusta
 * `Unsupported` (`import::adapters::skill`) i `stage_skills` odmówiłby jej nawet wtedy, gdyby
 * ktoś ją przepchnął — przycisk byłby więc kontrolką bez skutku (niezmiennik 16). Zostaje
 * zdanie, i stoi pod TĄ linią, która import zatrzymała: przy trzech znaleziskach „coś tu jest
 * nie tak" nie mówi, którego wiersza dotyczy. */
export const STOPS_IT = 'This one stops the import.';

/** Następny ruch, nie sam fakt. Ścieżki plików stoją w tym samym wierszu, nad tym zdaniem. */
export const OPEN_IT = 'Open the file above, read it, and then say so here.';

/** Co zostaje w wierszu po odklikaniu. Kontrolka znika, ale ślad po decyzji nie: pusty wiersz
 *  po kliknięciu czyta się jak kliknięcie, które nie doszło. */
export const READ_ALREADY = 'You read this one.';

/** Czy tę pozycję trzeba przeczytać, zanim wolno ją wnieść. */
export function mustBeRead(item: ImportItem, compatibility: Compatibility | undefined): boolean {
  return item.kind === 'skill' && compatibility === 'adjusted';
}

/** Zgodność zgłoszona przez raport dla tej pozycji — mapa liczona raz, na cały ekran. */
export function compatibilityIn(preview: ImportPreview): Map<string, Compatibility> {
  return new Map(
    preview.draft.report.mappings.map((mapping) => [mapping.itemId, mapping.compatibility]),
  );
}

/**
 * Umiejętności, które wchodzą do importu i nie zostały przeczytane.
 *
 * Pozycja wyjęta z importu (`excludedItems`) nie jest tu liczona i to jest cała droga wyjścia:
 * człowiek, który nie chce czytać cudzej umiejętności, odznacza jej `Import` i wnosi resztę.
 */
export function stillUnread(
  preview: ImportPreview,
  excludedItems: readonly string[],
  read: readonly string[],
): string[] {
  const compatibility = compatibilityIn(preview);
  return preview.draft.items
    .filter(
      (item) =>
        !excludedItems.includes(item.id) &&
        !read.includes(item.id) &&
        mustBeRead(item, compatibility.get(item.id)),
    )
    .map((item) => item.id);
}

/** Zdanie w stopce, kiedy to właśnie czekające umiejętności trzymają przycisk Import. */
export function readingSays(unread: number): string {
  return `${String(unread)} skill(s) here have not been read yet. Read each one, or take it out of the import.`;
}
