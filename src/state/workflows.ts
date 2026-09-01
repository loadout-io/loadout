/* Otwarty dokument workflow: `WorkflowFile` w pamięci plus akcje, które go zmieniają.
 *
 * Ten plik nie zna ANI JEDNEJ nazwy komendy Rusta. Zna je jedno miejsce w sekcji —
 * `src/sections/workflows/io.ts` — i to ono wstrzykuje tu `WorkflowIo` (niezmiennik 23),
 * dokładnie tak jak `AgentsIo` w `src/state/agents.ts`. Test wstrzykuje atrapę zamiast mockować
 * transport, którego magazyn i tak nie widzi.
 *
 * 2026-08-18 — JEDEN import z `../ipc` jednak tu jest i to nie jest złamanie zdania wyżej:
 * `ipc/why.ts` nie niesie żadnej nazwy komendy, tylko odpowiedź na pytanie „czym Tauri odrzuciło
 * to wywołanie". Tauri odrzuca NAPISEM, więc `error instanceof Error` jest zawsze fałszywe i każda
 * precyzyjna odmowa Rusta ginęła. Druga kopia tego rozpakowania, wpisana tutaj lokalnie, byłaby
 * ósmym miejscem, dla którego `why` w ogóle powstało.
 *
 * Dlaczego `saveAgent` siedzi w `WorkflowIo`, choć to sekcja workflow: panel kroku ma w liście
 * „Who does this" pozycję `＋ Create a new agent…` (`docs/mockup/index.html:603`), więc ta sekcja
 * NAPRAWDĘ umie zapisać plik agenta. To jest jedyny powód, dla którego zdanie „edycja kroku nie
 * dotyka agenta" da się w ogóle udowodnić: `expect(io.saveAgent).not.toHaveBeenCalled()` na
 * funkcji, której w interfejsie nie ma, nie dowodzi niczego.
 *
 * Typy niżej są lustrem `src-tauri/src/workflow/mod.rs` — tak samo jak typy w
 * `src/state/agents.ts` są lustrem `src-tauri/src/library/agents.rs`. Dopóki nie ma generatora
 * (`ts-rs` albo `specta`, T3 §3.2), obie kopie stoją obok siebie i rozjazd łapie recenzja.
 *
 * Czego tu świadomie NIE MA: cofnij/ponów. PLAN §7 stawia je w v1.1, a TASK.md mówi „magazyn ma
 * zostawić na to miejsce, nie implementować" — tym miejscem jest `commit`, jedyna droga, którą
 * nowy dokument wchodzi do stanu.
 */
import { create } from 'zustand';
import { why } from '../ipc/why';
import { applyPanelEdit, withoutOverride } from '../sections/workflows/step-panel/overrides';
import type { Agent, FileAccess } from './agents';

/** Waga uwagi z walidatora Rusta. `Problem` blokuje Run, `Warning` nie blokuje niczego. */
export type Level = 'problem' | 'warning';

/** Jedna uwaga o jednym defekcie — lustro `workflow::check::Note`.
 *
 * `message` idzie WPROST na ekran: to jest gotowe angielskie zdanie, nie klucz i nie kod. */
/** Naprawa, którą Loadout umie wykonać sam — lustro `workflow::roster::Fix`.
 *
 * Wariant istnieje wyłącznie tam, gdzie naprawa jest JEDNOZNACZNA. Uwaga bez `fix` to uwaga,
 * której naprawę wybiera człowiek — i tak zostaje wszystko, co walidator liczy z samego pliku:
 * kształt grafu poprawia się przeciągnięciem strzałki, nie przyciskiem. */
export type Fix =
  | { kind: 'widenFileAccess'; step: string; to: FileAccess; from: FileAccess }
  | { kind: 'dropTools'; agent: string; agentName: string; tools: string[] }
  /** Ten krok ma pracować we własnej kopii plików. Lustro `roster::Fix::GiveItAFreshCopy`. */
  | { kind: 'giveItAFreshCopy'; step: string };

export interface Note {
  level: Level;
  /** Krok, na którym ląduje kropka. `null`, kiedy uwaga dotyczy całego pliku. */
  stepId: string | null;
  message: string;
  /** Naprawa jednym kliknięciem, o ile Loadout wie, co dokładnie przestawić. */
  fix?: Fix;
}

/** Pozycja kafelka. Zapisywana zawsze jako całkowita wielokrotność [`GRID`]. */
export interface Point {
  x: number;
  y: number;
}

/** Skok siatki w pikselach [T3 §8.2 reguła 1]. Ta sama liczba co `workflow::GRID`. */
export const GRID = 24;

