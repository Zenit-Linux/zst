"use strict";
// Runtime Z# dla celu js (Node >= 16). Semantyka zgodna z runtime/native/zs_rt.c:
//  int = number (bezpieczny zakres) albo bigint (do int64, przepełnienie = błąd), float = Z.F, napisy = bajty UTF-8
//  (strlen/substr/index_of po bajtach), tablice uporządkowane z kopiowaniem przy zapisie (rc), wyjątki ZExc / ZObj.
const fs = require("fs");
const cp = require("child_process");
const nodePath = require("path");

class F { constructor(v) { this.v = v; } }
class ZExc { constructor(kind, msg) { this.kind = kind; this.msg = msg; this.line = 0; } }
class ZObj { constructor(cls) { this.cls = cls; this.f = new ZArr(); } }
class ZFn { constructor(id, cap) { this.id = id; this.cap = cap; } }
class ZArr {
  constructor() { this.keys = []; this.vals = []; this.map = new Map(); this.next = 0; this.rc = 1; }
}
const MIN64 = -(2n ** 63n), MAX64 = 2n ** 63n - 1n;

// ───── wyjście (buforowane, bajtowo-bezpieczne) ─────
let outBuf = [], outLen = 0;
function flush() { if (outLen) { const s = outBuf.join(""); outBuf = []; outLen = 0; writeAll(1, s); } }
function writeAll(fd, s) {
  const b = Buffer.from(s, "utf8"); let off = 0;
  while (off < b.length) {
    try { off += fs.writeSync(fd, b, off, b.length - off); }
    catch (e) { if (e.code === "EAGAIN") continue; if (e.code === "EPIPE") return; throw e; }
  }
}
function out(s) { outBuf.push(s); outLen += s.length; if (outLen > 65536) flush(); }
function err(s) { flush(); writeAll(2, s); }

let curLine = 0;
function setLine(n) { curLine = n; }
function raise(e) {
  if (e instanceof ZExc) { if (e.line === 0) e.line = curLine; }
  else if (e instanceof ZObj) { const l = e.f.get("line"); if (!(typeof l === "number") || l === 0) aset(e.f, "line", curLine); }
  throw e;
}
function die(msg) { const e = new ZExc(1, msg); e.line = curLine; throw e; }
function throwV(x) { if (!(x instanceof ZExc) && !(x instanceof ZObj)) die("throw wymaga obiektu typu Throwable"); raise(x); }
function toExc(e) {
  if (e instanceof ZExc || e instanceof ZObj) return e;
  if (e && e.__exit) throw e;
  if (e instanceof RangeError) { const x = new ZExc(1, "przepełnienie stosu: " + e.message); x.line = curLine; return x; }
  throw e;
}
function excIs(x, kind) { return x instanceof ZExc && (kind === 0 || x.kind === kind); }
function getMessage(x) { if (!(x instanceof ZExc)) die("wywołanie metody `getMessage` na wartości typu " + tn(x)); return x.msg; }

// ───── typy ─────
const isInt = v => typeof v === "number" || typeof v === "bigint";
const isNum = v => isInt(v) || v instanceof F;
function tn(v) {
  if (v === null) return "null"; if (typeof v === "boolean") return "bool"; if (isInt(v)) return "int";
  if (v instanceof F) return "float"; if (typeof v === "string") return "string"; if (v instanceof ZArr) return "array";
  if (v instanceof ZObj) return v.cls; if (v instanceof ZFn) return "function"; return "object";
}
const asF = v => v instanceof F ? v.v : Number(v);
const normI = b => (b >= -9007199254740991n && b <= 9007199254740991n) ? Number(b) : b;
function chkI(b) { if (b < MIN64 || b > MAX64) die("przepełnienie liczby całkowitej"); return normI(b); }

// ───── bajty UTF-8 ─────
const NONASCII = /[^\x00-\x7f]/;
const bl = s => NONASCII.test(s) ? Buffer.byteLength(s, "utf8") : s.length;
const toB = s => Buffer.from(s, "utf8");
const fromB = b => b.toString("utf8");

