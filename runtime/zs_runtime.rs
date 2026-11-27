#[allow(dead_code, unused_variables, unused_mut, unused_assignments, unused_parens, non_snake_case, non_upper_case_globals, unreachable_code, unused_imports, clippy::all)]
pub mod z {
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::rc::Rc;

pub type S = Rc<[u8]>;
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum K { I(i64), S(S) }
#[derive(Clone)]
pub struct Arr { pub keys: Vec<K>, pub vals: Vec<V>, pub map: HashMap<K, usize>, pub next: i64 }
pub struct ExcD { pub kind: i32, pub msg: V, pub line: Cell<i64> }
pub struct ObjD { pub cls: S, pub f: RefCell<Arr> }
#[derive(Clone)]
pub enum V { Null, Bool(bool), Int(i64), Float(f64), Str(S), Arr(Rc<Arr>), Obj(Rc<ObjD>), Exc(Rc<ExcD>), Fn(Rc<(V, V)>) }
pub enum PK { K(V), Push }
pub use PK::Push;
pub fn k(v: V) -> PK { PK::K(v) }
pub enum Fl { N, Ret(V), Jmp(i32) }
pub struct Pl(pub V);
unsafe impl Send for Pl {}

thread_local! {
    static LINE: Cell<i64> = Cell::new(0);
    static OUT: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static STATICS: RefCell<Arr> = RefCell::new(Arr::new());
    static ARGS: RefCell<Vec<Vec<u8>>> = RefCell::new(Vec::new());
}
pub fn line(n: i64) { LINE.with(|l| l.set(n)); }
pub fn s(x: &str) -> V { V::Str(Rc::from(x.as_bytes())) }
pub fn sb(x: &[u8]) -> V { V::Str(Rc::from(x)) }
pub fn sv(x: Vec<u8>) -> V { V::Str(Rc::from(x)) }
pub fn int(i: i64) -> V { V::Int(i) }

// ───── wyjście ─────
pub fn flush() { OUT.with(|o| { let mut o = o.borrow_mut(); if !o.is_empty() { let so = std::io::stdout(); let mut h = so.lock(); let _ = h.write_all(&o); let _ = h.flush(); o.clear(); } }); }
fn out(b: &[u8]) { OUT.with(|o| { let mut o = o.borrow_mut(); o.extend_from_slice(b); if o.len() > 65536 { drop(o); flush(); } }); }
fn err(b: &[u8]) { flush(); let se = std::io::stderr(); let mut h = se.lock(); let _ = h.write_all(b); }

// ───── wyjątki ─────
pub fn raise(e: V) -> ! {
    match &e {
        V::Exc(x) => { if x.line.get() == 0 { x.line.set(LINE.with(|l| l.get())); } }
        V::Obj(o) => { let mut f = o.f.borrow_mut(); let cur = f.get(&K::S(Rc::from(&b"line"[..]))); if !matches!(cur, Some(V::Int(n)) if *n != 0) { f.put(K::S(Rc::from(&b"line"[..])), V::Int(LINE.with(|l| l.get()))); } }
        _ => {}
    }
    std::panic::resume_unwind(Box::new(Pl(e)))
}
pub fn die(m: &str) -> ! { let e = V::Exc(Rc::new(ExcD { kind: 1, msg: s(m), line: Cell::new(LINE.with(|l| l.get())) })); std::panic::resume_unwind(Box::new(Pl(e))) }
pub fn new_exc(kind: i32, msg: V) -> V { V::Exc(Rc::new(ExcD { kind, msg, line: Cell::new(0) })) }
pub fn throwv(x: &V) -> ! { if !matches!(x, V::Exc(_) | V::Obj(_)) { die("throw wymaga obiektu typu Throwable"); } raise(x.clone()) }
pub fn guard<F: FnOnce() -> Fl>(f: F) -> Result<Fl, V> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => Ok(r),
        Err(p) => match p.downcast::<Pl>() {
            Ok(v) => Err(v.0),
            Err(p) => {
                if p.is::<ExitCode>() { std::panic::resume_unwind(p) }
                let m = if let Some(s) = p.downcast_ref::<&str>() { s.to_string() } else if let Some(s) = p.downcast_ref::<String>() { s.clone() } else { "błąd wewnętrzny".to_string() };
                Err(new_exc(1, s(&m)))
            }
        },
    }
}
pub fn rethrow(e: V) -> ! { std::panic::resume_unwind(Box::new(Pl(e))) }
pub fn exc_is(x: &V, kind: i32) -> bool { matches!(x, V::Exc(e) if kind == 0 || e.kind == kind) }
pub fn get_message(x: &V) -> V { match x { V::Exc(e) => e.msg.clone(), _ => die(&format!("wywołanie metody `getMessage` na wartości typu {}", tn(x))) } }

// ───── typy i tekst ─────
pub fn tn(v: &V) -> String {
    match v { V::Null => "null".into(), V::Bool(_) => "bool".into(), V::Int(_) => "int".into(), V::Float(_) => "float".into(), V::Str(_) => "string".into(),
        V::Arr(_) => "array".into(), V::Obj(o) => String::from_utf8_lossy(&o.cls).into_owned(), V::Fn(_) => "function".into(), V::Exc(_) => "object".into() }
}
fn is_num(v: &V) -> bool { matches!(v, V::Int(_) | V::Float(_)) }
fn as_f(v: &V) -> f64 { match v { V::Int(i) => *i as f64, V::Float(f) => *f, _ => 0.0 } }
pub fn fmt_float(f: f64) -> String {
    if f.is_nan() { return "NaN".into(); }
    if f.is_infinite() { return if f > 0.0 { "Infinity".into() } else { "-Infinity".into() }; }
    if f == f.floor() && f.abs() < 1e15 { return format!("{:.1}", f); }
    format!("{}", f)
}
fn dump_str(b: &[u8], o: &mut Vec<u8>) {
    o.push(b'"');
    for &c in b {
        match c { b'"' => o.extend_from_slice(b"\\\""), b'\\' => o.extend_from_slice(b"\\\\"), b'\n' => o.extend_from_slice(b"\\n"), b'\r' => o.extend_from_slice(b"\\r"),
            b'\t' => o.extend_from_slice(b"\\t"), 0 => o.extend_from_slice(b"\\0"), c if c < 0x20 || c == 0x7f => o.extend_from_slice(format!("\\u{{{:x}}}", c).as_bytes()), c => o.push(c) }
    }
    o.push(b'"');
}
fn key_text(k: &K, o: &mut Vec<u8>) { match k { K::I(i) => o.extend_from_slice(i.to_string().as_bytes()), K::S(s) => dump_str(s, o) } }
fn text_into(v: &V, o: &mut Vec<u8>, dump: bool) {
    match v {
        V::Null => o.extend_from_slice(b"null"), V::Bool(b) => o.extend_from_slice(if *b { b"true" } else { b"false" }),
        V::Int(i) => o.extend_from_slice(i.to_string().as_bytes()), V::Float(f) => o.extend_from_slice(fmt_float(*f).as_bytes()),
        V::Str(s) => if dump { dump_str(s, o) } else { o.extend_from_slice(s) },
        V::Arr(a) => {
            let seq = a.keys.iter().enumerate().all(|(i, k)| *k == K::I(i as i64));
            o.push(b'[');
            for i in 0..a.keys.len() { if i > 0 { o.extend_from_slice(b", "); } if !seq { key_text(&a.keys[i], o); o.extend_from_slice(b" => "); } text_into(&a.vals[i], o, true); }
            o.push(b']');
        }
        V::Obj(x) => { o.extend_from_slice(b"<object "); o.extend_from_slice(&x.cls); o.push(b'>'); }
        V::Fn(_) => o.extend_from_slice(b"<function>"),
        V::Exc(e) => o.extend_from_slice(if e.kind != 0 { b"<object Error>" } else { b"<object Exception>" }),
    }
}
pub fn text(v: &V) -> Vec<u8> { let mut o = Vec::new(); text_into(v, &mut o, false); o }
pub fn tstr(v: &V) -> V { match v { V::Str(_) => v.clone(), _ => sv(text(v)) } }
pub fn truthy(v: &V) -> bool {
    match v { V::Null => false, V::Bool(b) => *b, V::Int(i) => *i != 0, V::Float(f) => *f != 0.0, V::Str(s) => !(s.is_empty() || &**s == b"0"), V::Arr(a) => !a.keys.is_empty(), _ => true }
}

