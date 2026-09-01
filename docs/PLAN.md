# Loadout — plan budowy

2026-08-15 · v1 · czytaj po `docs/ARCHITECTURE.md`

Kolejność nie jest listą życzeń. Jest ułożona tak, żeby **najwcześniej obalić najdroższe założenia**.
Każde zadanie ma plik w `tasks/`, a każdy plik cytuje raport z researchu, który daną decyzję uzasadnia.

---

## 1. Reguła kolejności

Faza 1 kończy się jednym zdaniem, które albo jest prawdą, albo cały plan trzeba przemyśleć:

> **Naciskam Start i dwa prawdziwe procesy `claude` pracują jednocześnie, każdy w swojej kopii repo,
> a ja widzę czysty widok, który się nie przewija sam.**

To jest szkielet chodzący. Dopóki go nie ma, nie budujemy ani edytora workflow, ani pamięci,
ani umiejętności. poprzedni prototyp zbudował wszystko naokoło i **nigdy nie uruchomił agentów naprawdę
równolegle** (`docs/handoff.md:144-165`: cztery „równoległe" pasy w rozłącznych oknach po ~0,5 s).

---

## 2. Faza 0 — spike'i

Trzy niewiadome, które zmieniają projekt UI albo API. Każdy to najwyżej pół dnia i kończy się
akapitem w `docs/research/topics/`, nie kodem produkcyjnym.

> **Spike'i są zwolnione z pasma 5–8 kryteriów.** Mają po dwa i tak ma zostać. Kryterium spike'a
> brzmi „odpowiedź jest zapisana i jednoznaczna" — dopisanie trzech kolejnych oznaczałoby
> wymyślenie trzech plików testowych, których jedynym czytelnikiem jest licznik, czyli wprost
> niezmiennik 21. Zapisane tutaj, bo agent spójności słusznie odmówił załatania tego mechanicznie
> i oddał decyzję człowiekowi.

| # | Pytanie | Co blokuje | Jak sprawdzić |
|---|---|---|---|
| **S-1** | Czy sesja Claude może dostać **podzbiór** umiejętności? | Modal kroku ma przełącznik „wybierz umiejętności". `--disable-slash-commands` jest wszystko-albo-nic; `--plugin-dir` i `--setting-sources` wyglądają obiecująco, ale nikt tego nie zweryfikował [T3 §11.1]. | Odpal `claude -p` z kandydatami na flagi i przeczytaj `skills` w zdarzeniu `system/init`. Jeśli się nie da — modal degraduje się do Wszystkie/Żadne. **Zdecyduj przed budową UI.** |
| **S-2** | Czy `--max-turns` i `--max-budget-usd` istnieją? | T1 mówi tak (sonda po komunikacie o brakującym argumencie), T4 mówi nie (brak w `--help`). Sprzeczność w naszym własnym researchu [ARCHITECTURE §11]. | Odpal je z prawdziwą wartością na trywialnym promptcie. Do rozstrzygnięcia **nie budujemy na nich** — limit czasu ściennego działa u obu vendorów. |
| **S-3** | Czy `codex exec --json` działa end-to-end? | `CodexDriver` i cross-vendorowa druga opinia. Research nie mógł tego sprawdzić — konto było bez kredytów do 2026-08-20 [T1 ryzyko 8]. | Trywialne zadanie, zapisz surowy strumień do `docs/research/fixtures/codex-stream.jsonl`. Ten plik staje się złotym testem parsera. |

---

## 3. Faza 1 — szkielet chodzący

| ID | Zadanie | Zależy od | Dlaczego tutaj |
|---|---|---|---|
| **T-01** | Powłoka aplikacji: okno Tauri, pięć sekcji, tokeny, bez routera | — | Wszystko inne potrzebuje miejsca, w którym się wyświetli |
| **T-02** | Silnik: graf + planista (Kahn, `JoinSet`, `Semaphore`, `CancellationToken`) na `FakeDriver` | T-01 | Planista musi być testowalny **bez** prawdziwego agenta. Testy property. |
| **T-03** | Silnik: nadzór procesów — grupy, SIGTERM→SIGKILL, **dowód, że nie żyje** | T-02 | Osierocony agent pali limit w tle. To błąd finansowy, nie higieniczny. |
| **T-04** | `AgentDriver` + `ClaudeDriver`: długo żyjący proces, dwukierunkowy stdin, sesja z góry | T-03 | Najdroższa niewiadoma techniczna. Im wcześniej, tym lepiej. |
| **T-05** | Strumień: NDJSON → `AgentEvent` → `Line` — **tu jest kuracja** + surowe tee na dysk | T-04 | Wartość produktu powstaje w tym mapowaniu, nie w CSS |
| **T-06** | Magazyn: schemat SQLite, jeden pisarz, migracje, wyzwalacze append-only | T-02 | Potrzebny, zanim pojawi się coś do odtworzenia |
| **T-07** | IPC: pompa sklejająca 16 ms / 2000 linii, `Channel<Vec<Line>>` | T-05, T-06 | Zmierzone: ~70× taniej niż `emit` i lepsza najgorsza klatka |
| **T-08** | Widok pracy: dwie strefy, wirtualizacja, pięć reguł zwijania, pasek loadoutu | T-07 | Pierwszy moment, w którym widać, czy teza „nie przyrasta" działa |
| **T-09** | Szyna agentów + sesja agenta („co dostał" / „co wyprodukował") | T-08 | |
| **T-10** | `CodexDriver` — `exec --json`, wznawianie per tura | T-04, S-3 | **Pierwszy prawdziwy test, czy trait jest abstrakcją, czy fikcją** |

**Bramka fazy:** zdanie z §1 jest prawdą, a test dowodzi **nakładania się w czasie** dwóch agentów
(nie tylko tego, że oba się skończyły).

---

## 4. Faza 2 — konfigurowalność

| ID | Zadanie | Zależy od | Uwaga |
|---|---|---|---|
| **T-11** | Definicje agentów: 9 pól widocznych, 3 pod „More settings", zapis na dysk | T-01 | Pole wchodzi tylko, jeśli zauważyłbyś jego brak w pierwszej godzinie [T4 §3] |
| **T-12** | Format pliku workflow + walidacja w Ruście (cykle, nakładające się ścieżki, osierocone kroki) | T-02 | Walidacja przy **zapisie**, nie w trakcie biegu |
| **T-13** | Płótno drag&drop (`@xyflow/react`), modal kroku, **znacznik nadpisań** | T-12, T-11, S-1 | Dwa rodzaje kafelka **niezależnie od tego, ile funkcji dowiozą vendorzy** (D6). Trzeci wymaga prawdziwej skargi |
| **T-14** | Lista workflow: utwórz / duplikuj / usuń | T-12 | |
| **T-15** | Uruchom workflow z płótna | T-13, T-08 | Domyka pętlę: buduję → uruchamiam → widzę |
| **T-24** | Workspace'y i karty: kilka folderów naraz, bez utraty sesji | T-08, T-21 | Dodane 2026-08-15. Wymusza **globalny** semafor: trzy karty × trzech agentów = zamrożony laptop. Model w `ARCHITECTURE.md` §6a |

---

## 5. Faza 3 — pamięć i umiejętności

| ID | Zadanie | Zależy od | Uwaga |
|---|---|---|---|
| **T-16** | Pliki przekazań: front-matter pisze **Loadout**, agent daje tylko treść | T-05 | Agent, który wymyśla metadane, zmyśli je |
| **T-17** | Sekcja Pamięć: dwa stany, promocja **wyłącznie przez człowieka** | T-16, T-06 | Bez człowieka notatka nigdy nie trafia do promptu |
| **T-18** | Umiejętności: silnik rozmieszczania (kanoniczny folder → 2 katalogi → 6 vendorów) | T-11 | ~300 linii. Nie ma kompilatora — format się skonwergował [T5 §0] |
| **T-19** | Wciąganie umiejętności z URL: pobranie, **wykrycie wstrzyknięcia**, test na śmieciowym pliku | T-18 | Tu jest cała prawdziwa trudność tej funkcji |

---

## 6. Faza 4 — zaufanie

| ID | Zadanie | Zależy od | Uwaga |
|---|---|---|---|
| **T-20** | Odzyskiwanie po awarii: `interrupted`, sprzątanie po `pgid` z zabezpieczeniem czasu startu, **pytaj, nie zgaduj** | T-03, T-06 | Ponowne użycie PID to realne zagrożenie poprawności, nie teoretyczne |
| **T-21** | Limity dostawcy + pauza/wznowienie + suwak „ile naraz" | T-02 | 583 MB na agenta: na 16 GB realnie 3–4. Domyślnie 3 i **uczciwy opis** |
| **T-22** | Sprawdzacze w bramce: słownictwo, gęstość, tokeny, granice modułów | T-08 | Baseline może tylko maleć |
| **T-23** | Harness Loadouta wyrażony **jako workflow Loadouta** | T-15, T-13 | Jedyny test, czy edytor jest wystarczająco ekspresyjny (ARCHITECTURE §2, pyt. 6) |

---

## 6a. Zobowiązanie: przejazd cross-vendor po odblokowaniu Codeksa

*Zapisane 2026-08-15, kiedy zapadła decyzja o budowie w pętli bez Codeksa.*

Konto Codex jest bez kredytów **do 2026-08-20** (`docs/research/topics/T1-agent-drivers.md`
ryzyko 8). Do tego czasu harness jedzie na parze **claude + claude**: inny model recenzenta,
rola recenzenta, sandbox read-only i schemat bez „zatwierdzam". To działa, ale jest **słabszym
trybem** i skrypt sam to wypisuje.

Dlaczego to nie jest formalność: według `docs/research/projects/06-spreadsheet-harness.md`
**każdy realny defekt w pierwszej wersji repo źródłowego znalazł recenzent innego vendora
na ZIELONEJ bramce.** Nie bramka. Nie recenzent tego samego vendora. Cross-vendor, na kodzie,
który przeszedł już wszystkie testy.

Więc po 20 sierpnia, zanim cokolwiek zostanie uznane za skończone:

1. `S-3` — pierwszy prawdziwy strumień z Codeksa do `docs/research/fixtures/codex-stream.jsonl`.
2. `T-10` — `CodexDriver` na tym złotym pliku.
3. **Przejazd `review.sh --reviewer codex` po każdej gałęzi, która wylądowała w trybie
   same-vendor.** Nie po diffie zbiorczym — po zadaniach, pojedynczo, bo recenzent czyta
   `TASK.md` razem z kryteriami i pyta o to jedno: *czy implementacja spełnia KRYTERIUM,
   czy tylko ASERCJĘ napisaną pod nie?*

Definicja zrobienia: każde zadanie zbudowane przed 20 sierpnia ma w `runs/<ID>/` plik
`review-codex.json`, a wszystkie uwagi są albo naprawione, albo odrzucone z pisemnym powodem.

**Dlaczego to nie jest plik w `tasks/`.** Bramka parsuje z zadania `## AC-n` i `check:`
uruchamiające jeden plik testowy. Przejazd recenzencki nie ma takiego kształtu — wciśnięcie go
w ten format oznaczałoby wymyślenie kryteriów, których jedynym czytelnikiem jest licznik,
czyli niezmiennik 21. Zobowiązanie mieszka tutaj i jest sprawdzalne oczami.

## 6b. Faza 6 — kontekst, pętle, learningi (dopisane 2026-08-23)

Analiza 23 biegów właściciela (0 zakończonych `succeeded`) i audyt kodu pokazały, że silnik
jest poprawny, a warstwa nad nim — kontrakt z agentem, kontekst między krokami, pętla uczenia —
jest w połowie nieistniejąca. Plan wykonawczy, mapa znalezisk i kolejność fal są
w [`docs/PLAN-AGENTS-CONTEXT.md`](PLAN-AGENTS-CONTEXT.md); tutaj tylko kolejność zależności.

| ID | Zadanie | Zależy od | Uwaga |
|---|---|---|---|
| **T-86** | Każdy krok agenta wie, jak oddać wynik i ile ma czasu | T-80 | Kontrakt Loadout↔agent wypowiedziany w prompcie |
| **T-89** | Kafelek „sprawdź" da się postawić i ustawić z płótna | — | Jedyny węzeł „co się stało"; równolegle z T-86 |
| **T-87** | Pętla pamięta swoje rundy, fan-in dostaje to, co przeszło | T-86 | Opcja B: nowa sesja, pełny kontekst pętli |
| **T-88** | „Pick up here" niesie przekazania poprzedniego biegu | T-87 | |
| **T-90** | Cztery martwe pola kroku dostają skutek | T-88 | `copies`, `vendorOptions`, `writeResultsTo`, `handover` |
| **T-96** | Sesja agenta mówi prawdę o tym, co dostał | T-88 | Front; równolegle z T-90 |
| **T-91** | Poziom myślenia dociera do obu vendorów | T-90 | `--effort` i `model_reasoning_effort` istnieją (zmierzone) |
| **T-92** | Learningi mają producenta | T-91 | Refleksja po biegu z T6 §5.3; auto-pamięć Claude'a w biegu |
| **T-93** | Dziedziczenie z repo gospodarza ma nośnik | T-92 | `borrow` na kroku |
| **T-94** | Jedna pula na aplikację, budżet biegu, ciężki slot | T-93 | S-2 rozstrzygnięte: `--max-budget-usd` istnieje |
| **T-95** | Po biegu nie zostają kopie ani gałęzie bez pracy | T-94 | |
| **T-97** | Codex na równi z Claude'em | T-95 | Kuracja, sieć, narzędzia, tokeny, Lead |
| **T-159** | Prywatny stan Claude'a zachowuje systemowe logowanie | T-109 | Osobny secure storage namespace bez kopiowania tokenów |
| **T-160** | Claude `inherit` używa domyślnego modelu | T-159 | Sentinel subagenta nie trafia jako literalne `--model inherit` |
| **T-150** | CLI działa po uruchomieniu z Docka/Finder | T-97 | Absolutne ścieżki obu vendorów i ludzka odmowa zamiast `os error 2` |

## 6c. Faza 8 — domknięcie produkcyjne po audycie 2026-08-27

Audyt aktualnego kodu i pięciu faktycznych workflowów potwierdził prawdziwą współbieżność
wewnątrz jednego biegu oraz działający tekstowy handoff. Ujawnił jednocześnie dwa braki rdzenia:
równoległe kopie nie składają zmian w plikach, a aplikacja ma jeden globalny uchwyt biegu.
Pozostałe zadania zamykają utratę nowszych bajtów, pre-start cleanup, nieograniczone bufory,
retencję zamkniętych sesji, prywatny transport oraz operacyjne mnożenie odmów.

### Tryb wykonania całej fazy

- Pisarz: **Codex**. Druga opinia: osobny **Codex na innym modelu**, tylko do odczytu. Jest to
  jawnie zatwierdzony przez właściciela wariant D3 z powodu małego budżetu Claude'a.
- Zadania idą bez `ship-task.sh`: osobny worktree, uczciwe czerwone `before`, implementacja,
  quick/full, `./review.sh --agent codex --reviewer codex`, najwyżej jedna naprawa przez
  `./repair.sh --agent codex --reviewer codex` i pojedyncze `integrate.sh`.
- Nie powstaje żadne zadanie zmieniające `harness/`, `checks/` ani `verify.sh`. Ręczna korekta
  lokalnych JSON-ów jest operacją po kodzie, nie fikcyjnym artefaktem w repo.
- Worktree mogą powstawać równolegle wyłącznie przy rozłącznym `OWNS`; ciężkie Cargo zawsze
  biegnie szeregowo (niezmiennik 26).

| ID | Zadanie | Zależy od | Dlaczego osobno |
|---|---|---|---|
| **T-151** | Nowsze bajty wygrywają, a Run używa widocznej rewizji | T-150, T-152 | Jeden kontrakt świeżości dla pamięci i plikowej prawdy; jawnie zastępuje wadliwą część T-139 |
| **T-152** | Każda próba biegu rozlicza się raz i sprząta przygotowanie | T-150 | Transakcja izolacji, jedna polityka start-error, bounded cleanup i uczciwy receipt |
| **T-153** | Równoległe gałęzie składają się w jedno działające drzewo | T-152 | Główna obietnica produktu: fizyczny fan-in + poprawna lineage kopii, bez nowego rodzaju kafelka |
| **T-154** | Receipt, prompt i capability opisują ten sam kontekst | T-151, T-153, T-156 | Fail-closed rerun, zamrożone skills i exact per-step context |
| **T-155** | Jeden bieg na workspace, wiele workspace'ów naraz | T-154 | Realizuje etap B świadomie odłożony przez T-69/T-71/T-65, zachowując jeden globalny limiter |
| **T-156** | Bufory i zamknięte sesje mają skończony lifecycle | — | Niezależna fala: Check/driver bounds, Serve reap, terminal sinks i frontend eviction |
| **T-157** | Prywatne instrukcje i sekrety nie trafiają do argv ani evidence | T-155 | Jedna wspólna granica prywatnego transportu i redakcji obu vendorów |
| **T-158** | Błędne tło nie mnoży prób ani logu bez końca | T-155 | Typed quarantine, kompaktowanie trigger ledgeru i rotacja lokalnego logu |

### Fale bez konfliktów `OWNS`

```text
Gotowe:  T-150 (wylądowało na main jako f665256)
Fala 0:  T-152               ||  T-156
Fala 1:  T-151               ||  T-153
Fala 2:  T-154
Fala 3:  T-155
Fala 4:  T-157               ||  T-158
```

### Operacyjne domknięcie — bez taska i bez Harnessu

1. Pierwszą czynnością operatora jest wyłączenie triggera `Urc` i zapisanie, czy wcześniej był
   włączony. Pozostaje wyłączony do końca tej checklisty.
2. Po T-153/T-154 Codex robi timestampowany backup całego `~/.loadout/workflows`, a potem
   przygotowuje wszystkie pięć zmian jako jedną transakcję z rollbackiem:
   - `Murmur-1`: Combine i QA kontynuują zintegrowaną pracę w plikach;
   - `Urc`: Serve stoi po integracji i sprawdzeniu kodu, nie jako niezależny root;
   - `Reaserch + implement`: C1/C2 używają osobnych kopii, a Implement kontynuuje ich fan-in;
   - `Deep reaserch`: każda para kopii używa `{{copy}}`/`{{copies}}` i własnej lineage;
   - `Easy`: pozostaje proste; realny Check dochodzi tylko wtedy, gdy workflow ma dostarczać kod.
3. Każdy kandydat przechodzi `jq -e`, produkcyjny save/reload i pełny preflight. Błąd dowolnego
   pliku przywraca cały backup. Drugi Codex porównuje backup i wynik wyłącznie read-only.
4. Trigger pozostaje wyłączony przez T-155/T-158. Po zielonym T-151…T-158 current-SHA smoke
   uruchamia tymczasową kopię `Murmur-1` na disposable repo/worktree z Codex writer + Codex
   reviewer. Nie mutuje globalnych agentów. Receipt musi być przypięty do SHA, a kontrola obejmuje
   overlap, finalne pliki, kontekst, brak sierot i cleanup.
5. `Urc` wraca do poprzedniego stanu wyłącznie po zielonym preflight i smoke; jeśli wcześniej był
   wyłączony, pozostaje wyłączony.

Nie powstaje teraz osobny task soak/release: T-149, pełna bramka i powyższy current-SHA smoke
dają istniejące oracles bez ich duplikowania. Task dystrybucyjny powstanie dopiero przy realnej
instalacji u drugiej osoby; wtedy kontrakt obejmie provenance SHA, podpisanie/notaryzację dokładnej
aplikacji wewnątrz DMG i bezpieczny transport poświadczeń Apple.

Raw evidence, handoffy i zamknięte rozmowy Lead są trwałą historią użytkownika, więc ich liczba
nie wraca do baseline'u w tej fazie. Przed publiczną dystrybucją właściciel musi wybrać jawne
Delete, quota/retention albo świadomie zaakceptować nieograniczoną historię; automatyczne kasowanie
bez tej decyzji łamałoby zasadę „pliki są prawdą".

## 7. Linia cięcia

| Zdolność | Kiedy | Powód |
|---|---|---|
| Kurowany strumień, bez PTY | **v1** | Decyzja D4 |
| Claude + Codex | **v1** | Decyzja D3 |
| Drag&drop workflow, dwa rodzaje kafelka | **v1** | Rdzeń prośby |
| Kreator agentów, dziedziczenie + nadpisania | **v1** | Rdzeń prośby |
| Umiejętności: rozmieszczanie + wciąganie z URL | **v1** | Rdzeń prośby |
| Pamięć: przekazania + dwa stany notatek | **v1** | Rdzeń prośby |
| Odzyskiwanie po awarii | **v1** | Bez tego crash = cicha utrata pracy |
| Karty = workspace'y, przełączanie bez utraty sesji | **v1** | Prośba użytkownika; bez tego apka obsługuje jeden folder naraz |
| Zmiana kolejności kart przeciąganiem | v1.1 | Kosmetyka |
| Cofnij/ponów na płótnie | v1.1 | ~40 linii, ale nie blokuje niczego |
| „Uruchom ponownie od tego kroku" | v1.1 | Pierwsza rzecz, o którą poprosisz po tygodniu |
| Prawdziwe PTY („otwórz shell tutaj") | v1.1 | D4 |
| Podpisywanie i notaryzacja | **przy pierwszej instalacji u drugiej osoby** | |
| Auto-aktualizacja | v1.0 publiczne | Klucze generujemy teraz, żeby się nie zabetonować |
| Windows | v2 | Po miesiącu codziennego użycia na macOS. Wtedy powtórzyć benchmark IPC na WebView2 |
| Ograniczone warunki i rozwijane pod-workflowy | **v1.1 — T-75** | Potrzebne do wiernego importu istniejącej ceremonii; wyłącznie typowane wyniki, bez dowolnych wyrażeń i bez czwartego rodzaju kafelka |
| Osadzone wektory w pamięci | nie planowane | FTS5 wystarcza dla kilku tysięcy notatek |
| Zatwierdzanie akcji agenta w locie | nie planowane w v1 | Wymaga hostowania serwera MCP albo hooka; listy dozwolonych narzędzi wystarczą |

---

## 8. Pięć najbardziej ryzykownych założeń

Uszeregowane po tym, ile kosztuje pomyłka. Każde ma **najtańszą sondę**, nie plan badawczy.

| # | Założenie | Jeśli fałszywe | Sonda |
|---|---|---|---|
| 1 | Długo żyjący proces `claude` ze stdin utrzyma sesję przez wiele tur i da się przerwać w paśmie | `ClaudeDriver` przechodzi na proces-na-turę z `--resume`; wolniej i drożej, ale trait ocaleje | Zweryfikowane `[ran]` w T1 §4.6 — **potwierdzić ponownie po każdej aktualizacji CLI** |
| 2 | Sklejanie 16 ms utrzyma płynność przy czterech agentach naraz | Wracamy do batcha po liczbie; gorsza najgorsza klatka | Benchmark mierzył **ścieżkę licznika**, nie **ścieżkę zegara** [T8 ryzyko 3]. Podłącz prawdziwego wolnego agenta i zmierz |
| 3 | Reguły zwijania naprawdę usuwają wrażenie ściany tekstu | Cała teza produktu upada; trzeba przeprojektować widok | Odpal T-08 na prawdziwym 10-minutowym biegu i **policz** przewinięcia. Zero to sukces |
| 4 | Podzbiór umiejętności da się ustawić per sesja | Modal kroku traci funkcję, którą obiecuje UI | **S-1** |
| 5 | Dwóch vendorów wystarczy, żeby `AgentDriver` był prawdziwą abstrakcją | Trzeci vendor rozwali sygnatury i będzie przepisanie | **T-10** jest tym testem. Jeśli `CodexDriver` wymusi zmianę traitu — to jest sygnał, nie porażka |

---

## 9. Czego ten plan świadomie nie zawiera

- **Ośmiu rodzajów autorytetu.** Trzy: agent / Loadout / ty.
- **Migracji schematu na przyszłość.** Jedna wersja, aż będzie druga.
- **Transakcyjnego outboxa.** 1545 linii w poprzednim prototypie; nie mamy sieci do uzgadniania.
- **Trzech maszyn stanów.** Jedna, siedem stanów, `paused` na poziomie biegu.
- **Wiązania planu z trzema hashami i kanonicznym stringiem zatwierdzenia.** Jeden hash JSON-a planu,
  porównany przed wysyłką, to 100% wartości za 5% kosztu.
- **Ewaluacji wewnątrz mierzonego systemu.**
- **Sandboxa bezpieczeństwa.** Mówimy wprost, że kopia plików to izolacja współbieżności.
