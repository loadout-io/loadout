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
  askAnAgent as askRustToDraft,
  authorSkill,
  install,
  listSkills,
  readLink,
  remove as removeFromDisk,
  stopWriting as stopTheDraft,
} from '../sections/skills/io';
/* Lista zapisanych agentów przychodzi krawędzią SEKCJI AGENCI, a nie drugą kopią tej samej
 * komendy w krawędzi umiejętności. `list_agents` już tam mieszka, już jest sądzone przez
 * `commands-wired.test.ts` i już zna nazwę po stronie Rusta — druga droga do tej samej komendy
 * byłaby drugim miejscem, w którym mieszka odpowiedź „jak nazywa się ta komenda"
 * (niezmiennik 23). Ten plik dalej nie zna ani jednej nazwy komendy. */
import { list as listSavedAgents } from '../sections/agents/io';
import { why } from '../ipc/why';
/* „Gdzie pracujemy" ma w tym repo JEDNĄ definicję i to jest ona — ta sama, którą pyta Start
 * biegu (`src/sections/run/launch.ts`) i którą po drugiej stronie granicy sądzi
 * `AppState::project_for`. Druga odpowiedź (zmienna środowiskowa, katalog roboczy, pole
 * skopiowane do tego magazynu) rozjeżdża się pierwszego dnia, w którym ktoś przełączy zakres
 * w bocznym menu — a rozjazd objawia się jako umiejętność zapisana w innym projekcie niż ten,
 * w którym pracuje bieg (niezmiennik 13). */
import { activeWorkspace } from './workspaces';

/** Dwie wagi i ani jednej więcej. Trzecia jest tym, jak lista znalezisk przestaje być czytana. */
export type Weight = 'warn' | 'block';

/**
 * Gdzie umiejętność ma wylądować [T5 §8.3]. Lustro `Landing` z
 * `src-tauri/src/commands/skills.rs`.
 *
 * DWIE WARTOŚCI, bo tyle znają narzędzia agentowe: „w tym repo" i „u mnie". Te napisy są SŁOWAMI
 * DRUTU i nie mają prawa trafić na ekran (niezmiennik 14) — pozycje wyboru, które czyta człowiek,
 * liczy ekran sekcji.
 */
export type Landing = 'this-project' | 'everywhere';

/** Trzy stany importu. Nie ma czwartego i nie ma „prawie czysto". */
export type Verdict = 'clean' | 'concerns' | 'blocked';

/**
 * Co ta sekcja wie o katalogach agentów. TRZY STANY, NIE DWA.
 *
 * 2026-08-31 — do tego dnia były dwa i jeden z nich kłamał przy każdym starcie. Magazyn
 * startował z `installed: []`, a odczyt biegnie dopiero w efekcie po zamontowaniu ekranu —
 * więc pierwszą rzeczą, jaką człowiek z dziesięcioma umiejętnościami na dysku czytał o swojej
 * maszynie, było „nie ma tu żadnej". „Nikt jeszcze nie zaglądał" i „zajrzeliśmy, nie ma nic"
 * to dwa różne zdania i dopiero drugie wolno postawić na ekranie.
 *
 * Trzecim jest ODMOWA i ona też nie jest pustką: katalog, którego nie umiemy przeczytać, może
 * być pełny. Dopóki stany były dwa, ekran pokazywał zdanie o awarii i zaproszenie „nic tu
 * jeszcze nie ma" JEDNOCZEŚNIE — a jedno z nich musiało być nieprawdą.
 *
 * `reading` jest wartością POCZĄTKOWĄ, nie tylko chwilą po wywołaniu `load()`: przed pierwszym
 * odczytem sekcja nie wie o katalogach nic, a to jest dokładnie ten stan, który `reading`
 * nazywa. Magazyn bez ani jednego wołającego `load()` zostałby na nim na zawsze i tak ma być —
 * jedyny ekran, który go czyta, pyta o katalogi przy każdym zamontowaniu.
 */