/** Strzałka. Bez portów i bez danych — znaczy „po" (T3 §3.1).
 *
 * JEDEN WYJĄTEK OD „BEZ WARUNKU": `maxTurns`. Strzałka z tą liczbą jest POWROTEM — wraca do
 * kroku, który już był, i zamyka pętlę o suficie zapisanym z góry. Lustro `workflow::Link`
 * z Rusta; projekt w `docs/superpowers/specs/2026-08-19-petla-z-limitem-tur-design.md`.
 */
export interface Link {
  from: string;
  to: string;
  /**
   * Ile razy ta strzałka może zawrócić bieg, 1–10. Brak pola znaczy „zwykłe po".
   *
   * NAZWA JEST `max_turns`, NIE `maxTurns`, i to nie jest przeoczenie: `workflow::Link` w Ruście
   * **nie ma** `#[serde(rename_all = "camelCase")]` — w odróżnieniu od kroku, który go ma. Klucz
   * jedzie więc przez granicę dosłownie tak, jak stoi w pliku. Przepisanie go tutaj na `maxTurns`
   * dałoby pole, które okno wypełnia, a Rust ignoruje — czyli kontrolkę bez skutku
   * (niezmiennik 16), i to taką, której nikt nie zauważy, bo plik dalej się zapisuje.
   *
   * `Link` nie ma po tamtej stronie `#[serde(flatten)] extra`, więc nie ma tu żadnej siatki
   * bezpieczeństwa: klucz, którego okno nie przewiezie, po prostu przestaje istnieć.
   */
  max_turns?: number;
}

/** Gdzie krok pracuje. Lustro `workflow::Folder`.
 *
 * 2026-08-23 — `same-copy` DOSZŁO DO TEJ UNII, choć w pliku istniało od dawna: plik workflow
 * z tą wartością przechodził przez okno jako klucz, którego okno nie zna (niezmiennik 5), więc
 * żadna kontrolka nie umiała go ustawić ani pokazać. Wypłynęło na kafelku „uruchom i zostaw",
 * gdzie ta wartość jest CAŁYM sensem kroku: serwer podnoszony w folderze projektu podaje kod
 * BEZ pracy, którą krok przed nim właśnie napisał — a strona, która się otwiera i pokazuje starą
 * wersję, wygląda na działającą. */
export type Folder =
  | { use: 'project' }
  | { use: 'fresh-copy' }
  /** To samo drzewo, w którym pracował krok przede mną. */
  | { use: 'same-copy' }
  | { use: 'pick'; path: string };

/** `'all'` albo lista nazw. Lustro `workflow::Skills`. */
export type Skills = 'all' | string[];

/** Co kontrolka Skills umie zapisać. `'none'` istnieje tylko w trybie `all-or-none`. */
export type SkillChoice = 'all' | 'none' | { only: string[] };

/** Co kafelek bierze z repozytorium, w którym pracuje bieg — trzy półki `<projekt>/.claude/`.
 *
 * Ten sam kształt, co `AgentStep::borrow` po stronie Rusta, klucz w klucz. Pole nieobecne znaczy
 * „nie pożyczam", a nie „pożyczam nic": plik zapisany, zanim ten wiersz istniał, ma się otworzyć
 * bez jednej zmiany, a pusty obiekt dopisany do każdego kroku przepisałby przy pierwszym zapisie
 * wszystkie workflow, jakie leżą na dysku.
 *
 * `agent`, nie `subagent`, bo tak nazywa się półka, z której pochodzi (`.claude/agents/`). */
export interface Borrow {
  skills?: string[] | undefined;
  learnings?: string | undefined;
  agent?: string | undefined;
}

/** Co komenda `list_host_material` znalazła w folderze aktywnego workspace.
 *
 * Trzy listy nazw i ani jednego bajtu treści: wiersz wyboru pokazuje, co MOŻNA wziąć, a treść
 * czyta dopiero bieg. Pusty wynik jest normalną odpowiedzią o cudzym repozytorium, nie awarią. */
export interface HostMaterial {
  skills: string[];
  learnings: string[];
  subagents: string[];
}

export interface HandoverField {
  name: string;
  describe: string;
  required?: boolean;
}

export type Handover = 'notes' | { fields: HandoverField[] };

/** Dziewięć pól, które krok może zmienić wobec agenta — lustro `OVERRIDABLE` z T-11.
 *
 * Czego tu nie ma: `id`, `name` i `runsWith`. Krok, który przestawia vendora, unieważnia
 * połowę reszty [T4 §6.4]. */
export type OverridableField =
  | 'instructions'
  | 'model'
  | 'thinking'
  | 'fileAccess'
  | 'giveUpAfterMinutes'
  | 'tools'
  | 'skills'
  | 'connections'
  | 'writeResultsTo';