// ───── tekst ─────
function fmtFloat(f) {
  if (Number.isNaN(f)) return "NaN";
  if (!Number.isFinite(f)) return f > 0 ? "Infinity" : "-Infinity";
  if (Math.floor(f) === f && Math.abs(f) < 1e15) return (Object.is(f, -0) ? "-0" : String(f)) + ".0";
  let s = String(f);
  if (s.indexOf("e") < 0) return s;
  const neg = s[0] === "-"; if (neg) s = s.slice(1);
  const [m, ex] = s.split("e"); const e = parseInt(ex, 10);
  const dot = m.indexOf("."); const digits = m.replace(".", ""); const ip = dot < 0 ? m.length : dot; const point = ip + e;
  let r;
  if (point <= 0) r = "0." + "0".repeat(-point) + digits;
  else if (point >= digits.length) r = digits + "0".repeat(point - digits.length);
  else r = digits.slice(0, point) + "." + digits.slice(point);
  return (neg ? "-" : "") + r;
}
function dumpStr(s) {
  let o = '"';
  const b = toB(s);
  for (let i = 0; i < b.length; i++) {
    const c = b[i];
    if (c === 34) o += '\\"'; else if (c === 92) o += "\\\\"; else if (c === 10) o += "\\n"; else if (c === 13) o += "\\r";
    else if (c === 9) o += "\\t"; else if (c === 0) o += "\\0"; else if (c < 0x20 || c === 0x7f) o += "\\u{" + c.toString(16) + "}";
    else if (c < 0x80) o += String.fromCharCode(c);
    else { let j = i + 1; while (j < b.length && (b[j] & 0xC0) === 0x80) j++; o += fromB(b.subarray(i, j)); i = j - 1; }
  }
  return o + '"';
}
function text(v, dump) {
  if (v === null) return "null"; if (v === true) return "true"; if (v === false) return "false";
  if (typeof v === "number") return String(v); if (typeof v === "bigint") return v.toString();
  if (v instanceof F) return fmtFloat(v.v);
  if (typeof v === "string") return dump ? dumpStr(v) : v;
  if (v instanceof ZArr) {
    let seq = true; for (let i = 0; i < v.keys.length; i++) if (v.keys[i] !== i) { seq = false; break; }
    const p = [];
    for (let i = 0; i < v.keys.length; i++) {
      const k = v.keys[i];
      p.push((seq ? "" : (typeof k === "string" ? dumpStr(k) : String(k)) + " => ") + text(v.vals[i], true));
    }
    return "[" + p.join(", ") + "]";
  }
  if (v instanceof ZObj) return "<object " + v.cls + ">";
  if (v instanceof ZFn) return "<function>";
  if (v instanceof ZExc) return v.kind ? "<object Error>" : "<object Exception>";
  return "<object>";
}
function truthy(v) {
  if (v === null || v === false) return false; if (v === true) return true;
  if (typeof v === "number") return v !== 0; if (typeof v === "bigint") return v !== 0n;
  if (v instanceof F) return v.v !== 0; if (typeof v === "string") return !(v === "" || v === "0");
  if (v instanceof ZArr) return v.keys.length > 0; return true;
}

// ───── tablice ─────
const mk = k => typeof k === "string" ? "s" + k : (typeof k === "bigint" ? "b" + k : k);
function arrNew() { return new ZArr(); }
function dup(v) { if (v instanceof ZArr) v.rc++; return v; }
function arrCopy(o) {
  const a = new ZArr(); a.keys = o.keys.slice(); a.vals = o.vals.slice(); a.next = o.next;
  for (let i = 0; i < a.keys.length; i++) { a.map.set(mk(a.keys[i]), i); if (a.vals[i] instanceof ZArr) a.vals[i].rc++; }
  return a;
}
function wr(v) {
  if (v === null) return new ZArr();
  if (!(v instanceof ZArr)) die("nie można indeksować wartości typu " + tn(v) + " przy zapisie");
  if (v.rc > 1) { v.rc--; return arrCopy(v); }
  return v;
}
function normKey(k) {
  if (typeof k === "number" || typeof k === "bigint") return k;
  if (typeof k === "boolean") return k ? 1 : 0;
  if (k instanceof F) { if (Number.isFinite(k.v) && Math.floor(k.v) === k.v) return normI(BigInt(k.v)); }
  else if (typeof k === "string") {
    const n = k.length;
    if (n > 0 && n <= 20 && /^-?(0|[1-9][0-9]*)$/.test(k) && k !== "-0") { const b = BigInt(k); if (b >= MIN64 && b <= MAX64) return normI(b); }
    return k;
  }
  die("niepoprawny klucz tablicy typu " + tn(k));
}
function put(a, k, v) {
  const m = mk(k); const i = a.map.get(m);
  if (i !== undefined) { a.vals[i] = v; return; }
  a.map.set(m, a.keys.length); a.keys.push(k); a.vals.push(v);
  if (typeof k === "number" && k >= a.next) a.next = k + 1;
  else if (typeof k === "bigint" && k >= BigInt(a.next)) a.next = Number(k) + 1;
}
ZArr.prototype.get = function (k) { const i = this.map.get(mk(k)); return i === undefined ? null : this.vals[i]; };
function apush(a, v) { put(a, a.next, v); return v; }
function aset(a, k, v) { if (!(a instanceof ZArr)) die("nie można indeksować wartości typu " + tn(a) + " przy zapisie"); put(a, normKey(k), v); return v; }
function wrappend(p) { const n = new ZArr(); apush(p, n); return n; }
function wrelem(p, k) {
  const nk = normKey(k); const i = p.map.get(mk(nk));
  if (i === undefined) { const n = new ZArr(); put(p, nk, n); return n; }
  const cur = p.vals[i]; const w = wr(cur); if (w !== cur) p.vals[i] = w; return w;
}
function keyText(k) { return typeof k === "string" ? dumpStr(k) : String(k); }
function index(a, k, quiet) {
  if (a instanceof ZArr) {
    const nk = normKey(k); const i = a.map.get(mk(nk));
    if (i !== undefined) return a.vals[i];
    if (quiet) return null; die("brak klucza " + keyText(nk));
  }
  if (typeof a === "string") {
    if (!isInt(k)) die("indeks tekstu musi być liczbą całkowitą");
    const b = toB(a), len = b.length; let i = Number(k); if (i < 0) i = len + i;
    if (i < 0 || i >= len) { if (quiet) return null; die("indeks " + i + " poza zakresem tekstu (długość " + len + ")"); }
    const c = b[i];
    if ((c & 0xC0) === 0x80) { if (quiet) return null; die("indeks " + i + " wskazuje środek znaku UTF-8"); }
    let n = c < 0x80 ? 1 : c < 0xE0 ? 2 : c < 0xF0 ? 3 : 4; if (i + n > len) n = len - i;
    return fromB(b.subarray(i, i + n));
  }
  if (a === null) { if (quiet) return null; die("indeksowanie wartości null"); }
  die("typ " + tn(a) + " nie obsługuje indeksowania");
}
const indexFn = (a, k) => index(a, k, false);
const indexOpt = (a, k) => index(a, k, true);
function foreachArr(v) { if (!(v instanceof ZArr)) die("foreach wymaga tablicy, jest " + tn(v)); return v; }

