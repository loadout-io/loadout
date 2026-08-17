# T-29 — Klikniecie w prawdziwej aplikacji, nie w tescie komponentu

Szesc razy 2026-08-16 wyszlo to samo: kryterium sprawdza komponent, a produkt nie dziala.
Sekcje nie mialy `index.tsx` i pieć ekranow bylo pustych mimo zielonych testow. Przyciski
istnialy, a `renderToStaticMarkup` nigdy nie odpala `onClick`, wiec nikt nie wiedzial, czy
cokolwiek robia. Za kazdym razem dowiadywal sie o tym **czlowiek, ktory uruchomil aplikacje**.

To zadanie daje ten dowod bramce: Playwright steruje **prawdziwym frontem w prawdziwej
przegladarce**, klika to, co klika uzytkownik, i sprawdza, co widzi.

## Czego to zadanie NIE dowodzi, i dlaczego -- przeczytaj przed pisaniem

`tauri-driver` obsluguje Linux i Windows; **na macOS nie ma czym wysterowac okna Tauri**, bo to
WKWebView, nie Chromium. Zaden test w tym repo nie klika po PRAWDZIWYM oknie aplikacji i to
zadanie tego nie zmienia.

Dowodzimy wiec warstwy, ktora **da sie** wysterowac: front na vite, z `window.__TAURI_INTERNALS__`
podstawionym przez **nagrywajaca atrape**. To jest granica, a nie kompromis do ukrycia: klikniecie
przechodzi przez prawdziwy DOM, prawdziwy React, prawdziwy magazyn i prawdziwy adapter, a zatrzymuje
sie dokladnie tam, gdzie zaczyna sie Rust. Druga strone tej granicy dowodzi T-27 (zlota lista nazw
komend czytana z obu stron plus przebiegi tam-i-z-powrotem przez prawdziwe funkcje).

Kryterium, ktore udawaloby, ze klika po Tauri, byloby gorsze niz brak kryterium.

**Read first:**
`docs/mockup/index.html` (co ma sie stac po kliknieciu) · `tasks/T-26.md` (montowanie sekcji --
to samo miejsce, inny rodzaj dowodu) · `tasks/T-27.md` (nazwy komend; ta sama zlota lista) ·
`AGENTS.md` niezmienniki 16 (kontrolka bez handlera nie wchodzi do repo) i 20.

## Kto to robi

- **Agent:** `react-ui`
- **Druga opinia:** inny vendor niz pisarz (D3).
- **Artefakty biegu:** `runs/T-29/`

## Co to zadanie posiada

- `e2e/harness.ts` -- start serwera vite na wolnym porcie, start chromium, atrapa
  `__TAURI_INTERNALS__` nagrywajaca `(cmd, args)`, sprzatanie obu.
- `e2e/tests/*.spec.ts` -- trzy pliki wymienione przy `check:`.
- `package.json` **nie** jest w OWNS: jesli potrzebny jest skrypt npm, zglos to jako znalezisko.
- **Kod frontu, ktory e2e zastanie NIEPODLACZONY** — `src/sections/run/index.tsx`,
  `src/sections/workflows/index.tsx`, `src/sections/workflows/list/{store.ts,tile.tsx}` i trzy
  testy tych plikow (`delete.test.tsx`, `duplicate.test.ts`, `tile.test.tsx`). **Waski mandat:**
  wylacznie PODPIECIE — sekcja ma brac prawdziwy adapter zamiast zaslepki i nazywac sie wlasnym
  naglowkiem. Ani jednej nowej funkcji, ani jednej oslabionej asercji w tych testach.

  **Dlaczego to jest w OWNS (§5c).** Zmierzone 2026-08-17 na galezi `task-T-29`: kryteria nie
  daly sie spelnic, bo sekcja workflows byla podpieta do **odmawiajacej zaslepki**, a ekran
  biegu nie mial naglowka, po ktorym spacer po sekcjach moze go rozpoznac. Kliknieciu w
  prawdziwej aplikacji nie da sie dowiesc, nie naprawiwszy tego, w co sie klika — proza tego
  zadania wymaga wiec zmiany, ktorej jego wlasny OWNS zabranial. To ta sama rodzina, co T-30
  i T-36.

  Osobno: `list/store.ts` niosl **wezsze lustro** schematu z T3 §3.1 i sam siebie opisywal jako
  tymczasowe — „kiedy T-13 wyladuje, te dwa opisy maja zostac zredukowane do jednego, to jest
  zadanie dla czlowieka". T-13 wyladowalo, a lustro klamalo w te jedna strone, ktora boli: plik
  zapisany przez ten magazyn przechodzil kontrole typow jako KOMPLETNY, majac kroki bez polowy
  pol (niezmiennik 13). Redukcja do jednego zrodla jest wiec wykonaniem tamtej notatki, a nie
  cicha decyzja tego pliku.

## Koszt i przenosnosc -- swiadomie przyjete

