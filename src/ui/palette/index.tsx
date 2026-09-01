/* Paleta poleceń jako jedna rzecz, którą powłoka montuje: nasłuch klawiatury plus okno.
 *
 * CO TU MIESZKA, A CZEGO NIE. Tutaj jest stan (czy otwarte, co wpisane, co podświetlone),
 * nasłuch na dokumencie i powrót ogniska. Nie ma tu ANI JEDNEJ reguły o tym, co znaczy klawisz
 * — to jest `./keys.ts` — ani o tym, co stoi na liście i w jakiej kolejności — to jest
 * `./items.ts`. Podział nie jest estetyczny: `renderToStaticMarkup` nie odpala efektów, więc
 * każda reguła zapisana w ciele nasłuchu byłaby regułą, której żadne kryterium nie umie
 * dotknąć. Tak zginęło siedemnaście kłamiących kontrolek, o których mówi niezmiennik 29.
 *
 * ZAMKNIĘTA PALETA RENDERUJE `null`. Nie schowany `<div>`, nie `hidden`, nie `display:none`.
 * Powód jest mierzalny: kolektor gęstości liczy `[role="dialog"]` jako region i każdy element
 * niosący tekst, więc paleta schowana arkuszem stylów podniosłaby zapadkę z
 * `checks/density-baseline.json` na ekranie, na którym nikt jej nie widzi (niezmiennik 18).
 *
 * NASŁUCH JEST NA DOKUMENCIE, w fazie bąbelkowania. Kontrolka, która sama obsłuży klawisz
 * i zawoła `stopPropagation`, wygrywa — i to jest właściwa kolejność: `Escape` w otwartym
 * wyborze folderu należy do tego wyboru, nie do palety, której akurat nie ma na ekranie.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import type { ReactElement } from 'react';

import { list as listAgents } from '../../sections/agents/io';
import { requestRun } from '../../sections/run/requested';
import { list as listWorkflows } from '../../sections/workflows/io';
import { useSectionStore } from '../shell/section-store';
import { askForAgent } from './asked';
import { focusedShape, moveFor, stepped } from './keys';
import type { PaletteItem, Saved } from './items';
import { matching, paletteItems } from './items';
import type { Showing } from './palette';
import { Palette } from './palette';
import { matchingShortcuts, shortcuts } from './shortcuts';

/**
 * Skąd paleta bierze zapisane rzeczy i dokąd je oddaje.
 *
 * Wstrzykiwane, nie wołane wprost, z tego samego powodu, dla którego magazyn agentów dostaje
 * `AgentsIo`: kryterium ma móc podstawić bibliotekę bez transportu i bez okna. Produkcja
 * dostaje [`LIVE`] niżej i to jest jedyne miejsce w tym poddrzewie, które zna sekcje.
 */
export interface PaletteSources {
  readonly workflows: () => Promise<readonly Saved[]>;
  readonly agents: () => Promise<readonly Saved[]>;
  /** Uruchamia zapisany workflow — nazwą jego pliku, tak jak nazywa go sekcja Workflows. */
  readonly runWorkflow: (path: string) => void;
  /** Otwiera zapisanego agenta: prośba jedzie przez `./asked.ts`, a odbiera ją ekran Agents. */
  readonly openAgent: (id: string) => void;
}

const LIVE: PaletteSources = {
  workflows: async () =>
    (await listWorkflows()).map((entry) => ({ id: entry.path, label: entry.workflow.name })),
  agents: async () => (await listAgents()).map((agent) => ({ id: agent.id, label: agent.name })),
  /* Polityka startu mieszka w JEDNYM miejscu (niezmiennik 23): kto, ile naraz, w którym folderze
   * i co powiedzieć przy odmowie, decyduje sekcja Bieg. Paleta mówi tylko, KTÓRY plik — tą samą
   * drogą, którą mówi to zielony `Run` z edytora workflow. */
  runWorkflow: requestRun,
  openAgent: askForAgent,
};

/** Co wczytano z biblioteki i czy w ogóle się dało. */
interface Library {
  readonly workflows: readonly Saved[];
  readonly agents: readonly Saved[];
  /** `true`, kiedy odczyt odmówił — wtedy okno mówi o tym zdaniem zamiast udawać pustą półkę. */
  readonly unread: boolean;
}

const NOTHING_YET: Library = { workflows: [], agents: [], unread: false };

/* Ile czeka uzbrojone `G` na swoją literę.
 *
 * Sekunda z ogonem, bo tyle trwa świadome „G, potem R" u kogoś, kto tego skrótu jeszcze nie ma
 * w palcach. Zapadka bez limitu czasu jest gorsza niż z za krótkim: `G` naciśnięte i porzucone
 * zostawałoby uzbrojone do końca życia okna, a wtedy pierwsze `r` naciśnięte pół godziny
 * później przenosi ekran bez niczyjej prośby. Litera spoza mapy rozbraja natychmiast
 * (`moveFor` w `./keys.ts`), więc ten limit dotyczy wyłącznie porzucenia w ciszy. */
const WAITS_MS = 1_200;

export interface CommandPaletteProps {
  /** Szew dla kryteriów. Produkcja nie podaje niczego i dostaje [`LIVE`]. */
  readonly sources?: PaletteSources;
}