// ───── porównania i operatory ─────
function ident(a, b) {
  if (a instanceof F) return b instanceof F && a.v === b.v;
  if (a === b) return true;
  if (a instanceof ZArr && b instanceof ZArr) {
    if (a.keys.length !== b.keys.length) return false;
    for (let i = 0; i < a.keys.length; i++) if (a.keys[i] !== b.keys[i] || !ident(a.vals[i], b.vals[i])) return false;
    return true;
  }
  return false;
}
function eq(a, b) { if (isNum(a) && isNum(b) && (a instanceof F) !== (b instanceof F)) return asF(a) === asF(b); return ident(a, b); }
function cmpStr(a, b) {
  if (a === b) return 0;
  if (!NONASCII.test(a) && !NONASCII.test(b)) return a < b ? -1 : 1;
  const c = Buffer.compare(toB(a), toB(b)); return c < 0 ? -1 : c > 0 ? 1 : 0;
}
function cmp(a, b) {
  if (isInt(a) && isInt(b)) return a < b ? -1 : a > b ? 1 : 0;
  if (isNum(a) && isNum(b)) { const x = asF(a), y = asF(b); return x < y ? -1 : x > y ? 1 : 0; }
  if (typeof a === "string" && typeof b === "string") return cmpStr(a, b);
  if (typeof a === "boolean" && typeof b === "boolean") return a === b ? 0 : (a ? 1 : -1);
  die("nie można porównać " + tn(a) + " z " + tn(b));
}
const badOp = (op, a, b) => die("operacja " + op + " niedostępna dla " + tn(a) + " i " + tn(b));
function add(a, b) {
  if (typeof a === "number" && typeof b === "number") { const r = a + b; if (Number.isSafeInteger(r)) return r; return chkI(BigInt(a) + BigInt(b)); }
  if (isInt(a) && isInt(b)) return chkI(BigInt(a) + BigInt(b));
  if (isNum(a) && isNum(b)) return new F(asF(a) + asF(b));
  if (a instanceof ZArr && b instanceof ZArr) {
    const r = arrCopy(a);
    for (let i = 0; i < b.keys.length; i++) if (!r.map.has(mk(b.keys[i]))) { if (b.vals[i] instanceof ZArr) b.vals[i].rc++; put(r, b.keys[i], b.vals[i]); }
    return r;
  }
  badOp("Add", a, b);
}
function sub(a, b) {
  if (typeof a === "number" && typeof b === "number") { const r = a - b; if (Number.isSafeInteger(r)) return r; return chkI(BigInt(a) - BigInt(b)); }
  if (isInt(a) && isInt(b)) return chkI(BigInt(a) - BigInt(b));
  if (isNum(a) && isNum(b)) return new F(asF(a) - asF(b));
  badOp("Sub", a, b);
}
function mul(a, b) {
  if (typeof a === "number" && typeof b === "number") { const r = a * b; if (Number.isSafeInteger(r)) return r; return chkI(BigInt(a) * BigInt(b)); }
  if (isInt(a) && isInt(b)) return chkI(BigInt(a) * BigInt(b));
  if (isNum(a) && isNum(b)) return new F(asF(a) * asF(b));
  badOp("Mul", a, b);
}
function div(a, b) {
  if (!(isNum(a) && isNum(b))) badOp("Div", a, b);
  if (asF(b) === 0) die("dzielenie przez zero");
  if (isInt(a) && isInt(b)) {
    if (typeof a === "number" && typeof b === "number") { if (a % b === 0) return a / b; }
    else { const x = BigInt(a), y = BigInt(b); if (x % y === 0n) return chkI(x / y); }
  }
  return new F(asF(a) / asF(b));
}
function mod(a, b) {
  if (isInt(a) && isInt(b)) {
    if (b === 0 || b === 0n) die("modulo przez zero");
    if (b === -1 || b === -1n) return 0;
    if (typeof a === "number" && typeof b === "number") return a % b === 0 ? 0 : a % b;
    return normI(BigInt(a) % BigInt(b));
  }
  badOp("Mod", a, b);
}
const small = v => typeof v === "number" && v >= -2147483648 && v <= 2147483647;
function bitand(a, b) { if (isInt(a) && isInt(b)) return small(a) && small(b) ? (a & b) : normI(BigInt.asIntN(64, BigInt(a) & BigInt(b))); badOp("BitAnd", a, b); }
function bitor(a, b) { if (isInt(a) && isInt(b)) return small(a) && small(b) ? (a | b) : normI(BigInt.asIntN(64, BigInt(a) | BigInt(b))); badOp("BitOr", a, b); }
function bitxor(a, b) { if (isInt(a) && isInt(b)) return small(a) && small(b) ? (a ^ b) : normI(BigInt.asIntN(64, BigInt(a) ^ BigInt(b))); badOp("BitXor", a, b); }
function shl(a, b) { if (isInt(a) && isInt(b)) return normI(BigInt.asIntN(64, BigInt(a) << (BigInt(b) & 63n))); badOp("Shl", a, b); }
function shr(a, b) { if (isInt(a) && isInt(b)) return normI(BigInt(a) >> (BigInt(b) & 63n)); badOp("Shr", a, b); }
function bitnot(a) { if (isInt(a)) return typeof a === "number" ? -a - 1 : normI(-a - 1n); die("operator jednoargumentowy BitNot niedostępny dla " + tn(a)); }
function pow(a, b) {
  if (isInt(a) && isInt(b) && b >= 0) {
    let r = 1n, base = BigInt(a), e = BigInt(b), ovf = false;
    while (e > 0n) {
      if (e & 1n) { r *= base; if (r < MIN64 || r > MAX64) { ovf = true; break; } }
      e >>= 1n; if (e > 0n) { base *= base; if (base < MIN64 || base > MAX64) { ovf = true; break; } }
    }
    if (!ovf) return normI(r);
    return new F(Math.pow(Number(a), Number(b)));
  }
  if (isNum(a) && isNum(b)) return new F(Math.pow(asF(a), asF(b)));
  badOp("Pow", a, b);
}
const concat = (a, b) => text(a) + text(b);
function neg(a) {
  if (isInt(a)) { if (typeof a === "number") return a === 0 ? 0 : -a; return chkI(-a); }
  if (a instanceof F) return new F(-a.v);
  die("operator jednoargumentowy Neg niedostępny dla " + tn(a));
}
function chk(v, type, what) {
  let ok = false;
  if (type === "int") ok = isInt(v);
  else if (type === "float") { ok = v instanceof F; if (!ok && isInt(v)) return new F(Number(v)); }
  else if (type === "string") ok = typeof v === "string"; else if (type === "bool") ok = typeof v === "boolean";
  else if (type === "array") ok = v instanceof ZArr; else if (v instanceof ZObj || v instanceof ZExc) ok = true;
  if (ok) return v;
  die(what + ": oczekiwano " + type + ", otrzymano " + tn(v));
}