/** Patch RFC 7396 nad definicją agenta: brak klucza znaczy „weź z agenta" [T4 §5.1].
 *
 * `{}` dla kroku nietkniętego — i to `{}` niesie informację, więc nie znika przy zapisie. */
export type Overrides = Partial<Pick<Agent, OverridableField>>;

/** Co ma się stać z robotą, kiedy ten krok nie przejdzie.
 *
 * 2026-08-23, zamówienie właściciela wprost: „workflows zawsze ma mieć opcje kontynuacji a nie
 * ślepe punkty". Do tego dnia każda porażka kasowała cały stożek potomków — bez zdania i bez
 * wyboru. Lustro `workflow::WhenItFails` z Rusta, wartość w wartość.
 *
 * BRAK POLA ZNACZY `carry-on` — decyzja właściciela z tego samego dnia, o jeden krok dalej niż
 * samo pole: „wiesz co to w sumie carry on powinno być domyślnie". Pierwsza wersja miała tu
 * `stop`, żeby pliki zapisane wcześniej biegły identycznie; to było prawdziwe i nie o to chodziło
 * — zgodność wsteczna zachowywała dokładnie ten stan, który był awarią. Powód, dla którego
 * przepuszczanie wolno postawić domyślnym, stoi przy `WhenItFails` w Ruście: krok zostaje
 * czerwony, a następny dowiaduje się, że materiał nie przeszedł. Pole zostaje opcjonalne, więc
 * plik zapisany po tej zmianie nie nosi `"carry-on"` w każdym kroku. */
export type WhenItFails = 'stop' | 'carry-on' | 'ask-me';

/** Krok, który uruchamia agenta.
 *
 * Vendora ani modelu tu nie ma: krok nazywa AGENTA, a vendor, model i narzędzia mieszkają
 * w jego definicji (T3 §3.1). Zmiana modelu dzieje się raz, nie w sześciu kafelkach. */
export interface AgentStep {
  kind: 'agent';
  id: string;
  name: string;
  /** Id zapisanego agenta (`src/state/agents.ts`). */
  agent: string;
  overrides: Overrides;
  /** Przelotka na opcje vendora. Loadout nie interpretuje jej zawartości. */
  vendorOptions?: Record<string, Record<string, string>>;
  /** Ile identycznych kopii naraz, 1–8 [T3 §4.4]. */
  copies: number;
  /** Prompt kroku, zwykły tekst. To NIE jest `Overrides.instructions`, które dotyczy agenta. */
  instructions: string;
  skills: Skills;
  /** Co ten kafelek pożycza z repozytorium, w którym pracuje bieg. Brak klucza znaczy „nic".
   *
   * `| undefined` JAWNIE, bo `exactOptionalPropertyTypes` odróżnia „klucza nie ma" od „klucz
   * jest i niesie `undefined`" — a odznaczenie ostatniego pola musi umieć podać to drugie:
   * `JSON.stringify` zdejmuje wtedy klucz i plik wraca do kształtu sprzed tego pola, co do
   * bajtu. Bez tego jedyną drogą byłoby zapisanie `{}`, czyli wiersza szumu w każdym kroku,
   * który kiedykolwiek czegoś dotknął. */
  borrow?: Borrow | undefined;
  folder: Folder;
  handover: Handover;
  /** Co zrobić z robotą, kiedy ten krok nie przejdzie. Brak znaczy `carry-on`. */
  whenItFails?: WhenItFails;
  at: Point;
}

/** Krok, który zatrzymuje bieg i pyta człowieka [T3 §6.1 punkt 5]. */
export interface CheckpointStep {
  kind: 'checkpoint';
  id: string;
  name: string;
  question?: string;
  at: Point;
}

/** Krok, który uruchamia polecenie i **idzie dalej, nie czekając na jego koniec**.
 *
 * 2026-08-23, prośba właściciela wprost („uruchom i zostaw"), po jego biegu na `urc-monorepo`.
 * Zderzają się tam dwie POPRAWNE reguły: proces poboczny nie ma prawa przeżyć swojego kroku
 * (niezmiennik 6), a sprawdzenie żywej aplikacji wymaga, żeby przeżył. Do tego dnia jedynym
 * kafelkiem, który cokolwiek uruchamiał, był „sprawdź" — a ten CZEKA na koniec komendy, więc
 * `npm run dev` wisiał w nim do limitu i meldował porażkę. Kafelek agenta nie jest tu odpowiedzią
 * z tego samego powodu: agent, któremu każe się odpalić serwer, siedzi w nim całą swoją turę.
 *
 * D6 („nie powtarzamy funkcji vendorów") tego nie dotyczy: żaden vendor nie ma czegoś takiego.
 * Właścicielem procesu jest tu Loadout — on go podnosi i on go ubija na koniec biegu.
 */