// ───── tablice ─────
impl Arr {
    pub fn new() -> Arr { Arr { keys: Vec::new(), vals: Vec::new(), map: HashMap::new(), next: 0 } }
    pub fn get(&self, k: &K) -> Option<&V> { self.map.get(k).map(|&i| &self.vals[i]) }
    pub fn put(&mut self, k: K, v: V) {
        if let Some(&i) = self.map.get(&k) { self.vals[i] = v; return; }
        if let K::I(n) = &k { if *n >= self.next { self.next = *n + 1; } }
        self.map.insert(k.clone(), self.keys.len()); self.keys.push(k); self.vals.push(v);
    }
}
pub fn arr_new() -> V { V::Arr(Rc::new(Arr::new())) }
fn norm_key(k: &V) -> K {
    match k {
        V::Int(i) => K::I(*i), V::Bool(b) => K::I(*b as i64),
        V::Float(f) => { if f.is_finite() && *f == f.floor() && f.abs() < 9.2e18 { K::I(*f as i64) } else { die("niepoprawny klucz tablicy typu float") } }
        V::Str(s) => {
            let n = s.len();
            if n > 0 && n <= 20 {
                let neg = s[0] == b'-'; let d = if neg { &s[1..] } else { &s[..] };
                if !d.is_empty() && d.iter().all(|c| c.is_ascii_digit()) && (d[0] != b'0' || d.len() == 1) && !(neg && d == b"0") {
                    if let Ok(t) = std::str::from_utf8(s) { if let Ok(i) = t.parse::<i64>() { return K::I(i); } }
                }
            }
            K::S(s.clone())
        }
        _ => die(&format!("niepoprawny klucz tablicy typu {}", tn(k))),
    }
}
fn key_val(k: &K) -> V { match k { K::I(i) => V::Int(*i), K::S(s) => V::Str(s.clone()) } }
pub fn arr_lit(items: Vec<(Option<V>, V)>) -> V {
    let mut a = Arr::new();
    for (k, v) in items { match k { None => { let n = a.next; a.put(K::I(n), v) } Some(k) => a.put(norm_key(&k), v) } }
    V::Arr(Rc::new(a))
}
fn slot<'a>(base: &'a mut V, key: &PK) -> &'a mut V {
    if matches!(base, V::Null) { *base = arr_new(); }
    match base {
        V::Arr(rc) => {
            let a = Rc::make_mut(rc);
            let kk = match key { PK::Push => K::I(a.next), PK::K(v) => norm_key(v) };
            if !a.map.contains_key(&kk) { a.put(kk.clone(), V::Null); }
            let i = a.map[&kk]; &mut a.vals[i]
        }
        other => die(&format!("nie można indeksować wartości typu {} przy zapisie", tn(other))),
    }
}
pub fn set_path(base: &mut V, path: &[PK], val: V) {
    let mut cur = base;
    for p in path { cur = slot(cur, p); }
    *cur = val;
}
pub fn get_path(base: &V, path: &[V]) -> V { let mut cur = base.clone(); for p in path { cur = index(&cur, p); } cur }
pub fn get_path_opt(base: &V, path: &[V]) -> V { let mut cur = base.clone(); for p in path { cur = index_opt(&cur, p); } cur }
fn index_impl(a: &V, k: &V, quiet: bool) -> V {
    match a {
        V::Arr(x) => { let nk = norm_key(k); if let Some(v) = x.get(&nk) { return v.clone(); } if quiet { return V::Null; } let mut t = Vec::new(); key_text(&nk, &mut t); die(&format!("brak klucza {}", String::from_utf8_lossy(&t))) }
        V::Str(s) => {
            let ki = match k { V::Int(i) => *i, _ => die("indeks tekstu musi być liczbą całkowitą") };
            let len = s.len() as i64; let i = if ki < 0 { len + ki } else { ki };
            if i < 0 || i >= len { if quiet { return V::Null; } die(&format!("indeks {} poza zakresem tekstu (długość {})", i, len)); }
            let c = s[i as usize];
            if c & 0xC0 == 0x80 { if quiet { return V::Null; } die(&format!("indeks {} wskazuje środek znaku UTF-8", i)); }
            let mut n = if c < 0x80 { 1 } else if c < 0xE0 { 2 } else if c < 0xF0 { 3 } else { 4 }; if i + n > len { n = len - i; }
            sb(&s[i as usize..(i + n) as usize])
        }
        V::Null => { if quiet { V::Null } else { die("indeksowanie wartości null") } }
        _ => die(&format!("typ {} nie obsługuje indeksowania", tn(a))),
    }
}
pub fn index(a: &V, k: &V) -> V { index_impl(a, k, false) }
pub fn index_opt(a: &V, k: &V) -> V { index_impl(a, k, true) }
pub fn foreach_arr(v: &V) -> Rc<Arr> { match v { V::Arr(a) => a.clone(), _ => die(&format!("foreach wymaga tablicy, jest {}", tn(v))) } }
pub fn fe_key(a: &Rc<Arr>, i: usize) -> V { key_val(&a.keys[i]) }
pub fn fe_val(a: &Rc<Arr>, i: usize) -> V { a.vals[i].clone() }
pub fn fe_len(a: &Rc<Arr>) -> usize { a.keys.len() }

