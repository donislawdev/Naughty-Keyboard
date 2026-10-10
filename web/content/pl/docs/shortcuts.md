---
title: Skróty
slug: skroty
seoTitle: Skróty klawiszowe palety i jak je zmienić
description: Każdy skrót palety Naughty Keyboard z domyślną kombinacją, to, które działają dziś, i jak zmienić dowolny z nich w oknie albo w settings.toml.
lead: "Każda akcja palety ma kombinację klawiszy. Oto domyślne, a każdą z nich można zmienić."
---

## Jakich kombinacji używa paleta?

Wszystkie domyślne to `Alt+Shift` i klawisz, więc uczy się ich jak jednej rodziny. Na Windows domyślna kombinacja nigdy nie jest `Ctrl+Alt`, bo Windows czyta `Ctrl+Alt` jako AltGr, a skrót globalny na takiej kombinacji zabrałby literę wszystkim aplikacjom na układzie klawiatury, który ją w tym miejscu pisze. Na polskim układzie to na przykład ą, ę i ś.

{{< shortcuts >}}

Akcje oznaczone jako jeszcze niedostępne są zarejestrowane, więc ich kombinacje są dla nich zarezerwowane, a naciśnięcie którejś z nich nic nie robi.

## Jak zmienić skrót?

W palecie wybierz odsyłacz **Change shortcuts**. Okno, które się otworzy, pokazuje każdą akcję z jej kombinacją. Wybierz akcję i naciśnij kombinację, którą chcesz. Dopóki to okno jest otwarte, skróty palety są wstrzymane, więc naciśnięcie kombinacji, żeby ją nagrać, nie uruchamia jej przy okazji. Delete albo Backspace na akcji przywraca jej domyślną kombinację.

Można też wpisać ją w `settings.toml`, pod `[shortcuts]`, z nazwą akcji z ostatniej kolumny tabeli wyżej:

```toml
[shortcuts]
next-value = "Ctrl+Alt+Win+N"
```

Na Windows ten plik to `%APPDATA%\Naughty Keyboard\settings.toml`. Paleta czyta go przy starcie.

## Których kombinacji nie przyjmie?

Kombinacja, której paleta nie może użyć, zostaje odrzucona z podaniem przyczyny, a akcja zachowuje kombinację, którą miała:

- kombinacja bez Ctrl, Alt ani Win, która zabrałaby klawisz pisaniu,
- klawisz inny niż litera od A do Z, cyfra od 0 do 9, F1 do F24 albo Space,
- kombinacja, którą ma już inna akcja albo którą trzyma już inny program,
- `Ctrl+Alt` z klawiszem, który któryś z zainstalowanych układów klawiatury pisze z AltGr jako znak - taki znak komunikat nazywa.
