/* Kontrolka „co uruchomić i ile naraz" — jedyne miejsce w oknie, przez które da się ZACZĄĆ bieg.
 *
 * DLACZEGO TEN PLIK W OGÓLE POWSTAŁ. `src/sections/run/io.ts` eksportuje `start` i `stop` od
 * T-27 i do 2026-08-17 nie miał ani jednego produkcyjnego wołającego: jedynym miejscem w repo,
 * które go importowało, był jego własny test. Silnik był gotowy, komendy zarejestrowane,
 * a okno nie miało czym ich zawołać — czyli aplikacji nie dało się uruchomić z aplikacji.
 * To ta sama rodzina, co zaślepki w sekcjach: mechanizm wylądował, nikt go nie podłączył.
 *
 * DLACZEGO WYBÓR Z LISTY, SKORO JEST TEŻ `/run`. Bo to są dwie drogi do JEDNEJ polityki, a nie dwie
 * polityki: obie kończą się w `./launch` i obie biorą limit z `./limits/chosen`. Lista jest dla
 * człowieka, który chce zobaczyć, co ma; `/run <workflow> <co zbudować>` jest dla tego, który już
 * wie i chce przy okazji powiedzieć, co zbudować — czego lista nie umiała przyjąć w ogóle
 * (zgłoszenie właściciela 2026-08-19, `entry/entry.tsx` i `run-command.ts`).
 *
 * Lista czyta katalog przez ten sam adapter, którego używa sekcja Workflow, więc nie powstaje druga
 * odpowiedź na pytanie „jakie workflow istnieją" (niezmiennik 13).
 *
 * Nazwa pliku jedzie do Rusta, nie cały workflow: to plik na dysku jest prawdą (niezmiennik 4),
 * a kopia treści wysłana z okna byłaby drugim opisem tego samego pliku — i tym, który się
 * rozjedzie, gdy ktoś zapisze workflow między wyborem a kliknięciem.
 *
 * 2026-08-18 — STAN BIEGU CZYTAMY SAMI, I TO JEST WYMÓG, NIE GUST (T-38 AC-3, AC-4).
 * `src/sections/run/index.tsx` należy do T-29 i tego zadania nie da się zrobić przez zmianę
 * jego propsów. Ale powód jest głębszy niż podział plików: Stop i „Continue" pytają o dwa
 * różne fakty, a każdy z nich ma dokładnie jednego właściciela (niezmiennik 13) — czy coś
 * biegnie wie magazyn biegu (`RunState.workflow`), a czy bieg stoi na pytaniu do człowieka
 * wie model widoku pracy (`feed/model.ts`, pole `pinned`). Trzeci opis któregokolwiek z nich,
 * przepisany do propsa albo do własnego `useState`, rozjedzie się pierwszego dnia.
 */
import type { ReactElement } from 'react';
import { useEffect, useState, useSyncExternalStore } from 'react';

import { why } from '../../ipc/why';
import { AtOnce } from './limits/at-once';
import { atOnce as atOnceNow, setAtOnce, subscribeToAtOnce } from './limits/chosen';
import type { Choice } from './choices';
import { choiceFor, firstRunnable, toChoices } from './choices';
import { launchRun } from './launch';
import { chosenWorkflow, setChosenWorkflow, subscribeToChosenWorkflow } from './chosen-workflow';
import { requestedRun, subscribeToRequests, takeRequestedRun } from './requested';
import { list } from '../workflows/io';
import { runFeed } from './feed/live';
import type { FeedView } from './feed/model';
import { continueRun, stop } from './io';
import { useRun } from '../../state/run';

