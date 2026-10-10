---
title: nkb fmt
command: fmt
layout: command
seoTitle: nkb fmt - kanoniczna postać pliku paczki
description: nkb fmt doprowadza plik paczki do postaci kanonicznej - pola w kolejności, którą określa format, wyrównane znaki równości - i nigdy nie zmienia wartości.
lead: "Doprowadza plik paczki do postaci kanonicznej i nigdy nie zmienia tego, co plik mówi."
---

## Co zmienia?

Kolejność pól w każdej tabeli, na tę, którą określa format, i odstęp przed każdym znakiem równości, żeby się wyrównały. Nic więcej: komentarze, puste wiersze i kolejność wartości zostają dokładnie takie, jakie są, bo kolejność wartości to kolejność, w jakiej spotyka je tester.

Jeśli zmiana zmieniłaby wartość, `fmt` niczego nie zapisuje i kończy się kodem 4. Formatter, który potrafiłby zmienić dane, które formatuje, byłby jeszcze jednym miejscem, w którym wartość może się zepsuć.

## Przykłady

```console
$ nkb fmt my-pack.toml --dry-run
my-pack.toml is not in shape. Run without --dry-run to rewrite it.
$ nkb fmt my-pack.toml
Wrote my-pack.toml in canonical shape.
$ nkb fmt my-pack.toml
my-pack.toml is already in shape. Nothing was written.
```
