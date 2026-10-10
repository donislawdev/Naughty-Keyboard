---
title: nkb packs
command: packs
layout: command
seoTitle: nkb packs - lista paczek wartości testowych
description: nkb packs wypisuje każdą paczkę wartości testowych, którą program może zaoferować, liczbę jej wartości oraz to, które źródła paczek przeczytał, a których nie.
lead: "Wypisuje paczki, które ta wersja programu może zaoferować, z liczbą wartości w każdej z nich."
---

## Co wypisuje?

Jeden wiersz na paczkę: jej nazwę do wpisania, liczbę wartości i tytuł. Potem wiersz, który je wszystkie liczy, a potem to, skąd paczki pochodzą. Lista z jednego źródła wygląda dokładnie tak samo jak pełna, więc każde uruchomienie mówi też, które źródła zostały przeczytane, a które nie.

```console
$ nkb packs
whitespace                12 values  Whitespace
unicode-text              12 values  Unicode and text
length-bombs              12 values  Length bombs
magic-values              12 values  Magic values
numbers-extreme           12 values  Extreme numbers
dates-impossible          12 values  Impossible dates
export-breakers           12 values  Export breakers
locale-pl                  6 values  Polish locale
filenames-paths           12 values  File names and paths

9 of 9 packs loaded, 102 values.
Read these pack sources: built-in.
Not read: team - the settings file has no key for this folder yet, so it cannot be named.
Not read: own - the settings file has no key for this folder yet, so it cannot be named.
```

Nazwa z pierwszej kolumny to to, co przyjmują inne polecenia: `nkb show whitespace`, `nkb emit magic-values`. Strona [Paczki](/packs/) pokazuje każdą z nich z każdą wartością.
