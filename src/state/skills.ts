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
import {
  authorSkill,
  install,
  listSkills,
  readLink,
  remove as removeFromDisk,
} from '../sections/skills/io';
import { why } from '../ipc/why';

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

/**
 * Trzy pytania z formularza, dokładnie te trzy [T5 §8.3]. Lustro `Authored`
 * z `src-tauri/src/commands/skills.rs`.
 *
 * Nazwy pól są nazwami PYTAŃ, nie pól `SKILL.md`: człowiek odpowiada „kiedy tego użyć",
 * a `description` jest tym, w co ta odpowiedź się zamienia — i zamienia się po drugiej stronie
 * granicy, w jednym miejscu. `name` jest tym, co człowiek wpisał, a nie nazwą katalogu: slug
 * liczy Rust, bo slug widziany na ekranie i nazwa katalogu na dysku to JEDEN fakt
 * (niezmiennik 13), a dwa liczenia rozjeżdżają się na pierwszym znaku spoza ASCII.
 */
export interface Authored {
  name: string;
  whenToUse: string;
  whatToDo: string;
}

/**
 * Agent zapisany na dysku, tak jak potrzebuje go ta sekcja: tożsamość i nazwa dla człowieka.
 *
 * DWA POLA, NIE PIĘTNAŚCIE. `Agent` z `src/state/agents.ts` niesie jeszcze model, instrukcje,
 * dial bezpieczeństwa i dziesięć innych pól — a wszystkie one są odpowiedzią po TAMTEJ stronie
 * granicy: model, prompt systemowy i dial liczy Rust z zapisanej definicji
 * (`library::agents::resolve`). Tutaj potrzebne jest dokładnie to, z czego składa się wybór na
 * ekranie: co pojedzie do Rusta (`id`) i co przeczyta człowiek (`name`). Nazwy vendora nie ma
 * i nie ma prawa być — `src/sections/skills/mounted.test.tsx` zamraża jej brak w markupie tej
 * sekcji i ma do tego zmierzony powód.
 */
export interface SavedAgent {
  id: string;
  name: string;
}

/**
 * Panel „Add a skill": jedno pole na adres i trzy pytania, oba wejścia pod TYM SAMYM
 * przyciskiem. `null` znaczy „zamknięty" — jedno miejsce na to pytanie (niezmiennik 13),
 * a nie osobna flaga „czy otwarty" obok treści, która potrafi się z nią rozjechać.
 *
 * DLACZEGO TREŚĆ PANELU MIESZKA W MAGAZYNIE, A NIE W `useState` EKRANU. Bo odmowa z Rusta musi
 * zostawić to, co człowiek wpisał, na ekranie: tekst tracony przy odmowie to ten sam defekt co
 * cisza, tylko droższy — człowiek pisze akapit, dostaje jedno zdanie o nazwie i traci akapit.
 * Odmowa ląduje w magazynie (`message`), więc pola muszą leżeć tam, gdzie ona.
 */