// ───── porównania i operatory ─────
pub fn ident(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Null, V::Null) => true, (V::Bool(x), V::Bool(y)) => x == y, (V::Int(x), V::Int(y)) => x == y, (V::Float(x), V::Float(y)) => x == y,
        (V::Str(x), V::Str(y)) => x == y,
        (V::Arr(x), V::Arr(y)) => Rc::ptr_eq(x, y) || (x.keys.len() == y.keys.len() && x.keys.iter().zip(y.keys.iter()).all(|(p, q)| p == q) && x.vals.iter().zip(y.vals.iter()).all(|(p, q)| ident(p, q))),
        (V::Obj(x), V::Obj(y)) => Rc::ptr_eq(x, y), (V::Exc(x), V::Exc(y)) => Rc::ptr_eq(x, y), (V::Fn(x), V::Fn(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}
pub fn eq(a: &V, b: &V) -> bool { if is_num(a) && is_num(b) && std::mem::discriminant(a) != std::mem::discriminant(b) { return as_f(a) == as_f(b); } ident(a, b) }
pub fn cmp(a: &V, b: &V) -> i32 {
    match (a, b) {
        (V::Int(x), V::Int(y)) => (x > y) as i32 - (x < y) as i32,
        (V::Str(x), V::Str(y)) => match x.cmp(y) { std::cmp::Ordering::Less => -1, std::cmp::Ordering::Equal => 0, _ => 1 },
        (V::Bool(x), V::Bool(y)) => (*x as i32) - (*y as i32),
        _ if is_num(a) && is_num(b) => { let (x, y) = (as_f(a), as_f(b)); if x < y { -1 } else if x > y { 1 } else { 0 } }
        _ => die(&format!("nie można porównać {} z {}", tn(a), tn(b))),
    }
}
pub fn lt(a: &V, b: &V) -> bool { cmp(a, b) < 0 } pub fn gt(a: &V, b: &V) -> bool { cmp(a, b) > 0 }
pub fn le(a: &V, b: &V) -> bool { cmp(a, b) <= 0 } pub fn ge(a: &V, b: &V) -> bool { cmp(a, b) >= 0 }
fn bad(op: &str, a: &V, b: &V) -> ! { die(&format!("operacja {} niedostępna dla {} i {}", op, tn(a), tn(b))) }
const OVF: &str = "przepełnienie liczby całkowitej";
pub fn add(a: &V, b: &V) -> V {
    match (a, b) {
        (V::Int(x), V::Int(y)) => V::Int(x.checked_add(*y).unwrap_or_else(|| die(OVF))),
        _ if is_num(a) && is_num(b) => V::Float(as_f(a) + as_f(b)),
        (V::Arr(x), V::Arr(y)) => { let mut r = (**x).clone(); for i in 0..y.keys.len() { if !r.map.contains_key(&y.keys[i]) { r.put(y.keys[i].clone(), y.vals[i].clone()); } } V::Arr(Rc::new(r)) }
        _ => bad("Add", a, b),
    }
}
pub fn sub(a: &V, b: &V) -> V { match (a, b) { (V::Int(x), V::Int(y)) => V::Int(x.checked_sub(*y).unwrap_or_else(|| die(OVF))), _ if is_num(a) && is_num(b) => V::Float(as_f(a) - as_f(b)), _ => bad("Sub", a, b) } }
pub fn mul(a: &V, b: &V) -> V { match (a, b) { (V::Int(x), V::Int(y)) => V::Int(x.checked_mul(*y).unwrap_or_else(|| die(OVF))), _ if is_num(a) && is_num(b) => V::Float(as_f(a) * as_f(b)), _ => bad("Mul", a, b) } }
pub fn div(a: &V, b: &V) -> V {
    if !(is_num(a) && is_num(b)) { bad("Div", a, b); }
    if as_f(b) == 0.0 { die("dzielenie przez zero"); }
    if let (V::Int(x), V::Int(y)) = (a, b) { if *y == -1 { return V::Int(x.checked_neg().unwrap_or_else(|| die(OVF))); } if x % y == 0 { return V::Int(x / y); } }
    V::Float(as_f(a) / as_f(b))
}
pub fn modulo(a: &V, b: &V) -> V { match (a, b) { (V::Int(x), V::Int(y)) => { if *y == 0 { die("modulo przez zero"); } if *y == -1 { V::Int(0) } else { V::Int(x % y) } } _ => bad("Mod", a, b) } }
pub fn bit_and(a: &V, b: &V) -> V { match (a, b) { (V::Int(x), V::Int(y)) => V::Int(x & y), _ => bad("BitAnd", a, b) } }
pub fn bit_or(a: &V, b: &V) -> V { match (a, b) { (V::Int(x), V::Int(y)) => V::Int(x | y), _ => bad("BitOr", a, b) } }
pub fn bit_xor(a: &V, b: &V) -> V { match (a, b) { (V::Int(x), V::Int(y)) => V::Int(x ^ y), _ => bad("BitXor", a, b) } }
pub fn shl(a: &V, b: &V) -> V { match (a, b) { (V::Int(x), V::Int(y)) => V::Int(((*x as u64) << (*y & 63)) as i64), _ => bad("Shl", a, b) } }
pub fn shr(a: &V, b: &V) -> V { match (a, b) { (V::Int(x), V::Int(y)) => V::Int(x >> (*y & 63)), _ => bad("Shr", a, b) } }
pub fn bit_not(a: &V) -> V { match a { V::Int(x) => V::Int(!x), _ => die(&format!("operator jednoargumentowy BitNot niedostępny dla {}", tn(a))) } }
pub fn pow(a: &V, b: &V) -> V {
    if let (V::Int(x), V::Int(y)) = (a, b) {
        if *y >= 0 {
            let (mut r, mut base, mut e, mut ovf) = (1i64, *x, *y, false);
            while e > 0 {
                if e & 1 == 1 { match r.checked_mul(base) { Some(v) => r = v, None => { ovf = true; break; } } }
                e >>= 1; if e > 0 { match base.checked_mul(base) { Some(v) => base = v, None => { ovf = true; break; } } }
            }
            if !ovf { return V::Int(r); }
            return V::Float((*x as f64).powf(*y as f64));
        }
    }
    if is_num(a) && is_num(b) { return V::Float(as_f(a).powf(as_f(b))); }
    bad("Pow", a, b)
}
pub fn concat(a: &V, b: &V) -> V { let mut o = text(a); text_into(b, &mut o, false); sv(o) }
pub fn neg(a: &V) -> V { match a { V::Int(x) => V::Int(x.checked_neg().unwrap_or_else(|| die(OVF))), V::Float(f) => V::Float(-f), _ => die(&format!("operator jednoargumentowy Neg niedostępny dla {}", tn(a))) } }
pub fn chk(v: V, ty: &str, what: &str) -> V {
    let ok = match ty { "int" => matches!(v, V::Int(_)), "float" => { if let V::Int(i) = v { return V::Float(i as f64); } matches!(v, V::Float(_)) }
        "string" => matches!(v, V::Str(_)), "bool" => matches!(v, V::Bool(_)), "array" => matches!(v, V::Arr(_)), _ => matches!(v, V::Obj(_) | V::Exc(_)) };
    if ok { v } else { die(&format!("{}: oczekiwano {}, otrzymano {}", what, ty, tn(&v))) }
}

// ───── wbudowane ─────
fn ns<'a>(v: &'a V, f: &str, n: i32) -> &'a S { match v { V::Str(s) => s, _ => die(&format!("{}: argument #{} powinien być tekstem, jest {}", f, n, tn(v))) } }
fn ni(v: &V, f: &str, n: i32) -> i64 { match v { V::Int(i) => *i, _ => die(&format!("{}: argument #{} powinien być liczbą całkowitą, jest {}", f, n, tn(v))) } }
fn nf(v: &V, f: &str, n: i32) -> f64 { if is_num(v) { as_f(v) } else { die(&format!("{}: argument #{} powinien być liczbą, jest {}", f, n, tn(v))) } }
fn na<'a>(v: &'a V, f: &str, n: i32) -> &'a Rc<Arr> { match v { V::Arr(a) => a, _ => die(&format!("{}: argument #{} powinien być tablicą, jest {}", f, n, tn(v))) } }
fn arg(a: &[V], i: usize) -> V { a.get(i).cloned().unwrap_or(V::Null) }
fn is_ws(c: u8) -> bool { c == b' ' || (9..=13).contains(&c) }
fn trim_b(s: &[u8]) -> &[u8] { let mut a = 0; let mut b = s.len(); while a < b && is_ws(s[a]) { a += 1; } while b > a && is_ws(s[b - 1]) { b -= 1; } &s[a..b] }
fn find(h: &[u8], n: &[u8], from: usize) -> Option<usize> { if n.is_empty() { return Some(from); } if n.len() > h.len() { return None; } (from..=h.len() - n.len()).find(|&i| &h[i..i + n.len()] == n) }
fn u8len(c: u8) -> usize { if c < 0x80 { 1 } else if c < 0xE0 { 2 } else if c < 0xF0 { 3 } else { 4 } }
fn lossy(b: &[u8]) -> V { sv(String::from_utf8_lossy(b).into_owned().into_bytes()) }
pub fn b_count(a: &[V]) -> V { match &a[0] { V::Arr(x) => V::Int(x.keys.len() as i64), V::Str(s) => V::Int(s.len() as i64), o => die(&format!("count: niedostępne dla {}", tn(o))) } }
pub fn b_strlen(a: &[V]) -> V { V::Int(ns(&a[0], "strlen", 1).len() as i64) }
pub fn b_typeof(a: &[V]) -> V { match &a[0] { V::Obj(o) => V::Str(o.cls.clone()), V::Exc(e) => s(if e.kind != 0 { "Error" } else { "Exception" }), o => s(&tn(o)) } }
pub fn b_is_int(a: &[V]) -> V { V::Bool(matches!(a[0], V::Int(_))) } pub fn b_is_float(a: &[V]) -> V { V::Bool(matches!(a[0], V::Float(_))) }
pub fn b_is_string(a: &[V]) -> V { V::Bool(matches!(a[0], V::Str(_))) } pub fn b_is_bool(a: &[V]) -> V { V::Bool(matches!(a[0], V::Bool(_))) }
pub fn b_is_array(a: &[V]) -> V { V::Bool(matches!(a[0], V::Arr(_))) } pub fn b_is_null(a: &[V]) -> V { V::Bool(matches!(a[0], V::Null)) }
pub fn b_strval(a: &[V]) -> V { tstr(&a[0]) } pub fn b_boolval(a: &[V]) -> V { V::Bool(truthy(&a[0])) }
pub fn b_intval(a: &[V]) -> V {
    match &a[0] {
        V::Int(_) => a[0].clone(), V::Float(f) => V::Int(if f.is_nan() { 0 } else { *f as i64 }), V::Bool(b) => V::Int(*b as i64), V::Null => V::Int(0),
        V::Str(st) => {
            let t = trim_b(st); let ts = std::str::from_utf8(t).unwrap_or("");
            if !t.is_empty() { if let Ok(i) = ts.parse::<i64>() { return V::Int(i); } if let Ok(f) = ts.parse::<f64>() { return V::Int(if f.is_nan() { 0 } else { f as i64 }); } }
            V::Int(0)
        }
        o => die(&format!("intval: nie można przekonwertować {}", tn(o))),
    }
}
pub fn b_floatval(a: &[V]) -> V { match &a[0] { V::Int(i) => V::Float(*i as f64), V::Float(_) => a[0].clone(), V::Str(st) => { let t = trim_b(st); V::Float(std::str::from_utf8(t).ok().and_then(|x| if x.is_empty() { None } else { x.parse::<f64>().ok() }).unwrap_or(0.0)) } _ => V::Float(0.0) } }
pub fn b_ord(a: &[V]) -> V { let st = ns(&a[0], "ord", 1); if st.is_empty() { die("ord: pusty tekst"); } V::Int(st[0] as i64) }
pub fn b_chr(a: &[V]) -> V { let n = ni(&a[0], "chr", 1); match u32::try_from(n).ok().and_then(char::from_u32) { Some(c) => s(&c.to_string()), None => s("") } }
pub fn b_trim(a: &[V]) -> V { sb(trim_b(ns(&a[0], "trim", 1))) }
pub fn b_contains(a: &[V]) -> V { V::Bool(find(ns(&a[0], "contains", 1), ns(&a[1], "contains", 2), 0).is_some()) }
pub fn b_starts_with(a: &[V]) -> V { V::Bool(ns(&a[0], "starts_with", 1).starts_with(ns(&a[1], "starts_with", 2))) }
pub fn b_ends_with(a: &[V]) -> V { V::Bool(ns(&a[0], "ends_with", 1).ends_with(ns(&a[1], "ends_with", 2))) }
pub fn b_index_of(a: &[V]) -> V { V::Int(find(ns(&a[0], "index_of", 1), ns(&a[1], "index_of", 2), 0).map(|i| i as i64).unwrap_or(-1)) }
pub fn b_repeat(a: &[V]) -> V { let x = ns(&a[0], "repeat", 1); let n = ni(&a[1], "repeat", 2).max(0) as usize; sv(x.repeat(n)) }
pub fn b_substr(a: &[V]) -> V {
    let x = ns(&a[0], "substr", 1); let len = x.len() as i64; let mut st = ni(&a[1], "substr", 2);
    if st < 0 { st = (len + st).max(0); } if st > len { st = len; }
    let mut n = match a.get(2) { Some(V::Int(l)) => if *l >= 0 { *l } else { (len - st + *l).max(0) }, _ => len - st };
    if st + n > len { n = len - st; }
    lossy(&x[st as usize..(st + n) as usize])
}
pub fn b_join(a: &[V]) -> V {
    let x = na(&a[0], "join", 1); let sep: &[u8] = match &a[1] { V::Str(s) => s, _ => b"" }; let mut o = Vec::new();
    for (i, v) in x.vals.iter().enumerate() { if i > 0 { o.extend_from_slice(sep); } text_into(v, &mut o, false); }
    sv(o)
}
pub fn b_in_array(a: &[V]) -> V { let x = na(&a[1], "in_array", 2); V::Bool(x.vals.iter().any(|v| ident(v, &a[0]))) }
pub fn b_range(a: &[V]) -> V {
    let (x, y) = (ni(&a[0], "range", 1), ni(&a[1], "range", 2)); let mut r = Arr::new();
    if x <= y { let mut i = x; loop { let n = r.next; r.put(K::I(n), V::Int(i)); if i == y { break; } i += 1; } } else { let mut i = x; loop { let n = r.next; r.put(K::I(n), V::Int(i)); if i == y { break; } i -= 1; } }
    V::Arr(Rc::new(r))
}
fn from_vec(v: Vec<V>) -> V { let mut r = Arr::new(); for x in v { let n = r.next; r.put(K::I(n), x); } V::Arr(Rc::new(r)) }
pub fn b_keys(a: &[V]) -> V { let x = na(&a[0], "keys", 1); from_vec(x.keys.iter().map(key_val).collect()) }
pub fn b_values(a: &[V]) -> V { let x = na(&a[0], "values", 1); from_vec(x.vals.clone()) }
pub fn b_has_key(a: &[V]) -> V { let x = na(&a[0], "has_key", 1); V::Bool(x.map.contains_key(&norm_key(&a[1]))) }
pub fn b_upper(a: &[V]) -> V { sv(ns(&a[0], "upper", 1).to_ascii_uppercase()) }
pub fn b_lower(a: &[V]) -> V { sv(ns(&a[0], "lower", 1).to_ascii_lowercase()) }
pub fn b_replace(a: &[V]) -> V {
    let (st, x, y) = (ns(&a[0], "replace", 1), ns(&a[1], "replace", 2), ns(&a[2], "replace", 3));
    if x.is_empty() { return a[0].clone(); }
    let mut o = Vec::new(); let mut i = 0;
    while let Some(p) = find(st, x, i) { o.extend_from_slice(&st[i..p]); o.extend_from_slice(y); i = p + x.len(); }
    o.extend_from_slice(&st[i..]); sv(o)
}
fn chars_of(st: &[u8]) -> Vec<V> { let mut r = Vec::new(); let mut i = 0; while i < st.len() { let mut n = u8len(st[i]); if i + n > st.len() { n = st.len() - i; } r.push(sb(&st[i..i + n])); i += n; } r }
pub fn b_split(a: &[V]) -> V {
    let (st, x) = (ns(&a[0], "split", 1), ns(&a[1], "split", 2));
    if x.is_empty() { return from_vec(chars_of(st)); }
    let mut r = Vec::new(); let mut i = 0;
    loop { match find(st, x, i) { Some(p) => { r.push(sb(&st[i..p])); i = p + x.len(); } None => { r.push(sb(&st[i..])); break; } } }
    from_vec(r)
}
pub fn b_chars(a: &[V]) -> V { from_vec(chars_of(ns(&a[0], "chars", 1))) }
fn pad(a: &[V], left: bool, f: &str) -> V {
    let st = ns(&a[0], f, 1); let w = ni(&a[1], f, 2).max(0) as usize;
    let pc: Vec<u8> = match a.get(2) { Some(V::Str(p)) if !p.is_empty() => { let n = u8len(p[0]).min(p.len()); p[..n].to_vec() } _ => vec![b' '] };
    let l = st.iter().filter(|c| **c & 0xC0 != 0x80).count(); if l >= w { return a[0].clone(); }
    let padding = pc.repeat(w - l); let mut o = Vec::new();
    if left { o.extend_from_slice(&padding); o.extend_from_slice(st); } else { o.extend_from_slice(st); o.extend_from_slice(&padding); }
    sv(o)
}
pub fn b_pad_left(a: &[V]) -> V { pad(a, true, "pad_left") } pub fn b_pad_right(a: &[V]) -> V { pad(a, false, "pad_right") }
pub fn b_array_slice(a: &[V]) -> V {
    let x = na(&a[0], "array_slice", 1); let len = x.keys.len() as i64; let mut st = ni(&a[1], "array_slice", 2);
    if st < 0 { st = (len + st).max(0); } if st > len { st = len; }
    let n = match a.get(2) { Some(V::Int(l)) => (*l).max(0), _ => len }; let end = (st + n).min(len);
    from_vec(x.vals[st as usize..end as usize].to_vec())
}
pub fn b_reverse(a: &[V]) -> V {
    match &a[0] {
        V::Str(st) => { let mut c = chars_of(st); c.reverse(); let mut o = Vec::new(); for x in c { text_into(&x, &mut o, false); } sv(o) }
        V::Arr(x) => { let mut v = x.vals.clone(); v.reverse(); from_vec(v) }
        o => die(&format!("reverse: niedostępne dla {}", tn(o))),
    }
}
pub fn b_sort(a: &[V]) -> V {
    let x = na(&a[0], "sort", 1); let mut v = x.vals.clone();
    // scalanie stabilne; błąd porównania propaguje się jako wyjątek
    fn ms(v: Vec<V>) -> Vec<V> { if v.len() < 2 { return v; } let m = v.len() / 2; let r = ms(v[m..].to_vec()); let l = ms(v[..m].to_vec()); let (mut i, mut j) = (0, 0); let mut o = Vec::with_capacity(l.len() + r.len()); while i < l.len() && j < r.len() { if cmp(&r[j], &l[i]) < 0 { o.push(r[j].clone()); j += 1; } else { o.push(l[i].clone()); i += 1; } } o.extend_from_slice(&l[i..]); o.extend_from_slice(&r[j..]); o }
    from_vec(ms(std::mem::take(&mut v)))
}
pub fn b_push(a: &[V]) -> V { na(&a[0], "push", 1); let mut r = a[0].clone(); set_path(&mut r, &[Push], a[1].clone()); r }
pub fn b_dump(a: &[V]) -> V { let mut o = Vec::new(); text_into(&a[0], &mut o, true); o.push(b'\n'); out(&o); V::Null }
fn first(a: &[V], f: &str) -> Option<u8> { ns(&a[0], f, 1).first().copied() }
pub fn b_is_digit(a: &[V]) -> V { V::Bool(first(a, "is_digit").map_or(false, |c| c.is_ascii_digit())) }
pub fn b_is_alpha(a: &[V]) -> V { V::Bool(first(a, "is_alpha").map_or(false, |c| c.is_ascii_alphabetic() || c == b'_' || c >= 0x80)) }
pub fn b_is_alnum(a: &[V]) -> V { V::Bool(first(a, "is_alnum").map_or(false, |c| c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80)) }
pub fn b_is_space(a: &[V]) -> V { V::Bool(first(a, "is_space").map_or(false, is_ws)) }
pub fn b_is_hex_digit(a: &[V]) -> V { V::Bool(first(a, "is_hex_digit").map_or(false, |c| c.is_ascii_hexdigit())) }
pub fn b_to_hex(a: &[V]) -> V { s(&format!("{:x}", ni(&a[0], "to_hex", 1) as u64)) }
pub fn b_read_line(a: &[V]) -> V {
    flush(); let mut o = Vec::new(); let mut any = false; let si = std::io::stdin(); let mut h = si.lock(); let mut b = [0u8; 1];
    loop { match h.read(&mut b) { Ok(1) => { any = true; if b[0] == b'\n' { break; } o.push(b[0]); } _ => break } }
    if !any { return V::Null; } while o.last() == Some(&b'\r') { o.pop(); } sv(o)
}
pub fn b_zs_obj_new(a: &[V]) -> V { V::Obj(Rc::new(ObjD { cls: ns(&a[0], "zs_obj_new", 1).clone(), f: RefCell::new(Arr::new()) })) }
pub fn b_zs_obj_class(a: &[V]) -> V { match &a[0] { V::Obj(o) => V::Str(o.cls.clone()), _ => V::Null } }
pub fn b_zs_obj_get(a: &[V]) -> V {
    let name = String::from_utf8_lossy(ns(&a[1], "zs_obj_get", 2)).into_owned();
    match &a[0] {
        V::Obj(o) => match o.f.borrow().get(&norm_key(&a[1])) { Some(v) => v.clone(), None => die(&format!("obiekt klasy {} nie ma właściwości {}", String::from_utf8_lossy(&o.cls), name)) },
        V::Null => die(&format!("dostęp do właściwości `{}` wartości null", name)),
        o => die(&format!("dostęp do właściwości `{}` typu {}", name, tn(o))),
    }
}
pub fn b_zs_obj_take(a: &[V]) -> V { let v = b_zs_obj_get(a); if let V::Obj(o) = &a[0] { o.f.borrow_mut().put(norm_key(&a[1]), V::Null); } v }
pub fn b_zs_obj_set(a: &[V]) -> V { match &a[0] { V::Obj(o) => { o.f.borrow_mut().put(norm_key(&a[1]), a[2].clone()); a[2].clone() } o => die(&format!("zapis właściwości `{}` na wartości typu {}", String::from_utf8_lossy(ns(&a[1], "zs_obj_set", 2)), tn(o))) } }
pub fn b_zs_obj_put(a: &[V]) -> V { b_zs_obj_set(a) }
pub fn b_zs_no_method(a: &[V]) -> V { let n = String::from_utf8_lossy(ns(&a[1], "zs_no_method", 2)).into_owned(); match &a[0] { V::Obj(o) => die(&format!("klasa {} nie ma metody {}", String::from_utf8_lossy(&o.cls), n)), o => die(&format!("wywołanie metody `{}` na wartości typu {}", n, tn(o))) } }
pub fn b_zs_no_class(a: &[V]) -> V { die(&format!("nieznana klasa {}", String::from_utf8_lossy(ns(&a[0], "zs_no_class", 1)))) }
pub fn b_zs_mkfn(a: &[V]) -> V { V::Fn(Rc::new((a[0].clone(), a[1].clone()))) }
pub fn b_zs_fn_id(a: &[V]) -> V { match &a[0] { V::Fn(f) => f.0.clone(), V::Str(_) => a[0].clone(), _ => V::Null } }
pub fn b_zs_fn_cap(a: &[V]) -> V { match &a[0] { V::Fn(f) => f.1.clone(), _ => V::Null } }
pub fn b_is_callable(a: &[V]) -> V { V::Bool(matches!(a[0], V::Fn(_))) }
pub fn b_zs_sget(a: &[V]) -> V { STATICS.with(|s| s.borrow().get(&norm_key(&a[0])).cloned().unwrap_or(V::Null)) }
pub fn b_zs_sset(a: &[V]) -> V { STATICS.with(|s| s.borrow_mut().put(norm_key(&a[0]), a[1].clone())); a[1].clone() }

