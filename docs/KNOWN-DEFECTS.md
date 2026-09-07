# Znane wady

Wady zmierzone, opisane i **świadomie niezamknięte** przed wydaniem. Wpis stąd wychodzi tylko
jedną drogą: razem z poprawką i kryterium, które ją sądzi.

Ten plik nie jest listą życzeń ani backlogiem. Wchodzi tu wyłącznie coś, co (1) da się
odtworzyć, (2) ma nazwane miejsce w kodzie i (3) ma opisany skutek dla człowieka. Wada bez
tych trzech rzeczy jest podejrzeniem, a podejrzenia nie wydajemy razem z aplikacją.

---

## D-1 — odpowiedź udzielona w chwili końca biegu znika z ekranu

**Zmierzone:** 2026-09-07, na gałęzi `feat/reliability-native-qa-generator`.
**Weszło z:** `f81d4b11` (snapshot WIP `workflow-reliability-build`, zadanie WF-08).
**Wydanie:** obecne w 0.3.0.

### Co się dzieje

Strumień biegu przyjmuje odpowiedź człowieka wyłącznie wtedy, gdy pytanie o tym numerze stoi
jeszcze w kolejce oczekujących (`recordAnswer` w `src/sections/run/feed/model.ts`). Strażnik
jest tam z dobrego powodu: potwierdzenie z granicy i zdarzenie biegu przyjeżdżają w dowolnej
kolejności, a jedna przyjęta odpowiedź ma dać dokładnie jeden wiersz, nie dwa.

Ścieżka, na której ten strażnik odrzuca odpowiedź prawdziwą, jest jedna i wąska.
`src/sections/run/index.tsx` woła `feed.answer(...)` dopiero **po** powrocie z granicy. Jeżeli
w tym oknie bieg się skończy, `runEnded()` czyści kolejkę oczekujących — i wtedy odpowiedź,
którą człowiek naprawdę kliknął, nie zostaje zapisana nigdzie.

### Co widzi człowiek

Wiersz „you answered…" nie pojawia się pod pytaniem, chociaż przycisk został naciśnięty
i odpowiedź poszła do Rusta. Na kafelku agenta w rejestrze może zostać „waiting on you" przy
pytaniu, na które już odpowiedziano. Sama odpowiedź **nie ginie** — dotarła do biegu i jest
w jego zapisie na dysku; nieprawdziwy jest wyłącznie obraz na ekranie, do czasu ponownego
odczytu historii biegu.

### Czego to NIE dotyczy

Zwykłej pracy. Kontrolka odpowiedzi wisi na przypiętym pytaniu, czyli na najstarszym
oczekującym, więc w chwili kliknięcia pytanie stoi w kolejce i strażnik nie odpala. Poza tym
jednym oknem rodzaj `questionAnswered` z Rusta zapisuje odpowiedź drugą drogą i obraz jest
poprawny.

### Dlaczego nie jest zamknięta w 0.3.0

Właściciel zdecydował 2026-09-07, że ta wada zostaje opisana, a nie naprawiona przed wydaniem.
Poprawka jest mała — warunek w `recordAnswer` ma odrzucać tylko odpowiedź **już zapisaną**,
zamiast każdej bez oczekującego pytania, plus zbiór odpowiedzianych numerów, żeby sprawdzenie
nie było liniowe. Żaden inny plik się nie rusza.

Zamknięcie wymaga kryterium, które sądzi zdanie na ekranie: odpowiedź kliknięta w chwili końca
biegu ma stać pod swoim pytaniem. Dopóki takiego kryterium nie ma, poprawka byłaby zmianą bez
wyroczni, a to jest dokładnie ta klasa zmian, dla której to repozytorium powstało.

### Co NIE jest tą wadą

Sufit listy odpowiedzi (`LINE_LIMIT`) działa i jest sądzony. Kryterium
`src/sections/run/tabs/a-closed-terminal-frees-what-it-held.test.ts` mierzyło go do 2026-09-07
bocznymi drzwiami — odpowiadało na pytania, których nikt nie zadał, więc po dołożeniu strażnika
mierzyło już tylko strażnika. Scena zadaje dziś pytania, zanim na nie odpowie. Dowód mutacyjny:
ze zdjętym `answers.slice(-LINE_LIMIT)` przypadek jest czerwony na zdaniu „the stream answer
list grew past the same ceiling as its line window" (2001 zamiast 2000).