export interface ServeStep {
  kind: 'serve';
  id: string;
  name: string;
  /**
   * Wiersz powłoki, jedna linia.
   *
   * Pusty znaczy „jeszcze niewypełniony" i bieg wtedy odmówi — **chyba że** stoi obok
   * [`ServeStep.commandFrom`], bo wtedy komendę oddaje krok przed tym.
   */
  command: string;
  /**
   * Skąd wziąć komendę, kiedy nie wpisał jej człowiek.
   *
   * Zamówienie właściciela 2026-08-30: „agent sam ma rozkminić jakie komendy użyć do odpalenia,
   * my nie ingerujemy bo nie chcę w każdym projekcie osobno wpisywać na front i backend command".
   * Wiersz uruchamiający aplikację jest inny w każdym repo, a wpisany ręcznie zamienia jeden
   * wielokrotnego użytku plik w plik na jeden projekt.
   *
   * Lustro `workflow::ServeStep::command_from`. Nieobecne, kiedy komendę wpisał człowiek — po
   * tamtej stronie znika przy zapisie (`skip_serializing_if`), więc klucz dopisany tu na siłę
   * rozjeżdżałby plik z tym, co Rust naprawdę zapisuje.
   */
  commandFrom?: { field: string } | undefined;
  folder: Folder;
  at: Point;
}

/** Krok, który uruchamia komendę należącą do Loadouta i SAM wystawia wynik — z kodu wyjścia
 * plus dowodu w wyjściu, a nie ze zdania agenta [D6, „Trzeci rodzaj: sprawdź"].
 *
 * Lustro `workflow::CheckStep` z Rusta, pole w pole. Tamta strona ma to w całości od T-23:
 * walidator odmawia zapisu bez `proof` (`check::a_command_step_left_empty`), sterownik liczy
 * `passed = kod wyjścia 0 ORAZ wyjście pasuje do wzorca`, a wynik jedzie do tras warunkowych
 * i do pętli.
 *
 * 2026-08-23 — POLA WCHODZĄ DO TEJ UNII, choć w plikach istniały od dawna. Do tego dnia okno
 * pomijało ten rodzaj świadomie („przychodzi wyłącznie z zaimportowanych plików"), więc każdy
 * kafelek sprawdzenia jechał przez okno jako klucz, którego okno nie zna (niezmiennik 5):
 * płótno go rysowało, ale `proof` nie dało się wpisać nigdzie, a klik w kafelek wpadał
 * w „wybierz agenta". Skutek jest większy, niż wygląda — bez tego kafelka KAŻDA pętla, jaką
 * człowiek zbuduje, jest pętlą „co agent powiedział", a rozróżnienie z `FOUNDATIONS.md` §2.1
 * nie ma na płótnie żadnego nośnika. */
export interface CheckStep {
  kind: 'check';
  id: string;
  name: string;
  /** Wiersz powłoki, jedna linia. Pusty znaczy „jeszcze niewypełniony" i zapis wtedy odmawia. */
  command: string;
  /**
   * Po czym poznać, że komenda naprawdę pobiegła: zwykły tekst z JEDNYM metaznakiem — `(\d+)`
   * znaczy „co najmniej jedna cyfra". Ta sama notacja, którą człowiek pisze w linii `expect:`
   * naszej własnej bramki (`AGENTS.md` §2a punkt 4) — jedna notacja, jedno znaczenie.
   *
   * Pusty jest ODMOWĄ ZAPISU po stronie Rusta, i to jest niezmiennik 19: bez dowodu werdykt
   * liczyłby się z samego kodu wyjścia, a suita, która nie uruchomiła ani jednego testu,
   * wychodzi zerem.
   */
  proof: string;
  /** Gdzie ta komenda biegnie. `cargo test` pisze po `target/`, więc to NIE jest krok tylko
   * do odczytu i reguła kolizji z niezmiennika 12 obowiązuje go tak samo jak agenta. */
  folder: Folder;
  /** Co zrobić z robotą, kiedy to sprawdzenie nie przejdzie. Brak znaczy `carry-on`. */
  whenItFails?: WhenItFails;
  at: Point;
}

/** Cztery rodzaje kafelka, które edytor zna. */
export type Step = AgentStep | CheckpointStep | CheckStep | ServeStep;

export interface WorkflowFile {
  format: 1;
  id: string;
  name: string;
  description?: string;
  /** Kolejność WSTAWIANIA, nigdy przesortowana [T3 §8.2 reguła 2]. */
  steps: Step[];
  links: Link[];
}

