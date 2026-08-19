# T-51 — Ikona, która czyta się w Docku

Ikona wylądowała w T-49 i **nie działa na pasku Docka**. Zgłoszone przez właściciela ze zrzutem
ekranu: „ikonka brzydka z białymi elementami jest". Zmierzone na wylądowanym `main`:

| co | wartość | dlaczego to boli |
|---|---|---|
| najjaśniejsza barwa tematu | `#e6e2ff` i czysta `#ffffff` | to są „białe elementy" ze zgłoszenia |
| szerokość tematu | **66%** płótna | temat pływa w polu, zamiast je wypełniać |
| wysokość tematu | **39%** płótna | ten sam problem w drugiej osi |
| najjaśniejszy przystanek tła | `#221f52`, luminancja **0,019** | kafla czyta się jak dziura między ikonami |
| kontrast temat ↔ tło | **12,1 : 1** | przy temacie zajmującym 2/3 kafli to nie forma, to okruchy |
| barwy tematu w rysunku 32 | `#8f96ff` — **nie ma jej** w rysunku pełnym | trzy rysunki rozjechały się w palecie |
| tło rysunku 16 | `#161436` — **nie ma go** w rysunku pełnym | to samo |

Kierunek wybrany przez właściciela z trzech wariantów pokazanych na tle Docka: **ciemne indygo**.
Zostaje ciemna rodzina i pokrewieństwo z aplikacją obok, ale tło jest **prawdziwym indygo**, a znak
jest większy i cięższy. Warianty „jasny kamień" i „pełny akcent" odrzucone.

## Czego to zadanie NIE zmienia

Struktura trzech rysunków jest wylądowana i sądzona przez T-49 AC-2: rysunek pełny niesie sheen,
blask, gradienty węzłów i krawędź wewnętrzną, rysunek 32 ma najwyżej jeden gradient i grubsze
krawędzie, rysunek 16 nie ma ani jednego gradientu, a squircle jest w tej samej proporcji do
płótna we wszystkich trzech. **Ta struktura zostaje.** To zadanie zmienia paletę i geometrię
tematu, nie przepis.

## AC-1 Temat nie niesie prawie-bieli w żadnym z trzech rysunków
check: npx --no-install vitest run src/ui/brand/subject-carries-no-near-white.test.ts
expect: (\d+) passed

Asercje: (a) żadna barwa **tematu** (krawędzie i węzły, czyli obrysy i wypełnienia w grupie
tematu — nie sheen, nie krawędź wewnętrzna, które są półprzezroczystymi warstwami tła) nie ma
wszystkich trzech kanałów `>= 224`; (b) **ani jedna** barwa tematu nie jest czystą bielą, w żadnym
zapisie (`#fff`, `#ffffff`, `white`); (c) sheen i krawędź wewnętrzna **wolno** mieć biel, bo są
warstwami przy 10% i 22% — kryterium wymienia je jawnie jako dozwolone, żeby nikt nie „naprawił"
ikony, kasując przepis z domu; (d) kontrola przeciw pustemu czytaniu: mniej niż osiem barw tematu
przeczytanych w trzech plikach to błąd testu, nie zieleń.

*Słaba wersja:* szukanie `#ffffff` w całym pliku. Pada na sheenie, który ma tam być, i przechodzi
na `#e6e2ff`, czyli na barwie, którą człowiek nazwał białą.

## AC-2 Temat wypełnia kaflę, a nie pływa w niej
check: npx --no-install vitest run src/ui/brand/subject-fills-the-tile.test.ts
expect: (\d+) passed

Zasięg tematu liczony z **węzłów razem z ich promieniami**, bo to one są skrajnymi punktami
rysunku. Asercje: (a) w każdym z trzech rysunków temat zajmuje **co najmniej 70%** szerokości
płótna; (b) **co najmniej 42%** wysokości; (c) temat jest wyśrodkowany — środek jego zasięgu leży
w obu osiach nie dalej niż 2% płótna od środka kafli; (d) temat **nie wychodzi** na krawędź:
zostaje co najmniej 8% marginesu z każdej strony, bo squircle obcina narożniki i ikona dotykająca
brzegu wygląda na obciętą; (e) kontrola przeciw pustemu czytaniu: mniej niż cztery węzły w pliku
to błąd testu.

*Słaba wersja:* asercja na jeden rysunek. Rysunek 32 ma dziś 68% i przechodzi progi ustawione na
rysunek pełny.

## AC-3 Trzy rysunki mówią jedną paletą
check: npx --no-install vitest run src/ui/brand/three-drawings-one-palette.test.ts
expect: (\d+) passed

Asercje: (a) **każda** barwa tematu z rysunku 32 i 16 występuje w palecie tematu rysunku pełnego;
(b) tło rysunku 32 ma **te same** przystanki, co tło rysunku pełnego; (c) płaskie tło rysunku 16
jest **jednym z** przystanków tła rysunku pełnego — przy 16 px gradient to jeden piksel szarości,
więc najmniejszy rysunek bierze barwę kafli płasko; (d) kontrola przeciw pustemu czytaniu: paleta
rysunku pełnego ma co najmniej pięć barw.

*Słaba wersja:* porównanie liczby barw. Trzy rysunki po trzy inne barwy przechodzą, a to jest
dokładnie dzisiejszy stan.

## AC-4 Kontrast tematu do tła mieści się w pasmie, w każdym rysunku
check: npx --no-install vitest run src/ui/brand/subject-and-ground-stay-in-band.test.ts
expect: (\d+) passed

Kontrast liczony wzorem WCAG na luminancji względnej, temat mierzony swoją **najjaśniejszą**
barwą, tło **najjaśniejszym** przystankiem — bo gradient tła jest wyśrodkowany dokładnie tam, gdzie
stoi temat. Asercje: (a) w każdym z trzech rysunków kontrast jest **nie mniejszy niż 3 : 1** —
poniżej tego znak ginie przy 16 px (próg WCAG dla grafiki nietekstowej); (b) **nie większy niż
9 : 1** — powyżej temat odczepia się od kafli i przy temacie mniejszym od niej czyta się jako
okruchy, i to jest dokładnie to, co właściciel zobaczył w Docku; (c) górny próg wolno przekroczyć
tylko wtedy, gdy temat zajmuje **całą** kaflę, czego AC-2 i tak nie pozwala — więc pasmo obowiązuje
bez wyjątku, i to jest zapisane, a nie domyślne; (d) kontrola: luminancja bieli wychodzi 1,0
i luminancja czerni 0,0, licząc tym samym wzorem.

*Słaba wersja:* sprawdzenie samego progu dolnego. Dzisiejsze 12,1 : 1 przechodzi z zapasem, a to
jest defekt, po którym to zadanie powstało.

<!-- OWNS
docs/branding/loadout-icon.svg
docs/branding/loadout-icon-32.svg
docs/branding/loadout-icon-16.svg
docs/design/DESIGN.md
src-tauri/icons/32x32.png
src-tauri/icons/128x128.png
src-tauri/icons/128x128@2x.png
src-tauri/icons/icon.png
src-tauri/icons/icon.icns
src/ui/brand/subject-carries-no-near-white.test.ts
src/ui/brand/subject-fills-the-tile.test.ts
src/ui/brand/three-drawings-one-palette.test.ts
src/ui/brand/subject-and-ground-stay-in-band.test.ts
-->
