/* Półka umiejętności: jedna ścieżka dodawania i lista umiejętności, każda ze swoim stanem
 * rozmieszczenia.
 *
 * SKĄD SIĘ TU WZIĄŁ TEN PLIK. Do 2026-08-31 to był `src/sections/skills/index.tsx`, czyli cały
 * ekran sekcji Skills, z własnym paskiem nagłówka i własnym pustym zaproszeniem na całą
 * wysokość. Sekcja zniknęła — scaliła się z Pamięcią w jedną sekcję Knowledge (decyzja
 * właściciela, `src/ui/sections.tsx`) — a to, co robił ekran, zostało półką. Pasek nagłówka
 * i zdanie o pustce należą teraz do ekranu, który obie półki trzyma; tu zostaje nagłówek
 * strefy, który mówi, czym ta półka różni się od tej nad nią.
 *
 * „USED WHEN IT FITS" JEST POŁOWĄ RÓŻNICY, nie ozdobnym podtytułem. Notatka w użyciu wchodzi
 * do KAŻDEGO promptu; po umiejętność model sięga sam, kiedy pasuje do tego, co właśnie robi.
 * To jest najważniejsza rzecz, jaką człowiek musi w tej sekcji zrozumieć, i do 2026-08-31 była
 * powiedziana raz, mimochodem, w zdaniu jednej strefy.
 *
 * KARTA PRZEGLĄDU BEZPIECZEŃSTWA ZOSTAJE TU I TYLKO TU. Umiejętność bywa cudza i wykonywalna,
 * więc przechodzi przez skan z blokującymi znaleziskami. Notatka jest własna i deklaratywna —
 * skanowanie własnych zdań o własnym repo zamieniłoby ten przegląd w rytuał, a rytuał
 * przeklikuje się bez czytania.
 *
 * CIENKI Z ZAŁOŻENIA. Karta przeglądu (`review-card.tsx`, T-19) jest wylądowana i to ona
 * pokazuje wciągniętą umiejętność: ciało, znaleziska i przycisk dodania. Drugiej karty ani
 * drugiego przepływu wciągania tu nie ma (niezmiennik 23) — między komponentem a sekcją
 * brakowało nagłówka, POLA NA LINK i listy, i tylko to jest w tym pliku.
 *
 * DLACZEGO UMIEJĘTNOŚĆ CZEKAJĄCA JEST WIERSZEM LISTY, A NIE OSOBNYM PANELEM. Makieta rysuje
 * ją dwa razy — raz w panelu „Add a skill", raz jako kafelek z chipem `needs a check`
 * (`docs/mockup/index.html:716-735`) — a to jest jeden fakt w dwóch miejscach (niezmiennik 13).
 * Tutaj jest jedno miejsce: wiersz tej umiejętności, a karta przeglądu siedzi w nim.
 *
 * ZNACZNIKA ROZMIESZCZENIA TU NIE MA I TO JEST NAPRAWA, NIE BRAK (zmierzone 2026-08-18).
 * Do tego dnia każdy wiersz zainstalowanej umiejętności nosił napis „Ready for Claude and
 * Codex", policzony przez `readyFor(placed)` z argumentem wpisanym na sztywno jako `true`.
 * Na dysku właściciela to było NIEPRAWDĄ dla wszystkich dziesięciu umiejętności: `notatki`
 * i `spotkanie` leżą tylko w `~/.claude/skills`, osiem `superset-*` tylko w
 * `~/.agents/skills`, żadna w obu. Kłamał nie napis, a jego źródło: `InstalledWire`
 * (`src-tauri/src/commands/skills.rs`) niesie WYŁĄCZNIE `name` i `fromTheInternet`, a
 * `list_skills_inner` zwija oba katalogi vendorów do jednego `BTreeSet` nazw — informacja
 * o tym, KTÓRY katalog trzymał plik, ginie po drugiej stronie granicy i nie ma jak tu
 * dojechać. Znacznik policzony z danych, których nie ma, jest zmyśloną relacją
 * (niezmiennik 17), więc nie ma go wcale. Wraca w tym samym commicie, w którym `InstalledWire`
 * dostaje pole per katalog — zgłoszone człowiekowi.
 *
 * O migawce serwerowej zustanda i o tym, dlaczego magazyn czyta się tu przez
 * `useSyncExternalStore`, przeczytaj w `src/sections/workflows/index.tsx`.
 */
import type { ReactElement } from 'react';
import { useSyncExternalStore } from 'react';
import type { Landing } from '../../state/skills';
import { useSkills } from '../../state/skills';
import { activeWorkspace, useWorkspaces } from '../../state/workspaces';
import { EVERY_PROJECT, THIS_PROJECT } from '../knowledge/reach';
import { evaluateSkill } from '../lab/evaluate';
/* NAPIS KONTROLKI PRZYCHODZI Z KONTROLKI, nie jest tu przepisany. Zdanie odsyłające do rzeczy
 * nazwanej na ekranie inaczej jest instrukcją, której nie da się wykonać — a to jest jedyne
 * wyjście, jakie ta sekcja umie zaproponować człowiekowi bez otwartego projektu. Jedna nazwa,
 * jedno miejsce (niezmiennik 13); `src/sections/run/launch.ts` ma ten sam obowiązek i wciąż
 * trzyma napis z palca, co jest długiem tamtego pliku, nie wzorem do skopiowania. */
import { FIRST_INVITE } from '../../ui/shell/workspace-switcher';
import { ReviewCard } from './review-card';

/** Magazyn umiejętności. Jest singletonem — `src/state/skills.ts` nie ma fabryki. */
export type SkillsStore = typeof useSkills;

export interface SkillsShelfProps {
  /** Bez propsu półka bierze swój prawdziwy magazyn, z propsem ten z testu. */
  store?: SkillsStore;
}

