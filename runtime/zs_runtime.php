<?php
// Runtime Z# dla celu php (PHP >= 8.1, bez rozszerzeń). Semantyka zgodna z runtime/native/zs_rt.c:
//  int = int64 (przepełnienie = błąd), float = float, napisy = bajty, tablice uporządkowane z kopiowaniem przy zapisie (natywne w PHP),
//  wyjątki ZExc / ZObj niesione przez ZEx.
error_reporting(E_ALL & ~E_DEPRECATED);
set_error_handler(function ($no, $str) { $GLOBALS['z_lasterr'] = $str; if (!(error_reporting() & $no)) return true; if ($no & (E_WARNING | E_NOTICE | E_USER_WARNING)) { z_die("błąd wewnętrzny PHP: " . $str); } return true; });

final class ZExc { public $kind; public $msg; public $line = 0; function __construct($k, $m) { $this->kind = $k; $this->msg = $m; } }
final class ZObj { public $cls; public $f = []; function __construct($c) { $this->cls = $c; } }
final class ZFn { public $id; public $cap; function __construct($i, $c) { $this->id = $i; $this->cap = $c; } }
final class ZEx extends Exception { public $val; function __construct($v) { parent::__construct("zs"); $this->val = $v; } }
final class Z { static $l = 0; }

function z_raise($e) {
    if ($e instanceof ZExc) { if ($e->line === 0) $e->line = Z::$l; }
    elseif ($e instanceof ZObj) { if (!isset($e->f['line']) || !is_int($e->f['line']) || $e->f['line'] === 0) $e->f['line'] = Z::$l; }
    throw new ZEx($e);
}
function z_die($m) { $e = new ZExc(1, $m); $e->line = Z::$l; throw new ZEx($e); }
function z_throwv($x) { if (!($x instanceof ZExc) && !($x instanceof ZObj)) z_die("throw wymaga obiektu typu Throwable"); z_raise($x); }
function z_excis($x, $kind) { return $x instanceof ZExc && ($kind === 0 || $x->kind === $kind); }
function z_getmessage($x) { if (!($x instanceof ZExc)) z_die("wywołanie metody `getMessage` na wartości typu " . z_tn($x)); return $x->msg; }
function z_last(...$a) { return $a[count($a) - 1]; }

function z_tn($v) {
    if ($v === null) return "null"; if (is_bool($v)) return "bool"; if (is_int($v)) return "int"; if (is_float($v)) return "float";
    if (is_string($v)) return "string"; if (is_array($v)) return "array"; if ($v instanceof ZObj) return $v->cls;
    if ($v instanceof ZFn) return "function"; return "object";
}
function z_isnum($v) { return is_int($v) || is_float($v); }

// ───── tekst ─────
function z_fmtfloat($f) {
    if (is_nan($f)) return "NaN"; if (is_infinite($f)) return $f > 0 ? "Infinity" : "-Infinity";
    if ($f == floor($f) && abs($f) < 1e15) return sprintf("%.1f", $f);
    for ($p = 0; $p < 17; $p++) { $e = sprintf("%.{$p}e", $f); if ((float)$e === $f) break; }
    $e = sprintf("%.{$p}e", $f);
    $neg = $e[0] === '-'; if ($neg) $e = substr($e, 1);
    $pos = strpos($e, 'e'); $mant = substr($e, 0, $pos); $ex = (int)substr($e, $pos + 1);
    $digits = str_replace('.', '', $mant); $nd = strlen($digits); $point = $ex + 1;
    if ($point <= 0) $r = "0." . str_repeat("0", -$point) . $digits;
    elseif ($point >= $nd) $r = $digits . str_repeat("0", $point - $nd);
    else $r = substr($digits, 0, $point) . "." . substr($digits, $point);
    return ($neg ? "-" : "") . $r;
}
function z_dumpstr($s) {
    $o = '"'; $n = strlen($s);
    for ($i = 0; $i < $n; $i++) {
        $c = ord($s[$i]);
        if ($c === 34) $o .= '\\"'; elseif ($c === 92) $o .= '\\\\'; elseif ($c === 10) $o .= '\\n'; elseif ($c === 13) $o .= '\\r';
        elseif ($c === 9) $o .= '\\t'; elseif ($c === 0) $o .= '\\0'; elseif ($c < 0x20 || $c === 0x7f) $o .= '\\u{' . dechex($c) . '}';
        else $o .= $s[$i];
    }
    return $o . '"';
}
function z_text($v, $dump = false) {
    if ($v === null) return "null"; if ($v === true) return "true"; if ($v === false) return "false";
    if (is_int($v)) return (string)$v; if (is_float($v)) return z_fmtfloat($v);
    if (is_string($v)) return $dump ? z_dumpstr($v) : $v;
    if (is_array($v)) {
        $seq = true; $i = 0; foreach ($v as $k => $_) { if ($k !== $i) { $seq = false; break; } $i++; }
        $p = [];
        foreach ($v as $k => $x) $p[] = ($seq ? "" : (is_string($k) ? z_dumpstr($k) : (string)$k) . " => ") . z_text($x, true);
        return "[" . implode(", ", $p) . "]";
    }
    if ($v instanceof ZObj) return "<object " . $v->cls . ">";
    if ($v instanceof ZFn) return "<function>";
    if ($v instanceof ZExc) return $v->kind ? "<object Error>" : "<object Exception>";
    return "<object>";
}
function z_truthy($v) {
    if ($v === null || $v === false) return false; if ($v === true) return true;
    if (is_int($v)) return $v !== 0; if (is_float($v)) return $v != 0.0;
    if (is_string($v)) return !($v === "" || $v === "0"); if (is_array($v)) return count($v) > 0; return true;
}

