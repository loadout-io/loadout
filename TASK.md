# T-67 — Strefa TERAZ mowi o tym, co sie dzieje TERAZ

Trzecie znalezisko drugiej opinii w tej fali i trzeci raz ta sama klasa wady, tylko o jeden
widget dalej. T-66 zamknal widmowy kafelek w szynie agentow; **ten sam wiersz okna dalej
fabrykuje wpis w strefie TERAZ**. Droga jest inna, wiec poprawka tamtego zadania go nie dotyka:
`feed/model.ts` w `appendLines` zapisuje KAZDA linie trasy `history` do mapy `doing`
(`doing.set(line.agent, ...)`), a `now.tsx` renderuje `now.rows` **bez bramki `live`** —
pulsujaca kropka i naglowek sa za `live`, sama lista wierszy nie.

Skutek widzi kazdy po pierwszym `/stop` przy niczym niebiegnacym: w strefie TERAZ zostaje
wiersz „Loadout — Nothing is running.", nieodrozninalny od pracujacego agenta, i stoi tam do
konca sesji.

**Druga wada, niezalezna od pierwszej i starsza:** `runEnded` jest aliasem na `unpark`, ktory
gasi `parked` i `toCarry`, a mapy `doing` **nie tyka**. Wiec po zakonczeniu biegu ostatnie
zdanie kazdego agenta zostaje w strefie „co sie dzieje teraz" na zawsze. To jest niezmiennik 17
w miejscu, ktore ARCHITECTURE §6 opisuje jako jeden z dwoch regionow wolnych ruszac sie na
ekranie — czyli w miejscu, w ktore czlowiek patrzy, zeby wiedziec, czy cokolwiek zyje.

**Dlaczego naprawa jest w MODELU, a nie w komponencie.** Bo obie powierzchnie — szyna i strefa
TERAZ — czytaja z tego samego modelu, a kuracja mieszka w mapowaniu, nie w widoku
(niezmiennik 15, decyzja D4). Bramka `live` dopisana w `now.tsx` zalatalaby objaw i zostawila
`doing` pelne widm dla nastepnego konsumenta — a tym konsumentem byl wlasnie `roster.ts`, ktory
juz raz na to wpadl. Jedno miejsce, jedna odpowiedz.

**Cicha porazka, przed ktora stoi ten kontrakt:** naprawa przez wylaczenie strefy. „Nie
pokazuj TERAZ, kiedy nie ma biegu" przechodzi obie oczywiste asercje i kasuje strefe TERAZ
dla biegow, ktore naprawde ida. AC-2(d) tego pilnuje.

**Read first:**
`src/sections/run/feed/model.ts` (`appendLines` — mapa `doing`; `unpark` pod `runEnded`;
`FeedView.now`),
`src/sections/run/feed/now.tsx` (co jest za bramka `live`, a co nie),
`src/sections/run/entry/echo.ts` (wiersz okna i jego ujemny identyfikator — T-58 AC-2c),
`src/sections/run/rail/roster.ts` (ta sama klasa wady, zamknieta w T-66 — nosnik jest ten sam),
`docs/ARCHITECTURE.md` §6 (piec regul strefy TERAZ),
`AGENTS.md` niezmienniki 13, 15, 17.

## Niezmienniki, ktorych to dotyczy

- **17 — UI nie rysuje relacji, ktorych nie ma w danych.** Agent, ktory nie pracuje, nie ma
  prawa stac w strefie „co sie dzieje teraz".
- **15 — kuracja w Ruście i w modelu, nie w CSS.** Poprawka wchodzi tam, gdzie wiersz staje sie
  faktem widoku.
- **13 — jeden fakt, jedno miejsce.** „Kto pracuje" ma jedna odpowiedz; dwie powierzchnie czytaja
  ta sama mape i zadna z nich nie filtruje jej po swojemu.

## Nachodzenie na kolejke, zapisane wprost

`src/sections/run/feed/model.ts` stoi w bloku OWNS **niewyladowanego T-41**. To zadanie bierze
ten plik, bo bez niego nie da sie naprawic przyczyny — a T-41 nie biegnie. Kiedy ruszy, jego
galaz zmerguje te zmiane jak kazda inna: dotykamy `appendLines` i `unpark`, czyli miejsc,
ktorych T-41 (odpowiedz czlowieka do agenta) nie przepisuje. Jesli okaze sie inaczej, **stoj
i zglos**.

## Szkielet, bez ktorego `before` nie jest czerwone

Nie ma nowego modulu: oba testy importuja `createFeed`, ktore istnieje. Kryteria maja padac na
ASERCJACH o zawartosci `view.now.rows`.

## Kryteria akceptacji

## AC-1 Wiersz zlozony w oknie nie wchodzi do strefy TERAZ
check: npx --no-install vitest run src/sections/run/feed/window-rows-stay-out-of-now.test.ts
expect: (\d+) passed

Asercje: (a) paczka zlozona z wierszy okna (ujemne identyfikatory) nie dokłada ani jednego
wpisu do `view.now.rows`; (b) te same wiersze **dalej wchodza do historii** — echo komendy ma
byc widoczne w strumieniu i to jest caly sens T-58, wiec naprawa, ktora je ukrywa, psuje cudze
kryterium; (c) wiersz prawdziwego agenta wchodzi do TERAZ tak, jak dzis (kontrola przeciw
naprawie przez wyciecie); (d) wiersz okna **nie gasi** `Thinking…` — slot gasi prawdziwa linia
od agenta, a nie echo wlasnego Entera; (e) kontrola przeciw pustemu przejsciu: test sprawdza,
ze fikstura niesie oba rodzaje wierszy.

*Slaba asercja:* `expect(view.now.rows).toHaveLength(0)` po paczce z okna. Przechodzi dla
implementacji, ktora nie wpuszcza do TERAZ niczego — czyli kasuje strefe, ktora jest jednym
z dwoch zywych regionow ekranu. Rozroznia to (c).

## AC-2 Koniec biegu oproznia strefe TERAZ
check: npx --no-install vitest run src/sections/run/feed/now-empties-when-the-run-ends.test.ts
expect: (\d+) passed

Asercje: (a) po `runEnded()` `view.now.rows` jest **puste**, bo bieg, ktorego nie ma, nie ma
nikogo pracujacego; (b) `view.now.thinking` tez gasnie — „Thinking…" po biegu jest klamstwem
o procesie, ktory nie istnieje; (c) **historia zostaje nietknieta**: koniec biegu nie kasuje
transkryptu, tylko strefe stanu; (d) **kontrola przeciw naprawie przez wylaczenie strefy:**
paczka linii po nastepnym starcie znowu wypelnia TERAZ; (e) `parked` i `toCarry` dalej gasna,
czyli to, co `unpark` robi dzis, nie znika przy okazji.

*Slaba asercja:* test na samym (a). Przechodzi dla implementacji, ktora czysci `doing` przy
kazdej paczce — a wtedy strefa TERAZ jest pusta zawsze i przez to bezuzyteczna. Rozrozniaja
to (d) razem z (c).

<!-- OWNS
src/sections/run/feed/model.ts
src/sections/run/feed/window-rows-stay-out-of-now.test.ts
src/sections/run/feed/now-empties-when-the-run-ends.test.ts
-->