export type Folders = 'reading' | 'read' | 'unreadable';

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
  /**
   * Po co ta umiejętność jest — pole `description` z jej `SKILL.md`, zwinięte do jednego wiersza.
   *
   * Pusty napis znaczy „ten plik nie mówi, po co jest", i to jest fakt o pliku, a nie brak
   * odpowiedzi. Kafelek pokazuje go zdaniem, nie pustką: pusty prostokąt czyta się jak awaria
   * wczytywania, a nie jak umiejętność bez opisu.
   */
  summary: string;
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
   * Co wiemy o katalogach agentów — jedno miejsce na ten fakt (niezmiennik 13).
   *
   * Nie jest drugą kopią [`SkillsState.message`]: tamto niesie ZDANIE, którym odmówiła
   * któraś z sześciu czynności tej sekcji, a to odpowiada na pytanie „czy wolno powiedzieć,
   * że katalogi są puste". Zdanie o odmowie odczytu bez tego pola nie da się odróżnić od
   * zdania o odmowie zapisu, a ekran pustej sekcji zależy wyłącznie od pierwszego.
   */
  folders: Folders;
  /**
   * Gdzie ma wylądować to, co człowiek doda — jego wybór, jedno miejsce na ten fakt.
   *
   * Stąd bierze się ZARAZEM zaznaczona pozycja wyboru, ZARAZEM zdanie o miejscu na ekranie
   * i ZARAZEM to, co jedzie do Rusta przy zapisie (niezmiennik 13). Dwa miejsca znaczyłyby ekran,
   * na którym zaznaczone jest jedno, a plik ląduje gdzie indziej — a ląduje w żywej konfiguracji
   * narzędzi agentowych człowieka.
   *
   * Domyślnie „wszędzie": to jest zakres, który ta sekcja miała od pierwszego dnia, więc wybór
   * niezmieniony nie ma prawa przenieść zapisu w nowe miejsce.
   */
  landing: Landing;
  /** Człowiek wybiera, gdzie to ma wylądować. */
  chooseLanding: (landing: Landing) => void;
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
  /**
   * Wejście w sekcję po raz drugi: przeczytaj, kogo człowiek ma zapisanego.
   *
   * OSOBNO OD [`SkillsState.load`], i to nie jest kaprys: tamta ścieżka odpowiada na pytanie
   * „co leży w katalogach agentów", ta na pytanie „kogo mogę o to poprosić". Sklejenie ich
   * w jedno wywołanie zamieniłoby jedno wejście w sekcję w dwie komendy pod jedną nazwą,
   * a `src/sections/read-paths-populate.test.ts` zamraża liczbę pytań tamtej ścieżki.
   */
  loadAgents: () => Promise<void>;
  /** Zdanie, które napisał człowiek: czego chce od umiejętności. */
  want: string;
  /** Człowiek pisze w polu „czego chcesz". */
  sayWhatYouWant: (said: string) => void;
  /** `id` agenta wybranego z listy. Pusty napis znaczy „nikt jeszcze nie wybrany". */
  chosenAgent: string;
  /** Człowiek wybiera, kto ma to napisać — `id`, nie nazwa (nazwa się zmienia, `id` nie). */
  chooseAgent: (id: string) => void;
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
   *
   * BIERZE MIEJSCE, A NIE NAZWĘ, i to jest cała różnica po 2026-08-31. Nazwę niesie stojące
   * pytanie ([`SkillsState.removing`]) — bez niego nie ma czego kasować i ta funkcja nic nie
   * robi. Miejsce przyjeżdża z kontrolki, którą człowiek nacisnął, więc kasowanie nie ma jak
   * uderzyć tam, gdzie ekran nie napisał, że uderzy.
   */
  remove: (from: Landing) => Promise<void>;
  /**
   * Nazwa umiejętności, o którą ekran właśnie pyta „na pewno?". `null` znaczy, że o żadną.
   *
   * 2026-08-31 — DO TEGO DNIA PYTANIA NIE BYŁO WCALE. Jedno naciśnięcie „Remove" jechało
   * prosto w `fs::remove_dir_all` po drugiej stronie granicy (`src-tauri/src/skills/place.rs`):
   * bez potwierdzenia, bez cofnięcia i bez ani jednego zdania o tym, co dokładnie zniknie.
   * Sekcja Agenci pyta dwustopniowo od T-39 i to jest ten sam wzorzec.
   *
   * NAZWA, NIE FLAGA. Pytanie stoi w wierszu TEJ umiejętności, więc jego tożsamością jest to,
   * o którą pyta; flaga „pytamy" obok osobnego pola z nazwą to dwa miejsca na jedną odpowiedź
   * i pierwsza okazja, żeby zapytać o jedną, a skasować drugą.
   */
  removing: string | null;
  /** Człowiek nacisnął „Remove" przy tej umiejętności — pytanie wchodzi, pliki zostają. */
  askToRemove: (name: string) => void;
  /** Człowiek odpowiedział „zostaw" — pytanie schodzi, pliki zostają. */
  keepIt: () => void;
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

