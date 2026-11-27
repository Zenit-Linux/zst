# zst — transpilator Z#

`zst --to <cel> [-o plik] [--no-bundle] [--run] [--check] prog.zs [-- argumenty]`

| cel | wynik | wymaga do uruchomienia |
|---|---|---|
| `js` | jeden plik JS (runtime wklejony) | Node >= 16 |
| `php` | jeden plik PHP | PHP >= 8.1 |
| `c` | jeden plik C11 (runtime wklejony) | `cc -O2 plik.c -lm` |
| `cpp` | jeden plik C++17 (nagłówkowy runtime) | `c++ -std=c++17 -O2 plik.cpp` |
| `rust` | jeden plik Rust, bez zależności | `rustc -O --edition 2021 plik.rs` |
| `php-src` | starsze tłumaczenie składniowe tokenów | PHP (bez gwarancji semantyki) |

Wszystkie cele (poza `php-src`) przechodzą przez ten sam front: linker modułów (klasy, domknięcia, `import`) → IR.
Cele `js/php/cpp/rust` mają własne runtimes w `runtime/` (`zs_runtime.{js,php,hpp,rs}`) z semantyką zgodną z `zs_rt.c`:
int64 z błędem przepełnienia, napisy bajtowe UTF-8, tablice z kopiowaniem przy zapisie, wyjątki, moduły `Math/System/File/Directory/Process`.
Cele `cpp` i `rust` korzystają ze wspólnego emitera spłaszczającego (`src/flat.zsharp`).

Przykłady: `zst --to js prog.zs --run` · `zst --to rust prog.zs -o prog.rs && rustc -O --edition 2021 prog.rs`.
Runtime znajdowany jest przez `ZST_HOME/runtime`, `<bin>/../runtime`, `<bin>/../share/z-sharp/zst/runtime`.

Weryfikacja: `tests/run.sh` (konformancja każdego celu) oraz samohosting — `zsc` przetranspilowany przez każdy cel
kompiluje samego siebie do identycznego LLVM IR.
Ograniczenia: `Socket.*` tylko w celu `c`; w js int64 powyżej 2^53 jest reprezentowany przez BigInt (wolniej); kompilacja dużych
programów w rust/cpp jest wolna. Kody wyjścia: 0 ok · 1 błąd · 2 użycie · 3 konstrukcja nieobsługiwana w celu.