// ───── UTF-8 ─────
function z_u8len($c) { return $c < 0x80 ? 1 : ($c < 0xE0 ? 2 : ($c < 0xF0 ? 3 : 4)); }
function z_lossy($s) {
    $n = strlen($s); $o = ""; $i = 0;
    while ($i < $n) {
        $c = ord($s[$i]); $need = 0; $lo = 0x80; $hi = 0xBF;
        if ($c < 0x80) { $o .= $s[$i]; $i++; continue; }
        elseif ($c >= 0xC2 && $c <= 0xDF) $need = 1;
        elseif ($c === 0xE0) { $need = 2; $lo = 0xA0; } elseif ($c >= 0xE1 && $c <= 0xEC) $need = 2; elseif ($c === 0xED) { $need = 2; $hi = 0x9F; } elseif ($c >= 0xEE && $c <= 0xEF) $need = 2;
        elseif ($c === 0xF0) { $need = 3; $lo = 0x90; } elseif ($c >= 0xF1 && $c <= 0xF3) $need = 3; elseif ($c === 0xF4) { $need = 3; $hi = 0x8F; }
        else { $o .= "\xEF\xBF\xBD"; $i++; continue; }
        $j = 1; $ok = true;
        for (; $j <= $need; $j++) {
            if ($i + $j >= $n) { $ok = false; break; }
            $d = ord($s[$i + $j]); $l = $j === 1 ? $lo : 0x80; $h = $j === 1 ? $hi : 0xBF;
            if ($d < $l || $d > $h) { $ok = false; break; }
        }
        if ($ok) { $o .= substr($s, $i, $need + 1); $i += $need + 1; } else { $o .= "\xEF\xBF\xBD"; $i += $j; }
    }
    return $o;
}

// ───── tablice ─────
function z_k($k) {
    if (is_int($k)) return $k; if (is_bool($k)) return $k ? 1 : 0;
    if (is_float($k)) { if (is_finite($k) && floor($k) == $k && abs($k) < 9.2e18) return (int)$k; z_die("niepoprawny klucz tablicy typu float"); }
    if (is_string($k)) return $k; // PHP normalizuje napisy-liczby tak samo jak Z# (kanoniczne int64 -> int)
    z_die("niepoprawny klucz tablicy typu " . z_tn($k));
}
function z_aset(&$a, $k, $v) {
    if ($a === null) $a = []; elseif (!is_array($a)) z_die("nie można indeksować wartości typu " . z_tn($a) . " przy zapisie");
    $a[z_k($k)] = $v; return $v;
}
function z_apush(&$a, $v) {
    if ($a === null) $a = []; elseif (!is_array($a)) z_die("nie można indeksować wartości typu " . z_tn($a) . " przy zapisie");
    $a[] = $v; return $v;
}
function z_keytext($k) { return is_string($k) ? z_dumpstr($k) : (string)$k; }
function z_index($a, $k, $quiet = false) {
    if (is_array($a)) {
        $nk = z_k($k);
        if (array_key_exists($nk, $a)) return $a[$nk];
        if ($quiet) return null; z_die("brak klucza " . z_keytext($nk));
    }
    if (is_string($a)) {
        if (!is_int($k)) z_die("indeks tekstu musi być liczbą całkowitą");
        $len = strlen($a); $i = $k < 0 ? $len + $k : $k;
        if ($i < 0 || $i >= $len) { if ($quiet) return null; z_die("indeks $i poza zakresem tekstu (długość $len)"); }
        $c = ord($a[$i]);
        if (($c & 0xC0) === 0x80) { if ($quiet) return null; z_die("indeks $i wskazuje środek znaku UTF-8"); }
        $n = z_u8len($c); if ($i + $n > $len) $n = $len - $i;
        return substr($a, $i, $n);
    }
    if ($a === null) { if ($quiet) return null; z_die("indeksowanie wartości null"); }
    z_die("typ " . z_tn($a) . " nie obsługuje indeksowania");
}
function z_indexopt($a, $k) { return z_index($a, $k, true); }
function z_foreach($v) { if (!is_array($v)) z_die("foreach wymaga tablicy, jest " . z_tn($v)); return $v; }

