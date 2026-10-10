---
title: nkb new-pack
command: new-pack
layout: command
seoTitle: nkb new-pack - zacznij własną paczkę wartości testowych
description: nkb new-pack zapisuje szkielet nowej paczki wartości testowych, z jedną przykładową wartością i uwagami, od razu w kanonicznej postaci i przechodzący nkb lint.
lead: "Zapisuje szkielet nowej paczki, gotowy do edycji i od razu przechodzący `nkb lint`."
---

## Co zapisuje?

Jeden plik w bieżącym folderze, nazwany tak jak paczka, z jedną przykładową wartością do zastąpienia i krótką uwagą o trzech rzeczach, które warto wiedzieć na początku: sednem jest zdanie o tym, co psuje wartość, znak, którego nikt nie zobaczy, zapisuje się w postaci escapowanej, a wynik sprawdza `nkb lint`. Nigdy nie nadpisuje pliku, który już istnieje.

Nazwa staje się i nazwą pliku, i identyfikatorem paczki, więc składa się z małych liter, cyfr i łączników i zaczyna się od litery.

```console
$ nkb new-pack my-pack
Wrote my-pack.toml.
Edit it, then run `nkb lint my-pack.toml` - it reports everything in one pass.
The pack format is not frozen yet: it freezes with the first public release that ships packs.
```

Własne paczki można dziś napisać i sprawdzić, ale jeszcze nie wczytać do palety. Więcej mówią [Uczciwe ograniczenia](/docs/limits/).
