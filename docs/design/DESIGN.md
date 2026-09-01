# Loadout — system projektowy

Wersja 1 · 2026-08-15 · wiążąca dla całego frontendu

Ten plik jest źródłem prawdy dla wyglądu. Kod nie zawiera literałów kolorów ani rozmiarów —
tylko odwołania do tokenów zdefiniowanych tutaj. Agent, który dodaje komponent, czyta ten plik pierwszy.

---

## 1. Teza

**Widok pracy nie przyrasta. Aktualizuje się w miejscu.**

Zwykły terminal dopisuje na dół w nieskończoność. Po dziesięciu minutach pracy czterech agentów
masz ścianę tekstu, w której nie widać, co się dzieje teraz. Poprzednia wersja (poprzedni prototyp) dokładnie
tak działała i to jest główna przyczyna, dla której jej UI był przytłaczający.

Loadout dzieli widok na dwie strefy o różnej fizyce:

```
┌─────────────────────────────────────────────┐
│  HISTORIA — przyrasta, jedna linia na krok  │  ← rośnie w dół, zwięzła
│  ✓ Plan          4 steps                    │
│  ✓ Research      3 agents · 2m              │
├─────────────────────────────────────────────┤
│  TERAZ — stała wysokość, nadpisywana        │  ← nie rośnie, mutuje
│  Forge    writing   src/parser.rs           │
│  Needle   tests     12 ✓  0 ✗               │
│  Rivet    waiting   on Needle               │
├─────────────────────────────────────────────┤
│  ❯ _                                        │
└─────────────────────────────────────────────┘
```

*(Teksty w UI są po angielsku — decyzja D5. Opisy i komentarze w dokumentacji po polsku.)*

Jeden agent = jedna linia, która się przepisuje. Jak `top`, nie jak `tail -f`.
Skończony krok zwija się do jednej linii historii. Nic nie znika — pełny zapis jest o kliknięcie dalej —
ale nic się też nie nawarstwia.

**Sprawdzian:** jeśli podczas biegu czterech agentów przez pięć minut widok przewinął się choć raz
sam z siebie — projekt jest złamany.

---

## 2. Marka i podpis wizualny

### Znak: najmniejszy prawdziwy graf

Jedno wejście, dwie równoległe gałęzie, jedna synteza. Cztery węzły, cztery krawędzie.

To nie jest ozdoba dobrana do nazwy. To **dwie z pięciu rzeczy z decyzji D6**, których żaden
vendor nie zbuduje, bo nie ma w tym interesu: odpalić kilku agentów naraz i zebrać ich wyniki
w jeden. Wnętrze aplikacji ma niezmiennik 17 — „UI nie rysuje relacji, których nie ma w danych" —
więc marka, która **jest** najmniejszym grafem prawdziwym, jest jedynym ornamentem, na jaki ten
produkt ma prawo.

Do 2026-08-19 znak był czterema luźnymi kwadratami obróconymi o 45°. Cztery luźne kwadraty nie
mają krawędzi, więc nie mają relacji, więc nie są grafem. Nowy znak dokłada dokładnie dwie
rzeczy: **krawędzie i kierunek.**

| | |
|---|---|
| Siatka | 24 × 24 |
| Węzły | `3,7·12` · `12·5,1` · `12·18,9` · `20,3·12` |
| Promień | 1,95; węzeł syntezy **2,15** (+10%, bo z wielu wychodzi jedno) |
| Krawędź | 1,25, zakończenia okrągłe |
| Stosunek | średnica węzła do grubości linii **3,1 : 1** |
| Sylwetka | romb 16,6 × 13,8 — **szerszy niż wysoki** |

Stosunek 3,1 : 1 jest liczbą, na której ten znak stoi: przy 2,4 : 1 węzeł czyta się jako
zgrubienie linii i cały znak zamyka się w pierścień (zmierzone na 176 px). Romb jest szerszy niż
wysoki, bo graf płynie w poziomie — symetryczny czytałby się jak karo.

**W chrome znak jest neutralny**: węzły `--body`, krawędzie `--muted`. Ani akcentu, ani coralu.
Akcent znaczy „to jest interaktywne", coral „to się dzieje teraz", a znak wisi w nawigacji także
wtedy, kiedy nic nie chodzi i nic nie jest klikalne.

Krawędzie brały najpierw `--line-strong`, czyli biel 16%. **Rodzina `--line-*` jest obramowaniem**
— rysuje włos na krawędzi szkła — a krawędzie tego znaku są jego tematem, nie ramką wokół niego.
Zmierzone na wyrenderowanej powłoce przy 22 px, w jedynym rozmiarze, w jakim znak stoi
w aplikacji: linia 1,25 px w bieli 16% na panelu daje około 1,7 : 1 kontrastu, czyli nie czyta się
wcale, a znak wraca do czterech kropek — czyli do tego, czym był stary znak i czym miał przestać
być. Wartość z rodziny tekstu czyta się, a hierarchia zostaje ta sama, bo `--muted` jest
ciemniejsze od `--body`: węzły nad krawędziami.

### Ikona: trzy rysunki, nie jeden przeskalowany

Przepis 1:1 z systemu, z którego wzięliśmy wartości: squircle `rx=232` na płótnie 1024, radialne
tło indygo, sheen zanikający na 34% wysokości, temat wyśrodkowany, ostra krawędź wewnętrzna
w bieli 10%. Dwie nasze aplikacje mają w Docku wyglądać na rodzeństwo.