export function CommandPalette({ sources = LIVE }: CommandPaletteProps = {}): ReactElement | null {
  const [showing, setShowing] = useState<Showing | null>(null);
  const [typed, setTyped] = useState('');
  const [at, setAt] = useState(0);
  const [library, setLibrary] = useState<Library>(NOTHING_YET);

  /* Ognisko, z którego przyszliśmy. W ref, nie w stanie: to nie jest fakt do narysowania,
   * a zapis do stanu w handlerze klawisza przerysowywałby okno bez powodu. Zdejmowane
   * W CHWILI NACIŚNIĘCIA, nie w efekcie — kiedy efekt biegnie, `autoFocus` już przestawił
   * ognisko na pole palety i „skąd przyszliśmy" byłoby samą paletą. */
  const cameFrom = useRef<Element | null>(null);
  const waiting = useRef(false);
  const disarm = useRef<ReturnType<typeof setTimeout> | null>(null);

  const close = useCallback((): void => {
    setShowing(null);
    setTyped('');
    setAt(0);
    const back = cameFrom.current as { focus?: () => void } | null;
    cameFrom.current = null;
    /* Bez `instanceof HTMLElement`: testy tego repo biegną w node, gdzie tej nazwy nie ma,
     * więc sprawdzenie przez konstruktor zamieniłoby powrót ogniska w wyjątek środowiska. */
    if (back !== null && typeof back.focus === 'function') back.focus();
  }, []);

  const show = useCallback((kind: Showing): void => {
    /* Zapamiętujemy ognisko tylko przy PIERWSZYM otwarciu, a pytamy o to SAMEJ referencji:
     * `close` zeruje ją przy każdym zamknięciu, więc pusta znaczy „paleta nie stoi". `?`
     * naciśnięte przy otwartej palecie przełącza listę i nie ma prawa uznać pola palety za
     * miejsce, do którego wrócimy — wtedy Escape oddawałby ognisko elementowi, którego już nie
     * ma. Warunek stoi TUTAJ, a nie w aktualizatorze `setShowing`: React wolno wywołać
     * aktualizator więcej niż raz, a zapis do referencji w jego wnętrzu jest skutkiem ubocznym,
     * który by się wtedy powtórzył. */
    if (cameFrom.current === null) cameFrom.current = document.activeElement;
    setShowing(kind);
    setTyped('');
    setAt(0);
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      const next = moveFor(
        {
          key: event.key,
          metaKey: event.metaKey,
          ctrlKey: event.ctrlKey,
          altKey: event.altKey,
          shiftKey: event.shiftKey,
        },
        focusedShape(document.activeElement),
        waiting.current,
      );

      if (next.move !== 'wait') {
        waiting.current = false;
        if (disarm.current !== null) {
          clearTimeout(disarm.current);
          disarm.current = null;
        }
      }

      switch (next.move) {
        case 'open':
          event.preventDefault();
          show('items');
          return;
        case 'shortcuts':
          event.preventDefault();
          show('shortcuts');
          return;
        case 'jump':
          event.preventDefault();
          useSectionStore.getState().go(next.section);
          return;
        case 'wait':
          waiting.current = true;
          if (disarm.current !== null) clearTimeout(disarm.current);
          disarm.current = setTimeout(() => {
            waiting.current = false;
            disarm.current = null;
          }, WAITS_MS);
          return;
        case 'none':
          return;
      }
    };

    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('keydown', onKey);
      if (disarm.current !== null) clearTimeout(disarm.current);
    };
  }, [show]);

  const isOpen = showing !== null;
  useEffect(() => {
    let live = true;
    if (isOpen) {
      /* Odmowa dysku NIE JEST przełykana w ciszy: pusta biblioteka i biblioteka nie do odczytu
       * wyglądałyby wtedy identycznie, a to są dwie różne prawdy o świecie i tylko jedna z nich
       * jest winą człowieka. Zdanie stawia okno (`data-palette-unread`). */
      void Promise.all([sources.workflows(), sources.agents()])
        .then(([workflows, agents]) => {
          if (live) setLibrary({ workflows, agents, unread: false });
        })
        .catch(() => {
          if (live) setLibrary({ workflows: [], agents: [], unread: true });
        });
    }
    return () => {
      live = false;
    };
  }, [isOpen, sources]);

  if (showing === null) return null;

  const items = matching(paletteItems(library.workflows, library.agents), typed);
  const rows = matchingShortcuts(shortcuts(), typed);
  const howMany = showing === 'items' ? items.length : rows.length;
  /* Podświetlenie przycięte przy RENDERZE, nie przy wpisywaniu: zawężenie listy może zabrać
   * pozycję spod podświetlenia, a wskaźnik poza listą znaczy Enter, który nie robi nic. */
  const spot = howMany === 0 ? 0 : Math.min(at, howMany - 1);

  return (
    <Palette
      showing={showing}
      typed={typed}
      items={items}
      rows={rows}
      at={spot}
      unread={library.unread}
      onType={(next) => {
        setTyped(next);
        setAt(0);
      }}
      onStep={(by) => {
        setAt(stepped(spot, by, howMany));
      }}
      onShow={show}
      onClose={close}
      onChoose={(item: PaletteItem) => {
        close();
        if (item.kind === 'section') {
          useSectionStore.getState().go(item.section);
          return;
        }
        if (item.kind === 'workflow') {
          sources.runWorkflow(item.path);
          return;
        }
        sources.openAgent(item.agent);
      }}
    />
  );
}
