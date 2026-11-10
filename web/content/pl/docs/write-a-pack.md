---
title: Własna paczka
slug: wlasna-paczka
seoTitle: Własna paczka wartości testowych krok po kroku
description: "Jak napisać własną paczkę wartości testowych: gdzie leży plik, każde pole, znaki, których nie widać, długie wartości, daty, pary i nkb lint."
lead: "Paczka to jeden plik tekstowy. Wystarczy położyć go w folderze, a paleta pokaże jego wartości obok wbudowanych. Ta strona prowadzi od pustego folderu do pierwszej własnej wartości wpisanej w pole."
---

## Czym jest paczka?

Paczka to jeden plik tekstowy w formacie [TOML](https://toml.io/), nazwany tak jak paczka, z wartościami testowymi w kolejności, w jakiej tester je wpisuje. Każda wartość mówi, czym jest, co zwykle psuje w aplikacji i co poprawna aplikacja z nią robi. Tak wygląda cała paczka z jedną wartością:

{{< template-part "first-pack" "pack" "trailing-space" >}}

Wiersze do pierwszego `[[values]]` opisują paczkę, a każdy blok `[[values]]` to jedna wartość. Tekst po znaku `#` to komentarz, który program pomija. Tekst stoi w cudzysłowach prostych, liczba i data bez nich, a lista w nawiasach kwadratowych.

## Jak zrobić pierwszą paczkę?

1. **Otwórz własny folder.** W palecie naciśnij {{< kbd "Alt+Shift+Space" >}}, żeby otworzyć okno wartości, kliknij u góry **Pack folders**, a potem **Create and open** przy własnym folderze. Folder otworzy się w menedżerze plików.
2. **Połóż w nim szablon.** Pobierz do tego folderu {{< template-link "first-pack" >}} albo inny z [szablonów paczek](/docs/pack-templates/).
3. **Otwórz plik w edytorze tekstu**, na przykład w Notatniku, TextEdit albo edytorze kodu. Edytor dokumentów zapisuje formatowanie, którego program nie przeczyta.
4. **Zrób z niej swoją.** Nadaj paczce własne `id` i zmień nazwę pliku tak samo, więc `checkout.toml` ma w środku `id = "checkout"`. Potem zastąp przykładowe wartości swoimi.
5. **Użyj jej.** Naciśnij znów {{< kbd "Alt+Shift+Space" >}}: paczka jest na liście z dopiskiem `own folder`. Wybierz ją, kliknij w testowane pole i naciśnij {{< kbd "Alt+Shift+N" >}}.

Okno wartości czyta folder przy każdym otwarciu, więc zmianę w pliku widać bez restartu palety. Paczka z błędem jest na liście jako niewczytana, z liczbą problemów, a [sprawdzanie paczki](#jak-sprawdzić-paczkę) pokazuje, gdzie one są.

Kto używa wiersza poleceń, może zacząć od `nkb new-pack checkout`: zapisuje nowy plik paczki z jedną przykładową wartością, gotowy do edycji, a reszta tej strony dotyczy go tak samo.

## Gdzie położyć plik?

We własnym folderze albo w folderze zespołu. Własny folder to `packs` obok pliku ustawień:

- Windows: `%APPDATA%\Naughty Keyboard\packs`
- macOS: `~/Library/Application Support/Naughty Keyboard/packs`
- Linux: `~/.config/naughty-keyboard/packs`

Folder zespołu to dowolny folder, w którym zespół trzyma pliki paczek, [opisany niżej](#czy-zespół-może-mieć-wspólne-paczki). W obu plik paczki leży bezpośrednio w folderze, nie w folderze w środku, a jego nazwa to `id` paczki z końcówką `.toml`. Folder może mieć do {{< format "folder_files" >}} plików paczek, a jeden plik do {{< format "pack_file_mib" >}} MiB.

Paczka o tym samym `id` co wbudowana jest używana zamiast niej, a paczka z własnego folderu zamiast tej z folderu zespołu. Paleta, okno wartości i blok do zgłoszenia mówią, skąd paczka pochodzi, więc nikt nie wpisuje wartości z jednej paczki, myśląc o innej.

## Co jest na początku paczki?

Pierwszy wiersz, `format = 1`, a potem tabela `[pack]`, która opisuje całą paczkę:

| Pole | Wymagane | Co wpisać |
|---|---|---|
| `id` | tak | Nazwa paczki dla programu: małe litery, cyfry i łączniki, na początku litera. Plik nazywa się tak samo, z końcówką `.toml`. |
| `name` | tak | Krótka nazwa, którą ludzie czytają w palecie, do {{< format "name_limit" >}} znaków. |
| `description` | tak | Jedno albo dwa zdania o tym, do czego jest paczka, do {{< format "description_limit" >}} znaków. |
| `version` | tak | Wersja wartości paczki, na przykład `"1.0"`. Podnieś ją, gdy wartości się zmieniają. |
| `updated` | tak | Dzień ostatniej zmiany, jako `2026-11-10`, bez cudzysłowów. |
| `license` | tak | Licencja wartości, na przykład `"CC-BY-4.0"`. Dla paczki, która zostaje w zespole, wpisz to, czego używa zespół, na przykład `"proprietary"`. |
| `authors` | tak | Lista nazwisk, na przykład `["Jan Kowalski"]`. |
| `language` | tak | Język, w którym są napisane zdania, na przykład `"pl"` albo `"en"`. |
| `tags` | zalecane | Słowa, po których wyszukiwarka okna wartości znajduje paczkę. Bez nich `nkb lint` ostrzega: paczkę znajdzie tylko ktoś, kto zna jej nazwę. |
| `fields` | nie | Rodzaje pól, do których pasują wartości: {{< format "field_kinds" >}}. |
| `risk` | nie | {{< format "risk_levels" >}}. Wartość oznaczona `offensive` jest oznaczona w palecie, zanim zostanie wpisana. |
| `source` | nie | Skąd pochodzą wartości, gdy są czyjąś pracą. |

## Co jest w wartości?

Jeden blok `[[values]]` na każdą wartość:

| Pole | Wymagane | Co wpisać |
|---|---|---|
| `id` | tak | Własna nazwa wartości: małe litery, cyfry i łączniki, inna niż każde inne `id` w paczce. Blok do zgłoszenia cytuje wartość jako `paczka/id`, więc nie zmieniaj go, gdy inni już używają paczki. |
| `name` | tak | Krótka etykieta, którą pokazuje paleta. |
| `value` | tak, dla wartości zapisanej wprost | Tekst do wpisania, w cudzysłowach prostych. |
| `breaks` | tak | Co ta wartość zwykle psuje i dlaczego, w co najmniej {{< format "min_breaks" >}} znakach. |
| `expect` | zalecane | Co robi z nią poprawna aplikacja. Bez tego `nkb lint` ostrzega. |
| `since` | tak | `version` paczki, w której wartość pojawiła się pierwszy raz. |
| `fields`, `tags`, `risk` | nie | To samo co na początku paczki, dla tej jednej wartości. |
| `type` | nie | Wartość, którą wylicza program: `"repeat"` dla [bardzo długiej wartości](#jak-zapisać-bardzo-długą-wartość), `"next-date"` dla [daty, która zawsze jest przed nami](#jak-zapisać-datę-która-zawsze-jest-w-przyszłości). |

Paleta wpisuje wartości w kolejności, w jakiej stoją w pliku, więc wartość, która najpewniej znajdzie błąd, idzie pierwsza. Tester, który naciśnie {{< kbd "Alt+Shift+N" >}} pięć razy i pójdzie dalej, dostanie pięć najlepszych wartości. Paczka z więcej niż {{< format "max_values" >}} wartościami działa, a `nkb lint` radzi ją podzielić.

## Jak napisać dobre zdanie breaks?

Powiedz, co się psuje i dlaczego, żeby osoba, która wpisuje wartość, wiedziała, gdzie patrzeć. `breaks` to powód, dla którego paczka jest warta więcej niż lista napisów, i jedyna część, której nikt nie napisze za Ciebie. Zdanie jest po angielsku albo w języku z pola `language`.

- Za krótkie, `nkb lint` je odrzuci: `Breaks the login.`
- Mówi co, ale nie dlaczego: `Login fails with a trailing space.`
- Mówi co i dlaczego: `The sign-up form keeps the space and the login form trims it, or the other way round, so the account is created and then cannot be found.`

`expect` to druga połowa: co robi poprawna aplikacja, żeby tester odróżnił błąd od właściwej odpowiedzi. Dla wartości wyżej: `Trimmed everywhere or kept everywhere, so the same name always finds the same account.`

## Jak zapisać znak, którego nie widać?

Jako sekwencję ucieczki w cudzysłowach prostych, na przykład `\u0020` dla spacji na końcu. Znak, którego nie widać, przetrwa jeden edytor i zniknie w następnym, a recenzent nie ma jak zobaczyć, że w ogóle tam jest. Wszystko, co widać, zostaje jak jest: litery z ogonkami i akcentami, inne pisma i emoji zapisuje się wprost.

| Zapis | Znak |
|---|---|
| `\u0020` | spacja na początku albo na końcu wartości |
| `\u00A0` | twarda spacja |
| `\t` | tabulator, który paleta wpisuje klawiszem Tab |
| `\n` | nowy wiersz, który paleta wpisuje klawiszem Enter |
| `\u200B` | spacja o zerowej szerokości |
| `\u202E` | znak odwracający kierunek tekstu |
| `\uFEFF` | znacznik kolejności bajtów |
| `\\` | ukośnik wsteczny |
| `\"` | cudzysłów prosty |
| `\U0001F600` | dowolny znak po numerze, z ośmioma cyframi dla znaku powyżej `\uFFFF` |

Litera, która wygląda jak inna, na przykład cyrylickie o wśród liter łacińskich, też lepiej wygląda jako `\u043E`, bo recenzent widzi wtedy, że to nie jest `o`.

Nie wiadomo, które znaki tego wymagają? Wpisz wartość tak, jak jest, i uruchom `nkb lint`: nazwie każdy znak, który trzeba zapisać inaczej, i poda zapis.

W apostrofach nic nie jest sekwencją ucieczki. Dlatego `'C:\temp'` to ścieżka dokładnie tak, jak jest zapisana, a `'Kowalski\u0020'` to znaki `\u0020`, a nie spacja. Wartość z sekwencją ucieczki zapisuj w cudzysłowach prostych.

## Jak zapisać bardzo długą wartość?

Jako krótki kawałek i liczbę powtórzeń, z `type = "repeat"`, zamiast całej wartości:

{{< template-part "text-field-limit" "len-51" >}}

`unit` to kawałek, który może mieć sekwencje ucieczki, a `count` to liczba powtórzeń, od 1 do {{< format "max_repeat_count" >}}. Cała wartość może mieć do {{< format "max_code_points" >}} punktów kodowych. Paleta wpisuje ją w całości, w tempie, w jakim aplikacja przyjmuje znaki, a {{< kbd "Escape" >}} ją zatrzymuje. Wartość zapisana wprost, dłuższa niż {{< format "literal_length_hint" >}} znaków i złożona z jednego powtórzonego kawałka, dostaje od `nkb lint` ostrzeżenie z radą, żeby użyć `repeat`.

## Jak zapisać datę, która zawsze jest w przyszłości?

Jako regułę dnia, z `type = "next-date"`, zamiast samej daty:

{{< template-part "booking-dates" "spring-forward" >}}

W chwili wpisania program wylicza pierwszy dzień, który pasuje do reguły i jest co najmniej dwa dni po dzisiejszym w UTC, więc jest przyszłością w każdej strefie czasowej. Wpisuje go jako `YYYY-MM-DD`, a z polem `time` jako `YYYY-MM-DD HH:MM`. Data zapisana wprost któregoś dnia przechodzi w przeszłość i po cichu przestaje testować to, co obiecuje.

- `month` to od 1 do 12.
- `week` to tydzień miesiąca: {{< format "weeks" >}}.
- `weekday` to {{< format "weekdays" >}}.
- `time` jest opcjonalne, jako `HH:MM` w zapisie 24-godzinnym.

Blok do zgłoszenia podaje dzień, który został wpisany, więc znalezisko da się wpisać ponownie dokładnie tak samo.

## Jak zrobić parę?

Zapisz dwie wartości jedna po drugiej, a potem blok `[[pairs]]`, który je łączy. Para to dwie wartości wpisywane w dwa pola, na przykład adres e-mail i pole, które go powtarza:

{{< template-part "sign-up-form" "email" "email-trailing-space" "repeat-email" >}}

`a` i `b` to `id` obu wartości, a `b` stoi w pliku zaraz po `a`, bo drugie naciśnięcie wpisuje to, co jest następne. Paleta pokazuje `pair 1 of 2` i `pair 2 of 2` przy nazwie każdej połowy. `relation` to jedno z: {{< format "relations" >}}:

- `look-alike`: dwie wartości, które wyglądają tak samo, a różnią się bajtami, na przykład dwa loginy w dwóch alfabetach.
- `range`: początek i koniec, na przykład start wieczorem i koniec rano.
- `identity`: dwie wartości, które aplikacja powinna uznać za te same albo za różne, na przykład hasło i jego powtórzenie.

## Jak sprawdzić paczkę?

Uruchom `nkb lint checkout.toml`, z nazwą swojego pliku. Sprawdza plik wobec każdej reguły formatu i wypisuje od razu wszystkie problemy, każdy w osobnym wierszu, a na końcu podaje, ile błędów i ostrzeżeń znalazł. [Przykłady niżej](#co-znaczą-najczęstsze-błędy) pokazują, co wypisuje.

Wiersz podaje plik, wiersz z problemem, kod i zdanie, które mówi, co zrobić. Kod zaczynający się od `E` to błąd: paczka nie wczyta się, dopóki go nie poprawisz. Kod zaczynający się od `W` to ostrzeżenie: paczka się wczyta, a zdanie mówi, co by zyskała. Potem `nkb fmt checkout.toml` wyrównuje plik tak, jak oczekuje `nkb lint`, i nigdy nie zmienia wartości. [nkb lint](/docs/cli/lint/) i [nkb fmt](/docs/cli/fmt/) opisują każdą opcję.

`nkb` to wiersz poleceń, drugi program każdego wydania. Instalator na Windows dodaje go do `PATH` razem z paletą, a [Pobieranie](/download/) ma jego archiwum dla każdego systemu.

## Co znaczą najczęstsze błędy?

Każdy przykład niżej to plik z jednym błędem i to, co `nkb lint` o nim mówi, słowo w słowo. Program mówi po angielsku.

### Nazwa pliku i id się różnią

{{< lint-example "file-name" >}}

Zmień nazwę pliku albo `id`, żeby były takie same. Paleta znajduje paczkę po nazwie pliku.

### Wartość nie ma breaks

{{< lint-example "no-breaks" >}}

Dopisz do wartości `breaks`, [zdanie o tym, co psuje](#jak-napisać-dobre-zdanie-breaks).

### Zdanie breaks jest za krótkie

{{< lint-example "short-breaks" >}}

Powiedz, co się psuje i dlaczego, w co najmniej {{< format "min_breaks" >}} znakach.

### Spacja jest zapisana wprost

{{< lint-example "space-written-out" >}}

Wartość kończy się spacją, której nikt nie widzi. Zapisz ją jako `\u0020`, tak jak mówi zdanie.

### Wartość w apostrofach potrzebuje sekwencji ucieczki

{{< lint-example "single-quotes" >}}

Ujmij wartość w cudzysłowy proste i zapisz spację jako `\u0020`.

### Plik nie jest poprawnym TOML

{{< lint-example "curly-quotes" >}}

Nazwa stoi w cudzysłowach drukarskich, które niektóre edytory wstawiają zamiast prostych. TOML przyjmuje tylko proste. Wpisz je jeszcze raz albo użyj edytora, który ich nie zamienia. Ten sam błąd daje niezamknięty cudzysłów albo brakujący nawias.

### Rodzaju pola nie ma na liście

{{< lint-example "wrong-field-kind" >}}

Wybierz najbliższe słowo z listy w zdaniu, tutaj `email`.

### Literówka w nazwie pola

{{< lint-example "typo-in-a-key" >}}

Format nie ma pola `expected`, więc wartość liczy się tak, jakby nie miała `expect` wcale. Popraw nazwę, a znikną oba wiersze.

### Plik nie jest wyrównany

{{< lint-example "untidy" >}}

Uruchom `nkb fmt untidy.toml`. Wyrównuje znaki równości i ustawia pola w kolejności, a wartości nigdy nie zmienia.

## Czy zespół może mieć wspólne paczki?

Tak. Trzymajcie pliki paczek we wspólnym folderze, na przykład w folderze `qa-packs` w repozytorium testowanej aplikacji. Każdy tester raz wskazuje go palecie: **Pack folders** w oknie wartości, potem **Choose folder**, albo upuszcza folder na to okno. [Paleta](/docs/palette/#czy-może-pokazać-moje-paczki-albo-paczki-zespołu) mówi o tym oknie więcej.

Folder zespołu na udziale sieciowym, który nie odpowie w {{< format "folder_wait_seconds" >}} sekundy, jest pomijany ze zdaniem, które to mówi, a pozostałe paczki działają dalej. W potoku CI `--team-folder` wskazuje folder na jedno uruchomienie, więc build może sprawdzić paczki i ich użyć:

```console
$ for f in qa-packs/*.toml; do nkb lint "$f" || exit 1; done
$ nkb emit checkout --team-folder qa-packs --format json > checkout.json
```

## Czy mogę napisać paczkę po polsku?

Tak. Ustaw `language` na swój język, na przykład `"pl"`, i pisz w nim `name`, `description`, `breaks` i `expect`. Nazwy pól i słowa z zamkniętych list, takie jak `email` czy `look-alike`, zostają bez zmian, bo czyta je program.

## Czy mogę zmienić wbudowaną paczkę?

Tak, kładąc we własnym folderze paczkę o tym samym `id`: jest używana zamiast wbudowanej, a paleta to mówi. Pliki wbudowanych paczek są w [folderze packs w repozytorium](https://github.com/donislawdev/Naughty-Keyboard/tree/main/packs), do skopiowania i zmiany. Wartość, która przyda się wszystkim, lepiej [zaproponować](https://github.com/donislawdev/Naughty-Keyboard/issues/new?template=suggest_value.yml), z tym, co psuje i gdzie się trafiła.

## Czy format jest ostateczny?

Jeszcze nie. Kolejne wydanie może go zmienić, a jego notatki powiedzą jak. `nkb lint` mówi to przy każdym uruchomieniu, żeby plik, który przechodzi dziś, nie był czytany jako obietnica na jutro.