/** Wszystko, co magazyn robi poza swoją głową. Jedna atrapa w teście zastępuje całość. */
export interface WorkflowIo {
  /**
   * Zapis pliku workflow; oddaje rewizję, którą plik ma po zapisie. Odmowa przy problemie żyje
   * po stronie Rusta (`workflow::file::save`) — także ta o pliku zmienionym pod oknem.
   *
   * `expectedRevision` to rewizja, którą to okno CZYTAŁO. `null` znaczy „tego pliku ma jeszcze
   * nie być" i jest tu wyłącznie stanem początkowym magazynu, który nigdy nie dostał rewizji.
   */
  save(file: WorkflowFile, expectedRevision: string | null): Promise<string>;
  /** Uwagi z walidatora Rusta (T-12). Frontend ich nie liczy i nie tłumaczy. */
  check(file: WorkflowFile): Promise<Note[]>;
  /** Zapis pliku AGENTA — patrz nagłówek pliku. Edycja kroku nie ma prawa tego zawołać. */
  saveAgent(agent: Agent): Promise<void>;
}

export interface WorkflowState {
  /** Otwarty dokument. Magazyn bez dokumentu nie ma sensu, więc nie ma tu `null`. */
  document: WorkflowFile;
  /** Ostatnie uwagi z Rusta. Frontend ich nie wymyśla. */
  notes: Note[];
  /**
   * Dokument, o którym WIEMY, że leży na dysku — po referencji, nie po treści.
   *
   * Ziarno jest dokumentem otwierającym, bo edytor dostał go z dysku. Porównanie referencji
   * z `document` odpowiada więc na pytanie „czy ekran to plik" bez liczenia czegokolwiek
   * i bez zegara, który po pięciu minutach zamienia „saved just now" w nieprawdę.
   */
  savedDocument: WorkflowFile;
  /**
   * Zdanie odmowy z OSTATNIEGO zapisu, albo `null`, kiedy ostatni zapis się udał.
   *
   * 2026-08-18 — PO CO TO POLE ISTNIEJE, zmierzone na dysku właściciela. `save_workflow` odmawia
   * PRZED `fs::write` (`workflow::file::save`), a autosave wołał `void get().saveNow()` bez
   * `catch`, z komentarzem, że odrzucenie „trafia do globalnej obsługi błędów okna" — a takiej
   * obsługi w tej aplikacji nie ma. Skutek: płótno pokazywało krok, którego w pliku nie było,
   * i nic o tym nie mówiło. Plik właściciela ma `s_1` i `s_3`, bez `s_2`.
   */
  couldNotSave: string | null;
  /** Jedyna droga, którą nowy dokument wchodzi do stanu — i miejsce na stos cofnij/ponów. */
  commit: (next: WorkflowFile) => void;
  /**
   * Nowa nazwa workflow. Jedzie przez `commit`, więc autosave zabiera ją na dysk.
   *
   * 2026-08-18 — do tego dnia nazwy nie dało się zmienić z okna wcale (`editor.tsx` rysował
   * `<h1>{name}</h1>`), więc na dysku właściciela leżały „New workflow" i „New workflow 2",
   * a Run startował ten z nich, który wypadał pierwszy w sortowaniu bajtowym.
   */
  rename: (name: string) => void;
  /** Odświeża uwagi. Wołane po zapisie i przed Run. */
  recheck: () => Promise<void>;
  /** Zapisuje otwarty dokument i odświeża uwagi. Odrzucenie jest widoczne dla wołającego. */
  saveNow: () => Promise<void>;
  /** Zmiana wiersza panelu, wyrażona wartościami EFEKTYWNYMI. Różnicę liczy `applyPanelEdit`. */
  editStep: (stepId: string, agent: Agent, edit: Overrides) => void;
  /** `Reset` przy jednym wierszu: kasuje jeden klucz patcha i tylko jeden. */
  resetRow: (stepId: string, field: OverridableField) => void;
  chooseSkills: (stepId: string, choice: SkillChoice) => void;
  /** Wykonuje naprawę z uwagi i odświeża listę uwag.
   *
   * 2026-08-22 — TO JEST CAŁY AUTO-FIX. Dwa warianty trafiają w dwa różne miejsca i dlatego
   * `Fix` jest unią, a nie jednym kształtem: dial jest nadpisaniem KROKU (naprawa dotyczy tego
   * kafelka i nie rusza tej samej roli w pięciu innych workflow), a lista narzędzi należy do
   * AGENTA. Biblioteka przychodzi argumentem, tą samą drogą co przy `editStep`: magazyn
   * dokumentu jej nie trzyma i trzymać nie powinien. */
  applyFix: (fix: Fix, agents: readonly Agent[]) => Promise<void>;
}