Te kryteria wymagaja pobranego chromium (`~/Library/Caches/ms-playwright`). Na maszynie bez niego
`Failed to launch` jest na liscie `NOT_A_REAL_RED`, wiec beda tam czerwone. Decyzja czlowieka
2026-08-16: przyjmujemy ten koszt, bo szesc razy tego dnia jedynym, kto zauwazyl niedzialajacy
produkt, byl czlowiek przed ekranem.

## Niezmienniki

- **16 -- kontrolka bez handlera nie wchodzi do repo.** poprzedni prototyp ma trzy martwe przyciski.
  Tutaj martwy przycisk swieci na czerwono.
- **20 -- test sprawdza zachowanie.** Zaden z trzech plikow nie oglada zrodel; wszystkie klikaja.

## Kryteria akceptacji

## AC-1 Kliknięcie „Create" w Workflows daje workflow, ktory widac na ekranie
check: npx --no-install vitest run e2e/tests/create-workflow.spec.ts

Otworz aplikacje, przejdz do Workflows, kliknij jedyna kontrolke tworzenia. Asercje: atrapa
`__TAURI_INTERNALS__` dostala **dokladnie jedno** wywolanie, nazwa komendy jest z
`src-tauri/commands.golden.txt`; a po powrocie odpowiedzi **na ekranie jest kafelek**, ktorego
przed kliknieciem nie bylo, i **znika zdanie pustego stanu**.

Kontrola przeciw pustej asercji: przed kliknieciem zdanie pustego stanu **musi** byc obecne.
Bez niej „kafelek jest" przechodzi na ekranie, ktory rysuje kafelek zawsze.

*Slaba asercja:* sprawdzenie, ze atrapa dostala wywolanie. Przechodzi na przycisku, ktory wola
Rusta i **nie rysuje niczego** -- czyli w tym samym stanie, w ktorym dzis jest cala aplikacja.
Dyskryminuje: zmiana widoczna dla uzytkownika, sprawdzona po obu stronach kliknięcia.

## AC-2 Kazda sekcja montuje wlasny ekran, sprawdzone w przegladarce
check: npx --no-install vitest run e2e/tests/sections-mount.spec.ts

Przejdz kolejno po pieciu przelacznikach. Dla kazdej sekcji: w dokumencie **nie ma** zdania
z rejestru (`sectionEntry(id).empty`), a jest naglowek tej sekcji. Piec sekcji, piec przejsc,
zadnych wyjatkow.

To jest to samo pytanie, ktore zadaje T-26 przez `renderToStaticMarkup`, zadane **w dzialajacej
aplikacji, po kliknięciu**. Roznica nie jest kosmetyczna: montowanie przez glob `import.meta.glob`
zachowuje sie inaczej w buildzie niz w tescie, a to jest dokladnie ta klasa rozjazdu, ktorej nikt
nie zobaczy, dopoki nie uruchomi.

*Slaba asercja:* sprawdzenie samego naglowka. Przechodzi na ekranie, ktory rysuje naglowek
i pustke z rejestru pod nim. Dyskryminuje: **brak** zdania z rejestru w tym samym dokumencie.

## AC-3 Zaden przycisk na ekranie nie jest martwy
check: npx --no-install vitest run e2e/tests/no-dead-controls.spec.ts

Dla kazdej z pieciu sekcji zbierz wszystkie widoczne `<button>` w `<main>` i kliknij kazdy
z osobna, na swiezo otwartej aplikacji. Asercja dla kazdego: **cos sie stalo** -- zmienil sie
dokument, albo atrapa dostala wywolanie, albo pojawil sie dialog. Przycisk, po ktorym nie zmienia
sie nic i nie leci nic, jest martwy i lamie niezmiennik 16.

Wyjatki wolno wypisac **po nazwie** w tescie, z powodem przy kazdym (np. przelacznik widoku,
ktory wraca na ten sam ekran). Lista pusta jest najlepsza; lista bez powodow jest zakazana.

*Slaba asercja:* klikniecie jednego przycisku na sekcje. Trzy martwe przyciski poprzedniego prototypu byly
w miejscach, do ktorych nikt nie klikal drugi raz. Dyskryminuje: **kazdy** widoczny przycisk.

## Swiadomie poza zakresem

- **Prawdziwe okno Tauri** -- nie ma czym na macOS, patrz wyzej.
- **Granica Rusta** -- T-27.
- **Dwa prawdziwe agenty naraz** -- T-28, po stronie silnika.
- **Wyglad** (kolory, odstepy, sufit gestosci) -- DESIGN i T-22. Tutaj pytamy wylacznie
  „czy dziala", nigdy „czy ladne".

<!-- OWNS
e2e/harness.ts
e2e/tests/create-workflow.spec.ts
e2e/tests/sections-mount.spec.ts
e2e/tests/no-dead-controls.spec.ts
src/sections/run/index.tsx
src/sections/workflows/index.tsx
src/sections/workflows/list/store.ts
src/sections/workflows/list/tile.tsx
src/sections/workflows/list/delete.test.tsx
src/sections/workflows/list/duplicate.test.ts
src/sections/workflows/list/tile.test.tsx
-->
