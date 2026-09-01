# Harness Loadouta wyrażony jako workflow Loadouta

Ten dokument jest pomiarem, nie projektem. Pytanie brzmi: czy najcięższa ceremonia, jaką
znamy — pętla, którą to repo biegnie na sobie samym — da się zapisać jako **zwykły plik
workflow**, bez dokładania czegokolwiek do silnika. Jeśli tak, długość ceremonii definiuje
konfiguracja, a nie kod, i decyzja D7 jest prawdziwa, a nie zadeklarowana. Jeśli nie —
któryś etap jest zaszyty w Ruście i to jest złamanie niezmiennika 27.

Mierzonym plikiem jest [`.loadout/workflows/ship-task.json`](../.loadout/workflows/ship-task.json).

## Odpowiedź

**Cztery etapy z sześciu wyrażają się czysto. Dwa nie — i oba przewracają się o tę samą
brakującą rzecz.**

Nie ma rodzaju kroku, który **uruchamia komendę należącą do Loadouta i sam mówi, czy
przeszła**. Etap sprawdzenia i etap wejścia na trunk to dokładnie takie komendy
(`./verify.sh full`, `git merge --no-ff` plus ponowne sprawdzenie), więc oba stoją dziś na
kafelku kontrolnym, czyli na pytaniu do człowieka.

Nie jest to niedopatrzenie ani wygoda i **nie wolno tego naprawić dopisaniem trzeciego
rodzaju kafelka**. Gdyby te dwa etapy zrobić krokami agenta o instrukcji „uruchom
`./verify.sh full` i powiedz, czy przeszło", plik by się zwalidował, bieg by wystartował,
transkrypt powiedziałby `checks passed` — i sprzedalibyśmy jedyne rozróżnienie, dla którego
ten produkt powstał: **co agent powiedział** kontra **co się stało**
(`FOUNDATIONS.md` §2.1). Loadout uruchamia sprawdzenia sam; nie pyta agenta, czy zadziałało.
Pytanie do człowieka jest tu odpowiedzią **uczciwą**: ktoś naprawdę musi to dziś zrobić ręcznie.

## Sześć etapów, pięć kafelków

```
  s_implement  →  s_gate  →  s_review  →  s_fix  →  s_land
  Write the       Run the    Second       One fix   Land the
  code            checks     opinion      round     branch
  (agent)         (kontr.)   (agent)      (agent)   (kontr.)
       ▲
       └─ etap „workspace" siedzi tutaj, jako pole folder.use = fresh-copy
```

Etapów jest sześć, kafelków pięć, i ta różnica jest ustaleniem, nie zaokrągleniem. Etap
workspace **nie dostaje kafelka**: własna kopia plików jest polem kroku, który jej potrzebuje
(T3 §3.1, `Folder`). Kafelek „utwórz workspace" byłby krokiem agenta raportującym własny
efekt uboczny — czyli tą samą awarią co wyżej, tylko o jeden etap wcześniej.

Kolejność jest łańcuchem, nie sugestią: `s_implement` jest jedynym źródłem, `s_land` jedynym
ujściem, a żadna para kroków nie może być gotowa naraz. Powód nie jest estetyczny — harness
wchodzi na trunk **po jednej gałęzi naraz i przepuszcza pełne sprawdzenie po każdej**
`[06 §10.7]`. Z `s_review` nie wychodzi żadna krawędź wstecz: schemat odpowiedzi recenzenta ma
`concern` albo `none`, więc nie ma czym zatwierdzić ani zablokować, a strzałka zawracająca
oddałaby mu tę władzę tylnymi drzwiami — „wróć i popraw, aż uwag nie będzie" to jest właśnie
ta nieograniczona pętla, przez którą jedno zadanie zajmuje cały dzień.

## Blok, który czyta test

Poniższa lista jest jedynym miejscem, w którym ten dokument mówi coś o pliku, i jest czytana
przez `src-tauri/tests/harness_workflow_findings_match_doc.rs` (AC-6). Dokument, którego nikt
nie parsuje, rozjeżdża się z plikiem w ciągu tygodnia — więc każde zdanie o tym, co **da się**
wyrazić, ma tu swój odpowiednik, a test sprawdza odwzorowanie w obie strony: pozycja bez
kafelka jest czerwona tak samo jak kafelek bez pozycji.

Klucze `stage` są nazwami z wyroczni (`STAGES` w pliku testu) i dlatego stoi wśród nich
`gate`, słowo zakazane w tekście widocznym dla użytkownika. To jest klucz danych w bloku,
który czyta wyłącznie test — na ekran idą nazwy z pliku: `Run the checks`, `Land the branch`,
`Second opinion`, `One fix round`, `Write the code`.