// ───── wbudowane ─────
const needS = (v, f, n) => { if (typeof v !== "string") die(f + ": argument #" + n + " powinien być tekstem, jest " + tn(v)); return v; };
const needI = (v, f, n) => { if (!isInt(v)) die(f + ": argument #" + n + " powinien być liczbą całkowitą, jest " + tn(v)); return v; };
const needF = (v, f, n) => { if (!isNum(v)) die(f + ": argument #" + n + " powinien być liczbą, jest " + tn(v)); return asF(v); };
const needA = (v, f, n) => { if (!(v instanceof ZArr)) die(f + ": argument #" + n + " powinien być tablicą, jest " + tn(v)); return v; };
const WS = /^[ \t-\r]+|[ \t-\r]+$/g;
function toInt(v) {
  if (isInt(v)) return v;
  if (v instanceof F) { const f = v.v; return Number.isNaN(f) ? 0 : f >= 9.2233720368547758e18 ? normI(MAX64) : f <= -9.2233720368547758e18 ? normI(MIN64) : normI(BigInt(Math.trunc(f))); }
  if (typeof v === "boolean") return v ? 1 : 0; if (v === null) return 0;
  if (typeof v === "string") {
    const t = v.replace(WS, "");
    if (/^[+-]?[0-9]+$/.test(t)) { const b = BigInt(t); if (b >= MIN64 && b <= MAX64) return normI(b); }
    if (t !== "" && /^[+-]?(\d+\.?\d*([eE][+-]?\d+)?|\.\d+([eE][+-]?\d+)?)$/.test(t)) { const f = parseFloat(t); return Number.isNaN(f) ? 0 : normI(BigInt(Math.trunc(f))); }
    return 0;
  }
  die("intval: nie można przekonwertować " + tn(v));
}
function toFloat(v) {
  if (isInt(v)) return new F(Number(v)); if (v instanceof F) return v;
  if (typeof v === "string") { const t = v.replace(WS, ""); return new F(t !== "" && /^[+-]?(\d+\.?\d*([eE][+-]?\d+)?|\.\d+([eE][+-]?\d+)?|inf(inity)?|nan)$/i.test(t) ? parseFloat(t) : 0); }
  return new F(0);
}
function sliceBytes(s, st, n) { return fromB(toB(s).subarray(st, st + n)); }
function pad(a, n, p, left, f) {
  const s = needS(a, f, 1); let w = Number(needI(n, f, 2)); if (w < 0) w = 0;
  let pc = " "; if (typeof p === "string" && p.length > 0) pc = Array.from(p)[0];
  const l = Array.from(s).length; if (l >= w) return a;
  const padding = pc.repeat(w - l); return left ? padding + s : s + padding;
}
function u8chars(s) { return Array.from(s); }
const B = {
  count(v) { if (v instanceof ZArr) return v.keys.length; if (typeof v === "string") return bl(v); die("count: niedostępne dla " + tn(v)); },
  strlen(v) { return bl(needS(v, "strlen", 1)); },
  typeof(v) { if (v instanceof ZObj) return v.cls; if (v instanceof ZExc) return v.kind ? "Error" : "Exception"; return tn(v); },
  is_int: isInt, is_float: v => v instanceof F, is_string: v => typeof v === "string", is_bool: v => typeof v === "boolean",
  is_array: v => v instanceof ZArr, is_null: v => v === null,
  strval(v) { return text(v); }, boolval(v) { return truthy(v); }, intval: toInt, floatval: toFloat,
  ord(v) { const s = needS(v, "ord", 1); if (!s.length) die("ord: pusty tekst"); return toB(s)[0]; },
  chr(v) { const n = Number(needI(v, "chr", 1)); return (n >= 0 && n < 0x110000 && !(n >= 0xD800 && n < 0xE000)) ? String.fromCodePoint(n) : ""; },
  trim(v) { return needS(v, "trim", 1).replace(WS, ""); },
  contains(a, b) { return needS(a, "contains", 1).includes(needS(b, "contains", 2)); },
  starts_with(a, b) { return needS(a, "starts_with", 1).startsWith(needS(b, "starts_with", 2)); },
  ends_with(a, b) { return needS(a, "ends_with", 1).endsWith(needS(b, "ends_with", 2)); },
  index_of(a, b) {
    const x = needS(a, "index_of", 1), y = needS(b, "index_of", 2); const i = x.indexOf(y);
    return i < 0 ? -1 : (NONASCII.test(x) ? Buffer.byteLength(x.slice(0, i), "utf8") : i);
  },
  repeat(a, b) { const x = needS(a, "repeat", 1); let n = Number(needI(b, "repeat", 2)); if (n < 0) n = 0; return x.repeat(n); },
  substr(s, a, l) {
    const x = needS(s, "substr", 1); const len = bl(x); let st = Number(needI(a, "substr", 2));
    if (st < 0) { st = len + st; if (st < 0) st = 0; } if (st > len) st = len;
    let n; if (l !== undefined && isInt(l)) { n = Number(l) >= 0 ? Number(l) : len - st + Number(l); if (n < 0) n = 0; } else n = len - st;
    if (st + n > len) n = len - st;
    return NONASCII.test(x) ? sliceBytes(x, st, n) : x.substr(st, n);
  },
  join(a, sep) { const x = needA(a, "join", 1); return x.vals.map(v => text(v)).join(typeof sep === "string" ? sep : ""); },
  in_array(n, arr) { const x = needA(arr, "in_array", 2); for (const v of x.vals) if (ident(v, n)) return true; return false; },
  range(a, b) {
    const x = needI(a, "range", 1), y = needI(b, "range", 2); const r = new ZArr();
    if (x <= y) for (let i = x; ; i = add(i, 1)) { apush(r, i); if (i === y) break; } else for (let i = x; ; i = sub(i, 1)) { apush(r, i); if (i === y) break; }
    return r;
  },
  keys(a) { const x = needA(a, "keys", 1); const r = new ZArr(); for (const k of x.keys) apush(r, k); return r; },
  values(a) { const x = needA(a, "values", 1); const r = new ZArr(); for (const v of x.vals) apush(r, dup(v)); return r; },
  has_key(a, k) { return needA(a, "has_key", 1).map.has(mk(normKey(k))); },
  upper(v) { return needS(v, "upper", 1).replace(/[a-z]/g, c => c.toUpperCase()); },
  lower(v) { return needS(v, "lower", 1).replace(/[A-Z]/g, c => c.toLowerCase()); },
  replace(a, f, t) { const s = needS(a, "replace", 1), x = needS(f, "replace", 2), y = needS(t, "replace", 3); return x === "" ? s : s.split(x).join(y); },
  split(a, sep) {
    const s = needS(a, "split", 1), x = needS(sep, "split", 2); const r = new ZArr();
    if (x === "") { for (const c of u8chars(s)) apush(r, c); return r; }
    for (const p of s.split(x)) apush(r, p); return r;
  },
  chars(a) { return B.split(a, ""); },
  pad_left(a, n, p) { return pad(a, n, p, true, "pad_left"); }, pad_right(a, n, p) { return pad(a, n, p, false, "pad_right"); },
  array_slice(a, s, l) {
    const x = needA(a, "array_slice", 1); const len = x.keys.length; let st = Number(needI(s, "array_slice", 2));
    if (st < 0) { st = len + st; if (st < 0) st = 0; } if (st > len) st = len;
    let n = (l !== undefined && isInt(l)) ? Number(l) : len; if (n < 0) n = 0; let end = st + n; if (end > len) end = len;
    const r = new ZArr(); for (let i = st; i < end; i++) apush(r, dup(x.vals[i])); return r;
  },
  reverse(a) {
    if (typeof a === "string") return u8chars(a).reverse().join("");
    if (!(a instanceof ZArr)) die("reverse: niedostępne dla " + tn(a));
    const r = new ZArr(); for (let i = a.vals.length - 1; i >= 0; i--) apush(r, dup(a.vals[i])); return r;
  },
  sort(a) {
    const x = needA(a, "sort", 1); const idx = x.vals.map((v, i) => [v, i]);
    // sortowanie stabilne scalaniem; błąd porównania propaguje się jako wyjątek
    const ms = arr => { if (arr.length < 2) return arr; const m = arr.length >> 1, l = ms(arr.slice(0, m)), r = ms(arr.slice(m)); const o = []; let i = 0, j = 0; while (i < l.length && j < r.length) o.push(cmp(r[j][0], l[i][0]) < 0 ? r[j++] : l[i++]); while (i < l.length) o.push(l[i++]); while (j < r.length) o.push(r[j++]); return o; };
    const r = new ZArr(); for (const [v] of ms(idx)) apush(r, dup(v)); return r;
  },
  push(a, v) { needA(a, "push", 1); const r = wr(dup(a)); apush(r, v); return r; },
  dump(v) { out(text(v, true) + "\n"); return null; },
  is_digit: v => { const s = needS(v, "is_digit", 1); return s.length > 0 && /^[0-9]/.test(s); },
  is_alpha: v => { const s = needS(v, "is_alpha", 1); return s.length > 0 && /^[A-Za-z_\u0080-\uffff]/.test(s); },
  is_alnum: v => { const s = needS(v, "is_alnum", 1); return s.length > 0 && /^[A-Za-z0-9_\u0080-\uffff]/.test(s); },
  is_space: v => { const s = needS(v, "is_space", 1); return s.length > 0 && /^[ \t-\r]/.test(s); },
  is_hex_digit: v => { const s = needS(v, "is_hex_digit", 1); return s.length > 0 && /^[0-9A-Fa-f]/.test(s); },
  to_hex(v) { const n = needI(v, "to_hex", 1); return (BigInt.asUintN(64, BigInt(n))).toString(16); },
  read_line() {
    flush(); const buf = Buffer.alloc(1); const bytes = []; let any = false;
    for (;;) {
      let n = 0; try { n = fs.readSync(0, buf, 0, 1, null); } catch (e) { if (e.code === "EAGAIN") continue; if (e.code === "EOF") n = 0; else throw e; }
      if (n === 0) break; any = true; if (buf[0] === 10) break; bytes.push(buf[0]);
    }
    if (!any) return null; while (bytes.length && bytes[bytes.length - 1] === 13) bytes.pop();
    return fromB(Buffer.from(bytes));
  },
  // klasy / domknięcia
  zs_obj_new(cls) { return new ZObj(cls); },
  zs_obj_class(o) { return o instanceof ZObj ? o.cls : null; },
  zs_obj_get(o, name) {
    if (!(o instanceof ZObj)) { if (o === null) die("dostęp do właściwości `" + name + "` wartości null"); die("dostęp do właściwości `" + name + "` typu " + tn(o)); }
    const i = o.f.map.get(mk(name)); if (i === undefined) die("obiekt klasy " + o.cls + " nie ma właściwości " + name);
    return dup(o.f.vals[i]);
  },
  zs_obj_take(o, name) { const v = B.zs_obj_get(o, name); const i = o.f.map.get(mk(name)); if (v instanceof ZArr) v.rc--; o.f.vals[i] = null; return v; },
  zs_obj_set(o, name, v) { if (!(o instanceof ZObj)) die("zapis właściwości `" + name + "` na wartości typu " + tn(o)); put(o.f, name, dup(v)); return v; },
  zs_obj_put(o, name, v) { put(o.f, name, v); return v; },
  zs_no_method(o, name) { if (o instanceof ZObj) die("klasa " + o.cls + " nie ma metody " + name); die("wywołanie metody `" + name + "` na wartości typu " + tn(o)); },
  zs_no_class(name) { die("nieznana klasa " + name); },
  zs_mkfn(id, cap) { return new ZFn(id, cap); },
  zs_fn_id(f) { if (f instanceof ZFn) return f.id; if (typeof f === "string") return f; return null; },
  zs_fn_cap(f) { return f instanceof ZFn ? f.cap : null; },
  is_callable(f) { return f instanceof ZFn; },
  zs_sget(name) { return statics.has(name) ? dup(statics.get(name)) : null; },
  zs_sset(name, v) { statics.set(name, dup(v)); return v; },
};
const statics = new Map();

