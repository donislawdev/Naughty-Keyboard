---
title: nkb show
command: show
layout: command
seoTitle: nkb show - cała paczka wartości testowych
description: "nkb show wypisuje całą paczkę: każdą wartość w postaci escapowanej, żeby było widać niewidoczne znaki, jej rozmiar, co psuje i co robi poprawna aplikacja."
lead: "Wypisuje jedną paczkę w całości, każdą wartość z tym, co psuje, i z tym, co robi poprawna aplikacja."
---

## Co wypisuje?

Nazwę paczki, jej opis, wersję i licencję, a potem każdą wartość w kolejności, w jakiej spotyka je tester. Wartości są wypisane w postaci escapowanej, bo to jedyna czytelna postać wartości złożonej ze znaków, których nikt nie zobaczy: spacja na końcu to `\u0020`, a nie puste miejsce na końcu wiersza.

```console
$ nkb show whitespace
whitespace - Whitespace
Characters that take up space, or claim to, and are impossible to see in a form.
version 1.0, updated 2026-09-07, CC-BY-4.0, fields: any

  trailing-space
    Trailing space
    value    Kowalski\u0020
    size     9 code points, 9 bytes
    breaks   Login and e-mail comparisons differ between browser and server; the account is created but cannot be found.
    expect   Trimmed everywhere or preserved everywhere - never one on sign-up and the other on sign-in.
```

Wartość zbyt długa, żeby ją wypisać, na przykład 65 535 znaków, jest w paczce zapisana jako przepis i `show` wypisuje przepis, a nie wartość. Żeby dostać samą wartość, użyj [nkb emit](/docs/cli/emit/).