/* Zdanie na pierwszy dzień na świeżej maszynie: biblioteka agentów jest pusta, więc nie ma kogo
 * poprosić. Mówi, co zrobić dalej — zapisać agenta — bo to jest rzecz, którą człowiek może
 * pójść i zrobić. Odmowa w ciszy czyta się dokładnie jak zepsute wejście. */
const NOBODY_TO_ASK =
  'There is nobody saved to ask yet. Save an agent first, then press this again.';

/* Zdania zapasowe na wypadek, gdyby granica odmówiła bez własnego zdania. Osobne dla pisania
 * i dla zatrzymywania: „coś poszło nie tak" w miejscu, w którym znamy czynność, jest gorsze niż
 * brak zdania (`src/ipc/why.ts`). */
const COULD_NOT_ASK = 'Loadout could not ask that agent to write this skill.';
const COULD_NOT_STOP = 'Loadout could not stop the agent that is writing.';
const COULD_NOT_READ_AGENTS = 'Loadout could not read the agents saved on this machine.';

/* Draft wrócił do panelu, którego już nie ma: człowiek zamknął go albo zapisał umiejętność,
 * kiedy agent jeszcze pisał. Trzy pola nie mają wtedy gdzie stanąć, a wpisanie ich w panel
 * otwarty na nowo byłoby tekstem, który pojawia się sam w polach, w których człowiek właśnie
 * pisze coś innego. */
const NOWHERE_TO_LAND =
  'The agent finished writing after this panel closed, so there was nowhere to put it. Ask again.';

/**
 * Folder, w którym człowiek pracuje — albo `null`, kiedy żadnego nie wskazał.
 *
 * FUNKCJA, NIE POLE TEGO MAGAZYNU. Skopiowany do stanu sekcji byłby drugą odpowiedzią na jedno
 * pytanie (niezmiennik 13) i zdążyłby się rozjechać z pierwszą już przy przełączeniu zakresu bez
 * ponownego wejścia w tę sekcję. Czytany przy każdym wywołaniu jest zawsze tym, co widzi bieg.
 *
 * `null` jest WARTOŚCIĄ, nie brakiem: znaczy „nie ma otwartego projektu", a co z tym zrobić,
 * decyduje Rust — lista pokazuje wtedy sam korzeń globalny, a zapis „w tym projekcie" odmawia
 * zdaniem z rdzenia (`skills::Error::NoProjectRoot`), zamiast zapisywać umiejętność pod
 * katalogiem, w którym akurat wstała aplikacja.
 */
