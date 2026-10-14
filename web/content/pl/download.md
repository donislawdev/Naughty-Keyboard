---
title: Pobierz
slug: pobierz
seoTitle: Pobierz na Windows, macOS i Linuksa - bez instalacji
description: Pobierz Naughty Keyboard na Windows, macOS albo Linuksa. Dwa programy, paleta i wiersz poleceń, podpisane na Windows i macOS, bez instalowania czegokolwiek.
lead: "Dwa programy na trzy systemy. Rozpakuj archiwum i uruchom: nic się nie instaluje i nic więcej nie jest potrzebne."
---

## Czy jest już wydanie?

Tak. Każde wydanie, razem z plikami, jest na [stronie wydań](https://github.com/donislawdev/Naughty-Keyboard/releases), a najnowsze zawsze pod adresem [ostatniego wydania](https://github.com/donislawdev/Naughty-Keyboard/releases/latest). Projekt jest wciąż na wczesnym etapie rozwoju, a format paczki nie jest jeszcze zamrożony: kolejne wydanie może go jeszcze zmienić, a jego notatki powiedzą jak.

## Którego pliku potrzebuję?

`nkb-gui` to paleta, okno, które wpisuje wartość w pole, w którym jesteś. `nkb` to wiersz poleceń. Każde wydanie ma jedno archiwum na każdy program i system, a `VERSION` niżej oznacza wersję, na przykład `0.2.0`:

{{< downloads >}}

Wpisywanie w inne okna działa na Windows. Na macOS i Linuksie wiersz poleceń działa w pełni, a paleta kładzie wartości w schowku. Więcej mówią [Uczciwe ograniczenia](/docs/limits/).

## Czy jest instalator na Windows?

Tak, {{< installer >}}, z oboma programami w środku. Prosi o uprawnienia administratora, bo instaluje dla każdego konta na komputerze: paleta trafia do menu Start jako Naughty Keyboard, a `nkb` do systemowej zmiennej `PATH`, więc działa w każdym nowym terminalu. Nowsza wersja instaluje się na starszą, a odinstalowanie w Ustawieniach, w Aplikacjach, zostawia ustawienia tam, gdzie są.

## Co jest w archiwum?

Program, `LICENSE`, `README.md` i `THIRD-PARTY-NOTICES.txt` z licencją wszystkiego, co jest wkompilowane w program. Nie ma wokół nich folderu, więc każde archiwum rozpakuj do osobnego folderu.

- **Windows.** Programy są podpisane, ze znacznikiem czasu, więc Windows pokazuje wydawcę. Uruchom `nkb.exe` z terminala albo `nkb-gui.exe`.
- **macOS.** Każdy program to pakiet, `nkb.app` albo `nkb-gui.app`, podpisany i poświadczony przez Apple, z biletem notaryzacji zszytym w środku. Obok leży dowiązanie, więc `./nkb` też działa. Trzymaj pakiet i dowiązanie razem. Kopia programu wyjęta z pakietu zostanie odrzucona, bo podpis obejmuje cały pakiet.
- **Linux.** Programy nie są podpisane, a podpisane poświadczenia dołączone do każdego wydania mówią, skąd pochodzą. Paleta potrzebuje sesji graficznej, X11 albo Wayland.

## Jak sprawdzić pobrany plik?

Każde wydanie wiezie SHA-256 każdego archiwum i dwa podpisane poświadczenia: jak zbudowano program i co zawiera każde archiwum. [Weryfikacja pobrania](/docs/verify-a-download/) ma polecenia i mówi, czego dowodzi każde z nich.

## Jak zbudować program ze źródeł?

Potrzebny jest [Rust](https://rustup.rs/) {{< rust-version >}} albo nowszy.

```console
$ git clone https://github.com/donislawdev/Naughty-Keyboard
$ cd Naughty-Keyboard
$ cargo build --release -p nkb-cli
$ cargo build --release -p nkb-gui
```

Wiersz poleceń to wtedy `target/release/nkb`, a paleta to `target/release/nkb-gui`. `cargo test --workspace` uruchamia cały zestaw testów.