// ───── porównania i operatory ─────
function z_ident($a, $b) { return $a === $b; }
function z_eq($a, $b) { if (z_isnum($a) && z_isnum($b) && is_int($a) !== is_int($b)) return (float)$a == (float)$b; return $a === $b; }
function z_cmp($a, $b) {
    if (is_int($a) && is_int($b)) return $a <=> $b;
    if (z_isnum($a) && z_isnum($b)) { $x = (float)$a; $y = (float)$b; return $x < $y ? -1 : ($x > $y ? 1 : 0); }
    if (is_string($a) && is_string($b)) { $c = strcmp($a, $b); return $c < 0 ? -1 : ($c > 0 ? 1 : 0); }
    if (is_bool($a) && is_bool($b)) return (int)$a <=> (int)$b;
    z_die("nie można porównać " . z_tn($a) . " z " . z_tn($b));
}
function z_badop($op, $a, $b) { z_die("operacja $op niedostępna dla " . z_tn($a) . " i " . z_tn($b)); }
function z_add($a, $b) {
    if (is_int($a) && is_int($b)) { $r = $a + $b; if (is_float($r)) z_die("przepełnienie liczby całkowitej"); return $r; }
    if (z_isnum($a) && z_isnum($b)) return (float)$a + (float)$b;
    if (is_array($a) && is_array($b)) return $a + $b;
    z_badop("Add", $a, $b);
}
function z_sub($a, $b) {
    if (is_int($a) && is_int($b)) { $r = $a - $b; if (is_float($r)) z_die("przepełnienie liczby całkowitej"); return $r; }
    if (z_isnum($a) && z_isnum($b)) return (float)$a - (float)$b;
    z_badop("Sub", $a, $b);
}
function z_mul($a, $b) {
    if (is_int($a) && is_int($b)) { $r = $a * $b; if (is_float($r)) z_die("przepełnienie liczby całkowitej"); return $r; }
    if (z_isnum($a) && z_isnum($b)) return (float)$a * (float)$b;
    z_badop("Mul", $a, $b);
}
function z_div($a, $b) {
    if (!(z_isnum($a) && z_isnum($b))) z_badop("Div", $a, $b);
    if ((float)$b == 0.0) z_die("dzielenie przez zero");
    if (is_int($a) && is_int($b)) {
        if ($b === -1) { if ($a === PHP_INT_MIN) return (float)$a / -1.0; return -$a; }
        if ($a % $b === 0) return intdiv($a, $b);
    }
    return (float)$a / (float)$b;
}
function z_mod($a, $b) {
    if (is_int($a) && is_int($b)) { if ($b === 0) z_die("modulo przez zero"); if ($b === -1) return 0; return $a % $b; }
    z_badop("Mod", $a, $b);
}
function z_bitand($a, $b) { if (is_int($a) && is_int($b)) return $a & $b; z_badop("BitAnd", $a, $b); }
function z_bitor($a, $b) { if (is_int($a) && is_int($b)) return $a | $b; z_badop("BitOr", $a, $b); }
function z_bitxor($a, $b) { if (is_int($a) && is_int($b)) return $a ^ $b; z_badop("BitXor", $a, $b); }
function z_shl($a, $b) { if (is_int($a) && is_int($b)) return $a << ($b & 63); z_badop("Shl", $a, $b); }
function z_shr($a, $b) { if (is_int($a) && is_int($b)) return $a >> ($b & 63); z_badop("Shr", $a, $b); }
function z_bitnot($a) { if (is_int($a)) return ~$a; z_die("operator jednoargumentowy BitNot niedostępny dla " . z_tn($a)); }
function z_pow($a, $b) {
    if (is_int($a) && is_int($b) && $b >= 0) {
        $r = 1; $base = $a; $e = $b; $ovf = false;
        while ($e > 0) {
            if ($e & 1) { $r = $r * $base; if (is_float($r)) { $ovf = true; break; } }
            $e >>= 1; if ($e > 0) { $base = $base * $base; if (is_float($base)) { $ovf = true; break; } }
        }
        if (!$ovf) return $r;
        return pow((float)$a, (float)$b);
    }
    if (z_isnum($a) && z_isnum($b)) return pow((float)$a, (float)$b);
    z_badop("Pow", $a, $b);
}
function z_concat($a, $b) { return (is_string($a) ? $a : z_text($a)) . (is_string($b) ? $b : z_text($b)); }
function z_neg($a) {
    if (is_int($a)) { if ($a === PHP_INT_MIN) z_die("przepełnienie liczby całkowitej"); return -$a; }
    if (is_float($a)) return -$a;
    z_die("operator jednoargumentowy Neg niedostępny dla " . z_tn($a));
}
function z_chk($v, $type, $what) {
    $ok = false;
    if ($type === "int") $ok = is_int($v);
    elseif ($type === "float") { $ok = is_float($v); if (!$ok && is_int($v)) return (float)$v; }
    elseif ($type === "string") $ok = is_string($v); elseif ($type === "bool") $ok = is_bool($v); elseif ($type === "array") $ok = is_array($v);
    elseif ($v instanceof ZObj || $v instanceof ZExc) $ok = true;
    if ($ok) return $v;
    z_die("$what: oczekiwano $type, otrzymano " . z_tn($v));
}

