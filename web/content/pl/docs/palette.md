---
title: Paleta
slug: paleta
seoTitle: Paleta - następna wartość testowa w dowolnym polu
description: Jak działa paleta, co pokazuje o następnej wartości, jak wpisuje bez zabierania fokusu i jak znaleźć dowolną wartość we wszystkich paczkach naraz.
lead: "Małe okno, które wpisuje następną wartość z paczki w pole, w którym jesteś, i nigdy nie zabiera do tego klawiatury."
---

## Czym jest paleta?

Paleta to `nkb-gui`, okno, które stoi obok testowanej aplikacji i jest sterowane skrótami. Klikasz pole aplikacji, naciskasz {{< kbd "Alt+Shift+N" >}} i następna wartość z paczki trafia w to pole. Paleta nie zabiera fokusu klawiatury, więc kursor nigdy nie opuszcza testowanego pola, a tę samą kombinację możesz nacisnąć sto razy z rzędu.

## Co pokazuje?

- **Paczkę** na górze, z tym, jak daleko w niej jesteś. Kliknięcie jej nazwy otwiera okno szukania wartości.
- **Następną wartość**, tę, którą wpisze następne naciśnięcie, narysowaną tak, żeby ją było widać. Znak, którego nikt nie zobaczy, jest rysowany jako znacznik, prostokąt otwarty od góry, a nie jako nic.
- **Ostatnią wpisaną wartość**, zwiniętą, dopóki jej nie rozwiniesz, z czterema licznikami: grafemy, punkty kodowe, bajty i jednostki UTF-16. Te cztery liczby różnią się właśnie przy wartościach, które najczęściej psują pola.
- **Sposób wpisania każdej wartości**: z klawiatury albo przez schowek, z którego ją wklejasz, i to, czy wiersz jest najpierw czyszczony, czy wartość trafia w miejsce kursora.
- **Klawisze** każdego działającego skrótu, na dole.

## Jak wartość trafia do pola?

Domyślnie paleta czyści bieżący wiersz pola klawiszami Home, Shift+End i Delete, a potem wpisuje wartość tak, jak robi to klawiatura, znak po znaku, w tempie, w jakim aplikacja je przyjmuje. Długa wartość pokazuje postęp w trakcie wpisywania, a {{< kbd "Escape" >}} ją zatrzymuje. Wtedy paleta mówi, ile z niej dotarło.

Odmawia, zamiast zgadywać. Nie wpisuje niczego, gdy wciśnięty jest Ctrl, Alt, Shift albo Win, bo wartość zamieniłaby się w skróty. Nie wpisuje niczego w przycisk, odnośnik ani element listy, który ma fokus. Nie czyści niczego, o czym nie wie na pewno, że jest polem tekstowym, i nigdy nie czyści terminala, gdzie te klawisze trafiają do programu, który w nim działa.

Okno działające z wyższymi uprawnieniami niż paleta, na przykład program uruchomiony jako administrator, odrzuciłoby naciśnięcia bez słowa. Dla takiego okna paleta kładzie wartość w schowku, mówi dlaczego i zostawia wklejenie Tobie. Więcej mówi [Tryb schowka](/docs/clipboard-mode/).

## Jak znaleźć wartość?

Naciśnij {{< kbd "Alt+Shift+Space" >}}. Okno, które się otworzy, pokazuje każdą paczkę jako listę do rozwinięcia i w trakcie pisania przeszukuje naraz wartości wszystkich paczek. Wybranie wartości ustawia jej paczkę jako używaną, a tę wartość jako następną, więc następne naciśnięcie ją wpisze. Na górze stoją wartości ostatnio wybrane w tym oknie, z każdej paczki.

## Czy mogę ją zmniejszyć?

{{< kbd "Alt+Shift+H" >}} zwija paletę do samego nagłówka i rozwija ją z powrotem. Paleta nigdy się nie chowa i nigdy sama się nie przesuwa. Jest dokładnie tak wysoka, jak to, co pokazuje.

## Co pamięta?

Wybraną paczkę, to, czy jest zwinięta, gdzie stała przy każdym układzie ekranów, i Twoje skróty, w `settings.toml`. Na Windows ten plik to `%APPDATA%\Naughty Keyboard\settings.toml`. Pliku, którego nie potrafi użyć, nie rusza i nigdy go nie nadpisuje.