function whereWeWork(): string | null {
  return activeWorkspace()?.folder ?? null;
}

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
  landing: 'everywhere',
  folders: 'reading',
  removing: null,

  chooseLanding: (landing: Landing) => {
    set({ landing });
  },

  load: async () => {
    /* „CZYTAM" ZAPALA SIĘ PRZED PYTANIEM, nie po nim. Ustawione po `await` nie zapaliłoby się
     * ani razu: między jednym a drugim nie ma renderu, a to właśnie ta chwila jest cała.
     * Drugie wejście w sekcję przechodzi tędy tak samo — lista zostaje na ekranie, dopóki
     * odczyt nie wróci, bo stan „czytam" rozstrzyga wyłącznie o PUSTYM ekranie. */
    set({ folders: 'reading' });
    try {
      /* PODMIANA CAŁEJ LISTY, nigdy dopisanie: drugie wejście w sekcję pokazałoby wtedy każdą
       * umiejętność dwa razy, a licznik nad sekcją policzyłby dwa razy te same pliki.
       * `pending` i `acknowledged` zostają nietknięte — odczyt katalogu nie ma nic wspólnego
       * z przeglądem, który czeka na człowieka, a skasowanie go tutaj kasowałoby to, co ktoś
       * właśnie czyta. */
      /* FOLDER JEDZIE RAZEM Z PYTANIEM, bo lista odpowiada na „co widzi agent pracujący TUTAJ",
       * a nie na „co kiedykolwiek zapisaliśmy". Bez niego umiejętność zapisana w projekcie nie
       * pojawiłaby się na ekranie — czyli człowiek by jej nie zobaczył i nie miałby jak jej
       * zabrać, choć leży w żywej konfiguracji jego narzędzi agentowych. Katalogi wylicza dalej
       * Rust i tylko Rust (`skills::place::destinations`, niezmiennik 23). */
      set({ installed: await listSkills(whereWeWork()), message: null, folders: 'read' });
    } catch (error) {
      /* Odmowa NIE leci w górę: wywołującym jest wejście w sekcję, a wyjątek stamtąd wywraca
       * ekran zamiast pokazać zdanie. Lista pustoszeje z rozmysłem — to, co sekcja pamięta
       * z poprzedniego odczytu, nie jest tym, co leży w katalogach agentów, a tylko o tym
       * drugim ta lista mówi (niezmiennik 4). */
      set({ installed: [], message: why(error, COULD_NOT_READ), folders: 'unreadable' });
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
       * raz byłoby tekstem, którego nikt nie przeskanował.
       *
       * WYBÓR CZŁOWIEKA I FOLDER JADĄ RAZEM Z NIM. Bez wyboru kontrolka na ekranie byłaby
       * kontrolką bez skutku (niezmiennik 16) dokładnie tam, gdzie skutkiem jest zapis do żywej
       * konfiguracji cudzych narzędzi; bez folderu Rust nie miałby korzenia, pod którym pisać,
       * a `place::destinations` odpowiada na zakres bez korzenia ścieżkami WZGLĘDNYMI.
       *
       * FOLDER JEDZIE PRZY OBU ZAKRESACH, nie tylko przy „ten projekt": pytanie „gdzie
       * pracujemy" ma jedną odpowiedź niezależnie od tego, co człowiek wybrał, a warunek tutaj
       * byłby drugim miejscem, w którym mieszka odwzorowanie wyboru na korzeń (niezmiennik 13).
       * Odwzorowanie stoi w `Landing -> Scope`, po tamtej stronie granicy. */
      await install(pending, get().landing, whereWeWork());
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
        /* Opis bierzemy z tego, co właśnie zainstalowaliśmy: `Import` już go niesie, a wiersz
         * wstawiony bez niego pokazywałby przez chwilę „ta umiejętność nie mówi, po co jest"
         * o umiejętności, której opis człowiek przed sekundą czytał na karcie przeglądu. */
        {
          name: pending.name,
          fromTheInternet: pending.fromTheInternet,
          summary: pending.summary,
        },
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

  loadAgents: async () => {
    try {
      const saved = await listSavedAgents();
      /* DWA POLA Z PIĘTNASTU. Model, prompt systemowy i dial liczy Rust z zapisanej definicji
       * (`library::agents::resolve`), więc trzymanie ich tutaj byłoby drugim egzemplarzem
       * odpowiedzi, której ta sekcja i tak nie używa — i pierwszym miejscem, przez które
       * okno mogłoby te trzy rzeczy podmienić. */
      const agents: SavedAgent[] = saved.map((one) => ({ id: one.id, name: one.name }));
      /* WYBÓR TRZYMA SIĘ TEGO, CO WIDAĆ. Pusty wybór przy niepustej liście znaczy ekran,
       * na którym przeglądarka pokazuje pierwszą pozycję jako zaznaczoną, a magazyn nie zgadza
       * się z ekranem (niezmiennik 13) — i wtedy pytanie jedzie do kogoś innego, niż człowiek
       * przeczytał. Agent, którego człowiek wybrał, a potem usunął, wraca tą samą drogą. */
      const chosen = get().chosenAgent;
      set({
        agents,
        chosenAgent: agents.some((one) => one.id === chosen) ? chosen : (agents.at(0)?.id ?? ''),
      });
    } catch (error) {
      /* Lista pustoszeje z rozmysłem, tak samo jak `installed` w `load`: to, co sekcja pamięta
       * z poprzedniego odczytu, nie jest tym, co leży na dysku (niezmiennik 4). */
      set({ agents: [], chosenAgent: '', message: why(error, COULD_NOT_READ_AGENTS) });
    }
  },

  sayWhatYouWant: (said: string) => {
    set({ want: said });
  },

  chooseAgent: (id: string) => {
    set({ chosenAgent: id });
  },

  askAnAgent: async () => {
    const { adding, agents, chosenAgent, want, writing } = get();
    /* Pytanie zadane, kiedy panelu nie ma, nie miałoby gdzie oddać trzech pól: „otwórz panel"
     * jest osobną decyzją człowieka i ma zostać jedna (`typeInto` odmawia tak samo). */
    if (adding === null) return;
    /* Drugie pytanie w trakcie pisania odbija się TUTAJ, a nie na wyłączonym przycisku: zgoda
     * musi być warunkiem WYWOŁANIA (nagłówek tego pliku). Bez zdania z rozmysłu — na ekranie
     * stoi już żywy region mówiący, że agent pisze, i kontrolka, która to zatrzymuje. Drugie
     * zdanie o tym samym fakcie byłoby drugim miejscem na jedną odpowiedź (niezmiennik 13). */
    if (writing) return;
    if (agents.length === 0) {
      set({ message: NOBODY_TO_ASK });
      return;
    }

    /* Pusty wybór przy niepustej liście to stan, w którym ekran pokazuje PIERWSZĄ pozycję jako
     * zaznaczoną — tak działa `<select>` bez pasującej opcji. Pytanie jedzie więc do tego,
     * kogo człowiek widzi, a nie do nikogo. Normalnie tego stanu nie ma: `loadAgents` ustawia
     * wybór razem z listą. */
    const agent = chosenAgent === '' ? (agents.at(0)?.id ?? '') : chosenAgent;

    set({ writing: true, message: null });
    try {
      const drafted = await askRustToDraft(want, agent);
      /* `null` znaczy „człowiek to zatrzymał" i jest WARTOŚCIĄ, nie odmową (niezmiennik 7):
       * gaśnie stan „pisze" i nie ma ani draftu, ani zdania o awarii. Dowód zejścia grupy
       * przyjeżdża tą samą drogą jako odmowa — i tylko wtedy, gdy go NIE MA. */
      if (drafted === null) {
        set({ writing: false });
        return;
      }
      const panel = get().adding;
      if (panel === null) {
        set({ writing: false, message: NOWHERE_TO_LAND });
        return;
      }
      /* Draft ląduje w TYCH SAMYCH trzech polach, w których człowiek pisze ręką, i nic nie
       * jedzie na dysk. Zapis idzie dalej jedną drogą (`writeItHere`), więc tekst poprawiony
       * po drafcie przechodzi przez skan tak samo jak wpisany od zera (niezmiennik 23) —
       * a tekst przeskanowany PRZED poprawką jest tekstem, którego nikt nie przeskanował. */
      set({
        writing: false,
        adding: {
          ...panel,
          name: drafted.name,
          whenToUse: drafted.whenToUse,
          whatToDo: drafted.whatToDo,
        },
      });
    } catch (error) {
      /* Stan „pisze" gaśnie także tutaj. Odmowa, która zostawia go zapalonym, zostawia na
       * ekranie Stop bez czego zatrzymywać i zabiera jedyną drogę do zadania pytania jeszcze
       * raz. Zdanie człowieka zostaje w polu: tekst tracony przy odmowie to ten sam defekt co
       * cisza, tylko droższy. */
      set({ writing: false, message: why(error, COULD_NOT_ASK) });
    }
  },

  stopWriting: async () => {
    /* Zatrzymywanie czegoś, co nie pisze, jest wywołaniem bez skutku po obu stronach granicy —
     * a nie jest ciszą wobec człowieka, bo wtedy na ekranie nie ma ani tej kontrolki, ani
     * zdania o pisaniu. */
    if (!get().writing) return;
    try {
      /* MUSI OPUŚCIĆ OKNO. Zgaszenie samego `writing` byłoby kontrolką, która melduje skutek
       * bez skutku (niezmiennik 16), i to w jedynym miejscu tej sekcji, gdzie kłamstwo kosztuje
       * pieniądze: proces vendora pisze dalej i dalej pali limit dostawcy. */
      await stopTheDraft();
    } catch (error) {
      set({ message: why(error, COULD_NOT_STOP) });
    }
    /* `writing` gasi ODPOWIEDŹ draftu, nie ta akcja, i to jest ta sama decyzja, co przy Stopie
     * biegu: dopóki tura się nie zwinęła, agent może jeszcze pisać, a ekran mówiący „już nie
     * pisze" zabierałby kontrolkę, która jako jedyna umie go dobić. Zdanie o grupie, która
     * mogła przeżyć, przyjeżdża odmową z `askAnAgent` (niezmiennik 6). */
  },

  askToRemove: (name: string) => {
    /* SAMO PYTANIE, ani jednego bajtu ruszonego. Naciśnięcie „Remove" przy innym wierszu
     * przestawia pytanie na tamten wiersz: dwa pytania naraz to dwa miejsca, w których stoi
     * jedna decyzja (niezmiennik 13), i pierwsza okazja, żeby odpowiedzieć na inne pytanie,
     * niż się czyta. */
    set({ removing: name });
  },

  keepIt: () => {
    set({ removing: null });
  },

  remove: async (from: Landing) => {
    /* ZGODA JEST WARUNKIEM WYWOŁANIA, nie stanem widoku (nagłówek tego pliku). Pytanie, które
     * nie stoi na ekranie, znaczy, że nikt o nic nie został zapytany — a po drugiej stronie
     * granicy stoi `fs::remove_dir_all` (`src-tauri/src/skills/place.rs`), bez cofnięcia.
     * Warunek na przycisku byłby sugestią: zostaje klawiatura, zostaje skrót i zostaje druga
     * ścieżka w interfejsie.
     *
     * Bez zdania z rozmysłu: w tym stanie na ekranie nie ma ani tej kontrolki, ani nazwy,
     * o której zdanie miałoby mówić. */
    const name = get().removing;
    if (name === null) return;

    try {
      /* MIEJSCE PRZYJEŻDŻA Z KONTROLKI, KTÓRĄ CZŁOWIEK NACISNĄŁ, i to jest cała ta poprawka
       * (zmierzone 2026-08-31).
       *
       * Do tego dnia stało tu `get().landing` — wybór z grupy radiowej, która renderuje się
       * WYŁĄCZNIE wewnątrz karty czekającego importu („Available in",
       * `src/sections/skills/shelf.tsx`). Bez czekającego importu tej kontrolki na ekranie nie
       * ma wcale, a wartość zostaje ta z ostatniego zapisu: człowiek, który raz dodał
       * umiejętność „w tym projekcie", od tej chwili każdym „Remove" celował w katalog WEWNĄTRZ
       * swojego repozytorium, patrząc na wiersz umiejętności leżącej w katalogu domowym.
       * Ustawienie, którego w chwili decyzji nie widać, nie ma prawa rozstrzygać o kasowaniu.
       *
       * TA SAMA NAZWA W DWÓCH ZAKRESACH TO DALEJ DWIE RZECZY: `place::remove` zdejmuje wyłącznie
       * kopie z podanego korzenia i zostawia drugą tam, gdzie jest. Zabranie obu naraz jest inną
       * czynnością, o którą nikt nie prosił — więc miejsce jest jedno i nazywa je zdanie
       * na przycisku, który człowiek nacisnął.
       *
       * FOLDER JEDZIE OSOBNO I DALEJ Z JEDNEJ DEFINICJI: „gdzie pracujemy" ma w tym repo jedną
       * odpowiedź (`whereWeWork`), a odwzorowanie miejsca na korzenie liczy Rust
       * (`Landing -> Scope`, niezmiennik 23).
       *
       * DŁUG, ZGŁOSZONY, NIE PRZEOCZONY: wiersz listy wciąż nie wie, w którym korzeniu leży
       * jego plik — `InstalledWire` (`src-tauri/src/commands/skills.rs`) niesie `name`,
       * `fromTheInternet` i `summary`, a `list_skills_in` zwija oba korzenie do jednego zbioru
       * nazw. Dopóki tak jest, odpowiedź na „skąd kasujemy" musi paść na ekranie, przy tym
       * wierszu, i pada tam. Kiedy `InstalledWire` dostanie pole per korzeń, pytanie o miejsce
       * zniknie wszędzie tam, gdzie kopia jest jedna. */
      await removeFromDisk(name, from, whereWeWork());
    } catch (error) {
      /* Odmowa Rusta wchodzi na ekran DOSŁOWNIE, jeśli ją napisał: „no skill named … is
       * installed" i „could not write to that folder" to dwie różne rzeczy do zrobienia,
       * a jedno zdanie zapasowe zamienia je w jedną.
       *
       * Pytanie schodzi z ekranu także tutaj: zostawione stojące obok zdania o awarii czyta się
       * jak drugie zaproszenie do naciśnięcia tego samego, a odpowiedź już padła. */
      set({ removing: null, message: why(error, COULD_NOT_REMOVE) });
      return;
    }

    set({ removing: null });

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