/** Ile ciszy po ostatniej zmianie czekamy z autosave'em [T3 §9, „MVP ships" punkt 6].
 *
 * Zapis na każdą zmianę osobno to jeden plik na literę wpisaną w nazwie kroku i jedno
 * przejście walidatora Rusta na każdy z nich. Zapis dopiero przy zamknięciu ekranu to plik,
 * który nie odzwierciedla ekranu przez cały czas pracy. 400 ms jest krótsze niż przerwa,
 * po której człowiek patrzy na wynik, i dłuższe niż przerwa między dwoma znakami. */
const AUTOSAVE_MS = 400;

/** Zdanie na wypadek odmowy, która nic nie powiedziała — zerwany kanał, wyjątek z `@tauri-apps`.
 *
 * Mówi, CO się nie udało, i mówi, że plik został nietknięty. „Something went wrong" w miejscu,
 * w którym znamy czynność, jest gorsze niż brak zdania: człowiek nie wie nawet, czego szukać. */
const COULD_NOT_SAVE = 'This workflow was not saved, so the file on disk is still the older one.';

/** Magazyn otwartego dokumentu.
 *
 * Drugi argument jest wymagany, bo „otwarty dokument" bez dokumentu to stan, którego ten ekran
 * nie ma — listę plików workflow i ich otwieranie posiada T-14. */
/** Dokument z podmienionym JEDNYM krokiem rodzaju `agent`.
 *
 * Kroki, których to nie dotyczy, zostają tymi samymi obiektami — nie kopiami — więc porównanie
 * referencji w Reakcie dalej mówi prawdę o tym, co się zmieniło. Punkt kontrolny nie ma
 * nadpisań ani umiejętności i przechodzi tędy nietknięty. */
function withAgentStep(
  file: WorkflowFile,
  stepId: string,
  edit: (step: AgentStep) => AgentStep,
): WorkflowFile {
  return {
    ...file,
    steps: file.steps.map((step) =>
      step.id === stepId && step.kind === 'agent' ? edit(step) : step,
    ),
  };
}