const PRIMARY = 'h-9 rounded-sm bg-accent px-4 text-ui text-bg disabled:opacity-40';
const DANGER = 'h-7 rounded-sm border border-fail-edge px-3 text-ui text-fail';
/** Kolor `--attend` odpowiada na jedno pytanie: co czeka na MOJĄ decyzję [DESIGN §3]. */
const ATTEND = 'h-7 rounded-sm border border-attend-edge px-3 text-ui text-attend';
/* Klasa domu, tak jak w pieciu sekcjach po T-48: `theme.css` ma `.field` z ta sama studnia,
 * mocnym obrysem, krojem maszynowym i `user-select: text`, bez ktorego z pola nie da sie skopiowac
 * wlasnego wpisu. Recznie opisane pole rozjezdza sie z pozostalymi przy pierwszej zmianie
 * (niezmiennik 13) — i wlasnie to sie stalo: tu obrys byl mocny, w Agents zwykly. */
const FIELD = 'field';

/**
 * Migawka magazynu biegu — TA SAMA dla okna i dla renderu statycznego.
 *
 * DLACZEGO NIE `useRun((state) => state.workflow)`. Wiązanie zustanda podaje `useSyncExternal
 * Store` jako migawkę serwerową `getInitialState()`, czyli stan SPRZED pierwszego zapisu.
 * W tej aplikacji nie ma serwera — okno renderuje się przez `createRoot`, więc produkcja tej
 * ścieżki nie dotyka — ale `renderToStaticMarkup` (jedyny renderer, jaki to repo ma w testach,
 * bo nie ma jsdom) dostaje właśnie ją. Komponent czytający magazyn tamtą drogą jest wtedy
 * NIEMOŻLIWY do sprawdzenia: pokazuje „nic nie biegnie" niezależnie od tego, co w magazynie
 * naprawdę stoi. `index.tsx` czyta tak `runFeed` i pisze przy tym dokładnie to zdanie.
 */
function runningWorkflow(): string {
  return useRun.getState().workflow;
}

/** Ta sama migawka dla okna i dla renderu statycznego; model widoku nie ma stanu serwerowego. */
function currentView(): FeedView {
  return runFeed.view;
}

export interface StartProps {
  /**
   * Gdzie ma stanąć zdanie o tym, czego nie udało się zacząć.
   *
   * 2026-08-18 — WCZEŚNIEJ TA KONTROLKA RENDEROWAŁA JE SAMA, we własnym panelu pod przyciskiem.
   * Odkąd stoi w pasku loadoutu (56 px, powód przy `StripProps.controls`), zdanie długości
   * „Nothing started: agents work inside a folder, and none is open…" nie ma tam gdzie się
   * zmieścić — a ucięte wielokropkiem przestaje mówić, co zrobić (DESIGN §8).
   *
   * Idzie więc do ekranu, który ma na to JEDEN slot pod paskiem — i to jest poprawka wobec
   * niezmiennika 13, nie ustępstwo: do tego dnia ekran Run miał DWA miejsca na „co powiedział
   * Loadout" (`data-said` w tej kontrolce i `data-screen-said` w ekranie), więc dwa zdania mogły
   * stać obok siebie i sprzeczać się o to samo.
   */
  onSaid: (said: string | null) => void;
  /**
   * Czy bieg trwa — POLE PRZYJMOWANE I ŚWIADOMIE NIECZYTANE, i to jest zapis decyzji, nie
   * przeoczenie.
   *
   * `src/sections/run/index.tsx` woła dziś `<Start running={run.workflow !== ''} />`, nie
   * należy do tego zadania i nie wolno go tknąć, więc usunięcie tego pola wywróciłoby
   * kompilację cudzego pliku. Czytanie go byłoby jednak gorsze: „czy coś biegnie" ma jednego
   * właściciela (`RunState.workflow`, niezmiennik 13), a komponent biorący tę odpowiedź
   * z propsa robi z siebie drugie miejsce, w którym ona mieszka — i wtedy test sprawdzający
   * ścieżkę magazynową sprawdza inną drogę niż ta, którą chodzi aplikacja. Zdanie z kontraktu
   * T-38 jest tu dosłowne: Stop **czyta stan sam, zamiast dostawać go propsem**.
   *
   * Wartość, którą tamten ekran podaje, jest dziś CO DO ZNAKU tą, którą ten komponent czyta
   * z magazynu (`useSyncExternalStore(useRun.subscribe, …)` po obu stronach), więc pominięcie
   * jej niczego nie zmienia w oknie. Pole znika w dniu, w którym `index.tsx` da się dotknąć.
   */
  running?: boolean;
}

