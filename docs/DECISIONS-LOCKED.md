# Loadout — decyzje zablokowane przez człowieka

Te cztery decyzje podjął Jakub 2026-08-15, przed syntezą. **Nie podważaj ich w planach ani w ADR-ach.**
Research może dodać szczegóły implementacyjne, ale nie zmienia kierunku.

---

## D1 — Wygląd: Loadout Quiet Glass

*Zrewidowane 2026-08-19. Pierwotna D1 — paleta mint-na-czerni z makiety poprzedniego prototypu,
`border-radius: 2px` wszędzie, krój Inter — jest **cofnięta w całości**. Powody, wszystkie
zmierzone: mint na czerni jest statystyczną średnią tego, co generują modele, i nie mówi nic
o tym produkcie; dwupikselowy narożnik jest antytezą macOS, o który prosiła ta sama decyzja;
a Inter był zadeklarowany w `theme.css` od pierwszego dnia i **nie istniał w drzewie** —
aplikacja przez cały ten czas rysowała się krojem systemowym, po cichu, bez ani jednego błędu
w konsoli.*

Baza wizualna: **system projektowy meetnotes** (`../meetnotes/src/design-tokens/`, nazwa własna
„Quiet Glass"). Bierzemy **wartości**, nie inspirację: powierzchnie, obrysy, akcent, promienie,
cienie, krzywe ruchu, przepis na szkło i oba kroje są 1:1. Dwie nasze aplikacje mają w Docku
wyglądać na rodzeństwo.

Reguła nadrzędna, wprost z tamtego systemu: **szkło jest chrome, treść jest papierem.** Szkło nie
wchodzi nigdy pod tekst ani pod kod, które człowiek ma przeczytać.

Co jest nasze, bo dom tego nie rozwiązuje:

- **Gęstość.** Listy meetnotes są znacznie luźniejsze — wiersz spotkania zajmuje w ich podglądzie
  kilkukrotność naszego wiersza strumienia. Te same wartości, ciaśniejsze zastosowanie; sufit
  gęstości z `docs/ARCHITECTURE.md` §7 obowiązuje bez zmian.
- **Rozdział „interaktywne" od „teraz".** `--accent` `#6e76ff` mówi wyłącznie „to jest
  interaktywne". `--live` `#ff7a5c` mówi wyłącznie „to się dzieje w tej chwili". Do 2026-08-19
  jedna barwa robiła obie prace naraz, więc pulsująca kropka i przycisk wyglądały na spokrewnione.
- **Przygaszone kolory tożsamości agentów.** Domowe barwy grafu są nasycone, bo obsługują legendę;
  u nas kolor agenta obok koloru stanu jest awarią, więc tożsamość jest przygaszona.
- **Znak.** Najmniejszy prawdziwy graf: jedno wejście, dwie równoległe gałęzie, jedna synteza.
  Cztery luźne kwadraty nie mają krawędzi, więc nie mają relacji, więc nie są grafem.

Promień bierze się z **roli**, nie z rozmiaru: kontrolka `--radius-sm`, pojemnik treści
`--radius-md`, rzecz nad treścią `--radius-lg`, rzecz, która jest odczytem — `--radius-pill`.
Piąta wartość jest piątą decyzją.

Akcent: `#6e76ff`. „Teraz": `#ff7a5c`. Pełna specyfikacja: `docs/design/DESIGN.md`.
---

## D2 — Nowe repo, czysty start

`~/Projects/Loadout` od zera. Poprzedni prototyp jest **źródłem pomysłów, nie kodu**.

Konsekwencja, której trzeba pilnować: kiedy w planie pojawia się „przenieśmy X z poprzedniego prototypu",
to znaczy „przeczytajmy jak X działa i napiszmy X od nowa, mniejsze". Kopiuj-wklej crate'a jest zakazany.

Powód: poprzednia wersja umarła na złożoność. Zaciąganie crate'ów zaciąga ontologię, która tę złożoność stworzyła.

---

## D3 — Claude Code **i** Codex w v1 — w aplikacji i w harnessie

*Zrewidowane 2026-08-15 po pierwszej rundzie pytań. Pierwotnie brzmiało „tylko Claude w v1";
użytkownik to cofnął. Dwóch vendorów jest teraz zakresem v1.*

**W aplikacji: pełna dowolność.** Każdy agent może mieć dowolnego vendora i dowolny model
z dowolnym poziomem wysiłku — np. Codex na `sol` z effortem `max`. Wybór vendora jest polem
w kreatorze agenta, nie ustawieniem globalnym.

**W harnessie budującym Loadout: wybieralna para pisarz/recenzent.** Wszystkie cztery kombinacje
muszą działać:

| Pisze | Recenzuje | Uwaga |
|---|---|---|
| Codex | Claude | |
| Claude | Codex | |
| Claude | Claude | inny model + rola recenzenta |
| Codex | Codex | inny model + rola recenzenta |

Kiedy recenzja biegnie, para jest domyślnie **cross-vendor**, bo według researchu każdy realny
defekt w pierwszej wersji spreadsheet znalazł właśnie recenzent innego vendora na **zielonej
bramce** (`docs/working-with-ai.md`, raport `06-spreadsheet-harness.md`). Same-vendor jest
wspierany, ale to słabszy tryb i tak ma być opisany.

*Zrewidowane 2026-08-28 decyzją Jakuba. Do tego dnia druga opinia była etapem KAŻDEGO biegu
harnessu, a jej uwaga odpalała rundę naprawczą. Zmierzone na 121 biegach: 97 recenzji na 105
zwracało uwagę, więc runda „doradcza" była obowiązkowa w 81% biegów i regularnie trwała dłużej
niż implementacja (T-103: 2 min implementacji, 45 min naprawy). Recenzja jest teraz **na
żądanie** — `ship.sh --review` albo `./review.sh` — i jest RAPORTEM: nie odpala rundy
naprawczej. Naprawę prowadzi paragon bramki, bo tylko on odróżnia „sprawdzenie padło" od
„ktoś ma zdanie".*

**Co z tej decyzji nie zostało ruszone:** wszystkie cztery kombinacje pisarz/recenzent muszą
działać, `AgentDriver` ma dwie implementacje od pierwszego dnia, recenzent nigdy nie może
zatwierdzić ani zablokować, a „recenzent niedostępny" to `exit 0` z notatką. Zmieniło się
wyłącznie to, **kiedy** recenzja biegnie.

Konsekwencje:

- `AgentDriver` ma **dwie** implementacje od początku: `ClaudeDriver` i `CodexDriver`. Trait z jedną
  implementacją to trait wymyślony; dwie sprawiają, że abstrakcja jest prawdziwa.
- `ship.sh` przyjmuje `--agent <vendor>` i `--reviewer <vendor>`, obie flagi niezależne.
- Recenzent **nigdy nie może zatwierdzić ani zablokować.** Schemat odpowiedzi ma `verdict ∈ {concern, none}`
  i `findings` z `maxItems: 6` — strukturalnie nie ma czego zatwierdzić.
- **Ryzyko operacyjne:** research odnotował, że Codex był bez kredytów do 2026-08-20 `[ran]`.
  Harness musi traktować „recenzent niedostępny" jako `exit 0` z notatką, nigdy jako czerwone.
  Niedostępność ≠ zepsute.

---

## D5 — Interfejs po angielsku, dokumentacja po polsku

UI: angielski. Przyciski, etykiety, komunikaty błędów, puste stany — `Run`, `Create`,
`Needs your answer`, `Couldn't reach the API`.

Powody: makieta jest po angielsku, tabela żargon→prosty-język z researchu (55 wierszy) jest po angielsku
i jest wiążąca, a część terminów po polsku brzmi gorzej niż po angielsku.

Dokumentacja, ADR-y, prompty biegów, komentarze w kodzie wyjaśniające *dlaczego*: polski.

Sprawdzacz słownictwa (`checks/quick-vocabulary.sh`) skanuje **wyłącznie tekst widoczny dla użytkownika**
i egzekwuje angielską tabelę z `FOUNDATIONS.md` §2.2.

---

## D4 — Kurowany strumień, PTY jako escape hatch

Agenci są odpalani z pipe'owanym stdio w trybie strumienia JSON. Renderujemy **własny widok**, nie emulator terminala.

- Widok wygląda jak terminal, ale każda linia jest naszą decyzją projektową, nie wyjściem procesu.
- Domyślnie zwinięte: wszystko poza tym, co się właśnie dzieje i co wymaga uwagi.
- Prawdziwe PTY: **odłożone**, jako przycisk „otwórz shell tutaj". Nie w v1.

Wzorzec docelowy (z odpowiedzi użytkownika):

```
❯ /plan zbuduj parser CSV

  ✓ plan gotowy · 4 kroki        [rozwiń]

  Forge   pisze  src/parser.rs
  Needle  testy  12 ✓  0 ✗
  Rivet   czeka  na Needle

❯ _
```

To jest docelowa gęstość informacji. Jeśli widok robi się gęstszy niż to — jest źle.

---

## D6 — Czym jest edytor workflow, a czym nie jest

*Zapisane 2026-08-15 na wyraźne polecenie użytkownika: „to jest ultra ważne".*

**Edytor workflow robi pięć rzeczy i tylko te pięć:**

1. **Kolejność i zależności** — kto po kim, co równolegle.
2. **Kontrola, który model pracuje** na danym kroku.
3. **Odpalenie kilku agentów naraz.**
4. **Synteza ich wyników** w jeden — krok, który czyta wiele przekazań i produkuje jedno.
5. **Dzielenie kontekstu i analiza z poziomu orchestratora** — orchestrator widzi, co wyprodukowali
   pozostali, i może na tym pracować.

**Czym nie jest: powtórką funkcji vendorów.** Nie konkurujemy z `--agents` Claude'a, jego skillami,
subagentami ani z czymkolwiek, co Anthropic albo OpenAI dowiozą w przyszłym miesiącu.

### Reguła wynikowa

**Wszystko, co vendor wprowadzi, konfigurujemy per agent — nigdy jako nowy typ węzła.**

Nowa flaga u Claude'a to nowe pole w definicji agenta. Nowy tryb u Codeksa to nowe pole w definicji
agenta. Liczba rodzajów kafelka na płótnie **nie rośnie od tego, ile funkcji dołożą vendorzy** —
i to jest cała treść tej reguły. Nowa flaga u Claude'a albo nowy tryb u Codeksa to zawsze pole
w definicji agenta, nigdy nowy kafelek.

### Trzeci rodzaj: „sprawdź". Dopisany 2026-08-20, świadomie

*Do 2026-08-20 ta reguła brzmiała „zostaje **dwa** (krok i punkt kontrolny)". Zmienione decyzją
człowieka po tym, jak brak trzeciego rodzaju zablokował dwa etapy naraz.*

Rodzajów jest **trzy**: krok, punkt kontrolny i **sprawdzenie**. Trzeci uruchamia komendę
należącą do Loadouta i **sam wystawia wynik** — z kodu wyjścia plus licznika przejść
(niezmiennik 19), nie ze zdania agenta.

**Dlaczego to nie jest złamanie reguły wyżej.** Ta reguła zabrania kafelków, które **powtarzają
funkcje vendorów** — i ten zakaz zostaje w mocy bez zmian. Żaden vendor nie dostarcza „uruchom
komendę i sam orzeknij, czy przeszła"; przeciwnie, cała ich powierzchnia zwraca to, co agent
**powiedział**. Rozróżnienie „co agent powiedział" kontra „co się stało" jest jedyną rzeczą,
dla której ten produkt powstał (`FOUNDATIONS.md` §2.1) — a bez tego rodzaju kroku nie ma go
czym wyrazić.

**Co ten brak kosztował, zmierzone.** `docs/harness-as-workflow.md` (ustalenie U-1) mierzy, czy
najcięższa znana ceremonia — pętla, którą to repo biegnie na sobie — da się zapisać jako zwykły
plik workflow. Odpowiedź: **cztery etapy z sześciu tak, dwa nie, i oba przewracają się o ten sam
brak.** Etap bramki i etap wejścia na trunk to komendy Loadouta z własnym wynikiem, więc stały
na kafelku kontrolnym, czyli na pytaniu do człowieka. Sam plik `harness_workflow_two_kinds.rs`
zgłaszał `check` jako brakujący rodzaj, z nazwy, od T-23.

**Czego ta zmiana NIE otwiera.** Czwarty rodzaj dalej wymaga prawdziwej skargi z pomiarem, nie
wygody. W szczególności **nie ma i nie będzie kafelka „recenzja"**: recenzent jest zwykłym
krokiem agenta, a etap nazwany w kodzie jest domyślny i nie da się go wyłączyć konfiguracją
(D7, niezmiennik 27). Wyrocznia z T-23 pilnuje teraz właśnie tego — odmowy dla rodzaju `review`.

### Konsekwencja projektowa, bez której ta reguła jest pustym hasłem

Definicja agenta i nadpisanie w węźle muszą mieć **przelotkę na opcje vendora** — surowe pole, które
leci prosto do argv albo do konfiguracji, bez pośrednictwa naszego modelu danych:

```jsonc
{
  "runsWith": "claude",
  "model": "opus",
  "vendorOptions": {                    // przelotka — Loadout nie interpretuje zawartości
    "claude": { "--jakas-nowa-flaga": "wartosc" },
    "codex":  { "model_reasoning_summary": "detailed" }
  }
}
```

Bez tego każda nowa flaga vendora wymaga **wydania Loadouta**. Z tym — wymaga wpisania jednej linii
w formularzu agenta, tego samego dnia, w którym vendor ją ogłosi.

Dwa ograniczenia, żeby przelotka nie stała się dziurą:

- **Kolizja jest odmową, nie nadpisaniem.** Jeśli przelotka podaje flagę, którą Loadout ustawia sam
  (`--session-id`, `--output-format`, `--permission-mode`, `-C`, `-s`), zapis workflow jest odrzucany
  z nazwaniem flagi. Cicha wygrana jednej ze stron to najgorszy możliwy wynik.
- **Przelotka nie omija dialu bezpieczeństwa.** Pole „co agent może zrobić z plikami" jest tłumaczone
  przez nas na flagi vendora; przelotka nie może go podnieść.

### Dlaczego to jest fosa, a nie wygoda

Vendorowy runner grafów orkiestruje **własnych** agentów. Edytor Loadouta orkiestruje **przez
vendorów**, z wyborem modelu na krok i syntezą wyników. Żaden vendor tego nie zbuduje, bo nie ma
w tym interesu. Trwałość edytora bierze się z pozycji cross-vendor — nie z płótna, które jest łatwe
do skopiowania. To podnosi rangę D3: dwóch vendorów w v1 to warunek istnienia przewagi, nie komfort.

---

## D7 — W aplikacji harness jest lekki domyślnie; długość definiuje workflow

*Zapisane 2026-08-15.*

Harness, którym **budujemy** Loadouta, jest ciężki: warstwy `before`/`quick`/`full`, reguła dowodu,
`NOT_A_REAL_RED`, obowiązkowa druga opinia, jedna runda poprawek, strażnicy. To jest właściwe dla
projektu, w którym agenci pracują godzinami bez nadzoru.

**Aplikacja nie może tego narzucać.** Ktoś, kto chce poprawić literówkę, nie będzie dowodził
czerwieni przed napisaniem kodu ani czekał na recenzenta innego vendora. To by było absurdalne
i nikt by z tego nie skorzystał drugi raz.

### Reguła

**Domyślnie: nic.** Workflow z jednym krokiem odpala agenta i pokazuje wynik. Bez bramki, bez
recenzji, bez rund naprawczych.

**Ceremonia jest elementem grafu, nie ustawieniem globalnym.** Każdy kawałek naszego ciężkiego
harnessu ma w aplikacji odpowiednik, który dokładasz świadomie:

| Mechanizm harnessu | W aplikacji |
|---|---|
| bramka (`verify.sh`) | krok typu „sprawdź" — uruchamia twoje checki |
| druga opinia | krok z agentem-recenzentem |
| zatwierdzenie człowieka | kafelek punktu kontrolnego |
| runda naprawcza | ustawienie kroku „jeśli próba się nie uda" |
| kopia plików per krok | przełącznik w modalu kroku |
| dowód czerwieni przed pracą | krok „sprawdź" **przed** krokiem piszącym |

Nic z tego nie jest domyślnie włączone. **Długość i głębokość ceremonii to konfiguracja workflow.**

### Konsekwencja dla silnika, która jest niezmiennikiem

**Żaden etap nie może być zaszyty w Ruście.** W `scheduler.rs` nie ma prawa istnieć
`if review_enabled`. Kolejność mieszka **wyłącznie w grafie** — silnik wykonuje graf i nie wie,
że coś takiego jak „recenzja" istnieje jako pojęcie. Krok z agentem-recenzentem jest dla silnika
zwykłym krokiem.

To jest jedyny sposób, żeby D7 była prawdziwa, a nie deklarowana: jeśli którykolwiek etap jest
w kodzie, to on jest domyślny i nie da się go wyłączyć konfiguracją.

### Pozorna sprzeczność z `ship-task.sh`, i jej rozwiązanie

`ship-task.sh` istnieje właśnie dlatego, że graf jest **w kodzie**: model, który dostaje sekwencję
w promptcie, pomija etap, kiedy uzna go za zbędny. Czy D7 tego nie łamie?

Nie — ochrona się przenosi, nie znika:

- **W harnessie** graf jest zamrożony w kodzie, bo to **agent** nie może pominąć etapu.
- **W aplikacji** graf zamraża **człowiek** w edytorze, a silnik pilnuje, żeby żaden agent nie
  zmienił go w trakcie biegu.

W obu przypadkach obowiązuje to samo: **bieg idzie po grafie, który zatwierdził człowiek, i nic
w trakcie tego nie przestawia.** Zmienia się tylko to, kto graf ustala.

### Co musi przetrwać nawet przy zerowej ceremonii

Rozdział trzech autorytetów. Przy workflow z jednym krokiem nie ma żadnych sprawdzeń — więc UI
mówi **„no checks configured"**, uczciwie, i nie pokazuje zieleni. Brak ceremonii znaczy „nikt tego
nie sprawdził", nigdy „sprawdzone i dobrze".

---

## Reguły nazewnictwa (z wymagań użytkownika)

Cała aplikacja mówi **prostym językiem, bez żargonu technicznego**. Nazwy przycisków: `Utwórz`, `Edytuj`, `Uruchom`.

Zakazane w UI: `ledger`, `work item`, `claim`, `rail`, `DAG`, `policy kernel`, `binding`, `artifact receipt`,
`plan.approval_requested`, `WI-31`, `A#8`, `authority`, `projection`, `durable record`.

Tabelę tłumaczeń żargon → język ludzki dostarcza `docs/FOUNDATIONS.md`.
Ta tabela jest **wiążąca** dla nazw w UI i dla nazw w kodzie frontendu.
