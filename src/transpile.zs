#( Transpilator Z# -> PHP 8 (składniowy, oparty o tokeny).

   ZAŁOŻENIE: cel `zst` nie był w specyfikacji zdefiniowany — wybrano PHP, bo składnia Z# z niego
   wywodzi się; zmiana celu = podmiana tego modułu. Transpilator NIE tłumaczy biblioteki
   standardowej (System, Math, File...) ani semantyki (ZAMM, domknięcia przez wartość).

   Reguły:
     func -> function; komentarze liniowe na //, blokowe na /* */, dokumentacyjne na /** */
     A.b(...)        -> A::b(...)                 A.B.c() -> A\B::c()   A.CONST -> A::CONST   A.prop -> A::$prop
     using E as $v { } -> $v = E; try { } finally { $v->close(); }
     import A.b;     -> require_once __DIR__ . '/A/b.php';   (moduły wbudowane -> komentarz)
     extern "x" {..} -> zakomentowane (PHP nie linkuje statycznie)
     "..$.."         -> $ w stringach escapowane (Z# nie ma interpolacji)
   Dostęp przez kropkę jest rozpoznawany tylko bez spacji wokół `.` (A.b), konkatenacja z
   odstępami (a . b) zostaje konkatenacją. )#

import Workspace.lexer;

namespace transpile;

class Builtins {
    const MODULES = ["System" => true, "Math" => true, "File" => true, "Directory" => true, "Process" => true, "Task" => true];
}

class Transpiler {
    public $src;
    public $toks;
    public $braces;
    public $warnings;

    public func __construct($src) {
        $this->src = $src;
        $this->toks = lexer.tokenize($src, true);
        $this->braces = [];
        $this->warnings = [];
    }

    func isOp($i, $text) { return $this->toks[$i]["kind"] === "op" && $this->toks[$i]["text"] === $text; }
    func isKw($i, $text) { return $this->toks[$i]["kind"] === "kw" && $this->toks[$i]["text"] === $text; }

    func escapeStr($text) {
        if (substr($text, 0, 1) !== "\"") { return $text; }
        $out = "";
        $n = strlen($text);
        $i = 0;
        while ($i < $n) {
            $c = $text[$i];
            if ($c === "\\") { $out .= $c; $i += strlen($c); if ($i < $n) { $d = $text[$i]; $out .= $d; $i += strlen($d); } continue; }
            if ($c === "$") { $out .= "\\$"; } else { $out .= $c; }
            $i += strlen($c);
        }
        return $out;
    }

    func comment($t) {
        $x = $t["text"];
        if (starts_with($x, "#(")) { return "/*" . replace(substr($x, 2, strlen($x) - 4), "*/", "* /") . "*/"; }
        return "//" . substr($x, 1);
    }

    # indeks tokenu kończącego deklarację extern (`;` lub zamykające `}`)
    func externEnd($i) {
        $j = $i + 1;
        $depth = 0;
        while ($this->toks[$j]["kind"] !== "eof") {
            if ($this->isOp($j, "{")) { $depth++; }
            else if ($this->isOp($j, "}")) { $depth--; if ($depth === 0) { return $j; } }
            else if ($this->isOp($j, ";") && $depth === 0) { return $j; }
            $j++;
        }
        return $j;
    }