/* OSIEM STAŁYCH KLASOWYCH ZNIKŁO 2026-08-31 (DESIGN §6, warstwa prymitywów).
 *
 * `PRIMARY`, `SECONDARY`, `DANGER`, `CHIP_QUIET`, `LABEL`, `ROW` i dwie dodatkowe kopie `FIELD`
 * to było osiem opisów wyglądu w pliku, w którym nikt tych opisów nie szuka — i rozjechały się
 * dokładnie tak, jak taki zapis się rozjeżdża: ta sama rola przycisku drugoplanowego miała tu
 * 32 px pod nazwą `SECONDARY`, a w Agents 28 px pod nazwą `QUIET`. Dziś klasa nazywa ROLĘ
 * (`btn-primary`, `btn`, `btn-danger`, `chip`, `label`, `stack`, `field`), a geometria, cztery
 * stany i wciśnięcie mieszkają raz, w `@layer components` w `src/styles/theme.css`.
 *
 * `ANSWER` i `ANSWER_LONG` były DRUGĄ I TRZECIĄ NAZWĄ tej samej wartości `'field'`, a ich
 * komentarze obiecywały różnicę, której w kodzie nie było od 2026-08-19: obie rozwijały się do
 * tego samego pola maszynowego. Wysokość pola wielowierszowego niesie `textarea.field`
 * w arkuszu, nie ten plik. Trzy nazwy na jedną wartość to trzy miejsca, w których ktoś poprawi
 * jedno i pojedzie dalej.
 *
 * `chip` bez `data-tone` jest wariantem neutralnym i to jest decyzja z DESIGN §6: skąd przyszła
 * umiejętność, jest zwykłym faktem, a fakt pomalowany barwą stanu wygląda jak problem. */
const FIELD = 'field';

/**
 * Co stoi w kafelku umiejętności, której `SKILL.md` nie ma pola `description`.
 *
 * ZDANIE, NIE PUSTKA. Pusty prostokąt w miejscu opisu czyta się jak awaria wczytywania —
 * człowiek wraca na tę sekcję i czeka, aż „się doładuje". Zdanie mówi, że to nie my zgubiliśmy
 * treść, tylko że jej tam nie ma, i mówi, gdzie ją dopisać.
 */
const NO_SUMMARY = 'This one does not say what it is for. Its SKILL.md has no description.';

/**
 * Gdzie to wyląduje — zdanie czytane PRZED naciśnięciem „Add this skill", nie po.
 *
 * Ta sekcja jest jedynym miejscem w Loadoucie, które pisze poza własną bibliotekę: cel to
 * katalogi, do których zaglądają narzędzia agentowe człowieka (`DESTINATION_DIRS`
 * w `src-tauri/src/skills/mod.rs`). Umiejętność dodana tutaj wchodzi więc do każdego
 * następnego uruchomienia tych narzędzi, także poza Loadoutem — i człowiek ma o tym wiedzieć
 * z ekranu, a nie z dokumentacji. Nazwy katalogów w tym zdaniu nie padają z rozmysłem: liczy
 * je Rust i to jest jedyne miejsce, w którym stoją (niezmiennik 13).
 */
export const WHERE_IT_LANDS =
  'This goes into the folders your agent apps read on this machine, so every later run can ' +
  'use it. Remove takes it back out.';

/**
 * To samo zdanie dla drugiego wyboru — i to jest CAŁY powód, dla którego wybór ma prawo tu stać.
 *
 * 2026-08-19 — DWA ZDANIA, NIGDY DWA NARAZ. Jedno miejsce na jeden fakt (niezmiennik 13): to,
 * które stoi na ekranie, liczy się z tego, co człowiek wybrał, a drugie z ekranu znika. Oba
 * jednocześnie znaczą ekran, na którym połowa nie zgadza się z drugą połową — i wtedy człowiek
 * wierzy tej, którą przeczytał pierwszą.
 *
 * Nazwy katalogów nie padają tu tak samo jak wyżej: liczy je `place::destinations` po drugiej
 * stronie granicy i to jest jedyne miejsce, w którym stoją (niezmiennik 23).
 */
export const WHERE_IT_LANDS_IN_THE_PROJECT =
  'This goes into the folders your agent apps read inside the project you have open, so it ' +
  'travels with that folder to anybody who works in it. Remove takes it back out.';

/**
 * Co powiedzieć, kiedy nie ma otwartego projektu, więc „this project" nie ma korzenia.
 *
 * ZDANIE, A NIE UKRYTA POZYCJA. Człowiek, który nie widzi możliwości, nie ma jak się dowiedzieć,
 * że istnieje ani czego wymaga — więc pozycja zostaje na ekranie, wygaszona, a obok stoi to
 * zdanie i mówi, co zrobić (DESIGN §8: każda odmowa nazywa wyjście).
 *
 * ZDANIE, A NIE DRUGI PRZYCISK. Zakres wybiera się w jednym miejscu, w bocznym menu — druga
 * kontrolka dodająca workspace'a byłaby drugą odpowiedzią na pytanie „gdzie pracuję"
 * (niezmiennik 13), a ekran Umiejętności nie jest miejscem, w którym wybiera się projekt.
 */
const NO_PROJECT_YET =
  'No project is open, so a skill can only go into the folders on this machine. ' +
  FIRST_INVITE +
  ' in the side menu to put one inside a project instead.';

/**
 * Dwie pozycje wyboru i dokładnie dwie, w słowach człowieka [T5 §8.3].
 *
 * NAPISY LICZY EKRAN, nie drut: `Landing` jest słowem granicy (`this-project`, `everywhere`)
 * i enum z drutu nigdy nie trafia na ekran (niezmiennik 14). Tabela stoi tu, przy jedynym
 * miejscu, które ją czyta.
 *
 * „This project" pierwsze, bo to jest wybór, o którym człowiek musi pomyśleć; drugi jest tym,
 * co ta półka robiła od pierwszego dnia, i zostaje domyślny w magazynie.
 *
 * NAPISY PRZYCHODZĄ Z `knowledge/reach.ts` (2026-08-31). Do tego dnia stało tu „Everywhere",
 * a notatka w dokładnie tym samym położeniu mówiła o sobie „Every project" — jeden fakt, dwa
 * brzmienia, na jednym ekranie i bez żadnej drogi, którą człowiek mógłby się dowiedzieć, że
 * to jedna oś (niezmiennik 13).
 */
