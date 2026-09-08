# Syntetyczne źródła biblioteki Context

Materiał dla `context_source_import::` (Rust) i `context-sources.spec.ts` (przeglądarka).
**Jedna kopia bajtów na obie strony granicy**: Rust bierze je przez `include_str!`, okno przez
`readFileSync`. Dwie kopie tego samego obrazu rozjechałyby się w dniu, w którym ktoś poprawi
jedną — a wtedy test Rusta i test przeglądarki mówiłyby o dwóch różnych plikach pod jedną nazwą.

## Dlaczego obrazy leżą jako `.b64`, a nie jako binaria

Bo mają być **czytelne w diffie i sprawdzalne okiem**. Plik binarny w repo jest zdaniem, którego
nikt nie umie zweryfikować w recenzji: widać wyłącznie „zmieniło się 44 bajty". Base64 zajmuje
o jedną trzecią więcej i to jest cała cena; te pliki mają po kilkadziesiąt bajtów.

| Plik                 | Co to jest                                                              |
| -------------------- | ----------------------------------------------------------------------- |
| `screenshot.png.b64` | Prawdziwy PNG 1×1. Dekoduje się naprawdę, nie tylko pasuje nagłówkiem.  |
| `pattern.webp.b64`   | Prawdziwy WebP 1×1 — trzeci format z zamkniętej listy.                  |
| `notes.md`           | Markdown, czyli źródło bez magicznych bajtów: dowodem jest UTF-8.       |
| `paper.pdf`          | Jedna pusta strona. Do drogi IMPORTU, nie do parsowania — powód w pliku. |
| `three-pages.pdf`    | Trzy strony: tekstowa, skan i mieszana. Ten OTWIERA się w `pdf.js`.      |
| `breaks-on-page-two.pdf` | Otwiera się, a pada dopiero przy drugiej stronie — powód w pliku.   |
| `not-really.png`     | Tekst pod nazwą `.png`. To jest ten jeden plik z pięciu, który odmawia. |

Obrazu przekraczającego sufit 16 mln pikseli tu **nie ma** i celowo: ważyłby tyle, co cała reszta
repo razem, a powstaje w jednej linii testu (`GrayImage::new`) dokładnie wtedy, kiedy jest
potrzebny.

## `three-pages.pdf` jest w całości ASCII i to jest warunek, nie ciekawostka

Obraz na stronach 2 i 3 jedzie filtrem `/ASCIIHexDecode`, a strumienie nie są skompresowane —
dzięki temu **cały plik da się przeczytać i zrecenzować okiem**, tak samo jak base64 wyżej.
Tabela `xref` niesie prawdziwe przesunięcia bajtowe (`grep -bo "^[0-9] 0 obj"`), więc `pdf.js`
otwiera go zwykłą drogą, a nie ścieżką odzyskiwania uszkodzonego pliku — inaczej ten sam plik
dowodziłby jednocześnie, że dokument jest dobry i że jest zepsuty.

**Przy każdej zmianie tego pliku przelicz `xref` i `startxref` na nowo.** Przesunięcia są
bajtowe: dopisana spacja w obiekcie 3 unieważnia wszystkie następne.