```json
[
  {
    "stage": "workspace",
    "expressible": true,
    "as": "step-property",
    "where": "/steps/0/folder/use",
    "value": "fresh-copy",
    "note": "Nie kafelek, tylko pole kroku, który tej kopii potrzebuje. Osobny kafelek byłby agentem raportującym własny efekt uboczny."
  },
  {
    "stage": "implement",
    "expressible": true,
    "as": "agent",
    "where": "s_implement",
    "note": "Zwykły krok agenta. Jedyny etap harnessu, który jest krokiem agenta bez zastrzeżeń."
  },
  {
    "stage": "gate",
    "expressible": false,
    "missing_kind": "check",
    "stand_in": "s_gate",
    "note": "Brakuje rodzaju kafelka, który uruchamia komendę należącą do Loadouta i sam wystawia wynik. Dziś: pytanie do człowieka, bo krok agenta raportujący własne sprawdzenie sprzedaje rozróżnienie agent-powiedział/stało-się."
  },
  {
    "stage": "second-opinion",
    "expressible": true,
    "as": "agent",
    "where": "s_review",
    "note": "Krok agenta bez krawędzi wstecz. Cross-vendorowość jest własnością dwóch definicji agentów, nie grafu — patrz ustalenie U-3."
  },
  {
    "stage": "fix",
    "expressible": true,
    "as": "agent",
    "where": "s_fix",
    "note": "Wyrażalny, ale bezwarunkowy: schemat nie ma warunków, więc biegnie także bez uwag. Koszt policzony w ustaleniu U-4."
  },
  {
    "stage": "land",
    "expressible": false,
    "missing_kind": "check",
    "stand_in": "s_land",
    "note": "Ten sam brak co przy sprawdzeniu: merge plus ponowne sprawdzenie to komenda Loadouta z własnym wynikiem, nie zdanie agenta. Jeden brakujący rodzaj blokuje dwa etapy."
  }
]
```

## Ustalenia o edytorze

Cztery. Pierwsze jest tym, po co to zadanie powstało; pozostałe trzy wyszły po drodze
i zostają nazwane, bo etap, o którym nikt nie musiał powiedzieć, czy się wyraża, jest
etapem policzonym jako wyrażalny.

### U-1 — brak rodzaju kroku, który sam uruchamia komendę i sam wystawia wynik

**Blokuje: `gate`, `land`.** Zamknięty zbiór to dziś `agent` i `checkpoint` (T3 §3.1).
Pierwszy odpala vendora i wraca z tym, co agent **powiedział**. Drugi zatrzymuje bieg
i pyta człowieka. Żaden nie umie tego, co robi `verify.sh`: uruchomić komendę, której
właścicielem jest Loadout, przeczytać jej wynik i **samemu** orzec, czy przeszła.

Czego taki rodzaj musiałby umieć, żeby te dwa etapy przestały być pytaniami:

- uruchomić komendę wprost z silnika, nie przez sesję agenta;
- wystawić wynik z tego, co się stało — kod wyjścia **plus licznik przejść**, bo samo
  zero jest twierdzeniem, nie dowodem (niezmiennik 19);
- pokazać ten wynik jako fakt Loadouta, a nie jako zdanie agenta.

**Świadomie tego nie dopisujemy.** Zmiana schematu dotyka `src-tauri/src/workflow/**` (T-12)
i płótna (T-13), a to są ścieżki spoza bloku OWNS tego zadania — wejście w nie jest
zatrzymaniem się i pytaniem do człowieka (AGENTS.md §7). Dopisanie brakującego rodzaju po
cichu, żeby graf się zmieścił, byłoby odpowiedzią „tak, da się" na pytanie, którego to
zadanie nie zadało.

### U-2 — `fresh-copy` jest kluczowane per krok, więc łańcuch nie umie dzielić jednej kopii

Etap workspace wyraża się jako pole i to jest w bloku wyżej. Ale harness ma **jedną**
kopię repo, którą `worktree.sh` wycina raz i w której pracują po kolei implementacja,
sprawdzenie, druga opinia i poprawka. Tego schemat nie umie powiedzieć.
`Folder` ma trzy warianty — `project`, `fresh-copy`, `pick { path }` — i nie ma wśród nich
„ta sama kopia, którą zrobił krok wcześniej". W silniku `fresh-copy` rozwiązuje się na
`<katalog biegu>/work/<id kroku>` (`src-tauri/src/commands/run.rs:567`), czyli osobny katalog
**dla każdego kroku z osobna**.