// ───── moduły ─────
function ioDie(p, e) { die(p + ": " + (e.code === "ENOENT" ? "No such file or directory" : e.code === "EACCES" ? "Permission denied" : e.code === "EISDIR" ? "Is a directory" : e.code === "EEXIST" ? "File exists" : e.code === "ENOTDIR" ? "Not a directory" : e.code === "ENOTEMPTY" ? "Directory not empty" : e.message)); }
function isFileP(p) { try { return fs.statSync(p).isFile(); } catch (e) { return false; } }
function dirnameZ(s) {
  let i = s.length; while (i > 1 && s[i - 1] === "/") i--; while (i > 0 && s[i - 1] !== "/") i--; while (i > 1 && s[i - 1] === "/") i--;
  if (i === 0) return "."; if (i === 1 && s[0] === "/") return "/"; return s.slice(0, i);
}
function withExt(base) { for (const e of [".zsharp", ".zs"]) if (isFileP(base + e)) return base + e; return null; }
function manifestMember(toml, name) {
  let t; try { t = fs.readFileSync(toml, "utf8"); } catch (e) { return null; }
  for (const ln of t.split("\n")) { const m = ln.match(/^[ \t]*([A-Za-z0-9_]+)[ \t]*=[ \t]*"([^"]*)"/); if (m && m[1] === name) return m[2]; }
  return null;
}
function resolveImport(segs, dir) {
  if (!(segs instanceof ZArr)) die("System.resolveImport: argument 1 musi być tablicą");
  needS(dir, "System.resolveImport", 2); const names = segs.vals.map(v => text(v)); if (!names.length) return null;
  if (names[0] === "Workspace" && names.length >= 2) {
    let cur = dir;
    for (let up = 0; up < 40; up++) {
      for (const sub of ["", "/Z-Sharp"]) {
        const mem = manifestMember(cur + sub + "/Z#.toml", names[1]);
        if (mem !== null) {
          let base = cur + sub + "/" + mem;
          if (isFileP(base)) {
            for (let k = 2; k < names.length; k++) { base = withExt(nodePath.dirname(base) + "/" + names[k]); if (base === null) return null; }
            return base;
          }
        }
      }
      const sl = cur.lastIndexOf("/");
      if (sl <= 0) { if (cur !== "/" && cur !== ".") cur = "."; else break; if (up > 0 && cur === ".") break; } else cur = cur.slice(0, sl);
      if (cur === "." && up > 0) cur = process.cwd();
    }
    return null;
  }
  if (names[0] === "Std" && names.length === 2) {
    const lo = names[1].toLowerCase();
    for (const p of [process.env.ZS_STD && process.env.ZS_STD + "/" + lo + ".zsharp", process.env.ZS_HOME && process.env.ZS_HOME + "/stdlib/" + lo + ".zsharp"]) if (p && isFileP(p)) return p;
    return null;
  }
  const dirs = [dir, process.cwd()]; if (process.env.ZS_PATH) for (const t of process.env.ZS_PATH.split(":")) if (t) dirs.push(t);
  for (const d of dirs) { const r = withExt(d + "/" + names.join("/")); if (r) return r; }
  return null;
}
function spawn(cmd, args, inherit) {
  needS(cmd, "Process", 1); const av = needA(args, "Process", 2).vals.map(v => text(v));
  flush();
  const r = cp.spawnSync(cmd, av, inherit ? { stdio: "inherit" } : { maxBuffer: 1 << 30 });
  if (r.error) { const msg = cmd + ": " + (r.error.code === "ENOENT" ? "No such file or directory" : r.error.message) + "\n"; return { status: 127, stdout: Buffer.alloc(0), stderr: Buffer.from(msg) }; }
  return { status: r.status === null ? -1 : r.status, stdout: r.stdout || Buffer.alloc(0), stderr: r.stderr || Buffer.alloc(0) };
}
function assoc(pairs) { const a = new ZArr(); for (const [k, v] of pairs) put(a, k, v); return a; }
const t0 = process.hrtime.bigint();
function noSocket(n) { return () => die("Socket." + n + ": niedostępne w celu js (użyj celu c / zsc)"); }
const M = {
  Math: {
    sqrt: v => new F(Math.sqrt(needF(v, "Math.sqrt", 1))),
    floor: v => normI(BigInt(Math.floor(needF(v, "Math.floor", 1)))), ceil: v => normI(BigInt(Math.ceil(needF(v, "Math.ceil", 1)))),
    round: v => { const x = needF(v, "Math.round", 1); return normI(BigInt(x < 0 ? -Math.round(-x) : Math.round(x))); },
    abs: v => { if (isInt(v)) return typeof v === "number" ? Math.abs(v) : chkI(v < 0n ? -v : v); if (v instanceof F) return new F(Math.abs(v.v)); die("Math.abs: " + tn(v)); },
    pow: (a, b) => pow(a, b), min: (a, b) => cmp(a, b) <= 0 ? a : b, max: (a, b) => cmp(a, b) <= 0 ? b : a,
  },
  System: {
    args: () => { const r = new ZArr(); for (const a of sysArgs) apush(r, a); return r; },
    env: k => { const e = process.env[needS(k, "System.env", 1)]; return e === undefined ? null : e; },
    os: () => process.platform === "darwin" ? "macos" : process.platform === "win32" ? "windows" : process.platform,
    architecture: () => process.arch === "x64" ? "x86_64" : process.arch === "arm64" ? "aarch64" : "unknown",
    time: () => new F(Date.now() / 1000), cpuCount: () => Math.max(1, require("os").cpus().length),
    version: () => "0.0.1", dirname: p => dirnameZ(needS(p, "System.dirname", 1)), resolveImport,
    setEnv: (k, v) => { process.env[needS(k, "System.setEnv", 1)] = text(v); return null; },
    unsetEnv: k => { delete process.env[needS(k, "System.unsetEnv", 1)]; return null; },
    environ: () => assoc(Object.keys(process.env).sort().map(k => [k, process.env[k]])),
    cwd: () => process.cwd(), chdir: p => { try { process.chdir(needS(p, "System.chdir", 1)); } catch (e) { ioDie(p, e); } return null; },
    pid: () => process.pid, hostname: () => require("os").hostname(),
    bytes: s => { const r = new ZArr(); for (const b of toB(needS(s, "System.bytes", 1))) apush(r, b); return r; },
    fromBytes: a => fromB(Buffer.from(needA(a, "System.fromBytes", 1).vals.map(v => Number(v) & 255))),
    nowNanos: () => normI(BigInt(Date.now()) * 1000000n), monotonic: () => normI(process.hrtime.bigint() - t0 + 1000000000n),
    sleep: ms => { flush(); const m = Number(needI(ms, "System.sleep", 1)); if (m > 0) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, m); return null; },
    localOffset: t => -new Date(Number(needI(t, "System.localOffset", 1)) * 1000).getTimezoneOffset() * 60,
    gc: () => null, gcStats: () => new ZArr(),
  },
  File: {
    read: p => { try { return fromB(fs.readFileSync(needS(p, "File.read", 1))); } catch (e) { ioDie(p, e); } },
    write: (p, d) => { needS(p, "File.write", 1); try { fs.writeFileSync(p, text(d)); } catch (e) { ioDie(p, e); } return null; },
    append: (p, d) => { needS(p, "File.append", 1); try { fs.appendFileSync(p, text(d)); } catch (e) { ioDie(p, e); } return null; },
    exists: p => isFileP(needS(p, "File.exists", 1)),
    delete: p => { try { fs.unlinkSync(needS(p, "File.delete", 1)); } catch (e) { ioDie(p, e); } return null; },
    stat: p => { let st; try { st = fs.lstatSync(needS(p, "File.stat", 1)); } catch (e) { return null; } return assoc([["kind", st.isSymbolicLink() ? "link" : st.isDirectory() ? "dir" : st.isFile() ? "file" : "other"], ["size", st.size], ["mtime", Math.floor(st.mtimeMs / 1000)], ["mode", st.mode & 0o7777]]); },
    rename: (a, b) => { try { fs.renameSync(needS(a, "File.rename", 1), needS(b, "File.rename", 2)); } catch (e) { ioDie(a, e); } return null; },
    realpath: p => { try { return fs.realpathSync(needS(p, "File.realpath", 1)); } catch (e) { ioDie(p, e); } },
    chmod: (p, m) => { try { fs.chmodSync(needS(p, "File.chmod", 1), Number(needI(m, "File.chmod", 2))); } catch (e) { ioDie(p, e); } return null; },
    symlink: (t, l) => { try { fs.symlinkSync(needS(t, "File.symlink", 1), needS(l, "File.symlink", 2)); } catch (e) { ioDie(l, e); } return null; },
    readlink: p => { try { return fs.readlinkSync(needS(p, "File.readlink", 1)); } catch (e) { ioDie(p, e); } },
  },
  Directory: {
    create: p => { try { fs.mkdirSync(needS(p, "Directory.create", 1), { recursive: true }); } catch (e) { ioDie(p, e); } return null; },
    exists: p => { try { return fs.statSync(needS(p, "Directory.exists", 1)).isDirectory(); } catch (e) { return false; } },
    list: p => { let n; try { n = fs.readdirSync(needS(p, "Directory.list", 1)); } catch (e) { ioDie(p, e); } n.sort((a, b) => cmpStr(a, b)); const r = new ZArr(); for (const x of n) apush(r, x); return r; },
    remove: p => { try { fs.rmdirSync(needS(p, "Directory.remove", 1)); } catch (e) { ioDie(p, e); } return null; },
  },
  Process: {
    run: (c, a) => { const r = spawn(c, a, false); return assoc([["status", r.status], ["stdout", fromB(r.stdout)], ["stderr", fromB(r.stderr)]]); },
    exec: (c, a) => spawn(c, a, true).status,
  },
  Socket: Object.fromEntries(["tcpConnect", "tcpListen", "accept", "read", "write", "close", "setTimeout", "udpBind", "sendTo", "recvFrom", "resolve", "localAddr", "peerAddr"].map(n => [n, noSocket(n)])),
};
let sysArgs = [process.argv[1] || "zs"];
function setArgs(a) { sysArgs = a; }