const LANDINGS: readonly { readonly value: Landing; readonly label: string }[] = [
  { value: 'this-project', label: THIS_PROJECT },
  { value: 'everywhere', label: EVERY_PROJECT },
];

/**
 * Miejsca, z których wolno zabrać zainstalowaną umiejętność — po jednym przycisku na miejsce.
 *
 * OSOBNA TABELA OD `LANDINGS` I NIE JEST JEJ DRUGĄ KOPIĄ (niezmiennik 13). Tamta odpowiada na
 * „gdzie ma wylądować to, co dodaję", ta na „skąd to zabrać", i są to dwa różne pytania zadane
 * w dwóch różnych chwilach. Napisy też muszą się różnić: pozycja wyboru („Everywhere") i
 * przycisk, który za sekundę skasuje katalog („Remove from this machine"), nie mają prawa
 * brzmieć tak samo — pierwszy zapowiada, drugi wykonuje.
 *
 * KAŻDY PRZYCISK NAZYWA SWÓJ CEL i to jest cała odpowiedź na wadę z 2026-08-31: do tego dnia
 * cel brał się z `landing`, czyli z grupy radiowej widocznej WYŁĄCZNIE w karcie czekającego
 * importu. Bez importu na ekranie nie było jej wcale, a `Remove` i tak gdzieś uderzał.
 */
const PLACES: readonly {
  readonly value: Landing;
  readonly label: string;
  /** Czy to miejsce w ogóle istnieje bez otwartego projektu. Bez korzenia nie istnieje. */
  readonly needsProject: boolean;
}[] = [
  { value: 'everywhere', label: 'Remove from this machine', needsProject: false },
  { value: 'this-project', label: 'Remove from this project', needsProject: true },
];

/**
 * Pytanie zadane PRZED zdjęciem plików: po imieniu i z tym, co dokładnie zniknie.
 *
 * PO IMIENIU, bo „Are you sure?" nie mówi, o co pytamy, a ta siatka trzyma dziesięć kafelków
 * obok siebie. I ze zdaniem o nieodwracalności, bo po drugiej stronie granicy stoi
 * `fs::remove_dir_all` (`src-tauri/src/skills/place.rs`) — nie kosz, nie kopia, nie cofnięcie.
 *
 * DWA ZDANIA, NIGDY DWA NARAZ. Przy otwartym projekcie ta sama nazwa znaczy dwa różne katalogi
 * i tego jednego nie umiemy dziś rozstrzygnąć za człowieka: `InstalledWire` niesie samą nazwę,
 * a `list_skills_in` zwija oba korzenie do jednego zbioru. Zdanie mówi wtedy wprost, że wybór
 * należy do niego, zamiast zgadywać po cichu.
 */
function askAbout(name: string, projectOpen: boolean): string {
  return projectOpen
    ? 'Remove ' +
        name +
        '? Say which copy goes: the one in the folders on this machine, or the one inside the ' +
        'project you have open. Nothing brings it back.'
    : 'Remove ' +
        name +
        '? It goes out of the folders your agent apps read on this machine, and nothing brings ' +
        'it back.';
}

/**
 * Zdanie stanu „czytam" — trzecie obok „nie ma nic" i „nie dało się przeczytać".
 *
 * 2026-08-31 — do tego dnia stany były dwa i jeden z nich kłamał przy każdym starcie. Odczyt
 * katalogów biegnie w efekcie po zamontowaniu, więc pierwszą rzeczą, jaką człowiek z dziesięcioma
 * umiejętnościami na dysku czytał o swojej maszynie, było „No skills yet.".
 */
const READING = 'Reading the folders your agent apps use.';

/** Nagłówek półki — to samo słowo czyta kryterium i człowiek. */
export const WHEN_IT_FITS = 'Used when it fits';

/**
 * Zdanie, które robi z tej półki połowę różnicy, a nie kolejnej listy.
 *
 * Mówi rzecz, której z samej listy nie da się zgadnąć, i mówi ją OBOK półki notatek, która
 * mówi swoją: notatka jedzie do modelu za każdym razem, a po umiejętność model sięga sam.
 * Dopiero te dwa zdania obok siebie odpowiadają na pytanie „którą z tych dwóch rzeczy mam
 * teraz napisać".
 */
const WHEN_IT_FITS_LEAD =
  'The model reaches for these on its own, when they fit the work in front of it.';

/** Kiedy katalogi odpowiedziały i naprawdę nic w nich nie ma (DESIGN §6: to jest zaproszenie). */
const NO_SKILLS_YET = 'No skills yet. Paste a link, or write one yourself.';

/* Klasy nagłówka i zdania strefy — te same stałe, co w półce notatek nad nią. Dwie półki na
 * jednym ekranie muszą wyglądać jak jedna rzecz w dwóch stanach, a nie jak dwa ekrany. */
const ZONE_TITLE = 'text-eyebrow text-muted';
const ZONE_LEAD = 'lead max-w-160';

