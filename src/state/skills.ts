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
 * Nazw komend nie zna ten plik: zna je `sections/skills/io.ts` i to jest JEDYNA krawędź, przez
 * którą cokolwiek jedzie do Rusta (niezmiennik 23). Zdanie „zero wywołań IPC" ma sens tylko
 * wtedy, kiedy jest jedna droga do policzenia — przy dwóch licznik pilnuje jednej, a instalacja
 * jedzie drugą.
 */
import { create } from 'zustand';
import { install, listSkills, readLink } from '../sections/skills/io';

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
  /**
   * Wejście w sekcję: przeczytaj katalogi agentów i pokaż, co w nich naprawdę leży.
   *
   * Do 2026-08-18 `installed` rosło wyłącznie po udanym `add`, więc licznik „N saved" mówił
   * „ile dodałeś w tej sesji", udając, że mówi „ile masz".
   */
  load: () => Promise<void>;
  review: (url: string) => Promise<void>;
  acknowledge: (findingId: string) => void;
  add: () => Promise<void>;
}

/** Znaleziska, które zatrzymują instalację i których człowiek jeszcze nie otworzył. */
function unread(item: Import, acknowledged: readonly string[]): Finding[] {
  return item.reviewed.findings.filter(
    (finding) => finding.weight === 'block' && !acknowledged.includes(finding.id),
  );
}

/* Zdanie, nie słowo. Odmowa w ciszy wygląda dokładnie jak zepsuty przycisk, a człowiek, który
 * nie wie, czego się od niego chce, klika drugi raz i zgłasza błąd. Nazwa reguły nie ma tu
 * prawa paść (niezmiennik 14): mówi, jak nazywa się sprawdzenie, a nie na czym polega ryzyko. */
function held(count: number): string {
  return count === 1
    ? 'One line in this skill has to be read before it can be added.'
    : String(count) + ' lines in this skill have to be read before it can be added.';
}

/* Zdanie na wypadek, gdyby Rust nie przysłał własnego. Odmowa w ciszy wygląda jak pusta sekcja,
 * a pusta sekcja i „nie umiem przeczytać tego katalogu" to dwie różne rzeczy. */
const COULD_NOT_READ = 'Loadout could not read the skills on this machine.';

/** Zdanie od Rusta, kiedy jakieś jest — jego odmowy są już napisane po ludzku. */
function why(error: unknown, fallback: string): string {
  const said = error instanceof Error ? error.message.trim() : '';
  return said.length > 0 ? said : fallback;
}

export const useSkills = create<SkillsState>()((set, get) => ({
  pending: null,
  acknowledged: [],
  message: null,
  installed: [],

  load: async () => {
    try {
      /* PODMIANA CAŁEJ LISTY, nigdy dopisanie: drugie wejście w sekcję pokazałoby wtedy każdą
       * umiejętność dwa razy, a licznik nad sekcją policzyłby dwa razy te same pliki.
       * `pending` i `acknowledged` zostają nietknięte — odczyt katalogu nie ma nic wspólnego
       * z przeglądem, który czeka na człowieka, a skasowanie go tutaj kasowałoby to, co ktoś
       * właśnie czyta. */
      set({ installed: await listSkills(), message: null });
    } catch (error) {
      /* Odmowa NIE leci w górę: wywołującym jest wejście w sekcję, a wyjątek stamtąd wywraca
       * ekran zamiast pokazać zdanie. Lista pustoszeje z rozmysłem — to, co sekcja pamięta
       * z poprzedniego odczytu, nie jest tym, co leży w katalogach agentów, a tylko o tym
       * drugim ta lista mówi (niezmiennik 4). */
      set({ installed: [], message: why(error, COULD_NOT_READ) });
    }
  },

  review: async (url: string) => {
    try {
      /* Przeczytane znaleziska NIE przenoszą się na następny import. Ta sama karta z tym samym
       * identyfikatorem znaleziska jest innym plikiem z innej strony. */
      set({ pending: await readLink(url), acknowledged: [], message: null });
    } catch (error) {
      set({
        pending: null,
        acknowledged: [],
        message: why(error, 'Loadout could not read that link.'),
      });
    }
  },

  acknowledge: (findingId: string) => {
    const { acknowledged } = get();
    if (acknowledged.includes(findingId)) return;
    set({ acknowledged: [...acknowledged, findingId] });
  },

  add: async () => {
    const { pending, acknowledged, installed } = get();
    if (pending === null) return;

    /* Warunek stoi na WYWOŁANIU, nie na widoku. Wyłączony przycisk jest sugestią: zostaje
     * klawiatura, skrót i druga ścieżka w interfejsie — a ta funkcja jest jedynym miejscem,
     * przez które umiejętność z sieci może trafić na dysk. */
    const waiting = unread(pending, acknowledged);
    if (waiting.length > 0) {
      set({ message: held(waiting.length) });
      return;
    }

    try {
      /* Jedzie CAŁY przegląd, ten sam obiekt, który przyszedł z Rusta. Ciało złożone tu jeszcze
       * raz byłoby tekstem, którego nikt nie przeskanował. */
      await install(pending);
    } catch (error) {
      set({ message: why(error, 'Loadout could not add that skill.') });
      return;
    }

    set({
      pending: null,
      acknowledged: [],
      message: null,
      /* Znacznik przeżywa instalację. Zastępuje podpisy i weryfikację pochodzenia, których w v1
       * nie ma, więc znacznik gasnący po sukcesie nie znaczy nic. */
      installed: [...installed, { name: pending.name, fromTheInternet: pending.fromTheInternet }],
    });
  },
}));