    ##
     * Tłumaczy tokeny [from, to) zachowując odstępy z oryginału.
    ##
    public func range($from, $to) {
        $out = "";
        $i = $from;
        $prevEnd = $this->toks[$from]["start"];
        while ($i < $to) {
            $t = $this->toks[$i];
            $kind = $t["kind"];
            if ($kind === "eof") { break; }
            $gap = substr($this->src, $prevEnd, $t["start"] - $prevEnd);
            if ($kind === "shebang") { $prevEnd = $t["end"]; $i++; continue; }
            $out .= $gap;
            $endTok = $i;
            if ($kind === "comment") { $out .= $this->comment($t); }
            else if ($kind === "doc") { $out .= "/**" . $t["value"] . "*/"; }
            else if ($kind === "str") { $out .= $this->escapeStr($t["text"]); }
            else if ($kind === "kw" && $t["text"] === "func") { $out .= "function"; }
            else if ($kind === "kw" && $t["text"] === "import") {
                $j = $i + 1; $parts = [];
                while (!$this->isOp($j, ";")) { if ($this->toks[$j]["kind"] === "ident") { $parts[] = $this->toks[$j]["text"]; } $j++; }
                if (has_key(Builtins.MODULES, $parts[0])) { $out .= "// import " . join($parts, ".") . ";  (moduł wbudowany Z#)"; }
                else { $out .= "require_once __DIR__ . '/" . join($parts, "/") . ".php';"; }
                $endTok = $j;
            }
            else if ($kind === "kw" && $t["text"] === "extern") {
                $j = $this->externEnd($i);
                $raw = substr($this->src, $t["start"], $this->toks[$j]["end"] - $t["start"]);
                $out .= "/* " . replace($raw, "*/", "* /") . " */";
                $this->warnings[] = "linia " . $t["line"] . ": extern zakomentowany (brak odpowiednika w PHP)";
                $endTok = $j;
            }
            else if ($kind === "kw" && $t["text"] === "using") {
                $a = $i + 1;
                while (!$this->isKw($a, "as")) { $a++; }
                $var = $this->toks[$a + 1];
                $expr = $this->range($i + 1, $a);
                $out .= $var["text"] . " = " . $expr . ";\ntry";
                $this->braces[] = $var["text"];
                $endTok = $a + 2;   # token `{` pochłonięty tutaj
                $out .= " {";
            }
            else if ($kind === "kw" && $t["text"] === "await") {
                $this->warnings[] = "linia " . $t["line"] . ": await bez odpowiednika w PHP";
                $out .= "await";
            }
            else if ($kind === "op" && $t["text"] === "{") { $this->braces[] = null; $out .= "{"; }
            else if ($kind === "op" && $t["text"] === "}") {
                $top = null;
                if (count($this->braces) > 0) { $top = $this->braces[count($this->braces) - 1]; $this->braces = array_slice($this->braces, 0, count($this->braces) - 1); }
                if ($top !== null) { $out .= "} finally { " . $top . "->close(); }"; } else { $out .= "}"; }
            }
            else if ($kind === "ident" && !($i > 0 && $this->isOp($i - 1, "->"))) {
                $parts = [$t["text"]];
                $j = $i;
                while ($this->isOp($j + 1, ".") && $this->toks[$j + 2]["kind"] === "ident"
                       && $this->toks[$j + 1]["start"] === $this->toks[$j]["end"]
                       && $this->toks[$j + 2]["start"] === $this->toks[$j + 1]["end"]) {
                    $parts[] = $this->toks[$j + 2]["text"];
                    $j += 2;
                }
                $np = count($parts);
                if ($np < 2) { $out .= $t["text"]; }
                else {
                    $last = $parts[$np - 1];
                    $ns = join(array_slice($parts, 0, $np - 1), "\\");
                    if ($this->isOp($j + 1, "(")) { $out .= $ns . "::" . $last; }
                    else if (upper($last) === $last) { $out .= $ns . "::" . $last; }
                    else { $out .= $ns . "::$" . $last; }
                    $endTok = $j;
                }
            }
            else { $out .= $t["text"]; }
            $prevEnd = $this->toks[$endTok]["end"];
            $i = $endTok + 1;
        }
        return $out;
    }
}

##
 * Zwraca ["php" => string, "warnings" => [string]].
##
func toPhp($src) {
    $tp = new Transpiler($src);
    $body = $tp->range(0, count($tp->toks));
    return ["php" => "<?php\n" . ltrimNl($body), "warnings" => $tp->warnings];
}

func ltrimNl($s) {
    $i = 0;
    while ($i < strlen($s) && ($s[$i] === "\n" || $s[$i] === "\r")) { $i++; }
    return substr($s, $i);
}