export function Start({ onSaid }: StartProps): ReactElement {
  const [choices, setChoices] = useState<readonly Choice[]>([]);
  /* WYBÓR MIESZKA W MODULE, NIE W TYM KOMPONENCIE, i to jest ten sam ruch, co przy „ile naraz"
   * o kilka linii niżej — z tego samego powodu i po tym samym zgłoszeniu. Od 2026-08-19 wiersz
   * wejścia przyjmuje prozę bez ukośnika („or just say what you want" z makiety) i musi wtedy
   * uruchomić TEN workflow, który człowiek widzi wybrany. Stan zamknięty tutaj zmuszałby go do
   * zgadywania, a zgadnięty bieg kosztuje pieniądze, nie render. */
  const picked = useSyncExternalStore(subscribeToChosenWorkflow, chosenWorkflow, chosenWorkflow);
  const setPicked = setChosenWorkflow;
  /* Zdanie nie mieszka tutaj — jedzie do ekranu (patrz `StartProps.onSaid`). Nazwa lokalna
   * zostaje, żeby ciała handlerów niżej czytały się tak samo jak przedtem. */
  const setSaid = onSaid;

  /* „ILE NARAZ" MIESZKA W MODULE, NIE W TYM KOMPONENCIE, i to nie jest kosmetyka. Ta liczba jest
   * stanem CAŁEJ aplikacji (jedna pula na okno, niezmiennik 11), a pasek kart musi ją pokazać
   * w zdaniu „N of M slots in use" — do 2026-08-18 dostawał tam zaszyte `atOnce={0}`, bo liczba
   * była zamknięta w `useState` tej kontrolki i nikt poza nią nie miał jak jej przeczytać. */
  const atOnce = useSyncExternalStore(subscribeToAtOnce, atOnceNow, atOnceNow);

  const workflow = useSyncExternalStore(useRun.subscribe, runningWorkflow, runningWorkflow);
  const view = useSyncExternalStore(runFeed.subscribe, currentView, currentView);

  /* Bieg trwa dokładnie wtedy, kiedy magazyn zna jego workflow — ta sama prawda, z której żyje
   * pasek loadoutu, i jedyna, jakiej ten komponent słucha (patrz `StartProps.running`). */
  const busy = workflow !== '';

  /* Bieg STOI dokładnie wtedy, kiedy tak mówi `parked`, i to jest naprawa, nie kosmetyka.
   *
   * 2026-08-18 — stało tu `view.pinned !== null`, czyli „czeka nieodpowiedziane pytanie".
   * `answer()` zdejmuje przypięcie, więc odpowiedź na pytanie ODMONTOWYWAŁA jedyną kontrolkę
   * wołającą `continue_run` — i bieg parkował na zawsze, bo po stronie Rusta
   * (`commands::run::wait_for_a_person`) stoi on dalej, dopóki nie podbije się licznik zgód.
   * To są dwa różne fakty i model widoku ma na nie dwa pola (`feed/model.ts`, komentarz przy
   * `parked`): pytanie bez odpowiedzi rysuje blok z opcjami, a `parked` mówi „nie ruszy, dopóki
   * go nie puścisz". Kontrolka „dalej" żyje z drugiego. */
  const atCheckpoint = view.parked;

  /* Katalog czytamy przy wejściu na sekcję. Pliki są prawdą, a ekran jest ich widokiem —
   * lista trzymana w pamięci między wejściami pokazywałaby workflow skasowany obok. */
  useEffect(() => {
    let alive = true;
    list()
      .then((entries) => {
        if (!alive) return;
        setChoices(toChoices(entries));
      })
      .catch((error: unknown) => {
        if (!alive) return;
        /* Odmowa Rusta jest już napisana po ludzku; własne zdanie dokładamy tylko wtedy,
         * gdy jego nie ma — cicha porażka czyta się jak pusty katalog. Wyjęcie zdania
         * z odmowy mieszka w `src/ipc/why.ts`: Tauri odrzuca NAPISEM, nie `Error`, więc
         * warunek `instanceof Error` stojący tu do 2026-08-18 był zawsze fałszywy. */
        setSaid(why(error, 'Loadout could not read the workflows folder.'));
      });
    return () => {
      alive = false;
    };
  }, []);

  /* DOMYŚLNY WYBÓR TO PIERWSZY WORKFLOW, KTÓRY MA KROKI — nie pierwszy z listy.
   *
   * 2026-08-18 — stało tu `choices[0]?.path`, a lista przychodzi posortowana BAJTOWO
   * (`commands/workflows.rs`, `paths.sort()`). `new-workflow-2.json` (znak `-`, 0x2D) wypada
   * przed `new-workflow.json` (znak `.`, 0x2E) — a ten pierwszy miał `"steps": []`. Skutek dla
   * człowieka: klikał Run na workflow z dwoma krokami, na tym ekranie stało „New workflow 2",
   * a Start odpowiadał „There are no steps yet." o czymś, co przed chwilą miało dwa kroki.
   * Ironia jest zapisana w `docs/STATUS.md:19`, który używa właśnie tego pliku jako dowodu,
   * że aplikacja nie jest atrapą.
   *
   * Pusty napis, kiedy ŻADEN workflow nie ma kroków: wtedy nie ma czego wybrać, a Start jest
   * wygaszony. Wybór, który z definicji odmówi, jest gorszy niż brak wyboru. */
  const chosen = picked === '' ? (firstRunnable(choices)?.path ?? '') : picked;

  async function go(): Promise<void> {
    setSaid(null);
    /* CAŁA POLITYKA STARTU MIESZKA W `./launch`, i to jest wymóg: ten sam skutek musi dać
     * przycisk Start tutaj i zielony `Run` w edytorze workflow (przez `./requested`). Dwie kopie
     * czterech decyzji — który plik, ile naraz, w jakim folderze, co powiedzieć przy odmowie —
     * rozjechałyby się na tej o folderze (niezmiennik 23). */
    setSaid(await launchRun(choiceFor(choices, chosen), atOnce));
  }

  /* ŻĄDANIE Z EDYTORA WORKFLOW. Zielony `Run` w edytorze wołał do 2026-08-18 samo przejście na
   * tę sekcję i wyrzucał ścieżkę pliku (`workflows/index.tsx`), więc nic nie startowało i nic
   * tego nie mówiło. Teraz mówi WPROST, który plik, a odbiera to ten efekt.
   *
   * Czeka na katalog: żądanie przychodzi w tej samej chwili, w której sekcja się montuje, a lista
   * workflow jeszcze się czyta. Zdjęcie żądania przed czasem zamieniłoby je w `GONE_FROM_DISK` —
   * zdanie o pliku, którego nie ma, wypowiedziane o pliku, który jest.
   *
   * `takeRequestedRun` ZDEJMUJE żądanie i to jest cała ochrona przed drugim biegiem: żądanie
   * zostawione w module startowałoby bieg przy każdym powrocie na ten ekran, a to jest klasa
   * błędu, która kosztuje pieniądze, nie render. */
  const asked = useSyncExternalStore(subscribeToRequests, requestedRun, requestedRun);
  useEffect(() => {
    if (asked === null || choices.length === 0) return;
    const taken = takeRequestedRun();
    if (taken === null) return;
    setPicked(taken.path);
    setSaid(null);
    void launchRun(choiceFor(choices, taken.path), atOnceNow()).then(setSaid);
  }, [asked, choices]);

  async function carryOn(): Promise<void> {
    setSaid(null);
    try {
      /* ZDANIE CZŁOWIEKA JEDZIE RAZEM ZE ZGODĄ — i ta linia jest jedynym miejscem, w którym
       * kolejka wysyłkowa modelu (`FeedView.toCarry`) spotyka kształt drutu. Tak mówi komentarz
       * przy tamtym polu: model nie zna `Option<String>` (niezmiennik 23), więc przełożenie
       * pustego napisu na `null` należy do sekcji. Bez tego człowiek pisał zdanie w karcie
       * „Needs your answer", a agent po drugiej stronie nie dostawał z niego ani litery —
       * kontrolka przyjmująca tekst i wyrzucająca go jest gorsza niż jej brak (niezmiennik 16). */
      await continueRun(view.toCarry === '' ? null : view.toCarry);
      /* BIEG JUŻ NIE STOI, więc kontrolka „dalej" ma zniknąć razem z kolejką wysyłkową. Dopiero
       * po powrocie komendy: `continue_run` wraca z dowodem, że bieg NAPRAWDĘ ruszył
       * (`wait_until_moving`), a zgaszenie tego stanu wcześniej pokazywałoby bieg w drodze
       * w chwili, w której on dalej stoi. Odmowa nie gasi nic — jest dalej co puścić. */
      runFeed.carriedOn();
    } catch (error: unknown) {
      setSaid(why(error, 'Loadout could not let that run carry on.'));
    }
  }

  async function halt(): Promise<void> {
    setSaid(null);
    try {
      await stop();
    } catch (error: unknown) {
      setSaid(why(error, 'Loadout could not stop the run.'));
    }
  }

  return (
    /* JEDEN WIERSZ, bez panelu i bez paddingu: ta kontrolka stoi teraz w prawej grupie paska
     * loadoutu, więc własne tło i obramowanie rysowałyby panel w panelu. */
    <div className="flex min-w-0 items-center gap-2">
      <select
        aria-label="Workflow to run"
        className={FIELD}
        value={chosen}
        disabled={busy || choices.length === 0}
        onChange={(event) => {
          setPicked(event.target.value);
        }}
      >
        {choices.length === 0 ? (
          <option value="">No workflows saved yet</option>
        ) : (
          choices.map((choice) => (
            <option key={choice.path} value={choice.path}>
              {choice.name}
            </option>
          ))
        )}
      </select>

      {/* Kontrolka „dalej" istnieje DOKŁADNIE wtedy, kiedy ma co puścić (niezmiennik 16).
            Wersja stale obecna i wyszarzona obiecuje sterowanie, którego nie ma, a wersja
            wyrenderowana bez zaparkowanego biegu woła `continue_run` w próżnię: Rust podbija
            wtedy licznik zgód i NASTĘPNY punkt kontrolny przelatuje bez pytania. */}
      {atCheckpoint ? (
        <button type="button" className={ATTEND} onClick={() => void carryOn()}>
          Continue
        </button>
      ) : null}

      {busy ? (
        <button type="button" className={DANGER} onClick={() => void halt()}>
          Stop
        </button>
      ) : (
        <button
          type="button"
          className={PRIMARY}
          disabled={chosen === ''}
          onClick={() => void go()}
        >
          Start
        </button>
      )}

      {/* Limit siedzi obok Startu, a nie w ustawieniach: to decyzja podejmowana przy każdym
       * biegu, bo zależy od tego, co jeszcze chodzi na tej maszynie. */}
      {/* WYGASZONY W TRAKCIE BIEGU, z podpisanym powodem. Do 2026-08-18 kontrolka była czynna
          zawsze: przesunięcie z 3 na 8 zmieniało liczbę na ekranie i ostrzeżenie o pamięci,
          a biegło dalej trzech — `atOnce` czyta się tylko przy starcie, `Limiter::new` powstaje
          raz, i żadna z komend nie zmienia limitu w trakcie. Kontrolka, która przyjmuje zmianę
          i jej nie wykonuje, jest gorsza niż wygaszona (niezmiennik 16). */}
      <AtOnce
        value={atOnce}
        onChange={setAtOnce}
        disabled={
          busy ? 'This run already started with its own limit. Stop it to change this.' : null
        }
      />
    </div>
  );
}