Skutek w tym pliku jest widoczny gołym okiem i taki miał zostać: `s_implement` ma
`fresh-copy`, a `s_review` i `s_fix` mają `project`. Żadne z tych dwóch nie jest prawdą
o harnessie — recenzja czyta wtedy folder projektu zamiast gałęzi, a poprawka pisze po
projekcie zamiast po tym, co napisał pisarz. Dałoby się je też ustawić na `fresh-copy`, ale
to jest gorsze, nie lepsze: każdy dostałby **własny** katalog i poprawka nie zobaczyłaby
kodu, który ma poprawić. Wybraliśmy wariant mniej udający, że działa.

Obejściem nie jest `pick { path }`: ścieżka workspace'u powstaje dopiero przy starcie biegu,
a plik workflow zna ją przed nim. Wpisanie tam ścieżki na sztywno przypięłoby graf do jednego
zadania na jednej maszynie.

### U-3 — para cross-vendorowa nie jest widoczna w grafie

Decyzja D3 mówi, że pisarz i druga opinia są domyślnie różnych vendorów i że wszystkie cztery
kombinacje muszą działać. W grafie tego nie widać i nie da się tego wymusić: krok nazywa
**agenta**, a vendor, model i uprawnienia mieszkają w definicji agenta (T3 §3.1) — świadomie,
bo inaczej zmiana modelu dzieje się w sześciu kafelkach zamiast raz. Graf umie więc powiedzieć
tylko tyle, że `s_review` wskazuje **innego** agenta niż `s_implement`, a `s_fix` tego samego.
Czy ta inność jest innością vendora, rozstrzyga się piętro niżej.

To nie jest defekt schematu — to jest granica tego, co ten plik poświadcza. AC-4 jest napisane
dokładnie tak: sprawdza relacje między krokami, nigdy nazwy vendorów.

### U-4 — runda poprawek jest bezwarunkowa i to kosztuje jedną turę na bieg

Schemat nie ma warunków ani wyrażeń, a strzałka znaczy „po" i nic więcej (T3 §1, §6.2).
`s_fix` biegnie więc **zawsze** — także wtedy, gdy druga opinia nie zgłosiła ani jednej uwagi.

Koszt, zapisany jako koszt: **jedna dodatkowa tura agenta w każdym biegu, w którym uwag nie
było.** W harnessie skryptowym tej tury nie ma, bo `ship-task.sh` sprawdza wynik recenzji
warunkiem w powłoce. Tutaj nie ma czym tego wyrazić, więc płacimy turą. Warunki są
nieplanowane (`docs/PLAN.md` §7) i ta pozycja ma zostać widoczna, dopóki tak jest.

Z tego samego braku pętli wynika, że „dokładnie jedna runda" jest **jednym literalnym
krokiem**, a nie licznikiem. Cztery próby łącznie i eskalacja do człowieka zostają w
`ship-task.sh`; graf ich nie zna i znać nie będzie.

## Kto czyta ten plik

Dokładnie jeden czytelnik: testy AC-1..AC-6. **`ship-task.sh` nie czyta
`.loadout/workflows/ship-task.json` i nie będzie go czytał w v1** — dopóki etap sprawdzenia
nie ma swojego rodzaju kafelka (U-1), harness sterowany tym plikiem byłby harnessem, w którym
sprawdzenie robi agent.

Niezmiennik 21 mówi „nie pisz artefaktu, którego żaden skrypt nie czyta", więc trzeba to
powiedzieć wprost, a nie przemilczeć: ten plik **jest** czytany — przez wyrocznię tego
zadania — i to jest świadoma, nazwana pozycja. Gdyby nie był czytany przez nic, byłby
`design/<task>/plan.json` z repo źródłowego: pisany co bieg, czytany nigdy `[07 §9]`.
Dokument i plik trzymają się nawzajem: pozycja bez kafelka i kafelek bez pozycji są
jednakowo czerwone.

Ten plik dziś **nie jest wykonywalny** i to też jest wynik pomiaru, nie brak. Poza U-1
stoi na przeszkodzie jeszcze jedno: pola `agent` niosą identyfikatory, których biblioteka
nie zawiera. T-11 nie dostarcza wbudowanych agentów o stałych identyfikatorach, a walidacja
tych pól nie rozwiązuje — `check()` nigdy nie czyta `AgentStep::agent`, więc plik jest
poprawny, a bieg by ich nie znalazł (`find_agent` w `src-tauri/src/commands/run.rs`).
Zanim ktokolwiek spróbuje to uruchomić, dwa identyfikatory trzeba wskazać na własnych
agentów: `...2301` to pisarz (kroki `s_implement` i `s_fix`), `...2302` to druga opinia
(`s_review`).
