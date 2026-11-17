---
title: Skróty
slug: skroty
seoTitle: Skróty klawiszowe palety i jak je zmienić
description: Każdy skrót palety Naughty Keyboard z domyślną kombinacją, to, które działają dziś, i jak zmienić dowolny z nich w oknie albo w settings.toml.
lead: "Każda akcja palety ma kombinację klawiszy. Oto domyślne, a każdą z nich można zmienić."
---

## Jakich kombinacji używa paleta?

Wszystkie domyślne to jedna para modyfikatorów i klawisz, więc uczy się ich jak jednej rodziny. Na Windows to `Alt+Shift`, a domyślna kombinacja nigdy nie jest `Ctrl+Alt`, bo Windows czyta `Ctrl+Alt` jako AltGr, a skrót globalny na takiej kombinacji zabrałby literę wszystkim aplikacjom na układzie klawiatury, który ją w tym miejscu pisze. Na polskim układzie to na przykład ą, ę i ś. Na macOS to `Option+Command`, bo Option pisze znaki, a Command nie, a kombinacją okna Find a value jest `Option+Command+F`, bo `Option+Command+Space` należy do Findera. Na Linuksie to `Ctrl+Alt`, bo przełącznik układu klawiatury często zabiera `Alt+Shift`. Tabela niżej pokazuje kombinacje z Windows. Pod Waylandem o tym, jakie klawisze dostaje skrót, decyduje pulpit, a paleta nazywa klawisze, które pulpit dał.

{{< shortcuts >}}

Akcje oznaczone jako jeszcze niedostępne są zarejestrowane, więc ich kombinacje są dla nich zarezerwowane, a naciśnięcie którejś z nich nic nie robi.

## Jak zmienić skrót?

W palecie wybierz odsyłacz **Change shortcuts**. Okno, które się otworzy, pokazuje każdą akcję z jej kombinacją. Wybierz akcję i naciśnij kombinację, którą chcesz. Dopóki to okno jest otwarte, skróty palety są wstrzymane, więc naciśnięcie kombinacji, żeby ją nagrać, nie uruchamia jej przy okazji. Delete albo Backspace na akcji przywraca jej domyślną kombinację.

Można też wpisać ją w `settings.toml`, pod `[shortcuts]`, z nazwą akcji z ostatniej kolumny tabeli wyżej:

```toml
[shortcuts]
next-value = "Ctrl+Alt+Win+N"
```

Ten plik to ten, który nazywa [strona palety](/docs/palette/). Paleta czyta go przy starcie.

## Których kombinacji nie przyjmie?

Kombinacja, której paleta nie może użyć, zostaje odrzucona z podaniem przyczyny, a akcja zachowuje kombinację, którą miała:

- klawisz inny niż litera od A do Z, cyfra od 0 do 9, F1 do F24 albo Space,
- kombinacja, którą ma już inna akcja albo którą trzyma już inny program.

## Przed którymi kombinacjami ostrzega?

Kombinacja, która zabiera coś wszystkim innym aplikacjom, zostaje przyjęta. Okno skrótów oznacza jej wiersz i mówi pod listą, co zabiera, a paleta mówi to samo, gdy startuje z taką kombinacją w `settings.toml`:

- kombinacja bez Ctrl, Alt ani Win (na Macu bez Control, Option ani Command): dopóki paleta działa, ten klawisz nie pisze i nie działa w żadnej aplikacji, także w oknach samej palety,
- `Ctrl+Alt` z klawiszem, który któryś z zainstalowanych układów klawiatury pisze z AltGr jako znak, albo na Macu `Option` z klawiszem, który układ pisze jako znak: dopóki paleta działa, tego znaku nie da się tak napisać.

Na Linuksie paleta pisze znak, którego klawisz trzyma jako skrót, innym klawiszem, więc wartości, które wpisuje, dochodzą w całości.