export interface AddPanel extends Authored {
  /** Adres wklejony przez człowieka. Pierwsza droga wejścia, ta, która już była. */
  link: string;
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
  /**
   * Panel dodawania z obydwoma wejściami. `null` znaczy zamknięty.
   *
   * 2026-08-19 — do tego dnia sekcja obiecywała na pustym ekranie „Paste a link, or write one
   * yourself" i umiała przyjąć WYŁĄCZNIE adres: `review_skill(url)` i nic więcej. Obietnica bez
   * kontrolki jest tym samym defektem, co kontrolka bez skutku, tylko odwróconym
   * (niezmiennik 16).
   */
  adding: AddPanel | null;
  openAdd: () => void;
  closeAdd: () => void;
  /** Człowiek pisze w jednym z pól panelu. */
  typeInto: (part: Partial<AddPanel>) => void;
  /**
   * „Save this skill" — trzy odpowiedzi jadą do Rusta i wracają jako przegląd, dokładnie tak
   * samo jak wklejony link.
   *
   * Nie składa `SKILL.md` tutaj i nie ma prawa złożyć: potok (złóż → zapisz → przeczytaj
   * i przeskanuj) mieszka po tamtej stronie granicy w jednym miejscu, a tekst zbudowany
   * w oknie byłby tekstem, którego nikt nie przeskanował (niezmiennik 23).
   */
  writeItHere: () => Promise<void>;
  /**
   * Agenci, których wolno poprosić o napisanie umiejętności — pozycje wyboru w trzecim wejściu.
   *
   * LISTA MIESZKA TU, A NIE W MAGAZYNIE SEKCJI AGENTS. Tamten jest fabryką
   * (`createAgentsStore(io)`), więc ta sekcja nie ma jak sięgnąć po jego egzemplarz, a drugi
   * egzemplarz obok byłby drugą odpowiedzią na pytanie „kogo mam zapisanych" (niezmiennik 13).
   * Wypełnia ją odczyt z dysku, dokładnie tak samo jak `installed` — nazwy vendorów nie ma tu
   * ani jednej i mieć nie może (`mounted.test.tsx`).
   */
  agents: SavedAgent[];
  /** Zdanie, które napisał człowiek: czego chce od umiejętności. */
  want: string;
  /** `id` agenta wybranego z listy. Pusty napis znaczy „nikt jeszcze nie wybrany". */
  chosenAgent: string;
  /**
   * Czy wybrany agent pisze właśnie teraz.
   *
   * Jedno miejsce na ten fakt (niezmiennik 13): to z niego bierze się ZARAZEM zdanie na ekranie
   * i podmiana kontrolki „napisz mi to" na „zatrzymaj". Dwie flagi znaczyłyby ekran, na którym
   * stoi zdanie o pisaniu i przycisk, który każe zacząć jeszcze raz.
   */
  writing: boolean;
  /**
   * „Write it for me" — jedno zdanie jedzie do wybranego agenta, wracają trzy pola.
   *
   * Nic nie zapisuje i nie ma prawa zapisać: draft ląduje w tych samych trzech polach, w których
   * człowiek pisze ręką (`adding`), a plik składa, skanuje i odkłada dopiero `writeItHere` —
   * czyli tekst poprawiony po drafcie przechodzi przez skan tak samo jak wpisany od zera
   * (niezmiennik 23).
   */
  askAnAgent: () => Promise<void>;
  /**
   * „Stop" — zatrzymaj agenta, który pisze.
   *
   * Musi OPUŚCIĆ OKNO. Zgaszenie samego `writing` byłoby kontrolką, która melduje skutek bez
   * skutku (niezmiennik 16), i to w miejscu, w którym cisza kosztuje pieniądze: proces vendora
   * pisze dalej i dalej pali limit dostawcy (niezmiennik 6).
   */
  stopWriting: () => Promise<void>;
  /**
   * „Remove" — zabierz tę umiejętność z katalogów agentów.
   *
   * 2026-08-18 — do tego dnia sekcja umiała wyłącznie DODAWAĆ, a dodaje do żywej konfiguracji
   * narzędzi człowieka (`src-tauri/src/skills/mod.rs`, `DESTINATION_DIRS`): błędne kliknięcie
   * „Add" zostawało w `~/.claude/skills` na stałe i wchodziło do każdego następnego
   * uruchomienia Claude Code. Droga powrotna nie jest wygodą, jest warunkiem, żeby wolno było
   * w ogóle pisać w tamte katalogi.
   */
  remove: (name: string) => Promise<void>;
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

/* Zdanie zapasowe dla odmowy usunięcia. Cisza po „Remove" jest tu gorsza niż gdziekolwiek
 * indziej: człowiek odchodzi w przekonaniu, że plik zniknął z katalogów, do których zagląda
 * jego Claude Code, a on tam dalej leży. */
const COULD_NOT_REMOVE = 'Loadout could not remove that skill.';

/* Panel otwarty i jeszcze pusty. Cztery puste napisy, nie `undefined`: pole kontrolowane
 * z wartością `undefined` przestaje być kontrolowane i React przestaje o nim wiedzieć. */
const NOTHING_TYPED: AddPanel = { link: '', name: '', whenToUse: '', whatToDo: '' };

export const useSkills = create<SkillsState>()((set, get) => ({
  pending: null,
  acknowledged: [],
  message: null,
  installed: [],
  adding: null,
  agents: [],
  want: '',
  chosenAgent: '',
  writing: false,

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
       * nie ma, więc znacznik gasnący po sukcesie nie znaczy nic.
       *
       * 2026-08-18 — pozycja o tej samej nazwie jest WYMIENIANA, nie doklejana. Nazwa
       * umiejętności jest nazwą katalogu na dysku, więc drugie dodanie tego samego linku
       * nadpisuje jeden plik, a lista pokazywała po nim DWA wiersze i licznik „N saved"
       * liczył ten jeden plik dwa razy. Rust liczy to samo zbiorem
       * (`list_skills_inner`, `BTreeSet`) — dwie odpowiedzi na jedno pytanie muszą się
       * zgadzać co do znaku (niezmiennik 13). */
      installed: [
        ...installed.filter((one) => one.name !== pending.name),
        { name: pending.name, fromTheInternet: pending.fromTheInternet },
      ],
    });
  },