**Trzy liczby, na których ta ikona stoi**, i wszystkie trzy są zmierzone 2026-08-19 po tym, jak
pierwsza wersja została odrzucona na zrzucie ekranu z Docka („brzydka, z białymi elementami"):

| Co | Wymóg | Dlaczego |
|---|---|---|
| Zasięg tematu | **≥ 70%** szerokości, **≥ 42%** wysokości, ≥ 8% marginesu | pierwsza wersja miała 66% i 39%: temat pływał w polu i przestawał być rozpoznawalny z odległości ręki |
| Najjaśniejsza barwa tematu | **żaden kanał ≥ 224 na wszystkich trzech** | `#e6e2ff` i czysta biel to były te „białe elementy"; przy 32 px zostawały z nich cztery jasne plamki |
| Kontrast temat ↔ tło | **pasmo 3 : 1 … 9 : 1** | pierwsza wersja miała 15,2 : 1 — tyle jest dobre, gdy temat wypełnia całą kaflę (czarno-biała ikona obok w Docku), ale przy dwóch trzecich odczepia temat od tła |

Dół pasma to próg WCAG dla grafiki nietekstowej: poniżej znak ginie przy 16 px. Górny obowiązuje
**bez wyjątku**, bo zasięg tematu i tak nie pozwala mu wypełnić kafli w całości — gdyby to się
kiedyś zmieniło, oba wymagania trzeba zmienić razem.

Tło jest **prawdziwym indygo** (`#4a44c8` → `#2a2486` → `#171240`), a nie prawie-czernią: kafla ma
czytać się w Docku jako barwa, nie jako dziura między ikonami. Sheen i krawędź wewnętrzna zostają
białe, bo działają przy 10% i są warstwami tła, nie tematem.

**Trzy rysunki mówią jedną paletą.** Osobny rysunek na 32 i 16 px jest decyzją o czytelności, nie
licencją na inne barwy: rysunek 32 bierze te same trzy przystanki tła, a rysunek 16 bierze
najjaśniejszy z nich płasko — przy tym rozmiarze gradient to jeden piksel szarości, a ikona ma być
kropką koloru. Barwy tematu w obu mniejszych muszą występować w rysunku pełnym.

| Rozmiar | Rysunek |
|---|---|
| 1024 / 512 / 256 / 128 | pełny |
| 32 | krawędzie grubsze, węzły jednolitą barwą, bez sheenu i bez krawędzi wewnętrznej |
| 16 | sylwetka i cztery kropki, jedna barwa |

Przy 32 px cztery krawędzie po 38 jednostek mierzą niecały 1,2 px i zlewają się w plamę, a sheen
i krawędź wewnętrzna operują na 0,1 px. **`.icns` jest zestawem, nie skalowaniem** — ikona, która
mydli się na pasku Docka, jest pierwszą rzeczą, jaką człowiek widzi o jakości aplikacji.

Źródłem prawdy są trzy pliki SVG w `docs/branding/`. PNG-i i `.icns` są z nich **generowane**
(`scripts/icons.sh`, przez `qlmanage` → `sips` → `iconutil`), więc nie ma dwóch rysunków tej samej
rzeczy. Gradienty mieszkają wyłącznie tam — poza `src/`, gdzie żaden literał barwy nie ma prawa
stanąć.

**Przepis jest deterministyczny i to jest zmierzone**: 2026-08-19 pełne przebudowanie dało pięć
plików bajt w bajt identycznych z tymi, które leżą w repo (md5 przed i po). Dlatego pochodzenie
rastrów sprawdza się jednym uruchomieniem skryptu i `git status`, a nie okiem. W bramce tego nie
ma z premedytacją: `qlmanage` wymaga sesji graficznej i skanowania Gatekeepera, a bramka ma chodzić
bez okna. Zamiast tego bramka pilnuje **wieku**: `.icns` młodszy od każdego z trzech rysunków
i od samego skryptu.

### Logotyp

`loadout` — **małymi literami, zawsze.** Hanken Grotesk 600, ciasny tracking, `--ink`, nigdy
w akcencie. Podpis: **many agents, one plan**.

`LOADOUT` w monospace z rozstrzeleniem `.12em` było cytatem z terminala, nie logotypem: mono
w tym systemie znaczy „to wyprodukowała maszyna", a nazwa produktu jest językiem ludzkim. Podpis
nie dokłada ani jednego terminu do słownika — `agent` i `plan` już są w interfejsie.

Plik `docs/branding/loadout-logo.svg` **niesie krój w sobie**, jako `data:`. Wymóg brzmi „logotyp
nie ma prawa po cichu spaść na inny krój", a powód jest zmierzony w tym repo: `theme.css`
deklarował Intera od pierwszego dnia i rysował się krojem systemowym przez całe życie repo, bez
ani jednego błędu w konsoli.

### Podpis wizualny: pasek loadoutu

Nazwa aplikacji pochodzi z gier: loadout to zestaw, który kompletujesz **przed** wyjściem w teren.
Nad widokiem pracy siedzi pasek — wybrany workflow jako ciąg bloków, jeden na krok:

```
▓▓▓▓  ▓▓▓▓  ████  ░░░░      Exact-diff delivery · krok 3 z 4
 plan  bada  pisze sprawdza
```

- Blok skończony: wypełniony, `--muted`
- Blok aktywny: wypełniony `--live`, jedyny nasycony element na ekranie
- Blok czekający: obrys `--line-strong`, puste wnętrze
- Segmenty stoją w **jednym szklanym torku** o promieniu kapsuły; nie są czterema luźnymi
  znaczkami. Liczba segmentów bierze się z liczby kroków w danych — **nie ma paska
  procentowego, bo kroki to nie procenty.**

Pasek jest jednocześnie nawigacją: klik w blok pokazuje historię tego kroku.

---

## 3. Kolor

Wartości pochodzą z systemu projektowego meetnotes (`../meetnotes/src/design-tokens/`, nazwa
własna „Quiet Glass"). Bierzemy **wartości**, nie inspirację — dwie nasze aplikacje mają w Docku
wyglądać na rodzeństwo (decyzja D1). Kopia wartości leży w `docs/design/house-values.json`
i jest porównywana z `theme.css` w każdym biegu testu.

Reguła nadrzędna całego systemu, wprost z tamtego pliku: **szkło jest chrome, treść jest
papierem.** Szkło nie wchodzi nigdy pod tekst ani pod kod, które człowiek ma przeczytać.

### Powierzchnie

| Token | Hex | Użycie |
|---|---|---|
| `--bg` | `#07070b` | tło aplikacji, kartka treści |
| `--panel` | `rgba(255, 255, 255, 0.045)` | wypełnienie szkła: menu, pasek, szyna |
| `--raised` | `rgba(255, 255, 255, 0.045)` | karty i kafelki na szkle |
| `--well` | `rgba(255, 255, 255, 0.035)` | pola wejściowe, bloki kodu |
| `--overlay` | `#1b1b24` | **nieprzejrzyste** menu i podpowiedzi |
| `--solid` | `#111118` | gdy potrzebna jest prawdziwie kryjąca powierzchnia |
| `--hover` | `rgba(255, 255, 255, 0.06)` | podkład wiersza pod kursorem |
| `--scrim` | `rgba(0, 0, 0, 0.5)` | przygaszenie za modalem |

Powierzchnie podniesione są **bielą-alfa**, nie własnym szarym. Jedna definicja daje dwa
zachowania: nad aurorą załamuje światło, nad kartką treści czyta się jako delikatne podniesienie.
Menu i podpowiedź **nie mogą** być szkłem — leżą nad treścią, którą człowiek właśnie czyta.

### Tekst

| Token | Hex | Użycie |
|---|---|---|
| `--ink` | `#f6f6fa` | nagłówki, wartości, aktywna treść |
| `--body` | `#a6a6b6` | zdania, opisy, treść domyślna |
| `--muted` | `#8a8a9c` | etykiety, metadane, rzeczy skończone |

Dom ma **cztery** stopnie tekstu; bierzemy trzy. `--muted` to domowy `--text-tertiary`, nie
`--text-muted`: tamten stopień (`#6c6c7d`) mierzy 3,62:1 na powierzchni podniesionej, czyli
**pod progiem czytelności**, i dom trzyma go wyłącznie dla ≥13 px. U nas prawie każdy przygaszony
napis to metadana ≤12 px, więc jaśniejszy stopień (5,50:1) jest jedynym poprawnym. Czwartego nie
wprowadzamy, żeby nikt nie miał czym sięgnąć po ciemniejszy.

### Linie

| Token | Hex |
|---|---|
| `--line` | `rgba(255, 255, 255, 0.09)` |
| `--line-strong` | `rgba(255, 255, 255, 0.16)` |
| `--line-subtle` | `rgba(255, 255, 255, 0.055)` |

### Akcent — mówi „to jest interaktywne" i nic więcej

| Token | Hex | Użycie |
|---|---|---|
| `--accent` | `#6e76ff` | focus, przycisk podstawowy, aktywny glif, kursor w polu |
| `--accent-hover` | `#8a90ff` | ten sam element pod kursorem |
| `--accent-active` | `#5b63f0` | ten sam element wciśnięty |
| `--accent-soft` | `rgba(110, 118, 255, 0.16)` | tło elementu wybranego |
| `--accent-ring` | `rgba(110, 118, 255, 0.5)` | obrys kontrolki, pierścień focusu |

Do 2026-08-19 jeden token odpowiadał na dwa różne pytania: ten dokument mówił jednocześnie
„`--accent` jest **jedynym kolorem interaktywnym**" i „`--accent` znaczy **teraz**". To są dwa
fakty, więc mają dwa tokeny (niezmiennik 13).

**Akcent nigdy nie wypełnia chrome.** Bierze go focus, przycisk podstawowy i aktywny glif —
i na tym koniec. Aktywny wiersz menu jest neutralny; barwę dostaje wyłącznie jego glif.

### Stan — pięć i ani jeden więcej

> **Było cztery do 2026-08-31.** Piąty (`--ok`) wchodzi razem z makietą, którą wybrał
> właściciel, i wchodzi z powodu, nie z upodobania: „krok się udał" nie miał własnej barwy,
> więc ptaszek zakończonego kroku, łączka między dwoma zrobionymi krokami i liczba `214 pass`
> obok `2 fail` brały albo szary — czyli „nic się nie stało" — albo literał w komponencie.
> Kolor, którego system nie ma, nie znika z ekranu: wraca jako literał, a `checks/tokens.sh`
> zamyka literały i słusznie.

| Token | Hex | Znaczy | Pytanie, na które odpowiada |
|---|---|---|---|
| `--live` | `#ff7a5c` | **teraz** | co się dzieje w tej chwili? |
| `--attend` | `#f5b14c` | **ty** | co czeka na moją decyzję? |
| `--fail` | `#ff6b6b` | **zepsute** | co poszło źle? |
| `--ok` | `#5ce1a6` | **skończone i dobre** | co już się udało? |
| `--human` | `#9d7bff` | **człowiek** | co zrobiła osoba, nie maszyna? |

Wash i edge dla każdego — tło chipa i jego obrys:

| Token | Hex | Token | Hex |
|---|---|---|---|
| `--live-soft` | `rgba(255, 122, 92, 0.16)` | `--live-edge` | `rgba(255, 122, 92, 0.5)` |
| `--attend-soft` | `rgba(245, 177, 76, 0.14)` | `--attend-edge` | `rgba(245, 177, 76, 0.5)` |
| `--fail-soft` | `rgba(255, 107, 107, 0.14)` | `--fail-edge` | `rgba(255, 107, 107, 0.5)` |
| `--ok-soft` | `rgba(92, 225, 166, 0.14)` | `--ok-edge` | `rgba(92, 225, 166, 0.5)` |
| `--human-soft` | `rgba(157, 123, 255, 0.14)` | `--human-edge` | `rgba(157, 123, 255, 0.5)` |

`--live` w domu nazywa się tak samo i pilnuje nagrywania; u nas pilnuje pracującego agenta.
Ta sama robota: **żywe, nie alarmujące.**

#### Reguła formy, bez której `--live` i `--fail` są nieodróżnialne

Te dwie barwy różnią się odcieniem o **~13°**, a w naszym strumieniu stoją w sąsiednich
wierszach — czego dom nigdy nie musi pokazać. Rozstrzyga to forma, nie barwa:

- `--live` występuje **wyłącznie** jako: podkład aktywnego wiersza strefy „teraz", jego obrys,
  aktywny segment paska loadoutu, pulsująca kropka, kropka karty w tle.
- `--fail` występuje **wyłącznie** jako: glif `✕`, obrys chipa, lewa krawędź bloku błędu.
- `--ok` występuje **wyłącznie** jako: glif `✓` zakończonego kroku, łączka między dwoma
  zrobionymi krokami, liczba, która przeszła (`214 pass`), kropka „śledzę na bieżąco".
  Nigdy jako podkład wiersza — inaczej ekran po udanym biegu jest zielony na całej wysokości
  i nic już nie znaczy.

Rozłączność tych słowników form jest **sprawdzana statycznie**, nie oceniana okiem.

### Akcent jako ATRAMENT nadoczka

Reguła nad tą sekcją mówi: **akcent znaczy „to jest interaktywne" i nic więcej.** Od 2026-08-31
ma dokładnie **jeden** wyjątek, i jest on wypisany tutaj zamiast rosnąć po cichu w komponentach.

**Nadoczko sekcji jest pisane akcentem.** `--text-eyebrow` niesie `color: var(--color-accent)`
w warstwie `components` (`src/styles/theme.css`), tak jak niesie wersaliki.

Dlaczego to nie jest zniesienie reguły:

- Akcent dalej **nie wypełnia** niczego, co nie jest kontrolką. To jest atrament na 11 px,
  nie powierzchnia.
- Nadoczko odpowiada na pytanie **„gdzie jestem"** — jest adresem, a nie ozdobą. W domu
  (`../meetnotes`) nad każdym tytułem ekranu stoi dokładnie to samo: `SHARED WORK` nad `Tasks`.
- Wyjątek jest **znoszalny**: reguła stoi w warstwie `components`, więc `text-live`, `text-muted`
  i każda inna klasa barwy dalej wygrywa. Makieta korzysta z tego od razu — nadoczko biegu
  jest w `--live`, bo mówi „teraz", a nie „jesteś tutaj".

Granica: **jedno nadoczko na ekran**. Drugie w tym samym widoku znaczy, że ekran ma dwa tytuły.

### Blask nie jest głębią

Do 2026-08-31 ten dokument miał jedno zdanie o cieniach — „wyłącznie pod tym, co pływa" —
i jeden egzekutor, który czyta **każdą** deklarację `box-shadow` bez `inset`
(`src/ui/shell/only-the-nav-floats.test.ts`). Zdanie było za wąskie o całą klasę zjawisk,
i to zmierzone: dom ma 379 deklaracji `box-shadow`, my mieliśmy 15.

Rozróżnienie, które ten dokument robi od dziś:

| | Zapis | Co mówi | Kto go ma |
|---|---|---|---|
| **Podniesienie** | niezerowe przesunięcie, kolor czarny | „to leży NAD stroną" | wyłącznie `.pane` (nawigacja), modal, menu, podpowiedź |
| **Blask** | `0 0 <promień> <barwa tokenu>` | „to świeci" — żywy krok, kropka biegu, twarz agenta, przycisk podstawowy | dowolny element, który **niesie stan albo tożsamość** |

Blask nie ma kierunku, więc nie udaje światła z góry i nie buduje warstw. Ma **barwę tokenu
stanu albo tożsamości** i gaśnie razem z nim — zielony ptaszek świeci na `--ok`, kropka biegu
na `--live`. Blask w kolorze neutralnym jest zakazany: byłby podniesieniem napisanym inaczej.

> **Dług zapisany, nie naprawiony po cichu.** `only-the-nav-floats.test.ts` egzekwuje starsze,
> węższe zdanie i **nie odróżnia blasku od podniesienia**: filtruje wyłącznie człony `inset`.
> Po tej zmianie sądzi regułę, której ten dokument już nie stawia. Naprawa jest jednolinijkowa
> i należy do właściciela tamtego pliku: `liftingShadows()` ma odrzucać także człon, którego
> oba przesunięcia są zerowe. Do tego czasu punkt „lifts NOTHING else" jest czerwony na
> makiecie, która jest zgodna z tym dokumentem — i to jest ta czerwień, o której AGENTS.md
> każe **powiedzieć**, a nie ją uciszyć zmianą sprawdzenia.

### Tożsamość ≠ stan

Agenci mają swoje kolory, żeby lista agentów dała się skanować wzrokiem. Ale kolor agenta
**nigdy nie może być pomylony z kolorem stanu** — inaczej pomarańczowy agent i „czeka na twoją
decyzję" znaczą to samo.

**Rozdział szedł po nasyceniu do 2026-08-31. Od dziś idzie po FORMIE**, i to jest zmiana, którą
wymusiła wybrana makieta: rysuje ona agenta jako świecącą twarz w barwie nasyconej (Scout jest
błękitny, Builder akcentowy, Needle w `--live`), bo pięć przygaszonych szarości nie da się
odróżnić z drugiego końca ekranu — a właśnie po to ten kolor istnieje.

| | Forma | Tokeny |
|---|---|---|
| **Stan** | podkład wiersza, obrys, pasek, słowo | `--live` `--attend` `--fail` `--ok` `--human` |
| **Tożsamość** | **wyłącznie** twarz agenta (kwadrat z inicjałem), kropka przy jego nazwie w strumieniu, chip filtru | `--sky #62d0ff` `--accent` `--live` `--human` `--ok`, oraz przygaszone `--id-1 #6f8496` `--id-2 #7f7597` `--id-3 #94886b` `--id-4 #6b9285` `--id-5 #96707d` |

Wash i edge dla członu nasyconego — tło chipa filtru i jego obrys:

| Token | Hex | Token | Hex |
|---|---|---|---|
| `--sky-soft` | `rgba(98, 208, 255, 0.14)` | `--sky-edge` | `rgba(98, 208, 255, 0.5)` |

Kontrola, która to trzyma: **tożsamość nigdy nie maluje powierzchni ani obrysu wiersza.**
Agent w kolorze `--live` i krok, który trwa, są rozróżnialne, bo pierwszy jest zawsze
kwadratem 26–34 px z inicjałem, a drugi zawsze podkładem i obrysem całego wiersza.
Stan agenta jest **słowem** w kolorze nasyconym, nigdy kolorem kwadratu.

> Kolory tożsamości są **nasze** i nie mają odpowiednika w domu: tamtejsze `--graph-*` są
> nasycone, bo obsługują legendę grafu. Reguła powstała przy budowie makiety: referencyjny
> redesign poprzedniego prototypu dawał agentowi Forge dokładnie ten sam kod barwy co „wymaga uwagi".
> Na jednym ekranie oznaczały dwie różne rzeczy tym samym kolorem.

### Nazwy zastępcze poprzedniej palety: **nie żyją**

`--accent-wash`, `--attend-wash`, `--fail-wash`, `--human-wash`, `--radius-sq` i `--radius-dot`
były przekierowaniami na czas migracji: paleta weszła **addytywnie**, żeby nazwa skasowana pod
niezmigrowanym komponentem nie zostawiła elementu bez ani jednej reguły CSS — to jest awaria,
która nie rzuca wyjątku i nie pojawia się w żadnym logu (niezmiennik 25).

Zniknęły 2026-08-19 razem z ostatnim wołającym: 44 wołania w 19 plikach zostały przeniesione,
a wtedy definicje wypadły z arkusza. **Wołanie którejkolwiek z nich jest dziś czerwienią bramki.**

`--accent-edge` **zostaje i nie jest nazwą zastępczą**: rodzina `-edge` ma pięć członów (live,
attend, fail, human, accent), a akcent po prostu ma tę samą wartość, co jego pierścień skupienia.

Zakazane: gradienty dekoracyjne, drugi kolor marki, kolor jako ozdoba, barwione szkło,
**szósty** kolor stanu, **podniesienie** pod czymś, co nie pływa (blask — patrz wyżej — nie jest
podniesieniem), blask w kolorze neutralnym.

---

## 4. Typografia

### Dwie rodziny, jedna reguła

- **Hanken Grotesk** — język ludzki. Zdania, etykiety, przyciski, nagłówki, opisy, logotyp.
- **JetBrains Mono** — wartości maszynowe. Ścieżki, identyfikatory, hashe, liczby, czas, nazwy
  plików, komendy.

**To jest reguła semantyczna, nie estetyczna.** Mono znaczy „to wyprodukowała maszyna i możesz
to skopiować". Widzisz mono → wiesz, że to fakt, nie opis.

Oba kroje są **zmiennymi plikami `.woff2` w repo** (`src/styles/fonts/`), te same, które niesie
`../meetnotes/src/assets/fonts/`. Oba OFL.

> Zamknięty defekt, dla pamięci. Do 2026-08-19 ten dokument żądał Intera, `theme.css` go
> deklarował, a w drzewie nie było ani jednego pliku kroju i ani jednej reguły `@font-face`.
> Aplikacja rysowała się krojem systemowym — po cichu, bez ani jednego błędu w konsoli.
> Dokładnie jedna rodzina na token stoi w cudzysłowie i dokładnie ta jedna ma swój `@font-face`;
> dalsze człony są systemowe i bez cudzysłowu. Cudzysłów jest obietnicą, że plik leży w drzewie.

### Drabinka

| Token | Rodzina | Rozmiar | Waga | Interlinia | Tracking | Użycie |
|---|---|---|---|---|---|---|
| `--t-display` | ui | 40px | 700 | 1.04 | -0.025em | **tytuł ekranu, jeden na widok** |
| `--t-hero` | ui | 34px | 700 | 1.08 | -0.022em | tytuł drugiego rzędu, powitanie, wielka liczba |
| `--t-title` | ui | 22px | 600 | 1.2 | -0.015em | tytuł karty, panelu i okna dialogowego |
| `--t-question` | ui | 17px | 600 | 1.4 | 0 | pytanie, na którym bieg stanął; etykieta dużego przycisku |
| `--t-heading` | ui | 15px | 600 | 1.3 | -0.01em | nagłówek sekcji |
| `--t-lede` | ui | 15px | 400 | 1.5 | 0 | zdanie POD tytułem ekranu |
| `--t-subhead` | ui | 14px | 600 | 1.3 | 0 | tytuł kafelka na liście |
| `--t-body` | ui | 13px | 400 | 1.5 | 0 | zdania i opisy |
| `--t-ui` | ui | 13px | 600 | 1.2 | 0 | przyciski, aktywne etykiety |
| `--t-note` | ui | 12px | 400 | 1.45 | 0 | drugie zdanie, podpowiedź pod polem |
| `--t-label` | ui | 11px | 600 | 1.2 | 0 | **etykieta pola, zdaniowo** |
| `--t-eyebrow` | ui | 11px | 600 | 1.2 | 0.16em | **nadoczko sekcji, WERSALIKI, w akcencie** |
| `--t-meta` | ui | 11px | 400 | 1.2 | 0 | wartość maszynowa w drugim planie |
| `--t-mono` | mono | 12px | 400 | 1.45 | 0 | wartości maszynowe |
| `--t-mono-strong` | mono | 12px | 700 | 1.2 | 0.06em | identyfikator, nazwa agenta |
| `--t-stream` | mono | 13px | 400 | 1.5 | 0 | linia w widoku pracy |

Waga 500 nie istnieje. Drabinka to 400 / 600 / 700.
Rozmiary poniżej 11px nie istnieją. Jeśli coś nie mieści się w 11px, jest niepotrzebne.

### Sufit 40px, i dlaczego to jest naprawa, nie ozdoba

Do 2026-08-31 najwyższym stopniem był `--t-title` = **20px**, czyli cała drabinka mieściła się
w zakresie 11 → 20px (1,8×). Dom (`../meetnotes`, ten sam system i ta sama paleta) ma zakres
9 → 30px i około pięćdziesięciu stopni. **Przy suficie 20px żaden ekran nie może mieć bohatera:**
da się zrobić rzecz grubszą albo szerszą, nigdy większą — a wtedy każda próba hierarchii kończy
się szarym prostokątem obok szarego prostokąta. To jest zapisana przyczyna, dla której dwie
poprzednie przebudowy interfejsu wyszły nudne, i jest to pomiar, nie opinia.

Wartości trzech górnych stopni są **z makiety**, nie z głowy: `h1` = 40, `h1.sm` = 34, `h2` = 22.

**Rodzeństwo nie znaczy „mniejszy".** Decyzja D1 wiąże nas z domem paletą, rodziną krojów
i materiałem — nie skalą w dół.

#### Migracja górnych stopni

`--t-title` **zmienia wartość** (20 → 22) i **nie zmienia nazwy**: nazwa skasowana pod dwunastoma
wołającymi zostawia elementy bez ani jednej reguły CSS, czyli awarię, która nie rzuca wyjątku
(niezmiennik 25). Zmienia się jej **rola**: tytułem ekranu jest teraz `--t-display`.

Siedem wołań `text-title` to `<h1>` sekcji i one przechodzą na `text-display`. Pięć pozostałych
stoi na czymś, co ekranem nie jest, i przechodzi na `text-title` w nowym znaczeniu albo niżej:

| Plik | Co to jest | Dokąd |
|---|---|---|
| `src/sections/workflows/list/tile.tsx` | nazwa workflow na kafelku w siatce | `text-title` (22px) |
| `src/sections/run/past/panel.tsx` | dwa nagłówki w wysuwanym panelu | `text-title` (22px) |
| `src/sections/run/session/session.tsx` | nazwa agenta na karcie biegu | `text-title` (22px) |
| `src/sections/workflows/editor.tsx` | pole z nazwą workflow (tytuł edytowalny) | `text-display` (40px) |
| `src/sections/settings/index.tsx` | liczba wyrównana do prawej | `text-value`/`text-mono-strong` |

Do czasu tej migracji te pięć miejsc rysuje się o 2px większe niż dotąd — i **ani jedno
kryterium nie sądzi tam rozmiaru**, więc migracja jest widoczna okiem, a nie bramką.

`font-variant-numeric: tabular-nums` obowiązuje wszędzie, gdzie cyfry stoją w kolumnie.

### Dwa stopnie tam, gdzie był jeden

`--t-label` i `--t-eyebrow` mają ten sam rozmiar i tę samą wagę, a różnią się jedną rzeczą:
**wersalikami**. Do 2026-08-19 istniał jeden stopień i obsługiwał oba zastosowania, więc
wersaliki wchodziły albo na **każdą** etykietę pola, albo na żadną.

Podział jest sprawdzalny i jest sprawdzany:

- **Nadoczko sekcji** — `AGENTS`, `BUILD`, `WHAT IT DOES`. Wersaliki, tracking `0.06em`.
- **Etykieta pola i rola agenta** — `Name`, `Give up after`, `writes code`. Zdaniowo, tracking 0.

Wersaliki na każdej etykiecie pola są najczęstszym ruchem domyślnego panelu admina i pierwszą
rzeczą, po której formularz przestaje wyglądać jak macOS.

> **Rozbieżność zapisana, nie naprawiona po cichu.** Ten dokument stawia `--t-eyebrow` w rodzinie
> `ui`, bo nadoczko jest językiem ludzkim, a `mono` znaczy „wartość maszynowa". Makieta trzyma
> trzy reguły nadoczka (`.side h3`, `.rail h2`, `.ctx .ch`) w `mono` i komponenty niosą przy nich
> klasę `font-mono`. **Żadna wyrocznia tego nie porównuje**, więc rozjazd nie świeci na czerwono
> — i właśnie dlatego stoi tu wypisany. Zmiana rodziny jest zmianą WYGLĄDU, a wygląd zmienia się
> razem z regułą w makiecie: należy do T-48, nie do zadania o słowniku.

Reguła `text-transform` mieszka **w definicji stopnia**, w warstwie `components`, a nie
w komponentach: Tailwind pozwala tokenowi `--text-*` nieść interlinię, rozstrzelenie i wagę,
ale nie `text-transform`. Warstwa `components` stoi niżej niż `utilities`, więc `normal-case`
nadal wygrywa tam, gdzie makieta nie ma wersalików.

---

## 5. Przestrzeń i kształt

Baza 4px. Skala: `4 · 8 · 12 · 16 · 24 · 32 · 48`.

```
--space-1: 4px    --space-2: 8px    --space-3: 12px   --space-4: 16px
--space-5: 24px   --space-6: 32px   --space-7: 48px
```

- Padding wiersza strumienia: `8px 16px`
- Padding karty: `12px`
- Padding panelu: `16px`
- Odstęp między sekcjami: `24px`

**Promienie: wyłącznie `--radius-sm 9px`, `--radius-md 13px`, `--radius-lg 18px`
i `--radius-pill 999px`.** Pasmo domu ma jeszcze `24px`; my go świadomie nie bierzemy, bo
narzędzie o tej gęstości przy 24 px wygląda jak aplikacja na iPada. **Wiersz strumienia nie ma
promienia wcale** — i to on utrzymuje gęstość.

Wysokość kontrolek: `28px` kompaktowa · `32px` domyślna · `36px` podstawowy przycisk.
Cel dotykowy nie dotyczy — to aplikacja desktopowa sterowana myszą i klawiaturą.

### Rama okna: dwie kartki, jeden odstęp, aurora pod nimi

```
okno (róg rysuje macOS)
└─ odstęp 8px, aurora + --bg
   ├─ nawigacja  308px · --radius-lg · szkło · PŁYWA (jedyny cień w aplikacji)
   │   ├─ 48px   kolumna glifów — zostaje, kiedy lista się zwęzi
   │   └─ reszta lista pozycji POGRUPOWANA pod nadoczkami MAKE / RUN / KNOW
   └─ treść      --radius-md · nieprzejrzysta · obrys --line
      ├─ pasek   52px · szkło · szukajka ⌘K + stan biegu — nad KAŻDYM ekranem
      ├─ karty   32px · workspace'y (tylko ekran biegu)
      └─ praca
```

**Nawigacja urosła z 208 do 308 px i jest DWUPOZIOMOWA** (2026-08-31). To jest zmiana produktu,
nie stylu, i ma zapisaną przyczynę: siedem sekcji stało w jednej płaskiej liście o **równej
wadze**, więc nic nie mówiło, od czego zacząć — a to jest połowa zdania „UX jest nieoczywisty".
Lista jest teraz pogrupowana (`MAKE` / `RUN` / `KNOW`, `Settings` w stopce), a pozycja, która nie
ma jeszcze sensu, jest **przygaszona i mówi czego jej brakuje** („Make an agent first — a workflow
is agents in a row"), zamiast wyglądać na równorzędną i prowadzić do pustego ekranu. Wąska kolumna
glifów zostaje niezależnie od listy: to jedyny element, który nie znika przy żadnej szerokości.

**Pasek loadoutu przeniósł się nad wszystkie ekrany.** Do 2026-08-31 stał wyłącznie na ekranie
biegu, więc bieg był niewidoczny z każdego innego miejsca w aplikacji. Niesie dziś trzy rzeczy:
szukajkę `⌘K` (jedno pole sięga wszystkiego po nazwie), stan biegu albo postęp pierwszego
uruchomienia, i podpis człowieka. Wysokość 52 px się **nie zmieniła** — budżet chrome z §7 jest
wydany dokładnie tak samo (8 + 1 + 32 + 52 = 93 przy suficie 96).

**Aurora mieszka wewnątrz okna**, nie na pulpicie: statyczna winieta przy lewej krawędzi, pod
kartkami. To rozwiązanie z systemu, z którego wzięliśmy wartości, i ma konsekwencję, która
oszczędza całą klasę pracy — **szkło ma co załamywać bez przezroczystego okna.** Żadnego
`transparent: true`, żadnego `windowEffects`, żadnej zależności od tapety użytkownika. Kolumna
czytania siedzi na czystym `--bg`, więc kod i tekst nigdy nie leżą na barwie.

**Trzy punkty, nie dwa** (2026-08-31). Dwa punkty przy jednej krawędzi dają światło, które pada
z jednej strony i gaśnie — okno wygląda wtedy jak zdjęcie z winietą, a nie jak powierzchnia.
Trzeci domyka przekątną w przeciwległym rogu i jest o połowę słabszy (6% zamiast 10%), więc
kolumna czytania nadal nie leży na barwie. Barwa jest ta sama, którą aurora już niosła — trzeci
punkt nie jest trzecim kolorem.

**Ziarno.** Nad wszystkim leży `body::after`: szum `feTurbulence` jako data-URI, `opacity 0.025`,
`pointer-events: none`. Problem, który rozwiązuje, jest mierzalny, nie estetyczny: gradient
o kontraście 6–15% rozłożony na 900 px wysokości przekracza 8-bitową rozdzielczość kanału,
sąsiednie pasy różnią się o jeden krok wartości i widać je jako **prążki** — tym wyraźniej, im
większe okno. Szum rozbija granicę pasa, bo przesuwa piksele po obu jej stronach w losowe strony;
to ta sama sztuczka, którą druk nazywa rastrem. `pointer-events: none` jest tu warunkiem, nie
ozdobą: bez niego ta warstwa zjada każde kliknięcie. Ziarno gaśnie razem z aurorą przy
`prefers-reduced-transparency`, bo na płaskim tle nie ma czego rozbijać i jest wtedy samym szumem.

**Pływa dokładnie jedna rzecz i tylko ona ma cień.** Refleks `inset` na górnej krawędzi szkła
nie jest głębią — to światło na materiale i wolno go mieć wszędzie.

### Budżet chrome jest prawie wydany, i to on wygrywa z projektem

`docs/ARCHITECTURE.md` §7 daje **96 px nad pierwszą treścią**:

| składnik | px |
|---|---|
| odstęp okna | 8 |
| obrys kartki treści | 1 |
| karty workspace | 32 |
| pasek loadoutu | 52 |
| **razem** | **93** |

Trzy piksele zapasu. Naiwna wersja pływającej kartki dawała 100 — projekt się ścisnął, limit nie
drgnął. §7 mówi wprost: kolejny pasek wymaga usunięcia innego, nie negocjacji limitu.

Suma jest **mierzona z dwóch stron**, bo żadna nie widzi wszystkiego: strona makiety czyta
cztery reguły CSS, strona aplikacji sumuje rodzeństwo nad treścią plus własny odstęp kontenera.
Obrysu zadeklarowanego klasą nie da się odczytać z renderu — i to jest w obu miejscach zapisane,
a nie przemilczane.

### Trzy klasy materiału

| Klasa | Czym jest | Kto ją nosi |
|---|---|---|
| `.glass` | płaskie wypełnienie `--panel`, rozmycie 30px, refleks 10% | pasek, szyna, karty |
| `.pane` | `.glass`, które **pływa**: gradient, rozmycie 36px, refleks 18%, promień `lg`, cień | nawigacja |
| `.paper` | nieprzejrzysta kartka, promień `md`, obrys `line` | treść |

Definicje mieszkają w warstwie `components` arkusza, nie w komponentach: rozmycie zapisane
w komponencie jest literałem dokładnie tam, gdzie `checks/quick-tokens.sh` go zamyka, a przy
trzech powierzchniach szklanych byłyby to trzy kopie jednej decyzji.

**Powłoka ma wypełnienie gradientowe, szkło zostaje płaskie — i to jest cała różnica między
nimi.** Zmierzone 2026-08-31: `--panel` i `--raised` to dziś ta sama wartość co do bajta,
a `.glass` różni się od gołego `bg-panel` wyłącznie rozmyciem i refleksem. Płaska alfa nie niesie
kierunku światła: powierzchnia wygląda tak samo u góry i u dołu, czyli nie wygląda na materiał.
Do 2026-08-31 `.pane` i `.glass` dzieliły wypełnienie co do bajta i różnił je **cień** — czyli
jedyna rzecz, której `prefers-reduced-transparency` nie zdejmuje.

| Token | Wartość | Do czego |
|---|---|---|
| `--glass-fill:` | `linear-gradient(160deg, biel 7,5% → 3,5% → 5,5%)` | wypełnienie powłoki |
| `--glass-blur-strong:` | `36px` | rozmycie powłoki |
| `--glass-highlight-strong:` | `inset 0 1px 0` bieli 18% | refleks powłoki |
| `--shadow-accent:` | `0 4px 14px` akcentu 22% | cień pod `.btn-primary` |
| `--focus-ring:` | `0 0 0 3px --accent-ring` | pierścień skupienia prymitywu |
| `--disabled-opacity:` | `0.4` | wygaszenie kontrolki wyłączonej |

Żaden z nich nie jest nowym kolorem: wszystkie sześć to zapisy istniejących barw, a gradient
powłoki jest tą samą bielą-alfą, którą niesie `--panel`, rozłożoną na dwa końce powierzchni.
`--shadow-accent` jest **czwartym** cieniem i nagłówek nad `--shadow-sm` żąda, żeby odpowiedzieć,
co pływa, a czego dotąd nie było. Odpowiedź: nic nie pływa. Ten cień nie jest głębią, jest
**barwą** — przycisk podstawowy stoi na płaskim akcencie i bez podbarwionego rozmycia pod spodem
czyta się jak prostokąt wklejony w tło. Nie ma prawa trafić na nic innego.

**`prefers-reduced-transparency: reduce` zamienia wszystkie trzy naraz** na `--solid` i zdejmuje
aurorę. Wymóg HIG. Zamiana jednej z trzech jest gorsza niż żadnej: okno miesza wtedy dwa
materiały dla jednego rodzaju powierzchni.

### Ikony: gramatyka, nie ozdoba

| Forma | Znaczy | Kto ją dostaje |
|---|---|---|
| węzły i krawędzie | rzecz, która **jest** grafem | Workflows |
| płyty | rzecz, która jest **zbiorem** | Agents, Skills, Memory |
| trójkąt | jedyna rzecz, która się **dzieje** | Run |

To niezmiennik 17 przeniesiony na ikonografię: nie rysujemy relacji tam, gdzie relacji nie ma.
Ikona z dwoma okręgami połączonymi linią obiecuje zależność — jeśli jej w danych nie ma, kłamie
dokładnie tak samo jak ozdobna krzywa między zakodowanymi na sztywno współrzędnymi.

---

## 6. Komponenty

Definicja komponentu = tokeny, nie hexy. Poniżej pełna lista v1.

### Warstwa prymitywów: nazwa zamiast napisu

**Prymityw to rola zapisana raz, jako klasa w `@layer components` arkusza — nie napis z listą
klas przepisywany w komponencie.** Do 2026-08-31 warstwy prymitywów nie było i to jest cała
przyczyna dryfu opisanego niżej.

**Co zmierzono, 2026-08-31.** W 59 nietestowych plikach `.tsx` stoi **641** napisów-list-klas,
z czego **376 to dosłowne duplikaty** innego napisu. Sam przycisk podstawowy ma **18 wystąpień
pod 9 nazwami stałych, w 5 zapisach geometrii**, które sprowadzają się do 3 realnych par
pikseli — mimo że §6 dopuszcza jedną. Etykieta pola: 74 napisy pod 6 nazwami. Zdanie
drugoplanowe: 118 napisów w dwóch różnych stopniach drabinki dla jednej roli. Wartość maszynowa:
73 napisy, w każdym `font-mono` napisane obok stopnia, który **jest** z rodziny mono.

**Dlaczego żaden check tego nie złapał i dlaczego to nie jest wada checka.**
`checks/tokens.sh` pilnuje barw i rozmiarów: `h-9` obok `h-8` to dwie **poprawne** klasy
tokenowe. Geometrii nie porównywał z tym dokumentem nikt, bo nie było czego porównywać —
nie istniała ani jedna nazwa, która by ją niosła.

**Gdzie mieszkają.** W `@layer components` w `src/styles/theme.css`, obok `.field`, `.glass`,
`.pane` i `.paper`. Nie w komponencie: reguła w komponencie jest kopią decyzji, a przy 18
wystąpieniach kopii jest 18. Nie w `utilities`: Tailwind wstawia tam swoje klasy, więc reguła
dopisana do tej samej warstwy biłaby każdą klasę narzędziową i nie dałoby się jej znieść nigdzie.
Poniżej `utilities` klasa narzędziowa nadal wygrywa — czyli `w-full`, `ml-auto` czy `flex-1`
dopisane do prymitywu robią dokładnie to, co obiecują.

**Czego prymityw NIE wchłania:** kleju układu (`flex items-center gap-2`, siatek
`grid-cols-[...]`). To nie jest rola, tylko rozmieszczenie, i ono należy do miejsca.

| Klasa | Rola | Ton | Zastępuje |
|---|---|---|---|
| `.btn` | przycisk drugoplanowy — obrys mocny, wypełnienie szkła, 32px | — | 8 wystąpień, 3 nazwy |
| `.btn-primary` | podstawowy — **płaski** akcent, 36px, cień w barwie akcentu | — | 18 wystąpień, 9 nazw |
| `.btn-quiet` | cichy — bez wypełnienia, obrys `--line`, 28px | — | 14 wystąpień, 4 nazwy |
| `.btn-bare` | goły — bez obrysu i bez wypełnienia do najechania, 28px | — | znaki i zamknięcia (`×`) |
| `.btn-danger` | niszczący — obrys `--fail-edge`, bez wypełnienia, 32px | — | 5 wystąpień |
| `.btn-attend` | czeka na ciebie — obrys `--attend-edge`, 32px | — | 1 wystąpienie, było poza dokumentem |
| `.chip` | pigułka odczytu, 20px, promień `pill` | `data-tone` | 11 wystąpień w 5 geometriach |
| `.row` | wiersz listy albo menu, z myjką pod kursorem | `aria-*` | 6 wystąpień, 2 paddingi, 2 podświetlenia |
| `.stack` | etykieta nad kontrolką, odstęp 4px | `data-gap` | 37 wystąpień |
| `.label` | etykieta pola, zdaniowa | — | 74 napisy |
| `.lead` | zdanie drugoplanowe, `--t-note`, `--muted` | `data-tone` | 118 napisów |
| `.value` | wartość maszynowa — rodzina **wchodzi razem ze stopniem** | `data-tone`, `data-strong` | 73 napisy |
| `.card` | pojemnik treści, promień `md`, padding 12px | `data-tone`, `data-interactive` | 31 wystąpień |
| `.screen-head` | pasek nagłówka ekranu, 52px, bez tła | — | 11 wystąpień w 5 geometriach |
| `.screen-body` | jedyny przewijany obszar sekcji, padding 16px | — | 15 wystąpień w 5 geometriach |
| `.mark` | znak pustego ekranu, 40px, obrys kreskowany | `data-tone` | 9 ręcznych kopii |
| `.field` | pole formularza — **istniał od 2026-08-18**, dostał trzy brakujące stany | — | 18 wołających |
| `.thinking` | „agent myśli" — trzy kropki | — | nie było czego zastępować |
| `.working` | pasek nieokreślony dla dysku i IPC | — | nie było czego zastępować |
| `.enter` / `.fade-in` | wejście elementu | — | §7 |

**Ton idzie atrybutem `data-tone`, nie klasą-bliźniakiem.** `.chip-fail` obok `.chip` to dwa
napisy, które trzeba trzymać zgodnie ręcznie; `[data-tone]` ma wyższą specyficzność od samej
klasy, więc ton wygrywa z bazą **niezależnie od kolejności reguł w pliku**. Przyciski są
wyjątkiem i mają własne nazwy klas, bo ton przycisku zmienia także jego **wysokość**, a nie
tylko barwę. Ton chipa nie wprowadza piątego koloru semantycznego: `live`, `attend`, `fail`,
`human` i `accent` już istnieją.

### Cztery stany, w każdym prymitywie

Zmierzone przed tą zmianą: w komponentach było **6** wariantów `hover:`, **0** `focus-visible:`,
**0** `active:` i **4** `disabled:` — przy 119 przyciskach i 38 stałych klasowych, z których
reakcję na najechanie miała **jedna**. Kontrolka, która nie odpowiada na najechanie, czyta się
jak napis; kliknięcie, po którym nic nie drgnie, czyta się jak kliknięcie, które nie doszło.

| Stan | Co robi | Skąd wartość |
|---|---|---|
| `:hover` | myjka `--hover` albo mocniejszy obrys | token |
| `:active` | wciśnięcie | `--press` |
| `:focus-visible` | pierścień | `--focus-ring` |
| `:disabled` | wygaszenie, kursor `not-allowed`, brak wciśnięcia | `--disabled-opacity` |

Trzy wartości mają nazwy, a nie liczby, i to jest cała różnica: `disabled:opacity-40` wpisane
w piątym komponencie **nie rozjeżdża się z niczym głośno**. Cztery ręczne bliźniaki stanu
wyłączonego (`SAVE_OFF`, `ADD_OFF`, `PRIMARY_OFF`, `RUN_OFF`) znikają w całości — bez ani jednej
nowej klasy, bo stan wyłączony jest regułą, a nie drugim przyciskiem.

**Pierścień, nie obrys, i to jest odstępstwo nazwane.** Prymityw zdejmuje obrys globalny
(`outline: none`) i rysuje `--focus-ring`, bo prostokąt 2px odsunięty o 2px od kontrolki
o promieniu 9px rysuje się **obok** jej rogu, a pierścień idzie po rogu. Globalna reguła
`:focus-visible` zostaje bez zmiany i obsługuje wszystko, co jeszcze nie jest prymitywem;
warunkiem jej zniknięcia jest domknięcie migracji sekcji. Do tego czasu **są dwie pisownie
jednego faktu** i to jest dług, nie decyzja.

### Rozbieżności rozstrzygnięte jedną wartością

Prymitywu nie da się napisać bez tych rozstrzygnięć. Każde idzie za tym dokumentem, nie za kodem:

| Co | Było w kodzie | Jest |
|---|---|---|
| wysokość przycisku podstawowego | 36px ×9, 32px ×5 | **36px** |
| padding przycisku podstawowego | 16px ×11, 12px ×3 | **16px** |
| wysokość przycisku cichego | 28px ×10, 32px ×4 | **28px** |
| wysokość przycisku niszczącego | 32px ×3, 28px ×2 | **32px** |
| wysokość chipa | 20px, 19px, 17px, brak | **20px** |
| padding przewijanego ciała | 16px ×6, 18px ×3, 14px ×2 | **16px** |
| padding karty | 12px ×8, 16px ×3 | **12px** |
| znak pustego ekranu | 40px ×1, 32px ×9 | **40px** |
| stopień zdania drugoplanowego | `--t-body` ×17, `--t-note` ×7 | **`--t-note`** |

Zdanie **pierwszoplanowe** nie potrzebuje po tej zmianie żadnej klasy: `--t-body` jest stopniem
prozy i `body` już go ma. To jest połowa naprawy tej rodziny — napis `text-body text-body`
(3 wystąpienia) czytał się jak literówka i nią był.

**Kwadrat tożsamości agenta: `22px`, rozstrzygnięte 2026-08-31 decyzją właściciela.**
Dokument mówił dwie rzeczy naraz — §3 `22px`, `agent-card` niżej `14px` — a kod miał `22px`
w dwóch zapisach. Wygrywa §3, bo to sekcja o znaku i tożsamości, i bo tak stoi w kodzie:
rozstrzygnięcie zgadza dokument z rzeczywistością, zamiast zamawiać zmianę w trzech miejscach.
Opis `agent-card` niżej jest poprawiony na tę samą liczbę.

Promień bierze **rolę kontrolki** (`--radius-sm`), zgodnie z tabelą promieni w tym rozdziale —
kwadrat tożsamości jest tam wymieniony wprost.

Uwaga o widoku pracy, sprawdzona przy tej samej okazji: **tam kwadratu tożsamości nie ma**.
Kto jest autorem, mówi w nim BARWA NAPISU (`color: var(--id-N)`, `rail/colour.ts`), a jedyny
mały kształt w strumieniu to pulsująca kropka stanu — pigułka, i słusznie. Wcześniejsza notatka
mówiła o trzech kwadratach bez promienia w tym widoku; po warstwie prymitywów nie ma tam ani
jednego, więc pytanie o ich promień nie zachodzi.

### Promień bierze się z ROLI, nie z rozmiaru elementu

To jest cała reguła i nie ma od niej wyjątku. Pasmo ma cztery wartości i każda odpowiada na inne
pytanie o element:

| Promień | Rola | Co to jest |
|---|---|---|
| `--radius-sm` `9px` | **kontrolka** | przycisk, pole, uchwyt, kwadrat tożsamości, wiersz w pudełku |
| `--radius-md` `13px` | **pojemnik treści** | kafelek, karta kroku, panel, blok cytatu, ramka znaku w pustym ekranie |
| `--radius-lg` `18px` | **rzecz nad treścią** | modal, wymuszony wybór, panel na całą szerokość okna |
| `--radius-pill` | **rzecz, która jest odczytem** | chip, przełącznik, złącze, kropka koloru |

Piąta wartość jest piątą decyzją. Wartość arbitralna (`rounded-[11px]`) jest decyzją zapisaną tam,
gdzie nikt jej nie znajdzie.

**Nazwy klas prymitywów, po jednej na nagłówek poniżej.** Nagłówki zostają dosłownie takie, jakie
były — wyrocznia `src/sections/agents/library-is-reachable.test.tsx` czyta regułę `button-danger`
wprost z tego dokumentu po treści nagłówka, a przepisanie go zamieniłoby żywe kryterium
w porównanie z pustym napisem:
`button-primary` → `.btn-primary` · `button-secondary` → `.btn` · `button-quiet` → `.btn-quiet` ·
`button-bare` → `.btn-bare` · `button-danger` → `.btn-danger` · `button-attend` → `.btn-attend` ·
`chip` → `.chip` · `field` → `.field` · `empty-state` → `.mark`.

### `button-primary`
`background: --accent` · `color: --bg` · `--t-ui` · `--radius-sm` · `padding 0 16px` · `height 36px`
Cień: `--shadow-accent`. Hover: `--accent-hover`. Active: `--accent-active`, cień zgaszony.

**Wypełnienie jest PŁASKIE: bez gradientu i bez poświaty.** Wypełniony akcent jest już
najmocniejszą rzeczą na ekranie i nie potrzebuje pomocy; gradient na 36 px wysokości widać
wyłącznie jako brud. Cień jest jedynym cieniem w aplikacji, który **nie** mówi „to pływa" —
mówi „to jest z materiału", i dlatego jest podbarwiony akcentem, a nie czarny.

### `button-secondary`
`background: --raised` · `color: --ink` · `border: 1px solid --line-strong` · `--t-ui` · `--radius-sm` · `height 32px`
To jest przycisk **domyślny**: klasa bez przyrostka, bo najczęstszy przypadek ma najkrótszą nazwę.

### `button-quiet`
`background: transparent` · `color: --body` · `border: 1px solid --line` · `--t-ui` · `--radius-sm` · `height 28px`

### `button-bare`
Bez obrysu i bez wypełnienia, aż do najechania · `color: --muted` · `height 28px`.
Dla znaków i zamknięć (`×`), gdzie obrys wokół jednego glifu rysuje pudełko, a nie przycisk.

### `button-danger`
Jak `button-secondary`, ale `border: 1px solid --fail-edge` · `color: --fail`. Bez wypełnienia — akcja
niszcząca nie ma być najbardziej rzucającym się w oczy elementem, ma być rozpoznawalna.

### `button-attend`
Jak `button-danger`, ale na tonie `attend`: `border: 1px solid --attend-edge` · `color: --attend`.

**Ten przycisk istniał w kodzie i nie istniał w tym dokumencie** (`src/sections/run/start.tsx`,
jedno wystąpienie). To nie jest piąty kolor semantyczny — `attend` jest jednym z czterech — ale
brak wiersza tutaj znaczył, że warstwa prymitywów albo przewiduje ten ton, albo zostawia jeden
przycisk sierotą poza warstwą. Dopisany 2026-08-31 jako opis stanu faktycznego.

### `chip`
`padding 2px 8px` · `--t-label` · `height 20px` · **`--radius-pill`** · `border 1px solid {stan}-edge` ·
`background {stan}-soft` · `color {stan}`
Wariant neutralny: `--line` / `--raised` / `--muted` — bo nie każdy chip mówi o stanie. Skąd przyszła
umiejętność jest zwykłym faktem, a fakt pomalowany barwą stanu wygląda jak problem.

**Chipa poznaje się po kształcie, nie po nazwie klasy**, i to jest reguła dla wyroczni: barwę stanu
niosą trzy różne rzeczy i tylko jedna z nich jest chipem.

| Co | Obrys | Wypełnienie | Promień |
|---|---|---|---|
| chip | pełny, `{stan}-edge` | `{stan}-soft` | `--radius-pill` |
| `button-danger` | pełny, `--fail-edge` | **żadne** | `--radius-sm` |
| pasek błędu | `border-b` | `--fail-soft` | żaden |

Wypełniony przycisk jest najbardziej rzucającą się w oczy rzeczą na ekranie, a to miejsce należy do
akcentu: wypełnione ostrzeżenie konkuruje z akcją, po którą człowiek przyszedł.

### `field`
`background: --well` · `border: 1px solid --line-strong` · `color: --ink` · `--t-mono` · `--radius-sm` ·
`padding 7px 9px` · `height 32px` · `user-select: text`
Focus: `border-color: --accent`. Etykieta **nad** polem w `--t-label` / `--muted`, zdaniowo.

**To jest JEDNA klasa `.field` w `theme.css` i wszystkie pięć sekcji ją wołają.** Do 2026-08-19
wołały ją dwa miejsca, a cztery sekcje przepisywały ten sam wygląd ręcznie w dwunastu stałych —
i rozjechały się: w Agents obrys był `--line`, w Skills `--line-strong`. Jeden fakt, jedno miejsce
(niezmiennik 13); dwa opisy tego samego pola czyta się jak dwa różne stany, a nie jak dwa pola.

`user-select: text` jest częścią pola, nie ozdobą: `body` wyłącza zaznaczanie w całej aplikacji,
więc pole bez tej linii jest polem, z którego nie da się skopiować własnego wpisu.

Skupienie mieszka też w jednym miejscu: `.field:focus` daje obwódkę w akcencie, a globalny
`:focus-visible` obrys — obwódka odpowiada na „które pole jest aktywne", obrys na „gdzie jest
klawiatura". Dopisywanie tego narzędziem na każdym polu byłoby trzecią kopią podjętej decyzji.

**Trzy brakujące stany dopisane 2026-08-31.** `:hover` podnosi obwódkę do `--accent-ring`,
`:focus-visible` dokłada pierścień, `:disabled` przygasza tekst do `--muted` i stawia kursor
`not-allowed`. Ten ostatni kasuje stałą `FIELD_OFF = 'field text-muted'` z komponentu — drugi
opis tego samego pola, w miejscu, w którym nikt go nie szuka.

**Etykieta zostaje nad polem, i to jest decyzja.** Inspektor dwukolumnowy z etykietą wyrównaną do
prawej (Ustawienia systemowe) potrzebuje szerokości, której ten panel nie ma: przy 330 px kolumna
etykiet szeroka na 90 px łamie „Give up after" i „File access" na dwa wiersze, a pole
wielowierszowe zostaje na 220 px. Dwie kolumny wracają razem z szerszym inspektorem.

### `stream-line` — wiersz w widoku pracy
Siatka `88px 1fr auto`. Padding `8px 16px`. Bez obramowania między wierszami — separacja przez odstęp.
Kolumna 1: nazwa agenta, `--t-mono-strong`, kolor agenta.
Kolumna 2: co robi, `--t-stream`, `--ink`.
Kolumna 3: liczba/czas, `--t-mono`, `--muted`.
Wiersz skończony: `opacity 0.55`.

**Strefa „teraz" ma dokładnie jeden żywy region na jeden fakt** (niezmiennik 13, limit 1):
pulsującą kropkę w `--live` z nadoczkiem `Now`. Fakt brzmi „coś się teraz dzieje", a **nie**
„ten konkretny agent pisze" — tego dane nie niosą, bo `NowRow` to `{ agent, text }`, a kto
pracuje, a kto czeka, jest treścią zdania. Wyprowadzanie tego w widoku przez szukanie słowa
`waiting` w napisie wymyśliłoby fakt (niezmiennik 17) i postawiło politykę „kto co robi” drugi
raz, w komponencie (niezmiennik 23). Kropki nie ma wcale, kiedy nie ma wierszy.

> **Reguła formy, powtórzona tu, bo tu się jej łamie.** `--live` występuje wyłącznie jako
> podkład aktywnego wiersza i jego obrys, aktywny segment paska, pulsująca kropka i kropka
> karty w tle. `--fail` wyłącznie jako glif `✕`, obrys chipa i lewa krawędź bloku błędu.
> Te dwie barwy różnią się odcieniem o ~13°, więc kształt jest jedyną rzeczą, która je
> rozróżnia — a kształt znaczący oba nie znaczy żadnego. Rozłączność jest sprawdzana statycznie.

### `history-line` — zwinięty krok
Siatka `20px 1fr auto`. Padding `6px 16px`. `--t-body`. Znacznik `✓` w `--muted`.
Klik rozwija pełny zapis kroku pod spodem.

### `agent-card` — kafelek w prawej szynie
`background: --panel` · `border: 1px solid --line` · `padding 12px`.
Aktywny: `border-color: --line-strong`, lewy pasek `inset 2px 0 {kolor agenta}`.
Zawiera: kwadrat 22px w kolorze agenta, nazwę (`--t-mono-strong`), rolę (`--t-label`, `--muted`),
jedno zdanie o tym, co robi (`--t-body`), chip stanu.
**Maksymalnie cztery linie tekstu.** Piąta linia to błąd projektowy.

### `node-card` — kafelek w edytorze workflow
Szerokość `280px` · `background: --raised` · `border: 1px solid --line-strong` · `padding 12px`.
Nagłówek: uchwyt `⠿`, nazwa (`--t-heading`), kropka koloru agenta, nazwa agenta (`--t-mono`), `✕`.
Ciało: jedno zdanie po ludzku o tym, co ten krok robi.
Stopka: `◂ po: {krok}` po lewej, `działa przed ▸` po prawej.
Promień `--radius-md`. Zaznaczony: `border-color: --accent`. Złącze i kropka koloru agenta biorą
`--radius-pill`: złącze jest rzeczą, którą się chwyta, a nie kontrolką z etykietą.

### `loadout-strip` — pasek loadoutu
Patrz §2. Blok `32×8px`, odstęp `8px`, etykieta pod blokiem `--t-label` / `--muted`.

### `modal`
`background: --overlay` (nieprzejrzysty) · `border: 1px solid --line-strong` · `--radius-lg` ·
`--shadow-lg` · `max-width 640px` · `padding 24px`.
Tło za modalem: `rgba(6,9,11,0.72)`. Bez rozmycia, bez animacji wjazdu poza `opacity 120ms`.

### `empty-state`
Wyśrodkowane. Znak `◇` w ramce `1px dashed --line-strong` z promieniem `--radius-md`, zdanie
w `--ink`, jedno zdanie instrukcji w `--muted`, jeden przycisk podstawowy.
**Pusty ekran to zaproszenie do działania, nie komunikat o braku danych.**

**`data-empty` siedzi na elemencie, który niesie SAMO zdanie** — nie na opakowaniu ze znakiem,
zaproszeniem i przyciskiem. Znacznik jest czytany przez wyrocznie, a wyrocznia, która dla jednej
sekcji dostaje zdanie, a dla drugiej „◇ zdanie zdanie ＋ Create", milczy dokładnie tam, gdzie ma
krzyczeć. Do 2026-08-19 Workflows trzymał go wyżej niż pozostałe trzy sekcje.

Czynna kontrolka jest wymogiem tam, gdzie autorem jest **człowiek**: Agents, Skills, Workflows.
W Memory notatki pisze agent, więc przycisk dopisany tam „dla symetrii" byłby kontrolką bez
czynności — i to jest decyzja, nie przeoczenie.

---

## 7. Ruch

Ruch odpowiada tu na trzy pytania i na żadne inne: **czy to zadziałało** (mikrointerakcja),
**czy to trwa** (wskaźnik trwania), **czy to właśnie weszło** (wejście). Wszystko poza tą
trójką jest ozdobą i nie wchodzi.

### Tokeny: czas osobno od krzywej

| Token | Wartość | Do czego |
|---|---|---|
| `--transition` | `200ms cubic-bezier(0.32, 0.72, 0, 1)` | **wyłącznie** skrót `transition:` |
| `--transition-fast` | `130ms cubic-bezier(0.32, 0.72, 0, 1)` | to samo, dla stanów kontrolki |
| `--ease-out` | `cubic-bezier(0, 0, 0.2, 1)` | krzywa bez czasu |
| `--ease-spring` | `cubic-bezier(0.34, 1.56, 0.64, 1)` | krzywa z przekroczeniem celu, **tylko wejście** |
| `--duration` | `200ms` | czas bez krzywej |
| `--duration-fast` | `130ms` | czas bez krzywej |
| `--press` | `scale(0.98)` | jedyne wciśnięcie w systemie |

**W skrócie `animation` używa się WYŁĄCZNIE `--duration` / `--duration-fast` plus osobnej
krzywej — nigdy `--transition`.** Powód jest mierzalny, nie stylistyczny: `--transition` niesie
czas **i** krzywą, a skrót `animation` ma slot na **drugi** czas i czyta go jako opóźnienie.
Zapis `animation: rise 320ms var(--transition) both` rozwija się do „320 ms ruchu i **200 ms
zwłoki**". Zmierzone 2026-08-31 w repo obok: **68 animacji jest tak opóźnionych niechcący,
a dwie nie ruszają wcale.** Awaria jest cicha — nic nie pada, element po prostu rusza później
albo nigdy.

Do 2026-08-31 pierwsze trzy tokeny nie miały **ani jednego wołającego** (`grep -rn
'var(--transition' src/` dawał zero). To ta sama klasa awarii co krój zadeklarowany bez pliku:
token wygląda na rozstrzygnięcie i nie robi nic (niezmiennik 21). Wszystkie siedem ma teraz
wołających w `theme.css`.

### Sprężyna wchodzi, bo zniknęła przesłanka zakazu

Do 2026-08-31 stało tu zdanie: „`--ease-spring` z domu **nie wchodzi**: nie mamy ani jednej
powierzchni, która wjeżdża." To był zakaz **warunkowy z podaną przesłanką**, a nie reguła —
i tak został napisany celowo.

Przesłanka zniknęła. Przebudowa wprowadza powierzchnie, które pojawiają się nad tym, co już
jest na ekranie: wysuwany strumień kroku, karta pytania, panel inspektora. Element, który
**pojawia się skokiem**, czyta się jak przeskok widoku — oko nie wie, czy patrzy na to samo
miejsce. Element, który dorasta do miejsca z lekkim przekroczeniem celu, mówi „przyszedłem
stamtąd" i nie zabiera na to więcej niż 200 ms.

Reguła sprężyny jest jedna i jest wąska:

- **wyłącznie na WEJŚCIU elementu** (`animation: … both`), nigdy na hoverze i nigdy
  na przejściu. Przekroczenie celu na hoverze czyta się jak usterka: element wraca do wartości,
  którą chwilę wcześniej minął. Tak to działa w repo obok — 29 użyć, wszystkie na pojawieniu się;
- nośnikami są dwie klasy: `.enter` (`--duration` + `--ease-spring`) i `.fade-in`
  (`--duration-fast` + `--ease-out`, bez sprężyny — to jest obiecane niżej wejście linii historii).

**Sprężyna jest na SKALI, nie na przesunięciu, i to jest ograniczenie narzucone, nie wybór.**
`src/ui/shell/palette.test.ts` żąda, żeby skompilowany arkusz nie zawierał napisu `slate` —
celem jest domyślna paleta Tailwinda (`bg-slate-800`). Napis `translateY` zawiera `slate` co do
znaku, więc **każde** przesunięcie zapisane transformacją zapala tę wyrocznię. To jest fałszywe
trafienie tamtego punktu i jest zgłoszone jako fałszywe; obejście go pisownią byłoby oszustwem,
więc wejście jest zapisane skalą, a podskok kropek i bieg paska — właściwościami `bottom`
i `left`.

### Co zostaje słuszne i nie drgnęło

- Jedna mikrointerakcja w całym systemie: `--press` na wciśnięciu. Ta sama wartość, którą podaje
  makieta (`docs/mockup/index.html:70`); od 2026-08-31 ma nazwę, żeby prymitywy mogły ją
  powtórzyć bez powtarzania liczby.
- Zmiana treści w strefie TERAZ: **bez animacji**. Tekst po prostu jest inny. Animowanie
  przepisania linii sprawia, że oko goni ruch zamiast czytać.
- Wejście nowej linii historii: `opacity 0 → 1` w `--duration-fast`. Bez przesunięcia.
  Klasa `.fade-in`.
- Pulsowanie: **tylko** kropka pracującego agenta, `opacity 1 → 0.35`, `1.4s steps(2)`.
  **Skokowo, nie płynnie** — płynne pulsowanie czyta się jak oddychanie i rozprasza.
- **Kropka gotowości w stopce nie pulsuje i nie jest akcentem.** Dostępność dostawcy nie jest
  ani interakcją, ani „teraz"; jest przygaszona i stoi w miejscu. Sufit z `ARCHITECTURE §7` daje
  **dwa** regiony animujące się od jednego zdarzenia i ta kropka nie ma prawa być jednym z nich.

### Wskaźnik trwania: dwa, i żaden nie jest wirującym krążkiem

Ta aplikacja uruchamia równoległe biegi agentów po kilkanaście minut, a do 2026-08-31 nie miała
**ani jednego** wskaźnika trwania: 41 metod przechodzi granicę bez jednego piksela zmiany
w chwili kliknięcia. Kliknięcie, po którym ekran milczy, czyta się jak kliknięcie, które nie
doszło — i drugie kliknięcie jest wtedy winą interfejsu, nie człowieka.

| Klasa | Kiedy | Kształt |
|---|---|---|
| `.thinking` | agent myśli | trzy kropki, fala z opóźnieniem `.16s` / `.32s`, `currentColor` |
| `.working` | dysk, granica IPC | pasek nieokreślony, segment w `--live` |

**Nie wirujący krążek.** Repo obok ma ich 28 kopii i to jest tam nazwane wadą: krążek nie mówi
ani **co** trwa, ani **ile zostało**, a kręci się tak samo przy 200 ms i przy 20 minutach.

Pasek jest **nieokreślony**, bo zapisu na dysk i przejścia przez IPC nie da się zmierzyć
w procentach; pasek udający postęp tam, gdzie postępu nikt nie liczy, kłamie dokładnie tak samo
jak ozdobna krzywa między zakodowanymi współrzędnymi (niezmiennik 17). Bierze `--live`, bo
reguła formy z §3 wymienia „aktywny segment paska" wprost — akcent tu nie wchodzi, bo akcent
mówi „to jest interaktywne", a paska nie da się kliknąć.

Kropki `.thinking` są **dziećmi** elementu, nie pseudoelementami: trzy `<span aria-hidden>` obok
zdania, które niesie treść.

`@media (prefers-reduced-motion: reduce)` wyłącza wszystko powyżej — jednym blokiem na końcu
arkusza, który neutralizuje każdą animację i każde przejście w aplikacji. Po jego zadziałaniu
kropki stoją nieruchomo w trzech widocznych punktach, a pasek zostaje statycznym segmentem:
żaden z tych wskaźników nie znika, tylko przestaje się ruszać.

---

## 8. Język interfejsu

**UI jest po angielsku** (decyzja D5). Czasownik w trybie rozkazującym, zdanie proste, bez żargonu.
Wiążąca jest tabela żargon→prosty-język z `docs/FOUNDATIONS.md` §2.2 (55 wierszy).

| Zamiast | Piszemy |
|---|---|
| Submit / Execute workflow | `Run` |
| Configuration | `Settings` |
| Initialize | `Create` |
| Terminate | `Stop` |
| Failed with exit code 1 | `Tests failed · 3 of 40` |
| No records found | `Nothing here yet. Type /plan to start.` |
| tool call, tool_use | *(nic — nazwij czynność: `Read`, `Edited`, `Ran`)* |
| stdout / stderr / exit code | `output`, `didn't work` |
| token, context window, compaction | `length`, `started a fresh page` |
| PTY, session, process, spawn | `terminal`, `agent`, `started` |
| orchestrator / DAG / node | `lead agent`, `workflow`, `step` |
| thinking / reasoning tokens | `Thinking…` |
| diff / hunk | `changes` |

Nazwa akcji nie zmienia się w trakcie przepływu: przycisk `Publish` → komunikat `Published`.

Błąd nie przeprasza i nie jest ogólny. Mówi, co się stało i co z tym zrobić:
> `Can't find claude on your system. Install Claude Code, or point Loadout at it in Settings.`

Pusty ekran to zaproszenie, nie komunikat o braku danych:
> `Nothing here yet. Type /plan to start.`

---

## 9. Kontrola jakości

Komponent nie trafia do repo, dopóki:

- [ ] nie ma w kodzie ani jednego literału hex, px rozmiaru czcionki ani `border-radius` innego niż token
- [ ] focus jest widoczny z klawiatury i wygląda tak samo we wszystkich kontrolkach
- [ ] wygląda poprawnie przy szerokości okna 1100px (najwęższe wspierane)
- [ ] nie używa `--accent` do niczego poza interakcją — „teraz" to `--live`
- [ ] `--live` i `--fail` nie dzielą w nim ani jednej formy
- [ ] nie dodaje **szóstego** koloru semantycznego (jest ich pięć: `--live` `--attend` `--fail`
      `--ok` `--human`)
- [ ] jego `box-shadow` jest albo **blaskiem** (`0 0 …`, w barwie tokenu stanu lub tożsamości),
      albo go nie ma — podniesienie z niezerowym przesunięciem należy do rzeczy, które pływają
- [ ] `prefers-reduced-motion` go nie psuje
- [ ] żaden tekst w nim nie jest w mono, jeśli nie jest wartością maszynową
- [ ] nie woła nazwy zastępczej: `rounded-sq`, `rounded-dot`, `*-wash` — żadna z nich nie istnieje
- [ ] jego promień wynika z **roli** (kontrolka `sm`, pojemnik `md`, rzecz nad treścią `lg`,
      odczyt `pill`), a nie z rozmiaru elementu
- [ ] pole formularza bierze klasę `.field`, a nie opisuje się samo
- [ ] `data-empty` na pustym ekranie siedzi na elemencie, który niesie **samo zdanie**
- [ ] rola, którą niesie prymityw z §6, jest zapisana **jego nazwą**, a nie listą klas: przycisk,
      chip, wiersz listy, etykieta, zdanie drugoplanowe, wartość maszynowa, karta, pasek nagłówka,
      przewijane ciało, znak pustego ekranu
- [ ] każda kontrolka odpowiada na `:hover`, `:active`, `:focus-visible` i `:disabled` — prymityw
      daje wszystkie cztery, więc kontrolka, która ich nie ma, nie jest prymitywem i ma powód
- [ ] stan wyłączony jest **regułą**, a nie drugą stałą klasową obok pierwszej
- [ ] operacja, która może trwać dłużej niż jedno mrugnięcie, mówi to na ekranie (`.thinking`
      albo `.working`) — a po jej końcu ekran nie jest **bardziej pusty** niż przed kliknięciem
- [ ] `--ease-spring` stoi wyłącznie na wejściu elementu, nigdy na hoverze ani na przejściu
- [ ] w skrócie `animation` stoi `--duration`, nigdy `--transition`: ten drugi wnosi cichą zwłokę

---

## 10. Co zmieniła przebudowa z 2026-08-31 i dlaczego

Właściciel odrzucił dwie poprzednie próby przebudowy interfejsu słowami „nudne" i „UX totalnie
nieoczywisty", a potem wybrał konkretny kierunek i powiedział o nim **„1 do 1 jak z projektem"**.
`docs/mockup/index.html` jest od tej chwili tym projektem. Ten rozdział jest spisem tego, co
z niego wynikło — po to, żeby żadna z tych zmian nie wyglądała później na czyjeś upodobanie.

**Paleta się NIE zmieniła.** D1 stoi: te same wartości co `../meetnotes`, ta sama rodzina krojów,
ten sam materiał. Doszły dwa kolory, których projekt używa, a system nie miał czym narysować
(`--ok`, `--sky`) — i ani jeden z nich nie jest nową barwą marki.

| Co | Było | Jest | Dlaczego |
|---|---|---|---|
| Sufit typografii | 20px | **40px** | przy 20px żaden ekran nie może mieć bohatera — §4 |
| Tytuł ekranu | `--t-title` 20px | `--t-display` 40px, nad nim nadoczko | dom pisze tytuł ekranu w 36–40px i stawia nad nim nadoczko |
| Nadoczko | 11px, tracking 0.06em, bez barwy | 11px, tracking **0.16em**, w **akcencie** | przy 0.06em bez barwy czyta się jak etykieta, nie jak adres — §3 |
| Kolory stanu | cztery | **pięć** (`--ok`) | „krok się udał" brał szary, czyli „nic się nie stało" |
| Tożsamość agenta | tylko przygaszone `--id-*` | także nasycone, rozdzielone **formą** | pięciu szarości nie odróżnisz z drugiego końca ekranu — §3 |
| Cień | jedna reguła: tylko to, co pływa | **blask** ≠ **podniesienie** | dom ma 379 `box-shadow`, my mieliśmy 15 — §3 |
| Nawigacja | 208px, płaska lista siedmiu równych pozycji | 308px, kolumna glifów + lista **pogrupowana**, pozycja bez sensu **mówi czego jej brakuje** | siedem równych drzwi to połowa zdania „nie wiem, od czego zacząć" — §5 |
| Pasek loadoutu | tylko na ekranie biegu | nad **każdym** ekranem, z szukajką `⌘K` | bieg był niewidoczny z każdego innego miejsca; wysokość 52px bez zmian |
| Kolumny ekranu biegu | strumień z lewej, lista agentów 268px z prawej | **plan 376px z lewej**, strumień z prawej | plan sprawdza się wzrokiem co kilka sekund i chce stałego miejsca; strumień się czyta i chce szerokości |
| Pusty ekran | „Your work will show up here." | zaproszenie: co da się uruchomić, jednym klawiszem, plus wiersz wejścia | siedem odmian zdania „coś się tu kiedyś pojawi" i ani jedno nie mówiło, co nacisnąć |

### Cztery kryteria, które ta zmiana przewraca — i to jest ich robota

Makieta jest wyrocznią wyglądu, więc kiedy zmienia się projekt, kryteria porównujące ją z kodem
**mają** zapalić się na czerwono: to jest dokładnie ten sygnał, po który je napisano. Żadne z nich
nie zostało tknięte. Każde wymaga zmiany po stronie kodu, poza blokiem OWNS tego zadania:

1. `src/ui/shell/shell-matches-mockup.test.tsx` — `NAV_WIDTH` w `src/ui/shell/titlebar.tsx`
   ma być **308**, nie 208.
2. `src/ui/shell/shell-matches-mockup.test.tsx` — kolejność `SECTIONS` w `src/ui/sections.tsx`
   ma być kolejnością makiety: Agents, Workflows, Run, Triggers, Knowledge, Lab, Settings.
   Zbiór etykiet się nie zmienia, wyłącznie porządek i grupy.
3. `src/sections/run/run-matches-mockup.test.tsx` — siatka `data-work` ma brzmieć
   `376px minmax(0,1fr)`, a plan ma stać w PIERWSZEJ kolumnie. Trzeci punkt tego pliku
   („stream column before the agents list") opiera się na przesłance, która przestała być
   prawdziwa, i musi się odwrócić razem z kolumnami.
4. `src/ui/shell/only-the-nav-floats.test.ts` — patrz „Blask nie jest głębią" w §3: filtr
   `liftingShadows()` odrzuca dziś wyłącznie człony `inset`, więc liczy blask jako podniesienie.
