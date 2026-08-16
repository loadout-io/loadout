/* Magazyn sekcji Umiejętności: co przyszło z linku, co z tego przeczytał człowiek i co wolno
 * z tego zainstalować.
 *
 * DLACZEGO ODMAWIA MAGAZYN, A NIE PRZYCISK. Wyłączony przycisk jest sugestią: zostaje klawiatura,
 * zostaje skrót, zostaje druga ścieżka w interfejsie i zostaje wywołanie akcji wprost. Zgoda musi
 * być warunkiem WYWOŁANIA, nie stanem widoku [T5 §5.4] — inaczej mechanizm z niezmiennika 16
 * działa dokładnie do pierwszego miejsca, w którym ktoś doda drugi przycisk.
 *
 * DLACZEGO ZGODA JEST PER ZNALEZISKO, A NIE JEDNĄ FLAGĄ. Jedna flaga „przeczytałem" znaczy, że
 * import z pięcioma blokadami odblokowuje się po przeczytaniu tej, która akurat stała pierwsza.
 * Identyfikator znaleziska jest tu jedynym powodem, dla którego `acknowledge` ma argument.
 *
 * Typy niżej są lustrem `src-tauri/src/skills/ingest.rs`. Dopóki nie ma generatora (`ts-rs`
 * albo `specta` — T4 §7.2), rozjazd łapią kryteria po stronie Rusta: tam te same pola są
 * zamrożone testem.
 *
 * Ciała akcji są jeszcze puste i mają paść w czasie wykonania. To jest ten sam szkielet, co
 * `todo!()` w Ruście (AGENTS.md §2a): import ma się rozwiązać, a kryterium paść na BRAKU
 * ZACHOWANIA, nie na braku modułu.
 */
import { create } from 'zustand';

/** Dwie wagi i ani jednej więcej. Trzecia jest tym, jak lista znalezisk przestaje być czytana. */
export type Weight = 'warn' | 'block';

/** Trzy stany importu. Nie ma czwartego i nie ma „prawie czysto". */
export type Verdict = 'clean' | 'concerns' | 'blocked';

/** Jedno znalezisko: która reguła, jak ciężko, w której linii i co dokładnie tam stało. */
export interface Finding {
  /**
   * Tożsamość TEGO znaleziska, nie reguły. `acknowledge` bierze właśnie ją: dwie linie łamiące
   * tę samą regułę to dwie rzeczy do przeczytania, nie jedna.
   */
  id: string;
  /** `hidden-text`, `instruction-override`, `exfiltration`, `role-manipulation`, `escalation`
   * albo `deep-scan-unavailable` — a przy głębokim skanie cokolwiek, co przyniósł skaner
   * (niezmiennik 5: nieznana reguła to znalezisko, nie awaria). Nigdy nie trafia na ekran. */
  rule: string;
  weight: Weight;
  /** Numer linii w ciele, które zapisujemy, liczony od 1. `null`, kiedy znalezisko nie dotyczy
   * żadnej konkretnej linii. */
  line: number | null;
  /** Linia zacytowana dosłownie. Człowiek ma przeczytać atak, nie jego opis. */
  quoted: string;
  /** Tekst ZDJĘTY z ciała: treść komentarza HTML albo napis odzyskany po usunięciu znaków
   * niewidzialnych. Wyłącznie dla `hidden-text` — pozostałe reguły niczego nie usuwają. */
  recovered: string | null;
}

/** Treść po potoku i wszystko, co po drodze o niej zauważyliśmy. */
export interface Reviewed {
  /** Ciało dokładnie takie, jakie pójdzie na dysk — i dokładnie to, które przeskanowaliśmy. */
  body: string;
  findings: Finding[];
  verdict: Verdict;
}

/** Pobrana umiejętność, przejrzana, jeszcze przed pierwszym zapisem. */
export interface Import {
  name: string;
  summary: string;
  reviewed: Reviewed;
  /** Ile dołączonych skryptów niesie umiejętność. Liczona z tego, co przyszło. */
  scripts: number;
  /** Czy przyszła z sieci. Znacznik jest TRWAŁY i przeżywa instalację [T5 §5.4]. */
  fromTheInternet: boolean;
}

/** Umiejętność, która już leży w katalogach vendorów. */
export interface InstalledSkill {
  name: string;
  /**
   * Zastępuje podpisy i weryfikację pochodzenia, których w v1 nie ma. Znacznik, który znika po
   * udanej instalacji, mówi o umiejętności z sieci dokładnie to samo, co o napisanej ręcznie —
   * czyli nic.
   */
  fromTheInternet: boolean;
}

export interface SkillsState {
  /** Import czekający na człowieka. `null` znaczy, że na nic nie czekamy (niezmiennik 13). */
  pending: Import | null;
  /** Identyfikatory znalezisk, które człowiek przeczytał. */
  acknowledged: string[];
  /** Zdanie po angielsku mówiące, co trzeba zrobić. `null`, kiedy nie ma nic do powiedzenia. */
  message: string | null;
  installed: InstalledSkill[];
  review: (url: string) => Promise<void>;
  acknowledge: (findingId: string) => void;
  add: () => Promise<void>;
}

export const useSkills = create<SkillsState>()(() => ({
  pending: null,
  acknowledged: [],
  message: null,
  installed: [],

  review: () => {
    throw new Error('not implemented: read the link and hold what came back for a person to see');
  },

  acknowledge: () => {
    throw new Error('not implemented: write down that this one finding was read');
  },

  add: () => {
    throw new Error(
      'not implemented: refuse while a blocking finding is unread, otherwise hand it over once',
    );
  },
}));