  openAdd: () => {
    /* Otwarty panel zostaje taki, jaki jest. Wyzerowanie go tutaj kasowałoby akapit, który
     * człowiek napisał, za drugie kliknięcie w ten sam przycisk. */
    set({ adding: get().adding ?? NOTHING_TYPED });
  },

  closeAdd: () => {
    set({ adding: null });
  },

  typeInto: (part: Partial<AddPanel>) => {
    const { adding } = get();
    /* Pisanie w panelu, którego nie ma, nie otwiera panelu: „otwórz" jest osobną decyzją
     * człowieka i ma zostać jedna. */
    if (adding === null) return;
    set({ adding: { ...adding, ...part } });
  },

  writeItHere: async () => {
    const { adding } = get();
    if (adding === null) return;
    try {
      const pending = await authorSkill({
        name: adding.name,
        whenToUse: adding.whenToUse,
        whatToDo: adding.whatToDo,
      });
      set({ pending, acknowledged: [], message: null, adding: null });
    } catch (error) {
      set({ message: why(error, 'Loadout could not save that skill.') });
    }
  },

  /* SZKIELET FAZY KONTRAKTU — te dwie akcje jeszcze NIC nie robią, i to jest cały ich stan.
   *
   * Istnieją, żeby `src/sections/skills/the-agent-writes-it.test.tsx` się WCZYTAŁO i padło na
   * asercji, a nie na `Cannot find` przy zbieraniu plików: vitest przewraca się już na zbieraniu,
   * a podpis „Failed to load" nie liczy się jako czerwień (AGENTS.md §2a.5). Puste ciało jest
   * odpowiednikiem `todo!()` z tamtego akapitu — mierzalnie brakuje zachowania, w czasie
   * wykonania: zero wywołań na granicy IPC, zero zdań na ekranie, draft, który nigdy nie
   * przychodzi. Implementacja zdejmuje te dwa ciała razem z tym komentarzem. */
  askAnAgent: async () => {
    /* Pusto z rozmysłu. Nie wolno tu wołać `askAnAgent` z `io.ts`: to jest DOKŁADNIE zachowanie,
     * którego kryterium ma nie znaleźć w fazie `before`. */
  },

  stopWriting: async () => {
    /* Pusto z rozmysłu, ten sam powód. Zgaszenie `writing` tutaj byłoby połową implementacji —
     * i tą połową, która zazielenia asercję o podmianie kontrolki, nie ubijając agenta. */
  },

  remove: async (name: string) => {
    try {
      await removeFromDisk(name);
    } catch (error) {
      /* Odmowa Rusta wchodzi na ekran DOSŁOWNIE, jeśli ją napisał: „no skill named … is
       * installed" i „could not write to that folder" to dwie różne rzeczy do zrobienia,
       * a jedno zdanie zapasowe zamienia je w jedną. */
      set({ message: why(error, COULD_NOT_REMOVE) });
      return;
    }

    /* Lista czytana JESZCZE RAZ Z DYSKU, nigdy odfiltrowana lokalnie.
     *
     * Instalacja pisze do DWÓCH katalogów vendorów naraz (`DESTINATION_DIRS`). Usunięcie,
     * które sprzątnęło jeden i nie sprzątnęło drugiego, po lokalnym odfiltrowaniu wygląda
     * dokładnie jak sukces: wiersz znika z ekranu, a plik dalej leży tam, gdzie agent po niego
     * sięga. To jest ten sam defekt, który ta fala naprawia — kontrolka reaguje, ekran melduje
     * skutek, skutek nie zachodzi (niezmiennik 16). Odczyt po zapisie jest jedyną odpowiedzią,
     * której nie musimy zgadywać (niezmiennik 4: pliki są prawdą). */
    await get().load();
  },
}));
