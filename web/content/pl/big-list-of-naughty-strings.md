---
title: Big List of Naughty Strings i Naughty Keyboard
linkTitle: Big List of Naughty Strings
url: /pl/porownania/big-list-of-naughty-strings/
seoTitle: Big List of Naughty Strings a Naughty Keyboard - porównanie
description: Czym różnią się Big List of Naughty Strings i Naughty Keyboard - lista napisów do testów i narzędzie, które wpisuje wartości w pole i mówi, co każda psuje.
lead: "Jedno jest listą, drugie narzędziem, a oba wyrastają z tego samego pomysłu. Ta strona mówi, co robi każde z nich, kiedy lista wystarcza i jak używać obu."
---

## Czym jest Big List of Naughty Strings?

Listą napisów, które z dużym prawdopodobieństwem sprawiają kłopot, gdy trafiają do programu jako dane od użytkownika. Zaczął ją Max Woolf w 2015 roku i udostępnił na licencji MIT. Plik `blns.txt` ma ponad 500 napisów w 31 sekcjach, od zarezerwowanych słów i liczb po Unicode, tekst pisany od prawej do lewej, wstrzykiwanie skryptów i SQL oraz nazwy plików zarezerwowane przez Windows. Każda sekcja ma jednowierszowy opis. `blns.json` ma te same napisy bez komentarzy, do czytania przez program, a pakiety utrzymywane przez innych ludzi wnoszą listę do testów w kilku językach programowania. To lista, którą zna większość testerów, i od niej pochodzi nazwa tego projektu.

Liczby pochodzą z [repozytorium](https://github.com/minimaxir/big-list-of-naughty-strings) i zostały policzone w październiku 2026. `blns.txt` zmienił się ostatnio w kwietniu 2021.

## Co Naughty Keyboard robi inaczej?

Robi resztę pracy wokół takiej listy:

- **Wpisuje wartość w pole.** Jeden skrót wpisuje następną wartość z paczki w pole, które ma fokus klawiatury, w stałej kolejności, a kursor nie opuszcza pola. Na macOS i Linuksie wartość idzie zamiast tego przez schowek.
- **Każda wartość mówi, co psuje.** Nie jeden wiersz na sekcję, tylko dwa zdania na wartość: co zwykle psuje i dlaczego oraz co zamiast tego robi poprawna aplikacja.
- **Pokazuje to, czego nie widać.** Niewidoczny znak jest rysowany jako znacznik, a każda wartość jest liczona na cztery sposoby: grafemy, punkty kodowe, bajty i jednostki UTF-16.
- **Pisze zgłoszenie błędu.** Jeszcze jeden skrót kopiuje blok do zgłoszenia, z wartością zapisaną tak, żeby ktoś inny mógł ją wpisać z powrotem.
- **Ma wartości za długie na plik tekstowy.** Big List celowo pomija napisy od 255 znaków w górę, żeby plik dało się czytać. Paczki trzymają takie wartości jako przepisy, jak te dwie:

{{< values "length-bombs/len-65535" "length-bombs/len-100000" >}}

## Kiedy Big List wystarcza?

Gdy napisy trafiają do testu automatycznego i nikt nie wpisuje ich ręcznie. Ma dużo więcej napisów, sekcje, których Naughty Keyboard nie ma, na przykład wstrzykiwanie skryptów, SQL i poleceń, oraz gotowe pakiety dla kilku języków programowania. Naughty Keyboard nie ma dziś paczki z takimi napisami: jego wartości to to, co produkują prawdziwi użytkownicy i prawdziwe dane.

## Jak wypadają obok siebie?

| | Big List of Naughty Strings | Naughty Keyboard |
|---|---|---|
| Co to jest | lista napisów, jako tekst, JSON i base64 | dwa programy, paleta i wiersz poleceń `nkb`, nad katalogiem paczek |
| Rozmiar | ponad 500 napisów w 31 sekcjach | paczki: {{< count "pack" >}}, wartości: {{< count "value" >}} |
| Co mówi o wartości | jeden wiersz na sekcję | co psuje i co robi poprawna aplikacja, przy każdej wartości |
| Wartości od 255 znaków | celowo pominięte | zapisane jako przepisy |
| Wstrzykiwanie skryptów, SQL i poleceń | tak, ze znanymi CVE | dziś nie |
| Wartość w polu | kopiuj i wklej | jeden skrót na Windows, schowek na macOS i Linuksie |
| W teście automatycznym | `blns.json` i pakiety dla kilku języków | `nkb emit`, jako JSON, CSV albo jedna wartość na wiersz |
| Licencja | MIT | GPL-3.0 dla programów, CC BY 4.0 dla paczek |

## Czy mogę używać obu?

Tak, w różnych chwilach. Przepuść Big List przez test automatyczny każdego pola aplikacji, a Naughty Keyboard weź do pól, które człowiek testuje ręcznie, gdzie zdanie o tym, co psuje wartość, mówi, czego szukać. Gdy katalog Naughty Keyboard ma trafić także do testu, `nkb emit` wypisze każdą paczkę:

```console
$ nkb emit magic-values --format json > magic-values.json
```

## Skąd nazwa Naughty Keyboard?

Od Big List of Naughty Strings, od której zaczyna się pomysł tego katalogu i którą zna większość testerów. To ich słowo.
