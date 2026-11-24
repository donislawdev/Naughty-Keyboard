---
title: Napisy testowe Unicode
url: /pl/poradniki/napisy-testowe-unicode/
seoTitle: Napisy testowe Unicode - i co każdy z nich psuje
description: Napisy testowe Unicode - sekwencje emoji, znaki łączące, litery nie do odróżnienia, tekst od prawej do lewej, znaki niewidoczne - i co każdy z nich psuje.
lead: "Tekst w Unicode psuje programy na kilka sposobów, które ciągle wracają: jest źle liczony, źle porównywany, źle rysowany albo źle zapisywany. Te napisy trafiają w każdy z nich, a każdy mówi, co zwykle psuje."
---

## Jakimi napisami Unicode testować?

Jednym na każdy sposób, w jaki tekst się psuje: znakiem, który dla programu jest kilkoma znakami, dwoma napisami, które wyglądają tak samo, a zapisują się inaczej, tekstem, który zmienia kierunek, znakami, których nikt nie widzi, i znakami, których nie pomieści zapis zbudowany pod węższe kodowanie. Pole, które przechodzi wszystkie, obsługuje tekst, a nie tylko angielski.

## Napisy liczone źle

Przetestuj tekst, który dla oka jest jednym znakiem, a dla programu kilkoma. Limit, licznik albo cięcie w złej jednostce rozbija go na niepowiązane kawałki.

{{< values "emoji-complex/emoji-family-zwj" "emoji-complex/emoji-skin-tone" "emoji-complex/flag-country" "unicode-text/combining-acute" >}}

## Napisy, które wyglądają tak samo, a porównują się inaczej

Przetestuj pary, których żaden człowiek nie odróżni. Wyszukiwanie, logowanie i każde sprawdzenie unikalności porównują to, co zapisane, a nie to, co widać.

{{< values "look-alike-pairs/cyrillic-small-a" "look-alike-pairs/nfd-lodz" "unicode-text/fullwidth-latin" "unicode-text/math-bold" >}}

## Tekst, który zmienia kierunek

Przetestuj znaki pisane od prawej do lewej wewnątrz tekstu pisanego od lewej do prawej. Ekran pokazuje wtedy co innego, niż jest zapisane, w innej kolejności.

{{< values "unicode-text/rtl-override" "unicode-text/arabic-in-latin" >}}

## Znaki, których nikt nie widzi

Przetestuj znaki, które nie zajmują miejsca na ekranie, a mimo to sprawiają, że dwa napisy są różne.

{{< values "whitespace/zwsp-only" "unicode-text/soft-hyphen" "unicode-text/bom-inside" "emoji-complex/zwj-orphan" >}}

## Znaki, które psują zapis i układ

Przetestuj znaki spoza płaszczyzny podstawowej, których nie pomieści zapis ustawiony na wąskie kodowanie, i tekst wyższy albo szerszy niż wiersz, w którym jest rysowany.

{{< values "unicode-text/emoji-in-name" "emoji-complex/astral-cjk" "unicode-text/zalgo" "unicode-text/cjk-mixed" >}}

## Jak je skopiować, nie gubiąc znaku?

Kopiowanie ze strony to słaby punkt: znak niewidoczny, łącznik albo znak kierunku łatwo gubi się po drodze, a test niczego wtedy nie dowodzi. Paleta wpisuje każdy napis dokładnie tak, jak trzyma go katalog, a `nkb emit` wypisuje je ze znakami ucieczki, więc widać każdy znak:

```console
$ nkb emit unicode-text --format lines --escaped
```

## Jak szybko przejść przez wszystkie?

Paletą, jeden skrót na napis: wybierz paczkę **Unicode i tekst**, **Złożone emoji** albo **Pary nie do odróżnienia**, kliknij pole, naciśnij {{< kbd "Alt+Shift+N" >}}, spójrz na wynik i naciśnij go jeszcze raz dla następnego. Gdy napis coś psuje, {{< kbd "Alt+Shift+B" >}} kopiuje blok do zgłoszenia, z wartością zapisaną tak, że ktoś inny może ją wpisać z powrotem. [Pierwsze kroki](/docs/getting-started/) zajmują około dwóch minut.

Napisy są na stronach paczek [Unicode i tekst](/packs/unicode-text/), [Złożone emoji](/packs/emoji-complex/) i [Pary nie do odróżnienia](/packs/look-alike-pairs/), razem z tym, co robi poprawna aplikacja.
