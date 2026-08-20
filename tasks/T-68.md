# T-68 — Koniec biegu gasi WSZYSTKO, co opisywalo zywy bieg

Czwarte znalezisko drugiej opinii w tej samej rodzinie i pierwszy kontrakt, ktory nie goni
kolejnej powierzchni, tylko domyka regule. Trzy poprzednie szly po kolei: widmowy kafelek
w szynie (T-66), widmowy wiersz w strefie TERAZ (T-67), a teraz **przypiete pytanie, ktore
przezywa bieg**. Za kazdym razem przyczyna byla ta sama: model trzyma kilka pol opisujacych
ZYWY bieg, a `runEnded()` gasi tylko niektore.

Zmierzone w kodzie: `runEnded` (alias `unpark`) gasi `parked` i `toCarry`, T-67 dokłada `doing`
i `thinking` — a `waiting` zostaje. `feed.tsx` renderuje karte „Needs your answer" wylacznie po
`view.pinned !== null`, bez zadnej bramki na to, czy bieg zyje. Wiec pytanie zadane przez agenta,
na ktore czlowiek nie odpowiedzial przed Stopem albo przed bledem, zostaje na ekranie: `attention`
stoi na `you`, klikniecie odpowiedzi dalej wola `answer()` i ustawia `toCarry` dla agenta, ktory
nie pracuje. Kontrolka, ktora po cichu nic nie robi (niezmiennik 16), przypieta do relacji,
ktorej w danych juz nie ma (niezmiennik 17).

**Najmocniejszy dowod, ze to jest wada, a nie decyzja:** docstring `runEnded` mowi „bieg,
ktorego nie ma, nie stoi na niczyim pytaniu" — i to zdanie opisuje dokladnie te karte, ktora
zostaje na ekranie.

**Dlaczego to jest jedno zadanie, a nie czwarty przypis.** Bo lista pol opisujacych zywy bieg
jest zamknieta i da sie ja WYPISAC — a kryterium, ktore ja wypisuje, lapie piata powierzchnie,
zanim ktos ja doda. Dokladnie tak dziala `feed/collapse.test.ts` dla rodzajow wierszy: dwie
listy wypisane, nie liczone, bo „dziewiec zlych to nadal dziewiec". Ten kontrakt robi to samo
dla stanu strefy zywej.

**Cicha porazka, przed ktora stoi:** naprawa przez `createFeed()` od nowa. Wyczyscilaby
wszystko naraz i skasowala HISTORIE — czyli transkrypt biegu, ktory wlasnie sie skonczyl,
i po ktory czlowiek do tego ekranu wraca.

**Read first:**
`src/sections/run/feed/model.ts` (`unpark` pod `runEnded`, pola `waiting`, `parked`, `toCarry`,
`doing`, `thinking`, `answers`, `attention`; `FeedView`),
`src/sections/run/feed/feed.tsx` (karta „Needs your answer" wisi na samym `view.pinned`),
`src/sections/run/io.ts` (`start().finally()` wola `view.runEnded()` — tez przy odmowie i przy Stopie),
`docs/ARCHITECTURE.md` §6 (piec regul strefy TERAZ),
`AGENTS.md` niezmienniki 13, 16, 17.

## Niezmienniki, ktorych to dotyczy

- **16 — kontrolka bez roboty.** Przyciski odpowiedzi po biegu wolaja `answer()` w prozne.
- **17 — relacja, ktorej nie ma w danych.** „Ktos czeka na twoja odpowiedz" po biegu, ktory
  zszedl.
- **13 — jeden fakt, jedno miejsce.** „Czy cokolwiek zyje" ma jedna odpowiedz; kazde pole
  strefy zywej musi z niej wynikac, a nie mieszkac obok niej wlasnym zyciem.

## Kryteria akceptacji

## AC-1 Zejscie biegu gasi kazde pole strefy zywej, i lista tych pol jest wypisana
check: npx --no-install vitest run src/sections/run/feed/nothing-live-survives-the-run.test.ts
expect: (\d+) passed

Asercje: (a) po strumieniu, ktory zapala WSZYSTKO naraz — linia agenta (`doing`), `thinking`,
pytanie bez odpowiedzi (`waiting`, `pinned`, `attention`), zdanie do przewiezienia (`toCarry`)
— `runEnded()` zostawia kazde z tych pol puste; (b) lista pol jest w tescie **wypisana**, nie
liczona: nowe pole strefy zywej dopisane do modelu bez wygaszenia ma zapalac to kryterium, a nie
przechodzic na pustej sumie; (c) **historia zostaje nietknieta** — koniec biegu nie kasuje
transkryptu, i to jest dokladnie to, po co czlowiek wraca na ten ekran; (d) kontrola przeciw
naprawie przez przebudowe modelu: identyfikatory wierszy historii po `runEnded()` sa te same,
co przed nim; (e) kontrola przeciw pustemu przejsciu: test sprawdza, ze przed `runEnded()`
kazde z wymienionych pol bylo NIEPUSTE.

*Slaba asercja:* `expect(view.pinned).toBeNull()` po `runEnded()`. Przechodzi dla implementacji,
ktora gasi jedno pole i zostawia cztery — czyli dla dokladnie tego stanu, z ktorego wzieły sie
T-66, T-67 i to zadanie. Rozrozniaja to (a) razem z (b).

## AC-2 Karta odpowiedzi nie przezywa biegu, ktory ja zadal
check: npx --no-install vitest run src/sections/run/feed/answer-card-dies-with-the-run.test.tsx
expect: (\d+) passed

Asercje na markupie: (a) po `runEnded()` markup widoku **nie zawiera** karty „Needs your answer";
(b) przed `runEnded()` ta karta w markupie JEST — inaczej test mierzy komponent, ktory jej nigdy
nie rysuje; (c) `attention` nie stoi na `you`, kiedy nikt nie pracuje i nic nie czeka;
(d) kontrola: pytanie zadane po nastepnym starcie znowu przypina karte — naprawa przez trwale
wylaczenie karty nie jest naprawa.

*Slaba asercja:* samo (a). Przechodzi dla komponentu, ktory nie umie narysowac tej karty nigdy.
Rozroznia to (b) razem z (d).

<!-- OWNS
src/sections/run/feed/model.ts
src/sections/run/feed/nothing-live-survives-the-run.test.ts
src/sections/run/feed/answer-card-dies-with-the-run.test.tsx
-->