// ───── wbudowane ─────
function z_ns($v, $f, $n) { if (!is_string($v)) z_die("$f: argument #$n powinien być tekstem, jest " . z_tn($v)); return $v; }
function z_ni($v, $f, $n) { if (!is_int($v)) z_die("$f: argument #$n powinien być liczbą całkowitą, jest " . z_tn($v)); return $v; }
function z_nf($v, $f, $n) { if (!z_isnum($v)) z_die("$f: argument #$n powinien być liczbą, jest " . z_tn($v)); return (float)$v; }
function z_na($v, $f, $n) { if (!is_array($v)) z_die("$f: argument #$n powinien być tablicą, jest " . z_tn($v)); return $v; }
const Z_WS = " \t\n\v\f\r";
function z_b_count($v) { if (is_array($v)) return count($v); if (is_string($v)) return strlen($v); z_die("count: niedostępne dla " . z_tn($v)); }
function z_b_strlen($v) { return strlen(z_ns($v, "strlen", 1)); }
function z_b_typeof($v) { if ($v instanceof ZObj) return $v->cls; if ($v instanceof ZExc) return $v->kind ? "Error" : "Exception"; return z_tn($v); }
function z_b_is_int($v) { return is_int($v); } function z_b_is_float($v) { return is_float($v); } function z_b_is_string($v) { return is_string($v); }
function z_b_is_bool($v) { return is_bool($v); } function z_b_is_array($v) { return is_array($v); } function z_b_is_null($v) { return $v === null; }
function z_b_strval($v) { return z_text($v); } function z_b_boolval($v) { return z_truthy($v); }
function z_b_intval($v) {
    if (is_int($v)) return $v;
    if (is_float($v)) return is_nan($v) ? 0 : ($v >= 9.2233720368547758e18 ? PHP_INT_MAX : ($v <= -9.2233720368547758e18 ? PHP_INT_MIN : (int)$v));
    if (is_bool($v)) return $v ? 1 : 0; if ($v === null) return 0;
    if (is_string($v)) {
        $t = trim($v, Z_WS);
        if (preg_match('/^[+-]?[0-9]+$/D', $t)) {
            $neg = $t[0] === '-'; $d = ltrim(ltrim($t, '+-'), '0'); if ($d === '') return 0;
            $lim = $neg ? '9223372036854775808' : '9223372036854775807';
            if (strlen($d) < 19 || (strlen($d) === 19 && strcmp($d, $lim) <= 0)) return $neg ? -(int)$d : (int)$d;
            return $neg ? PHP_INT_MIN : PHP_INT_MAX;
        }
        if ($t !== "" && preg_match('/^[+-]?([0-9]+\.?[0-9]*([eE][+-]?[0-9]+)?|\.[0-9]+([eE][+-]?[0-9]+)?)$/D', $t)) { $f = (float)$t; return is_nan($f) ? 0 : z_b_intval($f); }
        return 0;
    }
    z_die("intval: nie można przekonwertować " . z_tn($v));
}
function z_b_floatval($v) {
    if (is_int($v)) return (float)$v; if (is_float($v)) return $v;
    if (is_string($v)) { $t = trim($v, Z_WS); return ($t !== "" && preg_match('/^[+-]?([0-9]+\.?[0-9]*([eE][+-]?[0-9]+)?|\.[0-9]+([eE][+-]?[0-9]+)?|inf(inity)?|nan)$/Di', $t)) ? (float)$t : 0.0; }
    return 0.0;
}
function z_b_ord($v) { $s = z_ns($v, "ord", 1); if ($s === "") z_die("ord: pusty tekst"); return ord($s[0]); }
function z_b_chr($v) {
    $n = z_ni($v, "chr", 1);
    if ($n < 0 || $n >= 0x110000 || ($n >= 0xD800 && $n < 0xE000)) return "";
    if ($n < 0x80) return chr($n); if ($n < 0x800) return chr(0xC0 | $n >> 6) . chr(0x80 | ($n & 63));
    if ($n < 0x10000) return chr(0xE0 | $n >> 12) . chr(0x80 | (($n >> 6) & 63)) . chr(0x80 | ($n & 63));
    return chr(0xF0 | $n >> 18) . chr(0x80 | (($n >> 12) & 63)) . chr(0x80 | (($n >> 6) & 63)) . chr(0x80 | ($n & 63));
}
function z_b_trim($v) { return trim(z_ns($v, "trim", 1), Z_WS); }
function z_b_contains($a, $b) { return str_contains(z_ns($a, "contains", 1), z_ns($b, "contains", 2)); }
function z_b_starts_with($a, $b) { return str_starts_with(z_ns($a, "starts_with", 1), z_ns($b, "starts_with", 2)); }
function z_b_ends_with($a, $b) { return str_ends_with(z_ns($a, "ends_with", 1), z_ns($b, "ends_with", 2)); }
function z_b_index_of($a, $b) { $p = strpos(z_ns($a, "index_of", 1), z_ns($b, "index_of", 2)); return $p === false ? -1 : $p; }
function z_b_repeat($a, $b) { $x = z_ns($a, "repeat", 1); $n = z_ni($b, "repeat", 2); return $n <= 0 ? "" : str_repeat($x, $n); }
function z_b_substr($s, $a, $l = null) {
    $x = z_ns($s, "substr", 1); $len = strlen($x); $st = z_ni($a, "substr", 2);
    if ($st < 0) { $st = $len + $st; if ($st < 0) $st = 0; } if ($st > $len) $st = $len;
    if (is_int($l)) { $n = $l >= 0 ? $l : $len - $st + $l; if ($n < 0) $n = 0; } else $n = $len - $st;
    if ($st + $n > $len) $n = $len - $st;
    return z_lossy(substr($x, $st, $n));
}
function z_b_join($a, $sep) { $x = z_na($a, "join", 1); $p = []; foreach ($x as $v) $p[] = z_text($v); return implode(is_string($sep) ? $sep : "", $p); }
function z_b_in_array($n, $arr) { foreach (z_na($arr, "in_array", 2) as $v) if ($v === $n) return true; return false; }
function z_b_range($a, $b) { $x = z_ni($a, "range", 1); $y = z_ni($b, "range", 2); $r = []; if ($x <= $y) { for ($i = $x; ; $i++) { $r[] = $i; if ($i === $y) break; } } else { for ($i = $x; ; $i--) { $r[] = $i; if ($i === $y) break; } } return $r; }
function z_b_keys($a) { return array_keys(z_na($a, "keys", 1)); }
function z_b_values($a) { return array_values(z_na($a, "values", 1)); }
function z_b_has_key($a, $k) { return array_key_exists(z_k($k), z_na($a, "has_key", 1)); }
function z_b_upper($v) { return strtr(z_ns($v, "upper", 1), "abcdefghijklmnopqrstuvwxyz", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"); }
function z_b_lower($v) { return strtr(z_ns($v, "lower", 1), "ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz"); }
function z_b_replace($a, $f, $t) { $s = z_ns($a, "replace", 1); $x = z_ns($f, "replace", 2); $y = z_ns($t, "replace", 3); return $x === "" ? $s : str_replace($x, $y, $s); }
function z_chars($s) { $r = []; $n = strlen($s); for ($i = 0; $i < $n;) { $k = z_u8len(ord($s[$i])); if ($i + $k > $n) $k = $n - $i; $r[] = substr($s, $i, $k); $i += $k; } return $r; }
function z_b_split($a, $sep) { $s = z_ns($a, "split", 1); $x = z_ns($sep, "split", 2); return $x === "" ? z_chars($s) : explode($x, $s); }
function z_b_chars($a) { return z_chars(z_ns($a, "chars", 1)); }
function z_u8count($s) { $n = 0; $l = strlen($s); for ($i = 0; $i < $l; $i++) if ((ord($s[$i]) & 0xC0) !== 0x80) $n++; return $n; }
function z_pad($a, $n, $p, $left, $f) {
    $s = z_ns($a, $f, 1); $w = z_ni($n, $f, 2); if ($w < 0) $w = 0;
    $pc = " "; if (is_string($p) && $p !== "") { $k = z_u8len(ord($p[0])); $pc = substr($p, 0, $k); }
    $l = z_u8count($s); if ($l >= $w) return $a;
    $pad = str_repeat($pc, $w - $l); return $left ? $pad . $s : $s . $pad;
}
function z_b_pad_left($a, $n, $p = null) { return z_pad($a, $n, $p, true, "pad_left"); }
function z_b_pad_right($a, $n, $p = null) { return z_pad($a, $n, $p, false, "pad_right"); }
function z_b_array_slice($a, $s, $l = null) {
    $x = array_values(z_na($a, "array_slice", 1)); $len = count($x); $st = z_ni($s, "array_slice", 2);
    if ($st < 0) { $st = $len + $st; if ($st < 0) $st = 0; } if ($st > $len) $st = $len;
    $n = is_int($l) ? $l : $len; if ($n < 0) $n = 0; $end = $st + $n; if ($end > $len) $end = $len;
    return array_slice($x, $st, $end - $st);
}
function z_b_reverse($a) {
    if (is_string($a)) return implode("", array_reverse(z_chars($a)));
    if (!is_array($a)) z_die("reverse: niedostępne dla " . z_tn($a));
    return array_reverse(array_values($a));
}
function z_b_sort($a) { $x = array_values(z_na($a, "sort", 1)); usort($x, 'z_cmp'); return $x; }
function z_b_push($a, $v) { z_na($a, "push", 1); $a[] = $v; return $a; }
function z_b_dump($v) { echo z_text($v, true), "\n"; return null; }
function z_first($v, $f) { $s = z_ns($v, $f, 1); return $s === "" ? null : ord($s[0]); }
function z_b_is_digit($v) { $c = z_first($v, "is_digit"); return $c !== null && $c >= 48 && $c <= 57; }
function z_b_is_alpha($v) { $c = z_first($v, "is_alpha"); return $c !== null && (($c >= 65 && $c <= 90) || ($c >= 97 && $c <= 122) || $c === 95 || $c >= 0x80); }
function z_b_is_alnum($v) { $c = z_first($v, "is_alnum"); return $c !== null && (($c >= 48 && $c <= 57) || ($c >= 65 && $c <= 90) || ($c >= 97 && $c <= 122) || $c === 95 || $c >= 0x80); }
function z_b_is_space($v) { $c = z_first($v, "is_space"); return $c !== null && ($c === 32 || ($c >= 9 && $c <= 13)); }
function z_b_is_hex_digit($v) { $c = z_first($v, "is_hex_digit"); return $c !== null && (($c >= 48 && $c <= 57) || ($c >= 65 && $c <= 70) || ($c >= 97 && $c <= 102)); }
function z_b_to_hex($v) { return dechex(z_ni($v, "to_hex", 1)); }
function z_b_read_line() {
    $l = fgets(STDIN); if ($l === false) return null;
    $l = substr($l, -1) === "\n" ? substr($l, 0, -1) : $l; return rtrim($l, "\r");
}
function z_b_zs_obj_new($cls) { return new ZObj($cls); }
function z_b_zs_obj_class($o) { return $o instanceof ZObj ? $o->cls : null; }
function z_b_zs_obj_get($o, $name) {
    if (!($o instanceof ZObj)) { if ($o === null) z_die("dostęp do właściwości `$name` wartości null"); z_die("dostęp do właściwości `$name` typu " . z_tn($o)); }
    if (!array_key_exists(z_k($name), $o->f)) z_die("obiekt klasy " . $o->cls . " nie ma właściwości $name");
    return $o->f[z_k($name)];
}
function z_b_zs_obj_take($o, $name) { $v = z_b_zs_obj_get($o, $name); $o->f[z_k($name)] = null; return $v; }
function z_b_zs_obj_set($o, $name, $v) { if (!($o instanceof ZObj)) z_die("zapis właściwości `$name` na wartości typu " . z_tn($o)); $o->f[z_k($name)] = $v; return $v; }
function z_b_zs_obj_put($o, $name, $v) { $o->f[z_k($name)] = $v; return $v; }
function z_b_zs_no_method($o, $name) { if ($o instanceof ZObj) z_die("klasa " . $o->cls . " nie ma metody $name"); z_die("wywołanie metody `$name` na wartości typu " . z_tn($o)); }
function z_b_zs_no_class($name) { z_die("nieznana klasa $name"); }
function z_b_zs_mkfn($id, $cap) { return new ZFn($id, $cap); }
function z_b_zs_fn_id($f) { if ($f instanceof ZFn) return $f->id; if (is_string($f)) return $f; return null; }
function z_b_zs_fn_cap($f) { return $f instanceof ZFn ? $f->cap : null; }
function z_b_is_callable($f) { return $f instanceof ZFn; }
$GLOBALS['z_statics'] = [];
function z_b_zs_sget($name) { return $GLOBALS['z_statics'][z_k($name)] ?? null; }
function z_b_zs_sset($name, $v) { $GLOBALS['z_statics'][z_k($name)] = $v; return $v; }

// ───── moduły ─────
function z_iodie($p, $what = null) { $e = $GLOBALS['z_lasterr'] ?? null; $m = $what ?? ($e ? preg_replace('/^.*: /', '', $e) : "błąd wejścia/wyjścia"); z_die("$p: $m"); }
function z_isfile($p) { return is_file($p); }
function z_withext($base) { foreach ([".zsharp", ".zs"] as $e) if (is_file($base . $e)) return $base . $e; return null; }
function z_manifest_member($toml, $name) {
    $t = @file_get_contents($toml); if ($t === false) return null;
    foreach (explode("\n", $t) as $ln) if (preg_match('/^[ \t]*([A-Za-z0-9_]+)[ \t]*=[ \t]*"([^"]*)"/', $ln, $m) && $m[1] === $name) return $m[2];
    return null;
}
function z_m_System_resolveImport($segs, $dir) {
    if (!is_array($segs)) z_die("System.resolveImport: argument 1 musi być tablicą");
    z_ns($dir, "System.resolveImport", 2); $names = array_map('z_text', array_values($segs)); if (!count($names)) return null;
    if ($names[0] === "Workspace" && count($names) >= 2) {
        $cur = $dir;
        for ($up = 0; $up < 40; $up++) {
            foreach (["", "/Z-Sharp"] as $sub) {
                $mem = z_manifest_member($cur . $sub . "/Z#.toml", $names[1]);
                if ($mem !== null) {
                    $base = $cur . $sub . "/" . $mem;
                    if (is_file($base)) {
                        for ($k = 2; $k < count($names); $k++) { $base = z_withext(dirname($base) . "/" . $names[$k]); if ($base === null) return null; }
                        return $base;
                    }
                }
            }
            $sl = strrpos($cur, "/");
            if ($sl === false || $sl === 0) { if ($cur !== "/" && $cur !== ".") $cur = "."; else break; if ($up > 0 && $cur === ".") break; } else $cur = substr($cur, 0, $sl);
            if ($cur === "." && $up > 0) $cur = getcwd();
        }
        return null;
    }
    if ($names[0] === "Std" && count($names) === 2) {
        $lo = strtolower($names[1]);
        foreach ([getenv("ZS_STD") ? getenv("ZS_STD") . "/$lo.zsharp" : null, getenv("ZS_HOME") ? getenv("ZS_HOME") . "/stdlib/$lo.zsharp" : null] as $p) if ($p && is_file($p)) return $p;
        return null;
    }
    $dirs = [$dir, getcwd()]; if (getenv("ZS_PATH")) foreach (explode(":", getenv("ZS_PATH")) as $t) if ($t !== "") $dirs[] = $t;
    foreach ($dirs as $d) { $r = z_withext($d . "/" . implode("/", $names)); if ($r) return $r; }
    return null;
}
function z_spawn($cmd, $args, $inherit) {
    z_ns($cmd, "Process", 1); $av = array_map('z_text', array_values(z_na($args, "Process", 2)));
    $argv = array_merge([$cmd], $av);
    if ($inherit) {
        fflush(STDOUT); $p = @proc_open($argv, [], $pipes);
        if (!is_resource($p)) return [127, "", "$cmd: No such file or directory\n"];
        $rc = proc_close($p); return [$rc < 0 ? -1 : $rc, "", ""];
    }
    $p = @proc_open($argv, [0 => ["pipe", "r"], 1 => ["pipe", "w"], 2 => ["pipe", "w"]], $pipes);
    if (!is_resource($p)) return [127, "", "$cmd: No such file or directory\n"];
    fclose($pipes[0]); stream_set_blocking($pipes[1], false); stream_set_blocking($pipes[2], false);
    $o = ""; $e = ""; $open = [$pipes[1], $pipes[2]];
    while ($open) {
        $r = $open; $w = null; $x = null; if (@stream_select($r, $w, $x, 1) === false) break;
        foreach ($r as $s) { $d = fread($s, 65536); if ($d === false || ($d === "" && feof($s))) { $open = array_values(array_filter($open, fn($q) => $q !== $s)); continue; } if ($s === $pipes[1]) $o .= $d; else $e .= $d; }
    }
    fclose($pipes[1]); fclose($pipes[2]); $st = proc_get_status($p); $rc = proc_close($p);
    if (isset($st['running']) && !$st['running'] && $st['exitcode'] >= 0) $rc = $st['exitcode'];
    if ($rc === 127 && $e === "") $e = "$cmd: No such file or directory\n";
    return [$rc < 0 ? -1 : $rc, $o, $e];
}
function z_m_Process_run($c, $a) { [$s, $o, $e] = z_spawn($c, $a, false); return ["status" => $s, "stdout" => $o, "stderr" => $e]; }
function z_m_Process_exec($c, $a) { return z_spawn($c, $a, true)[0]; }
function z_m_Math_sqrt($v) { return sqrt(z_nf($v, "Math.sqrt", 1)); }
function z_m_Math_floor($v) { return (int)floor(z_nf($v, "Math.floor", 1)); }
function z_m_Math_ceil($v) { return (int)ceil(z_nf($v, "Math.ceil", 1)); }
function z_m_Math_round($v) { return (int)round(z_nf($v, "Math.round", 1)); }
function z_m_Math_abs($v) { if (is_int($v)) return $v < 0 ? z_neg($v) : $v; if (is_float($v)) return abs($v); z_die("Math.abs: " . z_tn($v)); }
function z_m_Math_pow($a, $b) { return z_pow($a, $b); }
function z_m_Math_min($a, $b) { return z_cmp($a, $b) <= 0 ? $a : $b; }
function z_m_Math_max($a, $b) { return z_cmp($a, $b) <= 0 ? $b : $a; }
function z_m_System_args() { global $argv; return array_values($argv); }
function z_m_System_env($k) { $e = getenv(z_ns($k, "System.env", 1)); return $e === false ? null : $e; }
function z_m_System_os() { return PHP_OS_FAMILY === "Darwin" ? "macos" : strtolower(PHP_OS_FAMILY); }
function z_m_System_architecture() { $m = php_uname("m"); return $m === "x86_64" || $m === "amd64" ? "x86_64" : ($m === "aarch64" || $m === "arm64" ? "aarch64" : "unknown"); }
function z_m_System_time() { return microtime(true); }
function z_m_System_cpuCount() { $n = (int)@shell_exec("nproc 2>/dev/null"); return $n > 0 ? $n : 1; }
function z_m_System_version() { return "0.0.1"; }
function z_m_System_dirname($p) {
    $s = z_ns($p, "System.dirname", 1); $i = strlen($s); while ($i > 1 && $s[$i - 1] === "/") $i--; while ($i > 0 && $s[$i - 1] !== "/") $i--; while ($i > 1 && $s[$i - 1] === "/") $i--;
    if ($i === 0) return "."; if ($i === 1 && $s[0] === "/") return "/"; return substr($s, 0, $i);
}
function z_m_System_setEnv($k, $v) { putenv(z_ns($k, "System.setEnv", 1) . "=" . z_text($v)); return null; }
function z_m_System_unsetEnv($k) { putenv(z_ns($k, "System.unsetEnv", 1)); return null; }
function z_m_System_environ() { $e = getenv(); ksort($e, SORT_STRING); return $e; }
function z_m_System_cwd() { return getcwd(); }
function z_m_System_chdir($p) { if (!@chdir(z_ns($p, "System.chdir", 1))) z_iodie($p); return null; }
function z_m_System_pid() { return getmypid(); }
function z_m_System_hostname() { return gethostname(); }
function z_m_System_bytes($s) { return array_values(unpack("C*", z_ns($s, "System.bytes", 1) . "") ?: []); }
function z_m_System_fromBytes($a) { $s = ""; foreach (z_na($a, "System.fromBytes", 1) as $b) $s .= chr($b & 255); return z_lossy($s); }
function z_m_System_nowNanos() { return (int)(microtime(true) * 1e6) * 1000; }
function z_m_System_monotonic() { static $t0 = null; $n = hrtime(true); if ($t0 === null) $t0 = $n; return $n - $t0 + 1000000000; }
function z_m_System_sleep($ms) { $m = z_ni($ms, "System.sleep", 1); if ($m > 0) usleep($m * 1000); return null; }
function z_m_System_localOffset($t) { return (new DateTime("@" . z_ni($t, "System.localOffset", 1)))->setTimezone(new DateTimeZone(date_default_timezone_get()))->getOffset(); }
function z_m_System_gc() { return null; }
function z_m_System_gcStats() { return []; }
function z_m_File_read($p) { $d = @file_get_contents(z_ns($p, "File.read", 1)); if ($d === false) z_iodie($p); return $d; }
function z_m_File_write($p, $d) { if (@file_put_contents(z_ns($p, "File.write", 1), z_text($d)) === false) z_iodie($p); return null; }
function z_m_File_append($p, $d) { if (@file_put_contents(z_ns($p, "File.append", 1), z_text($d), FILE_APPEND) === false) z_iodie($p); return null; }
function z_m_File_exists($p) { return is_file(z_ns($p, "File.exists", 1)); }
function z_m_File_delete($p) { if (!@unlink(z_ns($p, "File.delete", 1))) z_iodie($p); return null; }
function z_m_File_stat($p) {
    $s = @lstat(z_ns($p, "File.stat", 1)); if ($s === false) return null; $m = $s['mode'] & 0170000;
    return ["kind" => $m === 0120000 ? "link" : ($m === 0040000 ? "dir" : ($m === 0100000 ? "file" : "other")), "size" => $s['size'], "mtime" => $s['mtime'], "mode" => $s['mode'] & 07777];
}
function z_m_File_rename($a, $b) { if (!@rename(z_ns($a, "File.rename", 1), z_ns($b, "File.rename", 2))) z_iodie($a); return null; }
function z_m_File_realpath($p) { $r = realpath(z_ns($p, "File.realpath", 1)); if ($r === false) z_iodie($p, "No such file or directory"); return $r; }
function z_m_File_chmod($p, $m) { if (!@chmod(z_ns($p, "File.chmod", 1), z_ni($m, "File.chmod", 2))) z_iodie($p); return null; }
function z_m_File_symlink($t, $l) { if (!@symlink(z_ns($t, "File.symlink", 1), z_ns($l, "File.symlink", 2))) z_iodie($l); return null; }
function z_m_File_readlink($p) { $r = @readlink(z_ns($p, "File.readlink", 1)); if ($r === false) z_iodie($p); return $r; }
function z_m_Directory_create($p) { z_ns($p, "Directory.create", 1); if (!is_dir($p) && !@mkdir($p, 0777, true) && !is_dir($p)) z_iodie($p); return null; }
function z_m_Directory_exists($p) { return is_dir(z_ns($p, "Directory.exists", 1)); }
function z_m_Directory_list($p) { $l = @scandir(z_ns($p, "Directory.list", 1), SCANDIR_SORT_NONE); if ($l === false) z_iodie($p); $l = array_values(array_filter($l, fn($x) => $x !== "." && $x !== "..")); sort($l, SORT_STRING); return $l; }
function z_m_Directory_remove($p) { if (!@rmdir(z_ns($p, "Directory.remove", 1))) z_iodie($p); return null; }
foreach (["tcpConnect", "tcpListen", "accept", "read", "write", "close", "setTimeout", "udpBind", "sendTo", "recvFrom", "resolve", "localAddr", "peerAddr"] as $__n) { eval("function z_m_Socket_$__n(...\$a) { z_die('Socket.$__n: niedostępne w celu php (użyj celu c / zsc)'); }"); }

function z_echo(...$a) { echo implode(" ", array_map('z_text', $a)), "\n"; return null; }
function z_necho(...$a) { echo implode("", array_map('z_text', $a)); return null; }
function z_eecho(...$a) { fwrite(STDERR, implode(" ", array_map('z_text', $a)) . "\n"); return null; }
function z_exit($c = 0) { exit(is_int($c) ? $c : 0); }
function z_dump($v) { return z_b_dump($v); }
function z_run($top, $main, $callsMain) {
    $rc = 0;
    try { $top(); if ($main !== null && !$callsMain) { $r = $main(); if (is_int($r)) $rc = $r; } }
    catch (ZEx $e) {
        $v = $e->val;
        if ($v instanceof ZObj) { fwrite(STDERR, "Nieobsłużony wyjątek " . $v->cls . ": " . z_text($v->f['message'] ?? null) . " (linia " . (is_int($v->f['line'] ?? null) ? $v->f['line'] : 0) . ")\n"); }
        else fwrite(STDERR, "Nieobsłużony wyjątek " . ($v->kind ? "Error" : "Exception") . ": " . z_text($v->msg) . " (linia " . $v->line . ")\n");
        $rc = 1;
    }
    exit($rc);
}