// ───── moduły ─────
fn io_die(p: &[u8], e: &std::io::Error) -> ! {
    let m = match e.kind() { std::io::ErrorKind::NotFound => "No such file or directory".to_string(), std::io::ErrorKind::PermissionDenied => "Permission denied".to_string(), std::io::ErrorKind::AlreadyExists => "File exists".to_string(), _ => { let t = e.to_string(); t.split(" (os error").next().unwrap_or("").to_string() } };
    die(&format!("{}: {}", String::from_utf8_lossy(p), m))
}
fn path(v: &V, f: &str) -> std::path::PathBuf { use std::os::unix::ffi::OsStrExt; std::path::PathBuf::from(std::ffi::OsStr::from_bytes(ns(v, f, 1))) }
fn assoc(p: Vec<(&str, V)>) -> V { let mut a = Arr::new(); for (k, v) in p { a.put(K::S(Rc::from(k.as_bytes())), v); } V::Arr(Rc::new(a)) }
pub fn m_Math_sqrt(a: &[V]) -> V { V::Float(nf(&a[0], "Math.sqrt", 1).sqrt()) }
pub fn m_Math_floor(a: &[V]) -> V { V::Int(nf(&a[0], "Math.floor", 1).floor() as i64) }
pub fn m_Math_ceil(a: &[V]) -> V { V::Int(nf(&a[0], "Math.ceil", 1).ceil() as i64) }
pub fn m_Math_round(a: &[V]) -> V { V::Int(nf(&a[0], "Math.round", 1).round() as i64) }
pub fn m_Math_abs(a: &[V]) -> V { match &a[0] { V::Int(i) => V::Int(i.checked_abs().unwrap_or_else(|| die(OVF))), V::Float(f) => V::Float(f.abs()), o => die(&format!("Math.abs: {}", tn(o))) } }
pub fn m_Math_pow(a: &[V]) -> V { pow(&a[0], &a[1]) }
pub fn m_Math_min(a: &[V]) -> V { if cmp(&a[0], &a[1]) <= 0 { a[0].clone() } else { a[1].clone() } }
pub fn m_Math_max(a: &[V]) -> V { if cmp(&a[0], &a[1]) <= 0 { a[1].clone() } else { a[0].clone() } }
pub fn m_System_args(a: &[V]) -> V { ARGS.with(|x| from_vec(x.borrow().iter().map(|b| sb(b)).collect())) }
pub fn m_System_env(a: &[V]) -> V { match std::env::var_os(String::from_utf8_lossy(ns(&a[0], "System.env", 1)).as_ref()) { Some(v) => { use std::os::unix::ffi::OsStrExt; sb(v.as_bytes()) } None => V::Null } }
pub fn m_System_os(a: &[V]) -> V { s(std::env::consts::OS) }
pub fn m_System_architecture(a: &[V]) -> V { s(match std::env::consts::ARCH { "x86_64" => "x86_64", "aarch64" => "aarch64", _ => "unknown" }) }
pub fn m_System_time(a: &[V]) -> V { V::Float(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)) }
pub fn m_System_cpuCount(a: &[V]) -> V { V::Int(std::thread::available_parallelism().map(|n| n.get() as i64).unwrap_or(1)) }
pub fn m_System_version(a: &[V]) -> V { s("0.0.1") }
pub fn m_System_dirname(a: &[V]) -> V {
    let st = ns(&a[0], "System.dirname", 1); let mut i = st.len();
    while i > 1 && st[i - 1] == b'/' { i -= 1; } while i > 0 && st[i - 1] != b'/' { i -= 1; } while i > 1 && st[i - 1] == b'/' { i -= 1; }
    if i == 0 { s(".") } else if i == 1 && st[0] == b'/' { s("/") } else { sb(&st[..i]) }
}
fn is_file_p(p: &std::path::Path) -> bool { p.is_file() }
fn with_ext(base: &std::path::Path) -> Option<std::path::PathBuf> { for e in [".zsharp", ".zs"] { let mut s = base.as_os_str().to_owned(); s.push(e); let p = std::path::PathBuf::from(s); if p.is_file() { return Some(p); } } None }
fn manifest_member(toml: &std::path::Path, name: &str) -> Option<String> {
    let t = std::fs::read_to_string(toml).ok()?;
    for ln in t.lines() { let l = ln.trim_start(); if let Some(r) = l.strip_prefix(name) { let r = r.trim_start(); if let Some(r) = r.strip_prefix('=') { let r = r.trim_start(); if let Some(r) = r.strip_prefix('"') { if let Some(e) = r.find('"') { return Some(r[..e].to_string()); } } } } }
    None
}
pub fn m_System_resolveImport(a: &[V]) -> V {
    let segs = na(&a[0], "System.resolveImport", 1); let dir = String::from_utf8_lossy(ns(&a[1], "System.resolveImport", 2)).into_owned();
    let names: Vec<String> = segs.vals.iter().map(|v| String::from_utf8_lossy(&text(v)).into_owned()).collect(); if names.is_empty() { return V::Null; }
    let ps = |p: &std::path::Path| -> V { s(&p.to_string_lossy()) };
    if names[0] == "Workspace" && names.len() >= 2 {
        let mut cur = dir.clone();
        for up in 0..40 {
            for sub in ["", "/Z-Sharp"] {
                if let Some(mem) = manifest_member(std::path::Path::new(&format!("{}{}/Z#.toml", cur, sub)), &names[1]) {
                    let mut base = std::path::PathBuf::from(format!("{}{}/{}", cur, sub, mem));
                    if base.is_file() {
                        for k in 2..names.len() { let d = base.parent().map(|p| p.to_path_buf()).unwrap_or_default(); match with_ext(&d.join(&names[k])) { Some(p) => base = p, None => return V::Null } }
                        return ps(&base);
                    }
                }
            }
            match cur.rfind('/') { Some(0) | None => { if cur != "/" && cur != "." { cur = ".".into(); } else { break; } if up > 0 && cur == "." { break; } } Some(i) => cur.truncate(i) }
            if cur == "." && up > 0 { cur = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(); }
        }
        return V::Null;
    }
    if names[0] == "Std" && names.len() == 2 {
        let lo = names[1].to_lowercase();
        if let Ok(e) = std::env::var("ZS_STD") { let p = std::path::PathBuf::from(format!("{}/{}.zsharp", e, lo)); if p.is_file() { return ps(&p); } }
        if let Ok(e) = std::env::var("ZS_HOME") { let p = std::path::PathBuf::from(format!("{}/stdlib/{}.zsharp", e, lo)); if p.is_file() { return ps(&p); } }
        return V::Null;
    }
    let mut dirs = vec![dir.clone(), std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()];
    if let Ok(e) = std::env::var("ZS_PATH") { for t in e.split(':') { if !t.is_empty() { dirs.push(t.to_string()); } } }
    for d in dirs { if let Some(p) = with_ext(&std::path::PathBuf::from(format!("{}/{}", d, names.join("/")))) { return ps(&p); } }
    V::Null
}
pub fn m_System_setEnv(a: &[V]) -> V { std::env::set_var(String::from_utf8_lossy(ns(&a[0], "System.setEnv", 1)).as_ref(), String::from_utf8_lossy(&text(&a[1])).as_ref()); V::Null }
pub fn m_System_unsetEnv(a: &[V]) -> V { std::env::remove_var(String::from_utf8_lossy(ns(&a[0], "System.unsetEnv", 1)).as_ref()); V::Null }
pub fn m_System_environ(a: &[V]) -> V { let mut v: Vec<(String, String)> = std::env::vars().collect(); v.sort(); let mut r = Arr::new(); for (k, x) in v { r.put(K::S(Rc::from(k.as_bytes())), s(&x)); } V::Arr(Rc::new(r)) }
pub fn m_System_cwd(a: &[V]) -> V { s(&std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()) }
pub fn m_System_chdir(a: &[V]) -> V { if let Err(e) = std::env::set_current_dir(path(&a[0], "System.chdir")) { io_die(ns(&a[0], "", 1), &e); } V::Null }
pub fn m_System_pid(a: &[V]) -> V { V::Int(std::process::id() as i64) }
pub fn m_System_hostname(a: &[V]) -> V { s(std::fs::read_to_string("/etc/hostname").unwrap_or_default().trim()) }
pub fn m_System_bytes(a: &[V]) -> V { from_vec(ns(&a[0], "System.bytes", 1).iter().map(|b| V::Int(*b as i64)).collect()) }
pub fn m_System_fromBytes(a: &[V]) -> V { let x = na(&a[0], "System.fromBytes", 1); lossy(&x.vals.iter().map(|v| if let V::Int(i) = v { *i as u8 } else { 0 }).collect::<Vec<u8>>()) }
pub fn m_System_nowNanos(a: &[V]) -> V { V::Int(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(0)) }
thread_local! { static T0: std::time::Instant = std::time::Instant::now(); }
pub fn m_System_monotonic(a: &[V]) -> V { V::Int(T0.with(|t| t.elapsed().as_nanos() as i64) + 1_000_000_000) }
pub fn m_System_sleep(a: &[V]) -> V { flush(); let m = ni(&a[0], "System.sleep", 1); if m > 0 { std::thread::sleep(std::time::Duration::from_millis(m as u64)); } V::Null }
pub fn m_System_localOffset(a: &[V]) -> V { V::Int(0) }
pub fn m_System_gc(a: &[V]) -> V { V::Null }
pub fn m_System_gcStats(a: &[V]) -> V { arr_new() }
pub fn m_File_read(a: &[V]) -> V { match std::fs::read(path(&a[0], "File.read")) { Ok(b) => sv(b), Err(e) => io_die(ns(&a[0], "", 1), &e) } }
pub fn m_File_write(a: &[V]) -> V { if let Err(e) = std::fs::write(path(&a[0], "File.write"), text(&a[1])) { io_die(ns(&a[0], "", 1), &e); } V::Null }
pub fn m_File_append(a: &[V]) -> V { let r = std::fs::OpenOptions::new().append(true).create(true).open(path(&a[0], "File.append")).and_then(|mut f| f.write_all(&text(&a[1]))); if let Err(e) = r { io_die(ns(&a[0], "", 1), &e); } V::Null }
pub fn m_File_exists(a: &[V]) -> V { V::Bool(path(&a[0], "File.exists").is_file()) }
pub fn m_File_delete(a: &[V]) -> V { if let Err(e) = std::fs::remove_file(path(&a[0], "File.delete")) { io_die(ns(&a[0], "", 1), &e); } V::Null }
pub fn m_File_stat(a: &[V]) -> V {
    use std::os::unix::fs::MetadataExt;
    match std::fs::symlink_metadata(path(&a[0], "File.stat")) { Ok(m) => { let t = m.file_type(); assoc(vec![("kind", s(if t.is_symlink() { "link" } else if t.is_dir() { "dir" } else if t.is_file() { "file" } else { "other" })), ("size", V::Int(m.len() as i64)), ("mtime", V::Int(m.mtime())), ("mode", V::Int((m.mode() & 0o7777) as i64))]) } Err(_) => V::Null }
}
pub fn m_File_rename(a: &[V]) -> V { if let Err(e) = std::fs::rename(path(&a[0], "File.rename"), path(&a[1], "File.rename")) { io_die(ns(&a[0], "", 1), &e); } V::Null }
pub fn m_File_realpath(a: &[V]) -> V { match std::fs::canonicalize(path(&a[0], "File.realpath")) { Ok(p) => s(&p.to_string_lossy()), Err(e) => io_die(ns(&a[0], "", 1), &e) } }
pub fn m_File_chmod(a: &[V]) -> V { use std::os::unix::fs::PermissionsExt; if let Err(e) = std::fs::set_permissions(path(&a[0], "File.chmod"), std::fs::Permissions::from_mode(ni(&a[1], "File.chmod", 2) as u32)) { io_die(ns(&a[0], "", 1), &e); } V::Null }
pub fn m_File_symlink(a: &[V]) -> V { if let Err(e) = std::os::unix::fs::symlink(path(&a[0], "File.symlink"), path(&a[1], "File.symlink")) { io_die(ns(&a[1], "", 1), &e); } V::Null }
pub fn m_File_readlink(a: &[V]) -> V { match std::fs::read_link(path(&a[0], "File.readlink")) { Ok(p) => s(&p.to_string_lossy()), Err(e) => io_die(ns(&a[0], "", 1), &e) } }
pub fn m_Directory_create(a: &[V]) -> V { if let Err(e) = std::fs::create_dir_all(path(&a[0], "Directory.create")) { io_die(ns(&a[0], "", 1), &e); } V::Null }
pub fn m_Directory_exists(a: &[V]) -> V { V::Bool(path(&a[0], "Directory.exists").is_dir()) }
pub fn m_Directory_list(a: &[V]) -> V {
    use std::os::unix::ffi::OsStrExt;
    match std::fs::read_dir(path(&a[0], "Directory.list")) { Ok(rd) => { let mut v: Vec<Vec<u8>> = rd.filter_map(|e| e.ok()).map(|e| e.file_name().as_bytes().to_vec()).collect(); v.sort(); from_vec(v.into_iter().map(sv).collect()) } Err(e) => io_die(ns(&a[0], "", 1), &e) }
}
pub fn m_Directory_remove(a: &[V]) -> V { if let Err(e) = std::fs::remove_dir(path(&a[0], "Directory.remove")) { io_die(ns(&a[0], "", 1), &e); } V::Null }
fn cmd_of(a: &[V], f: &str) -> std::process::Command {
    use std::os::unix::ffi::OsStrExt;
    let mut c = std::process::Command::new(std::ffi::OsStr::from_bytes(ns(&a[0], f, 1)));
    for v in &na(&a[1], f, 2).vals { c.arg(std::ffi::OsStr::from_bytes(&text(v))); } c
}
fn status_of(st: std::process::ExitStatus) -> i64 { st.code().map(|c| c as i64).unwrap_or(-1) }
pub fn m_Process_run(a: &[V]) -> V {
    flush(); let mut c = cmd_of(a, "Process.run");
    match c.output() { Ok(o) => assoc(vec![("status", V::Int(status_of(o.status))), ("stdout", lossy(&o.stdout)), ("stderr", lossy(&o.stderr))]),
        Err(_) => assoc(vec![("status", V::Int(127)), ("stdout", s("")), ("stderr", sv(format!("{}: No such file or directory\n", String::from_utf8_lossy(ns(&a[0], "", 1))).into_bytes()))]) }
}
pub fn m_Process_exec(a: &[V]) -> V { flush(); let mut c = cmd_of(a, "Process.exec"); match c.status() { Ok(st) => V::Int(status_of(st)), Err(_) => V::Int(127) } }
macro_rules! nosock { ($($n:ident),*) => { $(pub fn $n(a: &[V]) -> V { die("Socket: niedostępne w celu rust (użyj celu c / zsc)") })* } }
nosock!(m_Socket_tcpConnect, m_Socket_tcpListen, m_Socket_accept, m_Socket_read, m_Socket_write, m_Socket_close, m_Socket_setTimeout, m_Socket_udpBind, m_Socket_sendTo, m_Socket_recvFrom, m_Socket_resolve, m_Socket_localAddr, m_Socket_peerAddr);