export function createWorkflowStore(
  io: WorkflowIo,
  open: WorkflowFile,
  openRevision: string | null = null,
) {
  /* Odliczanie autosave'u, w DOMKNIĘCIU, a nie w stanie magazynu.
   *
   * Uchwyt timera nie jest faktem o dokumencie: w stanie zustanda przerysowywałby płótno na
   * każde tyknięcie księgowania debounce'u, wjechałby do każdej migawki stosu cofnij/ponów
   * (PLAN §7, v1.1) i do każdego porównania „czy to, co widzę, to to, co zapisano".
   * Zapisywalny stan tego ekranu to `document` i nic poza nim (niezmiennik 13). */
  let autosave: ReturnType<typeof setTimeout> | null = null;

  /* Monotoniczna rewizja i jeden ogon zapisu NA TEN OTWARTY PLIK.
   *
   * 2026-08-28 (T-151): sam debounce nie serializuje IO. Kiedy pierwszy `save_workflow`
   * czekał po drugiej stronie IPC, kliknięcie Run uruchamiało drugi zapis równolegle, a ich
   * zakończenia mogły odwrócić kolejność bajtów na dysku. Store powstaje raz na otwarty plik,
   * więc ten ogon ma dokładnie właściwy zakres: nie blokuje innych workflow i nie przeżywa
   * zamknięcia edytora.
   *
   * `revision` nazywa stan widoczny, `savedRevision` — co najmniej ten stan potwierdzony przez
   * IO. Operacja czyta najnowszy dokument dopiero po dojściu do początku ogona, dlatego kilka
   * zmian czekających za wolnym zapisem koaleskuje się do najnowszej. Wołający czekający na
   * starszą rewizję może dostać nowszą, ale nigdy starszą — dokładnie tyle potrafi obiecać
   * protokół Run, który przekazuje nazwę pliku, nie snapshot jego bajtów. */
  let revision = 0;
  let savedRevision = 0;
  let saveTail: Promise<void> = Promise.resolve();

  /* Rewizja PLIKU — co innego niż licznik `revision` wyżej, i dlatego ma inną nazwę.
   *
   * Tamten liczy zmiany na ekranie i jest faktem o tym oknie. Ten opisuje bajty leżące na
   * dysku i jest faktem o pliku: okno wysyła go przy każdym zapisie, a Rust odmawia publikacji,
   * jeśli pod nazwą leży coś innego. Bez tego serializacja z T-151 pilnuje wyłącznie kolejności
   * zapisów TEGO okna — i przepuszcza zapis, który wystartował z nieaktualnym odczytem, czyli
   * cofa cudzą, nowszą pracę bez jednego słowa (2026-08-28).
   *
   * Podmieniamy go na to, co ODDAŁ Rust, zamiast liczyć samemu: liczba policzona tutaj
   * opisywałaby to, co okno WYSŁAŁO, a nie to, co naprawdę wylądowało. */
  let onDisk: string | null = openRevision;

  return create<WorkflowState>()((set, get) => ({
    document: open,
    notes: [],
    /* Ziarnem jest ten sam obiekt, nie jego kopia: edytor dostał go z dysku, więc w chwili
     * otwarcia ekran JEST plikiem. Kopia (`structuredClone`) dałaby tu nierówność referencji
     * i ekran twierdziłby, że ma niezapisane zmiany, zanim ktokolwiek czegokolwiek dotknął. */
    savedDocument: open,
    couldNotSave: null,

    /* Jedno miejsce, w którym dokument się zmienia. Stos cofnij/ponów (PLAN §7, v1.1) wchodzi
     * TUTAJ i nigdzie indziej — dopisany przy każdej akcji z osobna byłby pięcioma stosami,
     * z których cztery zapominałyby o piątej akcji. Z tego samego powodu autosave wisi tutaj,
     * a nie przy `editStep`, `resetRow` i `chooseSkills` z osobna: akcja dopisana jutro bez
     * własnej linijki zapisu daje zmianę, która żyje wyłącznie na ekranie. */
    commit: (next: WorkflowFile) => {
      revision += 1;
      set({ document: next });

      /* Debounce, nie throttle: przeciągnięcie kafelka albo wpisywanie nazwy to seria commitów,
       * a zapisać chcemy stan, na którym ta seria się zatrzymała. Skasowanie poprzedniego
       * odliczania jest tym, co robi z dziesięciu zapisów jeden. */
      if (autosave !== null) {
        clearTimeout(autosave);
      }
      autosave = setTimeout(() => {
        autosave = null;
        /* `catch`, który KOŃCZY W STANIE, a nie w konsoli — i to jest cała różnica.
         *
         * Do 2026-08-18 stało tu `void get().saveNow()` bez `catch`, z komentarzem, że odrzucenie
         * „trafia do globalnej obsługi błędów okna". Takiej obsługi w tej aplikacji nie ma
         * i nigdy nie było, więc odmowa Rusta — a `save_workflow` odmawia PRZED dotknięciem
         * dysku — kończyła jako nieobsłużone odrzucenie obietnicy: zero pikseli na ekranie
         * i plik o jedną (albo o dziesięć) zmian do tyłu wobec płótna.
         *
         * `.catch(console.error)` byłby tym samym defektem z wierszem w konsoli, której nikt nie
         * otwiera. Zdanie idzie więc do stanu, a ekran ma obowiązek je pokazać (`editor.tsx`). */
        /* `saveNow` samo zapisuje odmowę w stanie przed ponownym odrzuceniem. Tutaj wyłącznie
         * domykamy obietnicę autosave'u: drugie ustawienie po `.catch` mogłoby przywrócić starą
         * odmowę już po tym, jak nowsza rewizja zdążyła zapisać się poprawnie. */
        void get()
          .saveNow()
          .catch(() => undefined);
      }, AUTOSAVE_MS);
    },

    recheck: async () => {
      /* Uwagi liczy Rust (T-12). Gdyby liczył je też front, mielibyśmy dwa zdania o tym samym
       * defekcie i jedno z nich zawsze byłoby nieaktualne (niezmiennik 13). */
      set({ notes: await io.check(get().document) });
    },

    /* Zapis i odświeżenie uwag jednym ruchem, bo to jest jedna decyzja użytkownika: „zapisz to,
     * co widzę". Funkcja jest `async` i NIE łyka błędu — kto ją woła, ten pokazuje, że zapis
     * nie wyszedł. Autosave, który po cichu nie zapisał, jest gorszy niż jego brak: plik jest
     * prawdą, a użytkownik ma wtedy dwie różne prawdy i nie wie o żadnej. */
    saveNow: async () => {
      const wantedRevision = revision;

      /* Każde wywołanie dopina się do jednego ogona. Odrzucenie poprzednika jest rozliczone
       * w `saveTail`, żeby następna poprawna edycja mogła spróbować ponownie bez restartu;
       * własny `queued` nadal odrzuca, więc wołający Run nie pomyli odmowy z zapisem. */
      const queued = saveTail.then(async () => {
        if (savedRevision >= wantedRevision) return;

        /* Czytamy dopiero TERAZ, nie przy kliknięciu. Jeśli za starszym zapisem czekają trzy
         * rewizje, zapisujemy najnowszą i potwierdzamy wszystkie trzy naraz. Referencję trzymamy
         * przez `await`, żeby `savedDocument` opisywał dokładnie bajty wysłane do IO, nawet gdy
         * człowiek zdąży w tym czasie zrobić kolejną zmianę. */
        const writingRevision = revision;
        const saving = get().document;
        /* Rewizja pliku podmienia się DOPIERO po udanym zapisie i tylko wtedy: po odmowie na
         * dysku dalej leżą tamte bajty, więc następna próba ma pytać o dokładnie tę samą
         * rewizję. Przestawienie jej przed `await` zamieniłoby jedną odmowę w milczącą zgodę
         * na nadpisanie przy drugim podejściu. */
        onDisk = await io.save(saving, onDisk);
        savedRevision = writingRevision;
        set({ savedDocument: saving, couldNotSave: null });
        await get().recheck();
      });
      saveTail = queued.catch(() => undefined);

      try {
        await queued;
      } catch (error: unknown) {
        set({ couldNotSave: why(error, COULD_NOT_SAVE) });
        throw error;
      }
    },

    rename: (name: string) => {
      /* Przez `commit`, jak każda inna zmiana: to pod nim wisi autosave, więc nazwa wpisana
       * w nagłówku dojeżdża do pola `name` w pliku bez ani jednej dodatkowej drogi zapisu. */
      get().commit({ ...get().document, name });
    },

    editStep: (stepId: string, agent: Agent, edit: Overrides) => {
      get().commit(
        withAgentStep(get().document, stepId, (step) => applyPanelEdit(step, agent, edit)),
      );
    },

    resetRow: (stepId: string, field: OverridableField) => {
      get().commit(withAgentStep(get().document, stepId, (step) => withoutOverride(step, field)));
    },

    applyFix: async (fix: Fix, agents: readonly Agent[]) => {
      if (fix.kind === 'widenFileAccess') {
        const step = get().document.steps.find((one) => one.id === fix.step);
        const agent =
          step?.kind === 'agent' ? agents.find((one) => one.id === step.agent) : undefined;
        /* Krok albo agent zniknął między policzeniem uwagi a kliknięciem — naprawa milknie,
         * bo nie ma czego przestawić. Następny `recheck` powie prawdę o nowym stanie. */
        if (agent === undefined) return;
        /* WARTOŚĆ EFEKTYWNA, nie patch: różnicę wobec agenta liczy `applyPanelEdit`, więc dial
         * równy temu, co agent ma sam, kasuje nadpisanie zamiast zapisywać je drugi raz. */
        get().editStep(fix.step, agent, { fileAccess: fix.to });
        await get().recheck();
        return;
      }

      if (fix.kind === 'giveItAFreshCopy') {
        /* PROSTO NA DOKUMENT, bez `editStep`: `folder` należy do KROKU, nie do agenta, więc nie
         * przechodzi przez nadpisania i nie potrzebuje biblioteki. Ta sama droga, którą ustawia
         * je wiersz „fresh copy" w panelu kroku. */
        get().commit({
          ...get().document,
          steps: get().document.steps.map((one) =>
            one.id === fix.step && one.kind === 'agent'
              ? { ...one, folder: { use: 'fresh-copy' as const } }
              : one,
          ),
        });
        await get().recheck();
        return;
      }

      const agent = agents.find((one) => one.id === fix.agent);
      if (agent === undefined) return;
      const tools =
        agent.tools === 'everything'
          ? 'everything'
          : { only: agent.tools.only.filter((name) => !fix.tools.includes(name)) };
      /* JEDYNA droga tego magazynu do pliku agenta (`WorkflowIo.saveAgent`, nagłówek pliku).
       * Odmowa zapisu jedzie tą samą ścieżką co odmowa zapisu dokumentu: zdanie na ekranie,
       * plik nietknięty. */
      try {
        await io.saveAgent({ ...agent, tools });
      } catch (error: unknown) {
        set({ couldNotSave: why(error, COULD_NOT_SAVE) });
        return;
      }
      await get().recheck();
    },

    chooseSkills: (stepId: string, choice: SkillChoice) => {
      get().commit(
        withAgentStep(get().document, stepId, (step) => ({
          ...step,
          skills: chosenSkills(choice),
        })),
      );
    },
  }));
}

/** Co kontrolka Skills zapisuje w kroku.
 *
 * `'none'` to PUSTA LISTA, nie brak klucza: „bez umiejętności" jest decyzją użytkownika i ma
 * przeżyć zapis, a brak klucza znaczyłby „weź domyślne", czyli wszystkie. */
function chosenSkills(choice: SkillChoice): Skills {
  if (choice === 'all') return 'all';
  if (choice === 'none') return [];
  return choice.only;
}