// ───── wejście/wyjście programu ─────
class Exit { constructor(c) { this.__exit = true; this.code = c; } }
function echo(...a) { out(a.map(x => text(x)).join(" ") + "\n"); return null; }
function necho(...a) { out(a.map(x => text(x)).join("")); return null; }
function eecho(...a) { err(a.map(x => text(x)).join(" ") + "\n"); return null; }
function exit(c) { throw new Exit(c === undefined || !isInt(c) ? 0 : Number(c)); }
function run(top, main, callsMain) {
  sysArgs = [process.argv[1] || "zs"].concat(process.argv.slice(2));
  let rc = 0;
  try {
    top();
    if (main && !callsMain) { const r = main(); if (isInt(r)) rc = Number(r); }
  } catch (e) {
    if (e instanceof Exit) rc = e.code;
    else if (e instanceof ZExc || e instanceof ZObj) {
      flush();
      if (e instanceof ZObj) { const l = e.f.get("line"); err("Nieobsłużony wyjątek " + e.cls + ": " + text(e.f.get("message")) + " (linia " + (isInt(l) ? l : 0) + ")\n"); }
      else err("Nieobsłużony wyjątek " + (e.kind ? "Error" : "Exception") + ": " + text(e.msg) + " (linia " + e.line + ")\n");
      rc = 1;
    } else if (e instanceof RangeError) { flush(); err("Nieobsłużony wyjątek Error: przepełnienie stosu (linia " + curLine + ")\n"); rc = 1; }
    else throw e;
  }
  flush();
  process.exitCode = rc;
}

module.exports = {
  F, ZExc, ZObj, ZFn, ZArr, B, M, setLine, raise, throwV, toExc, excIs, getMessage, truthy, dup, wr, wrappend, wrelem, apush, aset,
  arrNew, index: indexFn, indexOpt, foreachArr, eq, ident, cmp, add, sub, mul, div, mod, pow, concat, neg, bitand, bitor, bitxor, shl, shr, bitnot, chk,
  echo, necho, eecho, exit, dump: v => B.dump(v), run, setArgs, text, die,
};
