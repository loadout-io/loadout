/* Ile człowiek NAPRAWDĘ ma — jedyna rzecz, z której boczne menu liczy swoje kłódki.
 *
 * DLACZEGO TO W OGÓLE ISTNIEJE. Menu, które rysuje kłódkę przy pozycji, twierdzi coś o świecie:
 * „tej rzeczy nie da się jeszcze użyć". Twierdzenie wzięte z niczego jest tą samą wadą, co
 * krzywa narysowana między zakodowanymi na sztywno punktami na płótnie (niezmiennik 17) —
 * wygląda na wiedzę i nią nie jest. Kłódka przy Workflows ma znaczyć DOKŁADNIE „w bibliotece
 * nie ma ani jednego agenta", a to jest zdanie o dysku, nie o interfejsie.
 *
 * TRZY STANY, NIE DWA, i to jest cała treść tego pliku. `null` znaczy „nikt jeszcze nie
 * czytał", zero znaczy „czytaliśmy i nie ma nic", liczba znaczy tyle, ile mówi. Zlanie
 * pierwszych dwóch w jedno jest awarią, która wygląda jak działanie: okno otwarte na maszynie
 * pełnej agentów pokazywałoby przez ułamek sekundy — a przy odmowie dysku na zawsze — cztery
 * kłódki i zdanie „zrób najpierw agenta" komuś, kto ma ich dwadzieścia.
 *
 * ODCZYT JEDZIE TĄ SAMĄ KRAWĘDZIĄ, CO WSZYSTKO INNE (niezmiennik 23): `list` z sekcji Agenci
 * i `list` z sekcji Workflow. Drugi `invoke('list_agents')` napisany tutaj byłby drugą drogą
 * do Rusta, czyli drugim miejscem, w którym trzeba pamiętać o nazwie komendy i o kształcie
 * odpowiedzi. Paleta poleceń czyta te same dwie funkcje i z tego samego powodu.
 *
 * ODMOWA DYSKU NIE JEST ZEREM. `catch` zostawia stan nietknięty: nie wiemy, ile jest, więc
 * mówimy dokładnie tyle, ile wcześniej. Kłódka postawiona na odmowie odczytu obwiniałaby
 * człowieka za awarię, której nie spowodował.
 */
import { create } from 'zustand';

import { list as listAgents } from '../../sections/agents/io';
import { list as listWorkflows } from '../../sections/workflows/io';

/** Skąd biorą się dwie liczby. Wstrzykiwane, żeby kryterium nie potrzebowało ani okna, ani dysku. */
export interface Shelves {
  readonly agents: () => Promise<readonly unknown[]>;
  readonly workflows: () => Promise<readonly unknown[]>;
}

const LIVE: Shelves = { agents: listAgents, workflows: listWorkflows };

export interface WhatYouHave {
  /** Ilu agentów leży w bibliotece; `null`, dopóki nikt jej nie czytał. */
  readonly agents: number | null;
  /** Ile workflow leży w bibliotece; `null`, dopóki nikt jej nie czytał. */
  readonly workflows: number | null;
  /** Przelicza obie półki. Cicha przy odmowie — patrz nagłówek. */
  count: () => Promise<void>;
}

export function createWhatYouHave(io: Shelves = LIVE) {
  return create<WhatYouHave>()((set) => ({
    agents: null,
    workflows: null,
    count: async () => {
      /* Dwa odczyty naraz i KAŻDY osobno rozliczony: jedna półka nie do odczytu nie ma prawa
       * skasować wiedzy o drugiej. `Promise.all` z jednym `catch` robiłby dokładnie to. */
      await Promise.all([
        io
          .agents()
          .then((all) => {
            set({ agents: all.length });
          })
          .catch(() => undefined),
        io
          .workflows()
          .then((all) => {
            set({ workflows: all.length });
          })
          .catch(() => undefined),
      ]);
    },
  }));
}

export const useWhatYouHave = createWhatYouHave();