export default function SkillsShelf({ store = useSkills }: SkillsShelfProps): ReactElement {
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);

  /* SUBSKRYPCJA ZAKRESÓW, ŻEBY PRZERYSOWAĆ — a odpowiedź czytamy funkcją. Bez tego wiersza
   * przełączenie projektu w bocznym menu zostawiłoby na ekranie wygaszoną pozycję „This project"
   * i zdanie o tym, że nie ma gdzie zapisać, czyli kontrolkę kłamiącą o stanie, który już się
   * zmienił.
   *
   * WARTOŚCI Z TEJ MIGAWKI NIE CZYTAMY ANI RAZU, i to jest rozmyślne. „Gdzie pracujemy" ma
   * w tym repo jedną definicję — `activeWorkspace()` — i to ona jedzie do Rusta z magazynu tej
   * sekcji. Drugie `find` po `activeId` tutaj byłoby drugą odpowiedzią na jedno pytanie
   * (niezmiennik 13) i pierwszym miejscem, w którym ekran mógłby pokazać coś innego, niż
   * pojedzie na dysk. `src/sections/run/index.tsx` liczy to u siebie i to jest dług tamtego
   * pliku, nie wzór.
   *
   * Trzecim argumentem jest BIEŻĄCY stan, nie `getInitialState`: `renderToStaticMarkup` woła
   * właśnie migawkę serwerową, więc magazyn zasiany przed renderem musi być tym, co ekran widzi
   * (ten sam powód stoi w `src/sections/workflows/index.tsx`). */
  useSyncExternalStore(useWorkspaces.subscribe, useWorkspaces.getState, useWorkspaces.getState);
  const openProject = activeWorkspace();

  /* ODCZYT PRZY WEJŚCIU W SEKCJĘ MIESZKA W `knowledge/index.tsx`, NIE TUTAJ (2026-08-31).
   *
   * Powód jest mechaniczny, nie estetyczny: kiedy nic jeszcze nie przeczytano, ekran Knowledge
   * pokazuje jedno zaproszenie zamiast półek, więc ta półka NIE JEST wtedy zamontowana. Efekt
   * odczytu zawieszony tutaj nie odpalałby się nigdy i lista zostawałaby pusta na zawsze —
   * pętla, w której pustka jest jednocześnie przyczyną i skutkiem.
   *
   * O tym, że `load()` w ogóle ma wołającego, mówi historia zamknięta 2026-08-18: magazyn
   * dostał `load()` w T-38 i przez czas nie miał ANI JEDNEGO. Komenda po stronie Rusta
   * istniała, krawędź `io.ts` istniała, magazyn umiał się wypełnić — i ekran nigdy o nic
   * nie pytał (niezmiennik 4 — pliki są prawdą). */
  /* NIE MA CZEGO POKAZAĆ — to jeszcze nie znaczy, że nie ma czego pokazywać. Zdanie o pustce
   * wolno postawić dopiero wtedy, gdy katalogi ODPOWIEDZIAŁY (`folders === 'read'`); przedtem
   * sekcja nie wie o nich nic, a po odmowie wie, że nie wie. Trzy stany, nie dwa. */
  const nothing = state.installed.length === 0 && state.pending === null;
  /* Co człowiek wpisał w panelu dodawania — adres ALBO trzy odpowiedzi. `null` znaczy, że
   * panel jest zamknięty: jedno miejsce na to pytanie (niezmiennik 13), a nie osobna flaga
   * „czy otwarty" obok treści, która potrafi się z nią rozjechać. */
  const panel = state.adding;

  /* Jedna funkcja na oba przyciski `data-create` — ten w nagłówku i ten na pustym ekranie.
   * Nigdy nie stoją w dokumencie naraz i otwierają ten sam panel. */
  const openPanel = (): void => {
    store.getState().openAdd();
  };

  return (
    <section data-zone="skills" data-gap="2" className="stack">
      {/* NAGŁÓWEK STREFY, nie pasek nagłówka ekranu: ten stoi wyżej i mówi „Knowledge" raz.
          Licznik i przycisk stoją w tym samym wierszu, co nazwa półki — licznik liczy WYŁĄCZNIE
          to, co leży w katalogach, bo umiejętność czekająca na przeczytanie nie jest jeszcze
          zapisana. Przy zerze licznika nie ma: zero jest już powiedziane zdaniem niżej
          (niezmiennik 13). */}
      <div className="flex items-center gap-2">
        <h2 className={ZONE_TITLE}>{WHEN_IT_FITS}</h2>
        {state.installed.length === 0 ? null : (
          <span className="value">{`${String(state.installed.length)} saved`}</span>
        )}
        <button data-create type="button" className="btn ml-auto" onClick={openPanel}>
          ＋ Add a skill
        </button>
      </div>

      {/* DRUGA POŁOWA RÓŻNICY. Nad tą półką stoi „Always on" i zdanie o tym, że tamto wchodzi
          do każdego promptu; tu stoi to, co odróżnia umiejętność: model sięga po nią sam. Bez
          tego zdania obie półki są dwiema listami, a człowiek nie ma jak zgadnąć, dlaczego
          jedna rzecz jest w jednej, a druga w drugiej. */}
      <p className={ZONE_LEAD}>{WHEN_IT_FITS_LEAD}</p>

      <div className="stack">
        {/* DWA WEJŚCIA, JEDEN PANEL, JEDEN PRZYCISK, KTÓRY GO OTWIERA.
            Adres i umiejętność napisana tutaj to jedna decyzja z dwiema odpowiedziami, a nie
            dwie decyzje — drugie zaproszenie obok pierwszego byłoby dwiema odpowiedziami na
            pytanie „jak dodać umiejętność" (niezmiennik 13).

            2026-08-19 — DO TEGO DNIA PANEL PRZYJMOWAŁ WYŁĄCZNIE ADRES, dokładnie tyle, ile
            umiało `review_skill(url)` po drugiej stronie granicy. Pusty ekran obiecywał przy
            tym „Paste a link, or write one yourself", więc obietnica stała bez kontrolki —
            ten sam defekt, co kontrolka bez skutku, tylko odwrócony (niezmiennik 16), i
            droższy, bo człowiek szuka przycisku, którego nie ma, zamiast zgłosić jego brak.

            DWA `<form>`, NIE JEDEN. Enter w polu wysyła formularz, w którym to pole stoi —
            przy jednym formularzu Enter wpisany w nazwę odpalałby czytanie PUSTEGO adresu.
            Panel jest za to jeden i to on nosi `data-add-panel`.

            Treść panelu mieszka w magazynie (`state.adding`), nie w `useState` ekranu, i to
            nie jest ustępstwo na rzecz testu: odmowa z Rusta ma zostawić wpisany akapit na
            ekranie, więc pola muszą leżeć tam, gdzie ląduje odmowa (niezmiennik 13). */}
        {panel === null ? null : (
          <div
            data-add-panel
            /* WEJŚCIE SPRĘŻYNĄ (DESIGN §7): panelu NIE MA w dokumencie, dopóki człowiek nie
               naciśnie `Add a skill`, i wchodzi NAD listę, która zostaje na miejscu. Element
               pojawiający się skokiem czyta się jak przeskok widoku. Jeden region na zdarzenie. */
            className="card enter mx-auto mb-6 flex max-w-160 flex-col gap-4"
          >
            <form
              className="stack"
              onSubmit={(event) => {
                event.preventDefault();
                void store.getState().review(panel.link);
                store.getState().closeAdd();
              }}
            >
              <label htmlFor="skill-link" className="label">
                Link
              </label>
              <input
                id="skill-link"
                className={FIELD}
                value={panel.link}
                onChange={(event) => {
                  store.getState().typeInto({ link: event.target.value });
                }}
              />
              <button type="submit" className="btn mt-1 mr-auto">
                Read it
              </button>
            </form>

            {/* TRZY PYTANIA I DOKŁADNIE TRZY [T5 §8.3]: jak się nazywa, kiedy tego użyć, co
                zrobić. Czwartym w badaniu jest zakres („ten projekt / wszędzie") i tu go dalej
                NIE MA — ale od 2026-08-19 z innego powodu niż wtedy, gdy ten panel powstał.

                Wtedy stało tu, że zakres zostaje globalny, a wybór jest osobnym zadaniem.
                Wybór już jest: stoi NAD KARTĄ PRZEGLĄDU, niżej w tym pliku, i pyta o miejsce
                w chwili, w której człowiek decyduje, czy tę umiejętność dodać. Dodanie go
                jeszcze raz TUTAJ byłoby dwoma miejscami na jedną odpowiedź (niezmiennik 13),
                i to na dwóch różnych etapach: ten panel oddaje TREŚĆ do przeglądu, a nie
                zapisuje jej do katalogów vendorów. Zapis następuje jedno wywołanie później,
                pod kartą, i to tam wybór ma skutek.

                ETYKIETY SĄ PYTANIAMI, a nie nazwami pól `SKILL.md`. „When should the agent
                use it?" jest tym, co w pliku nazywa się `description`, i pytanie zadane wprost
                jest jedynym powodem, dla którego człowiek pisze tam prawdziwy warunek zamiast
                drugiego tytułu — a to jest pole, po którym model decyduje, czy w ogóle sięgnąć
                (T5 §8.3). Nazwa pola z pliku nie pada tu ani razu (niezmiennik 14).

                SLUGA TU NIE LICZYMY. „Review pull requests" zamienia się w katalog
                `review-pull-requests` po tamtej stronie granicy i tylko tam (`slug_of`).
                Policzony drugi raz tutaj rozjechałby się z tamtym na pierwszym znaku spoza
                ASCII, a rozjazd widać dopiero jako katalog o innej nazwie niż zdanie, które
                człowiek przeczytał (niezmiennik 13). Człowiek widzi go raz — w nagłówku karty
                przeglądu, która wraca z Rusta z policzoną nazwą. */}
            <form
              className="stack border-t border-line pt-4"
              data-gap="2"
              onSubmit={(event) => {
                event.preventDefault();
                /* Bez `closeAdd()`: panel zamyka sam magazyn i TYLKO po udanym zapisie.
                   Zamknięty tutaj, bezwarunkowo, zabierałby ze sobą trzy odpowiedzi za każdym
                   razem, gdy Rust odmówi — a wtedy człowiek czyta jedno zdanie o nazwie
                   i pisze akapit drugi raz. */
                void store.getState().writeItHere();
              }}
            >
              <div className="stack">
                <label htmlFor="skill-name" className="label">
                  What should it be called?
                </label>
                <input
                  id="skill-name"
                  data-question="name"
                  className={FIELD}
                  value={panel.name}
                  onChange={(event) => {
                    store.getState().typeInto({ name: event.target.value });
                  }}
                />
              </div>

              <div className="stack">
                <label htmlFor="skill-when-to-use" className="label">
                  When should the agent use it?
                </label>
                <input
                  id="skill-when-to-use"
                  data-question="whenToUse"
                  className={FIELD}
                  value={panel.whenToUse}
                  onChange={(event) => {
                    store.getState().typeInto({ whenToUse: event.target.value });
                  }}
                />
              </div>

              <div className="stack">
                <label htmlFor="skill-what-to-do" className="label">
                  What should it do?
                </label>
                <textarea
                  id="skill-what-to-do"
                  data-question="whatToDo"
                  className={FIELD}
                  value={panel.whatToDo}
                  onChange={(event) => {
                    store.getState().typeInto({ whatToDo: event.target.value });
                  }}
                />
              </div>

              <button type="submit" data-write-it-yourself className="btn mt-1 mr-auto">
                Save this skill
              </button>
            </form>

            {/* TRZECIE WEJŚCIE: jedno zdanie człowieka i wybór tego, kto ma to napisać.
                W TYM SAMYM panelu, co adres i formularz — „dodaj umiejętność" jest jedną
                decyzją z trzema odpowiedziami, a nie trzema decyzjami (niezmiennik 13).

                2026-08-19 — do tego dnia OBIE istniejące drogi wymagały, żeby człowiek napisał
                treść sam: adres przyjmuje gotowy plik, formularz gotowe trzy odpowiedzi. Loadout
                miał przy tym dwa sterowniki agentów, żywy nadzór i dowód śmierci grupy — i ani
                jednej drogi, która zamienia zdanie człowieka w tekst od modelu.

                DRAFT NIE JEST ZAPISEM. Trzy pola lądują w formularzu wyżej, edytowalne, i to
                człowiek oddaje je dalej. Tekst poprawiony po drafcie przechodzi przez ten sam
                skan, co wpisany od zera (niezmiennik 23) — a tekst przeskanowany przed poprawką
                jest tekstem, którego nikt nie przeskanował.

                NAZWY VENDORÓW TU NIE MA I MIEĆ NIE MOŻE. Pozycje wyboru pochodzą z magazynu,
                czyli z dysku; informacja o tym, którym narzędziem biegnie agent, mieszka
                w jego zapisanej definicji po tamtej stronie granicy (`runsWith`), a każde
                zdanie o vendorze w tej sekcji byłoby zdaniem o czymś, czego nikt tu nie wie
                (`mounted.test.tsx` zamraża brak tych nazw w tym markupie). */}
            <form
              className="stack border-t border-line pt-4"
              data-gap="2"
              onSubmit={(event) => {
                event.preventDefault();
                void store.getState().askAnAgent();
              }}
            >
              <div className="stack">
                <label htmlFor="skill-what-you-want" className="label">
                  Or say what you want, and an agent writes it
                </label>
                <input
                  id="skill-what-you-want"
                  data-what-you-want
                  className={FIELD}
                  value={state.want}
                  onChange={(event) => {
                    store.getState().sayWhatYouWant(event.target.value);
                  }}
                />
              </div>

              <div className="stack">
                <label htmlFor="skill-who-writes-it" className="label">
                  Who should write it?
                </label>
                {/* Pozycją jest `id`, a widać nazwę: nazwa jest jedyną częścią zapisanego
                    agenta, którą człowiek rozpoznaje, a `id` jedyną, która przeżywa zmianę
                    nazwy (T4 §5.1) — i to ona jedzie do Rusta. */}
                <select
                  id="skill-who-writes-it"
                  data-pick-an-agent
                  className={FIELD}
                  value={state.chosenAgent}
                  onChange={(event) => {
                    store.getState().chooseAgent(event.target.value);
                  }}
                >
                  {state.agents.map((agent) => (
                    <option key={agent.id} data-agent={agent.id} value={agent.id}>
                      {agent.name}
                    </option>
                  ))}
                </select>
              </div>

              {/* PODMIANA KONTROLKI, dokładnie ta sama, którą robią Start i Stop w sekcji Praca
                  (`run/start.tsx`). „Napisz mi to" zostawione obok stanu „pisze" jest drugą turą
                  za drugie naciśnięcie, przy pierwszej, która dalej biegnie i dalej kosztuje.

                  WSKAŹNIK TRWANIA, DOPISANY 2026-08-31, bo to TRWA: model pisze umiejętność
                  kilkadziesiąt sekund, a przez cały ten czas nie zmieniał się ani jeden piksel.
                  Zdanie mówi, CO trwa; trzy kropki mówią, że wciąż trwa. Kropki są DZIEĆMI
                  z `aria-hidden`, obok zdania, które niesie treść — czytnik ekranu czyta zdanie,
                  nie ozdobę (DESIGN §7). Nie wirujący krążek: krążek nie mówi ani co trwa, ani
                  ile zostało, i kręci się tak samo przy 200 ms i przy 20 minutach. */}
              {state.writing ? (
                <>
                  <p data-writing className="lead">
                    An agent is writing this skill now.
                    <span className="thinking ml-1">
                      <span aria-hidden />
                      <span aria-hidden />
                      <span aria-hidden />
                    </span>
                  </p>
                  <button
                    type="button"
                    data-stop-writing
                    className="btn-danger mt-1 mr-auto"
                    onClick={() => {
                      void store.getState().stopWriting();
                    }}
                  >
                    Stop
                  </button>
                </>
              ) : (
                <button type="submit" data-ask-an-agent className="btn mt-1 mr-auto">
                  Write it for me
                </button>
              )}
            </form>

            {/* Wyjście z panelu jest jedno, bo panel jest jeden. Po jednym „Cancel" na wejście
                człowiek musiałby wiedzieć, które z trzech właśnie zamyka.

                W stanie „pisze" go NIE MA, i to jest ta sama podmiana, co wyżej: panel zamknięty
                w trakcie pisania zabiera ze sobą jedyną kontrolkę, która umie tego agenta
                zatrzymać, a agent pisze dalej i dalej kosztuje (niezmienniki 6 i 16). */}
            {state.writing ? null : (
              <button
                type="button"
                className="btn mr-auto"
                onClick={() => {
                  store.getState().closeAdd();
                }}
              >
                Cancel
              </button>
            )}
          </div>
        )}

        {/* Zdanie od magazynu: odmowa instalacji albo link, którego nie dało się przeczytać.
            Bez tego jedyną odpowiedzią na kliknięcie jest cisza, a człowiek klika drugi raz. */}
        {state.message === null ? null : (
          /* `.fade-in`, bo tego zdania nie ma, dopóki coś nie odmówi — a jest zdaniem DO
             PRZECZYTANIA, więc wchodzi samą przezroczystością, bez sprężyny (DESIGN §7). */
          <p className="lead fade-in mx-auto mb-6 max-w-160" data-tone="attend">
            {state.message}
          </p>
        )}

        {/* CZYTAM. Wskaźnik trwania, a nie zdanie o pustce: półka jest pusta, bo nikt jeszcze
            nie zajrzał, a „nikt nie zajrzał" i „nie ma nic" to dwa różne zdania o cudzej
            maszynie. Trzy kropki są DZIEĆMI z `aria-hidden` obok zdania niosącego treść —
            czytnik ekranu czyta zdanie, nie ozdobę (DESIGN §7).

            WIERSZ W PÓŁCE, NIE BLOK NA CAŁĄ WYSOKOŚĆ (2026-08-31). Wysokość całego ekranu
            należała do sekcji, która była sama; półka dzieli ekran z drugą półką i blok
            `h-full` zepchnąłby tamtą poza widok. */}
        {nothing && state.folders === 'reading' ? (
          <p data-reading className={ZONE_LEAD}>
            {READING}
            <span className="thinking ml-1">
              <span aria-hidden />
              <span aria-hidden />
              <span aria-hidden />
            </span>
          </p>
        ) : null}

        {/* NIE DAŁO SIĘ PRZECZYTAĆ. Zdanie o awarii stoi wyżej, w jedynym regionie, który je
            niesie (niezmiennik 13) — a zaproszenia „No skills yet." tu NIE MA i to jest cała
            ta poprawka: katalog, którego nie umiemy przeczytać, bywa pełny, a dwa zdania naraz
            znaczą ekran, na którym jedno musi być nieprawdą i nikt nie wie które. Droga dalej
            stoi w nagłówku półki i stoi tam zawsze, więc odmowa nie zostawia człowieka bez
            wyjścia. */}

        {nothing && state.folders === 'read' ? <p className={ZONE_LEAD}>{NO_SKILLS_YET}</p> : null}

        {nothing ? null : (
          <>
            {/* Czekająca stoi PIERWSZA i na całej szerokości: jest jedyną rzeczą w tej sekcji,
                która czegoś od człowieka chce, a rzecz wymagająca decyzji nie ma leżeć pod
                listą gotowych ani w kolumnie obok nich. */}
            {state.pending === null ? null : (
              <section
                data-skill={state.pending.name}
                data-tone="attend"
                /* WEJŚCIE SPRĘŻYNĄ: ta karta przychodzi po tym, jak Rust przeczytał link albo
                   model napisał tekst — nad listę, która zostaje. Panel dodawania w tej samej
                   chwili ZNIKA bez animacji: rzecz, która odchodzi, nie ma prawa ciągnąć oka
                   z rzeczy, która przyszła (DESIGN §7). */
                className="card enter mx-auto mb-6 flex max-w-160 flex-col gap-3"
              >
                {/* WYBÓR STOI TAM, GDZIE ZAPADA DECYZJA, i nad kontrolką, która ją wykonuje.
                    Cały mechanizm zakresu jest napisany i przetestowany po stronie Rusta od
                    T-18 (`Scope`, `place::plan`, `place::remove`) i do 2026-08-19 był
                    NIEOSIĄGALNY z okna: jedyny konstruktor `Roots` w produkcji miał wpisane
                    `project: None`, magazyn wysyłał stałą, a ekran nie miał ani jednej
                    kontrolki, którą dałoby się to zmienić. Umiejętność lądowała więc zawsze
                    w katalogach domowych człowieka, niezależnie od tego, w którym projekcie
                    pracował.

                    NAD KARTĄ, NIE W NIEJ. Propsy `ReviewCard` się nie zmieniają — wybór jest
                    pytaniem SEKCJI („gdzie zapisujemy"), a nie częścią przeglądu treści.

                    NAD KONTROLKĄ DODANIA, NIE POD NIĄ. Wybór miejsca zaproponowany po
                    przycisku, który tam zapisuje, nie jest wyborem: jest ostrzeżeniem
                    przeczytanym po decyzji, a ta sekcja pisze do katalogów, które przeczyta
                    każde następne uruchomienie narzędzi agentowych na tej maszynie.

                    `<fieldset>` z `<legend>`, a nie `<div>` z `<p>`: dwie pozycje radiowe są
                    jedną grupą i czytnik ekranu ma przeczytać pytanie przed odpowiedziami. */}
                <fieldset data-pick-where className="stack">
                  <legend className="label">Available in</legend>
                  <div className="flex items-center gap-4">
                    {LANDINGS.map((choice) => (
                      <label
                        key={choice.value}
                        htmlFor={`skill-landing-${choice.value}`}
                        className="flex items-center gap-1 text-ink"
                      >
                        <input
                          id={`skill-landing-${choice.value}`}
                          type="radio"
                          name="skill-landing"
                          data-landing={choice.value}
                          checked={state.landing === choice.value}
                          /* WYGASZONA, A NIE UKRYTA, i wygaszona TUTAJ, nie wariantem
                             `disabled:` Tailwinda — wariant zostawia słowo `disabled`
                             w atrybucie `class` także wtedy, gdy kontrolka działa
                             (`review-card.tsx` ma tę samą pułapkę opisaną). Bez otwartego
                             projektu nie ma korzenia, pod którym pisać, a wybór, którego Rust
                             i tak odmówi, każe człowiekowi czytać zdanie o czymś, czego nie
                             wybierał. */
                          disabled={choice.value === 'this-project' && openProject === null}
                          onChange={() => {
                            store.getState().chooseLanding(choice.value);
                          }}
                        />
                        {choice.label}
                      </label>
                    ))}
                  </div>
                </fieldset>

                {/* Odmowa czytana TAM, GDZIE ODMOWA ZACHODZI: pod pozycją, której nie da się
                    wybrać, a nie na dole ekranu. Znika, kiedy projekt jest otwarty — zdanie
                    stojące zawsze nie mówi nic i odsyła człowieka po coś, co już zrobił. */}
                {openProject === null ? (
                  <p className="lead" data-tone="attend">
                    {NO_PROJECT_YET}
                  </p>
                ) : null}

                {/* Zdanie o miejscu stoi TU, a nie w polu na link: pole zamyka się w chwili
                    wklejenia, a decyzja „dodać czy nie" jest podejmowana dopiero nad tą kartą.
                    Ostrzeżenie widoczne wcześniej niż decyzja nie jest ostrzeżeniem.

                    JEDNO ZDANIE, POLICZONE Z WYBORU. Nie dwa naraz i nie zdanie, które nie
                    zmienia się z wyborem: pierwsze łamie niezmiennik 13, drugie zamienia wybór
                    w kontrolkę bez widocznego skutku (niezmiennik 16) dokładnie tam, gdzie
                    skutkiem jest zapis do żywej konfiguracji cudzych narzędzi. */}
                <p data-where-it-goes className="lead">
                  {state.landing === 'this-project'
                    ? WHERE_IT_LANDS_IN_THE_PROJECT
                    : WHERE_IT_LANDS}
                </p>
                {/* Nazwy nie piszemy drugi raz — niesie ją nagłówek karty (niezmiennik 13). */}
                <ReviewCard
                  item={state.pending}
                  acknowledged={state.acknowledged}
                  onAcknowledge={(findingId) => {
                    store.getState().acknowledge(findingId);
                  }}
                  onAdd={() => {
                    void store.getState().add();
                  }}
                />
              </section>
            )}

            {/* Dwie kolumny, jak w makiecie (`docs/mockup/index.html`, `.grid.two`).
                2026-08-23 — OPIS JEST. Stało tu „opisu w kafelku NIE MA, bo `InstalledWire` nie
                niesie ani `summary`, ani `description`… Zgłoszone człowiekowi" — i to była
                prawda: siatka gołych nazw katalogów, po której nie dało się poznać, co
                którakolwiek z nich robi. `list_skills` czyta teraz `description` z `SKILL.md`
                jednym czytnikiem front-mattera, a kafelek składa się jak kafelek agenta:
                nazwa i znacznik, zdanie o tym, po co to jest, akcja pod kreską. */}
            {state.installed.length === 0 ? null : (
              <ul className="mx-auto grid max-w-160 grid-cols-2 gap-3">
                {state.installed.map((skill) => (
                  <li
                    key={skill.name}
                    data-skill={skill.name}
                    /* `.fade-in`, bo kafelek PRZYBYWA: lista jest pusta, dopóki katalogi nie
                       odpowiedzą. Samo `opacity` — sprężyna należy do rzeczy wchodzących NAD
                       treść, nie do wiersza, który dopiero wypełnia listę (DESIGN §7). */
                    className="card fade-in flex flex-col gap-2"
                  >
                    <div className="flex items-start gap-2">
                      {/* `text-subhead`, nie `text-heading`: ten stopień należy do nagłówka
                          panelu, a to jest tytuł kafelka — ta sama drabinka, co w Agents
                          i w Workflows (`src/styles/theme.css`). */}
                      <h2 className="min-w-0 text-subhead break-words text-ink">{skill.name}</h2>
                      {/* Znacznik pochodzenia jest TRWAŁY i przeżywa instalację [T5 §5.4]:
                          gasnący po zapisie mówiłby o umiejętności z sieci to samo, co
                          o napisanej ręcznie. */}
                      {skill.fromTheInternet ? (
                        <span className="chip ml-auto shrink-0">From the internet</span>
                      ) : null}
                    </div>

                    {/* PO CO TO JEST — drugie piętro kafelka, dokładnie tam, gdzie stoi
                        w kafelku agenta i w makiecie. Umiejętność, której plik tego nie mówi,
                        dostaje zdanie o tym, że nie mówi: pusty prostokąt czyta się jak awaria
                        wczytywania, a nie jak brak opisu (niezmiennik 17 od drugiej strony). */}
                    <p className="lead line-clamp-2">
                      {skill.summary === '' ? NO_SUMMARY : skill.summary}
                    </p>
                    {/* Jedyna droga powrotna z katalogów narzędzi agentowych — i od 2026-08-31
                        DWUSTOPNIOWA, dokładnie tak jak usunięcie agenta (`agents/index.tsx`).
                        Do tego dnia jedno naciśnięcie jechało prosto w `fs::remove_dir_all`
                        po drugiej stronie granicy: bez pytania, bez cofnięcia i bez ani jednego
                        zdania o tym, co zniknie.

                        POTWIERDZENIE JEST PRAWDZIWYM RENDEREM, nie `window.confirm`. Dialog
                        przeglądarki blokuje webview i zabiera całą pracę, a przy oknie Tauri
                        nie ma go czym odblokować.

                        MIEJSCE NAZYWA PRZYCISK, KTÓRY CZŁOWIEK NACISKA, i to jest ta sama
                        naprawa opisana w `src/state/skills.ts`: cel kasowania nie ma prawa
                        brać się z wyboru, którego w tej chwili na ekranie nie ma.

                        Magazyn po udanym usunięciu czyta katalogi JESZCZE RAZ, więc wiersz
                        znika dopiero wtedy, gdy pliku naprawdę już tam nie ma. */}
                    {/* PRZYSZŁO Z TRUNKU 2026-08-31, razem z sekcją Lab. Czasownik stoi przy
                          rzeczy, której dotyczy: zestaw ma dwie kolumny — bez tej umiejętności
                          i z nią — bo to jest całe pytanie, które da się o nią zadać. Polityka
                          mieszka w `../lab/evaluate`, nie tutaj.
                          Klasa jest prymitywem tej gałęzi (`.btn-quiet`), nie stałą `QUIET`
                          z trunku: ta stała zniknęła razem z migracją na warstwę prymitywów. */}
                    <button
                      type="button"
                      data-evaluate={skill.name}
                      className="btn-quiet"
                      onClick={() => {
                        void evaluateSkill(skill.name);
                      }}
                    >
                      Evaluate
                    </button>
                    <div className="mt-auto border-t border-line pt-2">
                      {state.removing === skill.name ? (
                        <div className="stack" data-gap="2">
                          {/* Pytanie WCHODZI: przed naciśnięciem „Remove" nie ma go w dokumencie
                              wcale, a staje tam, gdzie przed chwilą był przycisk. Sprężyna mówi
                              „to jest nowe", zamiast pozwolić dwóm rzeczom mrugnąć w jednym
                              miejscu (DESIGN §7). */}
                          <p data-confirm-remove={skill.name} className="enter text-ink">
                            {askAbout(skill.name, openProject !== null)}
                          </p>
                          <div className="flex flex-wrap items-center gap-2">
                            {PLACES.filter(
                              (place) => !place.needsProject || openProject !== null,
                            ).map((place) => (
                              <button
                                key={place.value}
                                type="button"
                                data-goes-from={place.value}
                                className="btn-danger"
                                onClick={() => {
                                  void store.getState().remove(place.value);
                                }}
                              >
                                {place.label}
                              </button>
                            ))}
                            {/* Wyjście, które nic nie rusza, stoi obok każdego, które rusza
                                wszystko. Pytanie z jedną odpowiedzią nie jest pytaniem. */}
                            <button
                              type="button"
                              data-keep-it
                              className="btn-quiet"
                              onClick={() => {
                                store.getState().keepIt();
                              }}
                            >
                              Keep it
                            </button>
                          </div>
                        </div>
                      ) : (
                        <div className="flex items-center">
                          <button
                            type="button"
                            data-remove={skill.name}
                            className="btn-danger mr-auto"
                            onClick={() => {
                              store.getState().askToRemove(skill.name);
                            }}
                          >
                            Remove
                          </button>
                        </div>
                      )}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </>
        )}
      </div>
    </section>
  );
}
