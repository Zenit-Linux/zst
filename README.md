# zst — transpilator Z# (0.0.1)

```
zst [--to php|js|c] [-o wyjście] [--bundle] [--run] [--check] plik.zs
zst --list-targets | --version | --help
```
Kody wyjścia: 0 ok, 1 błąd, 2 użycie, 3 konstrukcja nieobsługiwana w celu (jawny błąd `Exxxx` z linią).

| cel | jak działa | pokrycie | zgodność semantyczna z interpreterem |
|---|---|---|---|
| `php` (domyślny) | tłumaczenie **składniowe** tokenów (zachowuje układ linii), prelude dla `echo/necho/eecho` | pełna składnia (klasy, importy…) | **niska**: nie odtwarza semantyki wartości tablic, dzielenia int, komunikatów błędów (1/7 testów różnicowych) |
| `js` | Z# IR → JavaScript (Node ≥ 16) + `runtime/zs_runtime.js` (tablice COW, int/float, wyjątki, wbudowane) | podzbiór natywny (jak `zsc`) | **wysoka**: 6/6 programów w podzbiorze; ograniczenie: `int` to double (dokładny do 2^53, nie int64) |
| `c` | Z# IR → C11 w **jednym pliku** (nagłówek + runtime wklejone), `cc -O2 plik.c -lm` | podzbiór natywny | **wysoka**: 6/6 (to samo co `zsc`) |

`sample.zsharp` (klasy) → `UNSUPPORTED` dla `js` i `c` (jawnie, kod 3); dla `php` składniowo działa.
`--bundle` (js) wkleja runtime do pliku. `--run` uruchamia wynik przez `php`/`node`/`cc` i zwraca jego kod wyjścia.

## Testy
`tests/run_diff.sh` — tabela program × cel (PASS/FAIL/UNSUPPORTED) względem `zsc0 run`: stdout, stderr i kod wyjścia.
`TARGETS="js c" STRICT=1 tests/run_diff.sh` kończy się błędem przy jakimkolwiek FAIL.

## Plan
Cel `php` na IR (semantyka wartości jak w js), potem klasy/domknięcia w IR → wszystkie cele; cele `py`, `go`.