// ───── program ─────
pub struct ExitCode(pub i32);
pub fn echo(a: &[V]) { let mut o = Vec::new(); for (i, v) in a.iter().enumerate() { if i > 0 { o.push(b' '); } text_into(v, &mut o, false); } o.push(b'\n'); out(&o); }
pub fn necho(a: &[V]) { let mut o = Vec::new(); for v in a { text_into(v, &mut o, false); } out(&o); }
pub fn eecho(a: &[V]) { let mut o = Vec::new(); for (i, v) in a.iter().enumerate() { if i > 0 { o.push(b' '); } text_into(v, &mut o, false); } o.push(b'\n'); err(&o); }
pub fn exit(a: &[V]) -> V { flush(); std::panic::resume_unwind(Box::new(ExitCode(match a.get(0) { Some(V::Int(i)) => *i as i32, _ => 0 }))) }
pub fn run(top: fn(), main: Option<fn() -> V>, calls_main: bool) {
    use std::os::unix::ffi::OsStringExt;
    std::panic::set_hook(Box::new(|_| {}));
    let h = std::thread::Builder::new().stack_size(1 << 30).spawn(move || {
        ARGS.with(|x| *x.borrow_mut() = std::env::args_os().map(|a| a.into_vec()).collect());
        let r = std::panic::catch_unwind(|| { top(); let mut rc = 0; if let (Some(m), false) = (main, calls_main) { if let V::Int(i) = m() { rc = i as i32; } } rc });
        flush();
        match r {
            Ok(rc) => rc,
            Err(p) => {
                if let Some(c) = p.downcast_ref::<ExitCode>() { return c.0; }
                if let Some(Pl(v)) = p.downcast_ref::<Pl>() {
                    match v {
                        V::Obj(o) => { let f = o.f.borrow(); let l = match f.get(&K::S(Rc::from(&b"line"[..]))) { Some(V::Int(i)) => *i, _ => 0 }; let m = f.get(&K::S(Rc::from(&b"message"[..]))).cloned().unwrap_or(V::Null); err(format!("Nieobsłużony wyjątek {}: {} (linia {})\n", String::from_utf8_lossy(&o.cls), String::from_utf8_lossy(&text(&m)), l).as_bytes()); }
                        V::Exc(e) => err(format!("Nieobsłużony wyjątek {}: {} (linia {})\n", if e.kind != 0 { "Error" } else { "Exception" }, String::from_utf8_lossy(&text(&e.msg)), e.line.get()).as_bytes()),
                        _ => {}
                    }
                } else { err(b"Nieobsluzony blad wewnetrzny\n"); }
                1
            }
        }
    }).unwrap();
    let rc = h.join().unwrap_or(1);
    flush();
    std::process::exit(rc);
}
}
