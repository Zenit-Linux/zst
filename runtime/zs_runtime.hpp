#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cmath>
#include <string>
#include <vector>
#include <memory>
#include <unordered_map>
#include <functional>
#include <initializer_list>
#include <algorithm>
#include <chrono>
#include <thread>
#include <fstream>
#include <sstream>
#include <map>
#include <unistd.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <sys/select.h>
#include <dirent.h>
#include <fcntl.h>
#include <limits.h>
extern char** environ;

namespace zs {
enum class T : uint8_t { Null, Bool, Int, Float, Str, Arr, Obj, Exc, Fn };
struct Arr; struct Obj; struct Exc; struct Fn;
struct V {
    T t = T::Null; union { bool b; int64_t i; double f; }; std::shared_ptr<void> p;
    V() : i(0) {}
    V(bool x) : t(T::Bool), i(0) { b = x; }
    V(int64_t x) : t(T::Int), i(x) {}
    V(double x) : t(T::Float), f(x) {}
    bool isNull() const { return t == T::Null; }
};
struct K { bool isInt; int64_t i; std::string s; bool operator==(const K& o) const { return isInt == o.isInt && (isInt ? i == o.i : s == o.s); } };
struct KH { size_t operator()(const K& k) const { return k.isInt ? std::hash<int64_t>()(k.i) : std::hash<std::string>()(k.s) ^ 0x9e3779b9; } };
struct Arr {
    std::vector<K> keys; std::vector<V> vals; std::unordered_map<K, size_t, KH> map; int64_t next = 0;
    V* find(const K& k) { auto it = map.find(k); return it == map.end() ? nullptr : &vals[it->second]; }
    void put(const K& k, const V& v) {
        auto it = map.find(k); if (it != map.end()) { vals[it->second] = v; return; }
        if (k.isInt && k.i >= next) next = k.i + 1;
        map.emplace(k, keys.size()); keys.push_back(k); vals.push_back(v);
    }
};
struct Obj { std::string cls; Arr f; };
struct Exc { int kind; V msg; int64_t line = 0; };
struct Fn { V id, cap; };
struct Throw { V v; };
struct PK { bool push; V k; PK() : push(true) {} PK(const V& v) : push(false), k(v) {} };
struct AItem { V k, v; bool hasKey; };
struct Fin { std::function<void()> f; Fin(std::function<void()> g) : f(std::move(g)) {} ~Fin() noexcept(false) { f(); } };
struct ExitCode { int code; };

static int64_t g_line = 0; static std::string g_out; static Arr g_statics; static std::vector<std::string> g_args;
inline void line(int64_t n) { g_line = n; }
inline void flush() { if (!g_out.empty()) { fwrite(g_out.data(), 1, g_out.size(), stdout); g_out.clear(); } fflush(stdout); }
inline void outb(const std::string& s) { g_out += s; if (g_out.size() > 65536) flush(); }
inline void errb(const std::string& s) { flush(); fwrite(s.data(), 1, s.size(), stderr); fflush(stderr); }

inline std::shared_ptr<std::string> sp(const V& v) { return std::static_pointer_cast<std::string>(v.p); }
inline const std::string& S(const V& v) { return *static_cast<std::string*>(v.p.get()); }
inline Arr& A(const V& v) { return *static_cast<Arr*>(v.p.get()); }
inline V mkstr(std::string s) { V v; v.t = T::Str; v.p = std::make_shared<std::string>(std::move(s)); return v; }
inline V str(const char* s, size_t n) { return mkstr(std::string(s, n)); }
inline V str(const std::string& s) { return mkstr(s); }
inline V arr_new() { V v; v.t = T::Arr; v.p = std::make_shared<Arr>(); return v; }
inline V mkint(int64_t i) { return V(i); }

[[noreturn]] inline void raise(V e) {
    if (e.t == T::Exc) { auto* x = static_cast<Exc*>(e.p.get()); if (x->line == 0) x->line = g_line; }
    else if (e.t == T::Obj) { auto* o = static_cast<Obj*>(e.p.get()); K k{false, 0, "line"}; V* l = o->f.find(k); if (!(l && l->t == T::Int && l->i != 0)) o->f.put(k, V((int64_t)g_line)); }
    throw Throw{e};
}
inline V new_exc(int kind, const V& msg) { V v; v.t = T::Exc; v.p = std::make_shared<Exc>(Exc{kind, msg, 0}); return v; }
inline std::string tn(const V& v) {
    switch (v.t) { case T::Null: return "null"; case T::Bool: return "bool"; case T::Int: return "int"; case T::Float: return "float"; case T::Str: return "string";
        case T::Arr: return "array"; case T::Obj: return static_cast<Obj*>(v.p.get())->cls; case T::Fn: return "function"; default: return "object"; }
}
[[noreturn]] inline void die(const std::string& m) { V e = new_exc(1, mkstr(m)); static_cast<Exc*>(e.p.get())->line = g_line; throw Throw{e}; }
[[noreturn]] inline void throwv(const V& x) { if (x.t != T::Exc && x.t != T::Obj) die("throw wymaga obiektu typu Throwable"); raise(x); }
inline bool exc_is(const V& x, int kind) { return x.t == T::Exc && (kind == 0 || static_cast<Exc*>(x.p.get())->kind == kind); }
inline V get_message(const V& x) { if (x.t != T::Exc) die("wywołanie metody `getMessage` na wartości typu " + tn(x)); return static_cast<Exc*>(x.p.get())->msg; }
inline bool isnum(const V& v) { return v.t == T::Int || v.t == T::Float; }
inline double asf(const V& v) { return v.t == T::Int ? (double)v.i : v.f; }

// ───── tekst ─────
inline std::string fmt_float(double f) {
    if (std::isnan(f)) return "NaN"; if (std::isinf(f)) return f > 0 ? "Infinity" : "-Infinity";
    char b[400];
    if (f == std::floor(f) && std::fabs(f) < 1e15) { snprintf(b, sizeof b, "%.1f", f); return b; }
    int p; char e[64];
    for (p = 0; p < 17; p++) { snprintf(e, sizeof e, "%.*e", p, f); if (strtod(e, nullptr) == f) break; }
    snprintf(e, sizeof e, "%.*e", p, f);
    bool neg = e[0] == '-'; const char* q = e + neg; std::string digits;
    for (; *q && *q != 'e'; q++) if (*q != '.') digits += *q;
    int ex = atoi(q + 1); std::string r = neg ? "-" : ""; int point = ex + 1, nd = (int)digits.size();
    if (point <= 0) { r += "0."; r += std::string(-point, '0'); r += digits; }
    else if (point >= nd) { r += digits; r += std::string(point - nd, '0'); }
    else { r += digits.substr(0, point); r += "."; r += digits.substr(point); }
    return r;
}
inline void dump_str(const std::string& s, std::string& o) {
    o += '"';
    for (unsigned char c : s) {
        if (c == '"') o += "\\\""; else if (c == '\\') o += "\\\\"; else if (c == '\n') o += "\\n"; else if (c == '\r') o += "\\r"; else if (c == '\t') o += "\\t";
        else if (c == 0) o += "\\0"; else if (c < 0x20 || c == 0x7f) { char b[16]; snprintf(b, sizeof b, "\\u{%x}", c); o += b; } else o += (char)c;
    }
    o += '"';
}
inline void text_into(const V& v, std::string& o, bool dump) {
    switch (v.t) {
        case T::Null: o += "null"; break; case T::Bool: o += v.b ? "true" : "false"; break;
        case T::Int: o += std::to_string(v.i); break; case T::Float: o += fmt_float(v.f); break;
        case T::Str: if (dump) dump_str(S(v), o); else o += S(v); break;
        case T::Arr: {
            Arr& a = A(v); bool seq = true; for (size_t i = 0; i < a.keys.size(); i++) if (!(a.keys[i].isInt && a.keys[i].i == (int64_t)i)) { seq = false; break; }
            o += '[';
            for (size_t i = 0; i < a.keys.size(); i++) { if (i) o += ", "; if (!seq) { if (a.keys[i].isInt) o += std::to_string(a.keys[i].i); else dump_str(a.keys[i].s, o); o += " => "; } text_into(a.vals[i], o, true); }
            o += ']'; break;
        }
        case T::Obj: o += "<object " + static_cast<Obj*>(v.p.get())->cls + ">"; break;
        case T::Fn: o += "<function>"; break;
        case T::Exc: o += static_cast<Exc*>(v.p.get())->kind ? "<object Error>" : "<object Exception>"; break;
    }
}
inline std::string text(const V& v) { std::string o; text_into(v, o, false); return o; }
inline bool truthy(const V& v) {
    switch (v.t) { case T::Null: return false; case T::Bool: return v.b; case T::Int: return v.i != 0; case T::Float: return v.f != 0.0;
        case T::Str: { auto& s = S(v); return !(s.empty() || s == "0"); } case T::Arr: return !A(v).keys.empty(); default: return true; }
}

// ───── tablice ─────
inline K norm_key(const V& k) {
    switch (k.t) {
        case T::Int: return K{true, k.i, ""}; case T::Bool: return K{true, (int64_t)k.b, ""};
        case T::Float: if (std::isfinite(k.f) && k.f == std::floor(k.f) && std::fabs(k.f) < 9.2e18) return K{true, (int64_t)k.f, ""}; break;
        case T::Str: {
            const std::string& s = S(k); size_t n = s.size();
            if (n > 0 && n <= 20) {
                bool neg = s[0] == '-'; size_t st = neg ? 1 : 0; bool ok = st < n && (s[st] != '0' || n == st + 1) && !(neg && s[1] == '0');
                for (size_t j = st; ok && j < n; j++) if (s[j] < '0' || s[j] > '9') ok = false;
                if (ok) { errno = 0; char* e; long long x = strtoll(s.c_str(), &e, 10); if (!errno && *e == 0) return K{true, (int64_t)x, ""}; }
            }
            return K{false, 0, s};
        }
        default: break;
    }
    die("niepoprawny klucz tablicy typu " + tn(k));
}
inline V key_val(const K& k) { return k.isInt ? V(k.i) : mkstr(k.s); }
struct ArrLit { };
inline V arr_lit(std::initializer_list<AItem> items) {
    V r = arr_new(); Arr& a = A(r);
    for (const AItem& it : items) { if (!it.hasKey) a.put(K{true, a.next, ""}, it.v); else a.put(norm_key(it.k), it.v); }
    return r;
}
inline V& slot(V& base, const PK& key) {
    if (base.t == T::Null) base = arr_new();
    if (base.t != T::Arr) die("nie można indeksować wartości typu " + tn(base) + " przy zapisie");
    if (base.p.use_count() > 1) { base.p = std::make_shared<Arr>(A(base)); }
    Arr& a = A(base); K kk = key.push ? K{true, a.next, ""} : norm_key(key.k);
    V* e = a.find(kk); if (!e) { a.put(kk, V()); e = a.find(kk); }
    return *e;
}
inline void set_path(V& base, std::initializer_list<PK> path, V val) { V* cur = &base; for (const PK& p : path) cur = &slot(*cur, p); *cur = std::move(val); }
inline V index_impl(const V& a, const V& k, bool quiet) {
    if (a.t == T::Arr) {
        K nk = norm_key(k); V* e = A(a).find(nk); if (e) return *e; if (quiet) return V();
        std::string t; if (nk.isInt) t = std::to_string(nk.i); else dump_str(nk.s, t); die("brak klucza " + t);
    }
    if (a.t == T::Str) {
        if (k.t != T::Int) die("indeks tekstu musi być liczbą całkowitą");
        const std::string& s = S(a); int64_t len = (int64_t)s.size(), i = k.i < 0 ? len + k.i : k.i;
        if (i < 0 || i >= len) { if (quiet) return V(); die("indeks " + std::to_string(i) + " poza zakresem tekstu (długość " + std::to_string(len) + ")"); }
        unsigned char c = (unsigned char)s[i];
        if ((c & 0xC0) == 0x80) { if (quiet) return V(); die("indeks " + std::to_string(i) + " wskazuje środek znaku UTF-8"); }
        int64_t n = c < 0x80 ? 1 : c < 0xE0 ? 2 : c < 0xF0 ? 3 : 4; if (i + n > len) n = len - i;
        return mkstr(s.substr((size_t)i, (size_t)n));
    }
    if (a.t == T::Null) { if (quiet) return V(); die("indeksowanie wartości null"); }
    die("typ " + tn(a) + " nie obsługuje indeksowania");
}
inline V index(const V& a, const V& k) { return index_impl(a, k, false); }
inline V index_opt(const V& a, const V& k) { return index_impl(a, k, true); }
inline V get_path(const V& base, std::initializer_list<V> path) { V cur = base; for (const V& p : path) cur = index(cur, p); return cur; }
inline V get_path_opt(const V& base, std::initializer_list<V> path) { V cur = base; for (const V& p : path) cur = index_opt(cur, p); return cur; }
inline V foreach_arr(const V& v) { if (v.t != T::Arr) die("foreach wymaga tablicy, jest " + tn(v)); return v; }
inline size_t fe_len(const V& a) { return A(a).keys.size(); }
inline V fe_val(const V& a, size_t i) { return A(a).vals[i]; }
inline V fe_key(const V& a, size_t i) { return key_val(A(a).keys[i]); }

// ───── porównania i operatory ─────
inline bool ident(const V& a, const V& b) {
    if (a.t != b.t) return false;
    switch (a.t) {
        case T::Null: return true; case T::Bool: return a.b == b.b; case T::Int: return a.i == b.i; case T::Float: return a.f == b.f;
        case T::Str: return S(a) == S(b);
        case T::Arr: { if (a.p == b.p) return true; Arr &x = A(a), &y = A(b); if (x.keys.size() != y.keys.size()) return false;
            for (size_t i = 0; i < x.keys.size(); i++) if (!(x.keys[i] == y.keys[i]) || !ident(x.vals[i], y.vals[i])) return false; return true; }
        default: return a.p == b.p;
    }
}
inline bool eq(const V& a, const V& b) { if (isnum(a) && isnum(b) && a.t != b.t) return asf(a) == asf(b); return ident(a, b); }
inline int cmp(const V& a, const V& b) {
    if (a.t == T::Int && b.t == T::Int) return a.i < b.i ? -1 : a.i > b.i;
    if (isnum(a) && isnum(b)) { double x = asf(a), y = asf(b); return x < y ? -1 : x > y; }
    if (a.t == T::Str && b.t == T::Str) { int c = S(a).compare(S(b)); return c < 0 ? -1 : c > 0; }
    if (a.t == T::Bool && b.t == T::Bool) return (int)a.b - (int)b.b;
    die("nie można porównać " + tn(a) + " z " + tn(b));
}
inline bool lt(const V& a, const V& b) { return cmp(a, b) < 0; } inline bool gt(const V& a, const V& b) { return cmp(a, b) > 0; }
inline bool le(const V& a, const V& b) { return cmp(a, b) <= 0; } inline bool ge(const V& a, const V& b) { return cmp(a, b) >= 0; }
[[noreturn]] inline void bad(const char* op, const V& a, const V& b) { die(std::string("operacja ") + op + " niedostępna dla " + tn(a) + " i " + tn(b)); }
[[noreturn]] inline void ovf() { die("przepełnienie liczby całkowitej"); }
inline V add(const V& a, const V& b) {
    if (a.t == T::Int && b.t == T::Int) { int64_t r; if (__builtin_add_overflow(a.i, b.i, &r)) ovf(); return V(r); }
    if (isnum(a) && isnum(b)) return V(asf(a) + asf(b));
    if (a.t == T::Arr && b.t == T::Arr) { V r = arr_new(); A(r) = A(a); Arr& y = A(b); for (size_t i = 0; i < y.keys.size(); i++) if (!A(r).find(y.keys[i])) A(r).put(y.keys[i], y.vals[i]); return r; }
    bad("Add", a, b);
}
inline V sub(const V& a, const V& b) { if (a.t == T::Int && b.t == T::Int) { int64_t r; if (__builtin_sub_overflow(a.i, b.i, &r)) ovf(); return V(r); } if (isnum(a) && isnum(b)) return V(asf(a) - asf(b)); bad("Sub", a, b); }
inline V mul(const V& a, const V& b) { if (a.t == T::Int && b.t == T::Int) { int64_t r; if (__builtin_mul_overflow(a.i, b.i, &r)) ovf(); return V(r); } if (isnum(a) && isnum(b)) return V(asf(a) * asf(b)); bad("Mul", a, b); }
inline V div(const V& a, const V& b) {
    if (!(isnum(a) && isnum(b))) bad("Div", a, b);
    if (asf(b) == 0.0) die("dzielenie przez zero");
    if (a.t == T::Int && b.t == T::Int) { if (b.i == -1) { if (a.i == INT64_MIN) ovf(); return V(-a.i); } if (a.i % b.i == 0) return V(a.i / b.i); }
    return V(asf(a) / asf(b));
}
inline V modulo(const V& a, const V& b) { if (a.t == T::Int && b.t == T::Int) { if (b.i == 0) die("modulo przez zero"); if (b.i == -1) return V((int64_t)0); return V(a.i % b.i); } bad("Mod", a, b); }
inline V bit_and(const V& a, const V& b) { if (a.t == T::Int && b.t == T::Int) return V(a.i & b.i); bad("BitAnd", a, b); }
inline V bit_or(const V& a, const V& b) { if (a.t == T::Int && b.t == T::Int) return V(a.i | b.i); bad("BitOr", a, b); }
inline V bit_xor(const V& a, const V& b) { if (a.t == T::Int && b.t == T::Int) return V(a.i ^ b.i); bad("BitXor", a, b); }
inline V shl(const V& a, const V& b) { if (a.t == T::Int && b.t == T::Int) return V((int64_t)((uint64_t)a.i << (b.i & 63))); bad("Shl", a, b); }
inline V shr(const V& a, const V& b) { if (a.t == T::Int && b.t == T::Int) return V(a.i >> (b.i & 63)); bad("Shr", a, b); }
inline V bit_not(const V& a) { if (a.t == T::Int) return V(~a.i); die("operator jednoargumentowy BitNot niedostępny dla " + tn(a)); }
inline V pow(const V& a, const V& b) {
    if (a.t == T::Int && b.t == T::Int && b.i >= 0) {
        int64_t r = 1, base = a.i, e = b.i; bool o = false;
        while (e > 0) { if (e & 1) { if (__builtin_mul_overflow(r, base, &r)) { o = true; break; } } e >>= 1; if (e && __builtin_mul_overflow(base, base, &base)) { o = true; break; } }
        if (!o) return V(r); return V(std::pow((double)a.i, (double)b.i));
    }
    if (isnum(a) && isnum(b)) return V(std::pow(asf(a), asf(b)));
    bad("Pow", a, b);
}
inline V concat(const V& a, const V& b) { std::string o = a.t == T::Str ? S(a) : text(a); if (b.t == T::Str) o += S(b); else text_into(b, o, false); return mkstr(std::move(o)); }
inline V neg(const V& a) { if (a.t == T::Int) { if (a.i == INT64_MIN) ovf(); return V(-a.i); } if (a.t == T::Float) return V(-a.f); die("operator jednoargumentowy Neg niedostępny dla " + tn(a)); }
inline V chk(const V& v, const char* ty, const char* what) {
    std::string t = ty; bool ok;
    if (t == "int") ok = v.t == T::Int; else if (t == "float") { if (v.t == T::Int) return V((double)v.i); ok = v.t == T::Float; }
    else if (t == "string") ok = v.t == T::Str; else if (t == "bool") ok = v.t == T::Bool; else if (t == "array") ok = v.t == T::Arr; else ok = v.t == T::Obj || v.t == T::Exc;
    if (ok) return v; die(std::string(what) + ": oczekiwano " + t + ", otrzymano " + tn(v));
}

// ───── wbudowane ─────
inline const std::string& ns(const V& v, const char* f, int n) { if (v.t != T::Str) die(std::string(f) + ": argument #" + std::to_string(n) + " powinien być tekstem, jest " + tn(v)); return S(v); }
inline int64_t ni(const V& v, const char* f, int n) { if (v.t != T::Int) die(std::string(f) + ": argument #" + std::to_string(n) + " powinien być liczbą całkowitą, jest " + tn(v)); return v.i; }
inline double nf(const V& v, const char* f, int n) { if (!isnum(v)) die(std::string(f) + ": argument #" + std::to_string(n) + " powinien być liczbą, jest " + tn(v)); return asf(v); }
inline Arr& na(const V& v, const char* f, int n) { if (v.t != T::Arr) die(std::string(f) + ": argument #" + std::to_string(n) + " powinien być tablicą, jest " + tn(v)); return A(v); }
inline bool is_ws(unsigned char c) { return c == ' ' || (c >= 9 && c <= 13); }
inline std::string trim_s(const std::string& s) { size_t a = 0, b = s.size(); while (a < b && is_ws(s[a])) a++; while (b > a && is_ws(s[b - 1])) b--; return s.substr(a, b - a); }
inline size_t u8len(unsigned char c) { return c < 0x80 ? 1 : c < 0xE0 ? 2 : c < 0xF0 ? 3 : 4; }
inline std::string lossy_s(const std::string& s) {
    std::string o; size_t i = 0, n = s.size();
    while (i < n) {
        unsigned char c = (unsigned char)s[i]; size_t need = 0; unsigned char lo = 0x80, hi = 0xBF;
        if (c < 0x80) { o += (char)c; i++; continue; }
        else if (c >= 0xC2 && c <= 0xDF) need = 1;
        else if (c == 0xE0) { need = 2; lo = 0xA0; } else if (c >= 0xE1 && c <= 0xEC) need = 2; else if (c == 0xED) { need = 2; hi = 0x9F; } else if (c >= 0xEE && c <= 0xEF) need = 2;
        else if (c == 0xF0) { need = 3; lo = 0x90; } else if (c >= 0xF1 && c <= 0xF3) need = 3; else if (c == 0xF4) { need = 3; hi = 0x8F; }
        else { o += "\xEF\xBF\xBD"; i++; continue; }
        size_t j = 1; bool ok = true;
        for (; j <= need; j++) { if (i + j >= n) { ok = false; break; } unsigned char d = (unsigned char)s[i + j]; unsigned char l = j == 1 ? lo : 0x80, h = j == 1 ? hi : 0xBF; if (d < l || d > h) { ok = false; break; } }
        if (ok) { o.append(s, i, need + 1); i += need + 1; } else { o += "\xEF\xBF\xBD"; i += j; }
    }
    return o;
}
inline V from_vec(const std::vector<V>& v) { V r = arr_new(); for (auto& x : v) A(r).put(K{true, A(r).next, ""}, x); return r; }
inline std::vector<std::string> chars_of(const std::string& s) { std::vector<std::string> r; for (size_t i = 0; i < s.size();) { size_t n = u8len((unsigned char)s[i]); if (i + n > s.size()) n = s.size() - i; r.push_back(s.substr(i, n)); i += n; } return r; }
inline V b_count(const V& v) { if (v.t == T::Arr) return V((int64_t)A(v).keys.size()); if (v.t == T::Str) return V((int64_t)S(v).size()); die("count: niedostępne dla " + tn(v)); }
inline V b_strlen(const V& v) { return V((int64_t)ns(v, "strlen", 1).size()); }
inline V b_typeof(const V& v) { if (v.t == T::Obj) return mkstr(tn(v)); if (v.t == T::Exc) return mkstr(static_cast<Exc*>(v.p.get())->kind ? "Error" : "Exception"); return mkstr(tn(v)); }
inline V b_is_int(const V& v) { return V(v.t == T::Int); } inline V b_is_float(const V& v) { return V(v.t == T::Float); } inline V b_is_string(const V& v) { return V(v.t == T::Str); }
inline V b_is_bool(const V& v) { return V(v.t == T::Bool); } inline V b_is_array(const V& v) { return V(v.t == T::Arr); } inline V b_is_null(const V& v) { return V(v.t == T::Null); }
inline V b_strval(const V& v) { return v.t == T::Str ? v : mkstr(text(v)); } inline V b_boolval(const V& v) { return V(truthy(v)); }
inline V b_intval(const V& v) {
    switch (v.t) {
        case T::Int: return v; case T::Float: return V((int64_t)(std::isnan(v.f) ? 0 : v.f >= 9.2233720368547758e18 ? INT64_MAX : v.f <= -9.2233720368547758e18 ? INT64_MIN : (int64_t)v.f));
        case T::Bool: return V((int64_t)v.b); case T::Null: return V((int64_t)0);
        case T::Str: { std::string t = trim_s(S(v)); char* e; errno = 0; long long x = strtoll(t.c_str(), &e, 10);
            if (!t.empty() && !errno && *e == 0) return V((int64_t)x); double f = strtod(t.c_str(), &e); bool ok = !t.empty() && *e == 0; return V((int64_t)(ok && !std::isnan(f) ? (int64_t)f : 0)); }
        default: die("intval: nie można przekonwertować " + tn(v));
    }
}
inline V b_floatval(const V& v) { if (v.t == T::Int) return V((double)v.i); if (v.t == T::Float) return v; if (v.t == T::Str) { std::string t = trim_s(S(v)); char* e; double f = strtod(t.c_str(), &e); return V(!t.empty() && *e == 0 ? f : 0.0); } return V(0.0); }
inline V b_ord(const V& v) { auto& s = ns(v, "ord", 1); if (s.empty()) die("ord: pusty tekst"); return V((int64_t)(unsigned char)s[0]); }
inline V b_chr(const V& v) {
    int64_t n = ni(v, "chr", 1); std::string o;
    if (n >= 0 && n < 0x110000 && !(n >= 0xD800 && n < 0xE000)) {
        uint32_t c = (uint32_t)n;
        if (c < 0x80) o += (char)c; else if (c < 0x800) { o += (char)(0xC0 | c >> 6); o += (char)(0x80 | (c & 63)); }
        else if (c < 0x10000) { o += (char)(0xE0 | c >> 12); o += (char)(0x80 | ((c >> 6) & 63)); o += (char)(0x80 | (c & 63)); }
        else { o += (char)(0xF0 | c >> 18); o += (char)(0x80 | ((c >> 12) & 63)); o += (char)(0x80 | ((c >> 6) & 63)); o += (char)(0x80 | (c & 63)); }
    }
    return mkstr(o);
}
inline V b_trim(const V& v) { return mkstr(trim_s(ns(v, "trim", 1))); }
inline V b_contains(const V& a, const V& b) { return V(ns(a, "contains", 1).find(ns(b, "contains", 2)) != std::string::npos); }
inline V b_starts_with(const V& a, const V& b) { auto &x = ns(a, "starts_with", 1), &y = ns(b, "starts_with", 2); return V(y.size() <= x.size() && x.compare(0, y.size(), y) == 0); }
inline V b_ends_with(const V& a, const V& b) { auto &x = ns(a, "ends_with", 1), &y = ns(b, "ends_with", 2); return V(y.size() <= x.size() && x.compare(x.size() - y.size(), y.size(), y) == 0); }
inline V b_index_of(const V& a, const V& b) { size_t p = ns(a, "index_of", 1).find(ns(b, "index_of", 2)); return V(p == std::string::npos ? (int64_t)-1 : (int64_t)p); }
inline V b_repeat(const V& a, const V& b) { auto& x = ns(a, "repeat", 1); int64_t n = ni(b, "repeat", 2); std::string o; for (int64_t i = 0; i < n; i++) o += x; return mkstr(o); }
inline V b_substr(const V& s, const V& a, const V& l = V()) {
    auto& x = ns(s, "substr", 1); int64_t len = (int64_t)x.size(), st = ni(a, "substr", 2);
    if (st < 0) { st = len + st; if (st < 0) st = 0; } if (st > len) st = len;
    int64_t n; if (l.t == T::Int) { n = l.i >= 0 ? l.i : len - st + l.i; if (n < 0) n = 0; } else n = len - st;
    if (st + n > len) n = len - st; return mkstr(lossy_s(x.substr((size_t)st, (size_t)n)));
}
inline V b_join(const V& a, const V& sep) { Arr& x = na(a, "join", 1); std::string o; for (size_t i = 0; i < x.vals.size(); i++) { if (i && sep.t == T::Str) o += S(sep); text_into(x.vals[i], o, false); } return mkstr(o); }
inline V b_in_array(const V& n, const V& arr) { Arr& x = na(arr, "in_array", 2); for (auto& v : x.vals) if (ident(v, n)) return V(true); return V(false); }
inline V b_range(const V& a, const V& b) { int64_t x = ni(a, "range", 1), y = ni(b, "range", 2); std::vector<V> r; if (x <= y) { for (int64_t i = x;; i++) { r.push_back(V(i)); if (i == y) break; } } else { for (int64_t i = x;; i--) { r.push_back(V(i)); if (i == y) break; } } return from_vec(r); }
inline V b_keys(const V& a) { Arr& x = na(a, "keys", 1); std::vector<V> r; for (auto& k : x.keys) r.push_back(key_val(k)); return from_vec(r); }
inline V b_values(const V& a) { Arr& x = na(a, "values", 1); return from_vec(x.vals); }
inline V b_has_key(const V& a, const V& k) { Arr& x = na(a, "has_key", 1); return V(x.find(norm_key(k)) != nullptr); }
inline V b_upper(const V& v) { std::string s = ns(v, "upper", 1); for (auto& c : s) if (c >= 'a' && c <= 'z') c -= 32; return mkstr(s); }
inline V b_lower(const V& v) { std::string s = ns(v, "lower", 1); for (auto& c : s) if (c >= 'A' && c <= 'Z') c += 32; return mkstr(s); }
inline V b_replace(const V& a, const V& f, const V& t) {
    auto &s = ns(a, "replace", 1), &x = ns(f, "replace", 2), &y = ns(t, "replace", 3); if (x.empty()) return a;
    std::string o; size_t i = 0; for (;;) { size_t p = s.find(x, i); if (p == std::string::npos) break; o.append(s, i, p - i); o += y; i = p + x.size(); } o.append(s, i, std::string::npos); return mkstr(o);
}
inline V b_split(const V& a, const V& sep) {
    auto &s = ns(a, "split", 1), &x = ns(sep, "split", 2); std::vector<V> r;
    if (x.empty()) { for (auto& c : chars_of(s)) r.push_back(mkstr(c)); return from_vec(r); }
    size_t i = 0; for (;;) { size_t p = s.find(x, i); if (p == std::string::npos) { r.push_back(mkstr(s.substr(i))); break; } r.push_back(mkstr(s.substr(i, p - i))); i = p + x.size(); }
    return from_vec(r);
}
inline V b_chars(const V& a) { std::vector<V> r; for (auto& c : chars_of(ns(a, "chars", 1))) r.push_back(mkstr(c)); return from_vec(r); }
inline V pad_impl(const V& a, const V& n, const V& p, bool left, const char* f) {
    auto& s = ns(a, f, 1); int64_t w = ni(n, f, 2); if (w < 0) w = 0;
    std::string pc = " "; if (p.t == T::Str && !S(p).empty()) { size_t k = u8len((unsigned char)S(p)[0]); if (k > S(p).size()) k = S(p).size(); pc = S(p).substr(0, k); }
    int64_t l = 0; for (unsigned char c : s) if ((c & 0xC0) != 0x80) l++; if (l >= w) return a;
    std::string padding; for (int64_t i = 0; i < w - l; i++) padding += pc; return mkstr(left ? padding + s : s + padding);
}
inline V b_pad_left(const V& a, const V& n, const V& p = V()) { return pad_impl(a, n, p, true, "pad_left"); }
inline V b_pad_right(const V& a, const V& n, const V& p = V()) { return pad_impl(a, n, p, false, "pad_right"); }
inline V b_array_slice(const V& a, const V& s, const V& l = V()) {
    Arr& x = na(a, "array_slice", 1); int64_t len = (int64_t)x.keys.size(), st = ni(s, "array_slice", 2);
    if (st < 0) { st = len + st; if (st < 0) st = 0; } if (st > len) st = len;
    int64_t n = l.t == T::Int ? l.i : len; if (n < 0) n = 0; int64_t end = st + n; if (end > len) end = len;
    std::vector<V> r(x.vals.begin() + st, x.vals.begin() + end); return from_vec(r);
}
inline V b_reverse(const V& a) {
    if (a.t == T::Str) { auto c = chars_of(S(a)); std::string o; for (size_t i = c.size(); i > 0; i--) o += c[i - 1]; return mkstr(o); }
    if (a.t != T::Arr) die("reverse: niedostępne dla " + tn(a)); std::vector<V> r(A(a).vals.rbegin(), A(a).vals.rend()); return from_vec(r);
}
inline std::vector<V> msort(std::vector<V> v) {
    if (v.size() < 2) return v; size_t m = v.size() / 2; auto l = msort(std::vector<V>(v.begin(), v.begin() + m)); auto r = msort(std::vector<V>(v.begin() + m, v.end()));
    std::vector<V> o; size_t i = 0, j = 0; while (i < l.size() && j < r.size()) { if (cmp(r[j], l[i]) < 0) o.push_back(r[j++]); else o.push_back(l[i++]); }
    while (i < l.size()) o.push_back(l[i++]); while (j < r.size()) o.push_back(r[j++]); return o;
}
inline V b_sort(const V& a) { return from_vec(msort(na(a, "sort", 1).vals)); }
inline V b_push(const V& a, const V& v) { na(a, "push", 1); V r = a; set_path(r, {PK()}, v); return r; }
inline V b_dump(const V& v) { std::string o; text_into(v, o, true); o += '\n'; outb(o); return V(); }
inline int first_c(const V& v, const char* f) { auto& s = ns(v, f, 1); return s.empty() ? -1 : (unsigned char)s[0]; }
inline V b_is_digit(const V& v) { int c = first_c(v, "is_digit"); return V(c >= '0' && c <= '9'); }
inline V b_is_alpha(const V& v) { int c = first_c(v, "is_alpha"); return V(c >= 0 && (isalpha(c) || c == '_' || c >= 0x80)); }
inline V b_is_alnum(const V& v) { int c = first_c(v, "is_alnum"); return V(c >= 0 && (isalnum(c) || c == '_' || c >= 0x80)); }
inline V b_is_space(const V& v) { int c = first_c(v, "is_space"); return V(c >= 0 && is_ws((unsigned char)c)); }
inline V b_is_hex_digit(const V& v) { int c = first_c(v, "is_hex_digit"); return V(c >= 0 && isxdigit(c)); }
inline V b_to_hex(const V& v) { char t[32]; snprintf(t, sizeof t, "%llx", (unsigned long long)ni(v, "to_hex", 1)); return mkstr(t); }
inline V b_read_line() { flush(); std::string o; int c; bool any = false; while ((c = getchar()) != EOF) { any = true; if (c == '\n') break; o += (char)c; } if (!any) return V(); while (!o.empty() && o.back() == '\r') o.pop_back(); return mkstr(o); }
inline V b_zs_obj_new(const V& cls) { V v; v.t = T::Obj; auto o = std::make_shared<Obj>(); o->cls = ns(cls, "zs_obj_new", 1); v.p = o; return v; }
inline V b_zs_obj_class(const V& o) { return o.t == T::Obj ? mkstr(static_cast<Obj*>(o.p.get())->cls) : V(); }
inline V b_zs_obj_get(const V& o, const V& name) {
    const std::string& n = ns(name, "zs_obj_get", 2);
    if (o.t != T::Obj) { if (o.t == T::Null) die("dostęp do właściwości `" + n + "` wartości null"); die("dostęp do właściwości `" + n + "` typu " + tn(o)); }
    Obj* x = static_cast<Obj*>(o.p.get()); V* e = x->f.find(norm_key(name)); if (!e) die("obiekt klasy " + x->cls + " nie ma właściwości " + n); return *e;
}
inline V b_zs_obj_take(const V& o, const V& name) { V v = b_zs_obj_get(o, name); static_cast<Obj*>(o.p.get())->f.put(norm_key(name), V()); return v; }
inline V b_zs_obj_set(const V& o, const V& name, const V& v) { if (o.t != T::Obj) die("zapis właściwości `" + ns(name, "zs_obj_set", 2) + "` na wartości typu " + tn(o)); static_cast<Obj*>(o.p.get())->f.put(norm_key(name), v); return v; }
inline V b_zs_obj_put(const V& o, const V& name, const V& v) { return b_zs_obj_set(o, name, v); }
inline V b_zs_no_method(const V& o, const V& name) { const std::string& n = ns(name, "zs_no_method", 2); if (o.t == T::Obj) die("klasa " + tn(o) + " nie ma metody " + n); die("wywołanie metody `" + n + "` na wartości typu " + tn(o)); }
inline V b_zs_no_class(const V& name) { die("nieznana klasa " + ns(name, "zs_no_class", 1)); }
inline V b_zs_mkfn(const V& id, const V& cap) { V v; v.t = T::Fn; v.p = std::make_shared<Fn>(Fn{id, cap}); return v; }
inline V b_zs_fn_id(const V& f) { if (f.t == T::Fn) return static_cast<Fn*>(f.p.get())->id; if (f.t == T::Str) return f; return V(); }
inline V b_zs_fn_cap(const V& f) { return f.t == T::Fn ? static_cast<Fn*>(f.p.get())->cap : V(); }
inline V b_is_callable(const V& f) { return V(f.t == T::Fn); }
inline V b_zs_sget(const V& n) { V* e = g_statics.find(norm_key(n)); return e ? *e : V(); }
inline V b_zs_sset(const V& n, const V& v) { g_statics.put(norm_key(n), v); return v; }

// ───── moduły ─────
inline V assoc(std::initializer_list<std::pair<const char*, V>> p) { V r = arr_new(); for (auto& kv : p) A(r).put(K{false, 0, kv.first}, kv.second); return r; }
[[noreturn]] inline void io_die(const std::string& p, int e) { die(p + ": " + strerror(e)); }
inline bool is_file_p(const std::string& p) { struct stat st; return stat(p.c_str(), &st) == 0 && S_ISREG(st.st_mode); }
inline bool is_dir_p(const std::string& p) { struct stat st; return stat(p.c_str(), &st) == 0 && S_ISDIR(st.st_mode); }
inline V m_Math_sqrt(const V& v) { return V(std::sqrt(nf(v, "Math.sqrt", 1))); }
inline V m_Math_floor(const V& v) { return V((int64_t)std::floor(nf(v, "Math.floor", 1))); }
inline V m_Math_ceil(const V& v) { return V((int64_t)std::ceil(nf(v, "Math.ceil", 1))); }
inline V m_Math_round(const V& v) { return V((int64_t)std::round(nf(v, "Math.round", 1))); }
inline V m_Math_abs(const V& v) { if (v.t == T::Int) { if (v.i == INT64_MIN) ovf(); return V(v.i < 0 ? -v.i : v.i); } if (v.t == T::Float) return V(std::fabs(v.f)); die("Math.abs: " + tn(v)); }
inline V m_Math_pow(const V& a, const V& b) { return pow(a, b); }
inline V m_Math_min(const V& a, const V& b) { return cmp(a, b) <= 0 ? a : b; }
inline V m_Math_max(const V& a, const V& b) { return cmp(a, b) <= 0 ? b : a; }
inline V m_System_args() { std::vector<V> r; for (auto& a : g_args) r.push_back(mkstr(a)); return from_vec(r); }
inline V m_System_env(const V& k) { const char* e = getenv(ns(k, "System.env", 1).c_str()); return e ? mkstr(e) : V(); }
inline V m_System_os() { return mkstr("linux"); }
inline V m_System_architecture() {
#if defined(__x86_64__)
    return mkstr("x86_64");
#elif defined(__aarch64__)
    return mkstr("aarch64");
#else
    return mkstr("unknown");
#endif
}
inline V m_System_time() { return V(std::chrono::duration<double>(std::chrono::system_clock::now().time_since_epoch()).count()); }
inline V m_System_cpuCount() { long n = sysconf(_SC_NPROCESSORS_ONLN); return V((int64_t)(n > 0 ? n : 1)); }
inline V m_System_version() { return mkstr("0.0.1"); }
inline V m_System_dirname(const V& p) {
    auto& s = ns(p, "System.dirname", 1); int64_t i = (int64_t)s.size(); while (i > 1 && s[i - 1] == '/') i--; while (i > 0 && s[i - 1] != '/') i--; while (i > 1 && s[i - 1] == '/') i--;
    if (i == 0) return mkstr("."); if (i == 1 && s[0] == '/') return mkstr("/"); return mkstr(s.substr(0, (size_t)i));
}
inline bool manifest_member(const std::string& toml, const std::string& name, std::string& out) {
    std::ifstream f(toml); if (!f) return false; std::string ln;
    while (std::getline(f, ln)) {
        size_t p = ln.find_first_not_of(" \t"); if (p == std::string::npos) continue; if (ln.compare(p, name.size(), name) != 0) continue; p += name.size();
        p = ln.find_first_not_of(" \t", p); if (p == std::string::npos || ln[p] != '=') continue; p = ln.find_first_not_of(" \t", p + 1); if (p == std::string::npos || ln[p] != '"') continue;
        size_t e = ln.find('"', p + 1); if (e == std::string::npos) continue; out = ln.substr(p + 1, e - p - 1); return true;
    }
    return false;
}
inline bool with_ext(const std::string& base, std::string& out) { for (const char* e : {".zsharp", ".zs"}) { std::string c = base + e; if (is_file_p(c)) { out = c; return true; } } return false; }
inline std::string dirn(const std::string& p) { size_t s = p.rfind('/'); return s == std::string::npos ? "." : p.substr(0, s); }
inline V m_System_resolveImport(const V& segs, const V& dir) {
    Arr& sa = na(segs, "System.resolveImport", 1); std::string here = ns(dir, "System.resolveImport", 2); std::vector<std::string> names; for (auto& v : sa.vals) names.push_back(text(v));
    if (names.empty()) return V();
    if (names[0] == "Workspace" && names.size() >= 2) {
        std::string cur = here;
        for (int up = 0; up < 40; up++) {
            for (const char* sub : {"", "/Z-Sharp"}) {
                std::string mem; if (manifest_member(cur + sub + "/Z#.toml", names[1], mem)) {
                    std::string base = cur + sub + "/" + mem;
                    if (is_file_p(base)) { for (size_t k = 2; k < names.size(); k++) { std::string nb; if (!with_ext(dirn(base) + "/" + names[k], nb)) return V(); base = nb; } return mkstr(base); }
                }
            }
            size_t sl = cur.rfind('/');
            if (sl == std::string::npos || sl == 0) { if (cur != "/" && cur != ".") cur = "."; else break; if (up > 0 && cur == ".") break; } else cur.resize(sl);
            if (cur == "." && up > 0) { char cw[4096]; if (!getcwd(cw, sizeof cw)) break; cur = cw; }
        }
        return V();
    }
    if (names[0] == "Std" && names.size() == 2) {
        std::string lo = names[1]; for (auto& c : lo) c = (char)tolower((unsigned char)c);
        if (const char* e1 = getenv("ZS_STD")) { std::string p = std::string(e1) + "/" + lo + ".zsharp"; if (is_file_p(p)) return mkstr(p); }
        if (const char* e2 = getenv("ZS_HOME")) { std::string p = std::string(e2) + "/stdlib/" + lo + ".zsharp"; if (is_file_p(p)) return mkstr(p); }
        return V();
    }
    std::vector<std::string> dirs{here}; char cw[4096]; if (getcwd(cw, sizeof cw)) dirs.push_back(cw);
    if (const char* zp = getenv("ZS_PATH")) { std::stringstream ss(zp); std::string t; while (std::getline(ss, t, ':')) if (!t.empty()) dirs.push_back(t); }
    std::string joined; for (size_t i = 0; i < names.size(); i++) joined += (i ? "/" : "") + names[i];
    for (auto& d : dirs) { std::string o; if (with_ext(d + "/" + joined, o)) return mkstr(o); }
    return V();
}
inline V m_System_setEnv(const V& k, const V& v) { setenv(ns(k, "System.setEnv", 1).c_str(), text(v).c_str(), 1); return V(); }
inline V m_System_unsetEnv(const V& k) { unsetenv(ns(k, "System.unsetEnv", 1).c_str()); return V(); }
inline V m_System_environ() { std::map<std::string, std::string> m; for (char** e = environ; e && *e; e++) { std::string s = *e; size_t p = s.find('='); if (p != std::string::npos) m[s.substr(0, p)] = s.substr(p + 1); } V r = arr_new(); for (auto& kv : m) A(r).put(K{false, 0, kv.first}, mkstr(kv.second)); return r; }
inline V m_System_cwd() { char cw[4096]; if (!getcwd(cw, sizeof cw)) return V(); return mkstr(cw); }
inline V m_System_chdir(const V& p) { if (chdir(ns(p, "System.chdir", 1).c_str())) io_die(S(p), errno); return V(); }
inline V m_System_pid() { return V((int64_t)getpid()); }
inline V m_System_hostname() { char h[256]; if (gethostname(h, sizeof h)) return mkstr(""); return mkstr(h); }
inline V m_System_bytes(const V& s) { std::vector<V> r; for (unsigned char c : ns(s, "System.bytes", 1)) r.push_back(V((int64_t)c)); return from_vec(r); }
inline V m_System_fromBytes(const V& a) { std::string o; for (auto& v : na(a, "System.fromBytes", 1).vals) o += (char)(v.t == T::Int ? v.i : 0); return mkstr(lossy_s(o)); }
inline V m_System_nowNanos() { return V((int64_t)std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::system_clock::now().time_since_epoch()).count()); }
inline V m_System_monotonic() { static auto t0 = std::chrono::steady_clock::now(); return V((int64_t)(std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now() - t0).count() + 1000000000LL)); }
inline V m_System_sleep(const V& ms) { flush(); int64_t m = ni(ms, "System.sleep", 1); if (m > 0) std::this_thread::sleep_for(std::chrono::milliseconds(m)); return V(); }
inline V m_System_localOffset(const V& t) { time_t x = (time_t)ni(t, "System.localOffset", 1); struct tm tm; memset(&tm, 0, sizeof tm); localtime_r(&x, &tm); return V((int64_t)tm.tm_gmtoff); }
inline V m_System_gc() { return V(); } inline V m_System_gcStats() { return arr_new(); }
inline V m_File_read(const V& p) { std::ifstream f(ns(p, "File.read", 1), std::ios::binary); if (!f) io_die(S(p), errno ? errno : ENOENT); std::stringstream ss; ss << f.rdbuf(); return mkstr(ss.str()); }
inline V file_put(const V& p, const V& d, bool app, const char* fn) { std::ofstream f(ns(p, fn, 1), std::ios::binary | (app ? std::ios::app : std::ios::trunc)); if (!f) io_die(S(p), errno ? errno : ENOENT); std::string t = text(d); f.write(t.data(), (std::streamsize)t.size()); return V(); }
inline V m_File_write(const V& p, const V& d) { return file_put(p, d, false, "File.write"); }
inline V m_File_append(const V& p, const V& d) { return file_put(p, d, true, "File.append"); }
inline V m_File_exists(const V& p) { return V(is_file_p(ns(p, "File.exists", 1))); }
inline V m_File_delete(const V& p) { if (unlink(ns(p, "File.delete", 1).c_str())) io_die(S(p), errno); return V(); }
inline V m_File_stat(const V& p) {
    struct stat st; if (lstat(ns(p, "File.stat", 1).c_str(), &st)) return V();
    const char* kind = S_ISLNK(st.st_mode) ? "link" : S_ISDIR(st.st_mode) ? "dir" : S_ISREG(st.st_mode) ? "file" : "other";
    return assoc({{"kind", mkstr(kind)}, {"size", V((int64_t)st.st_size)}, {"mtime", V((int64_t)st.st_mtime)}, {"mode", V((int64_t)(st.st_mode & 07777))}});
}
inline V m_File_rename(const V& a, const V& b) { if (rename(ns(a, "File.rename", 1).c_str(), ns(b, "File.rename", 2).c_str())) io_die(S(a), errno); return V(); }
inline V m_File_realpath(const V& p) { char r[PATH_MAX]; if (!realpath(ns(p, "File.realpath", 1).c_str(), r)) io_die(S(p), errno); return mkstr(r); }
inline V m_File_chmod(const V& p, const V& m) { if (chmod(ns(p, "File.chmod", 1).c_str(), (mode_t)ni(m, "File.chmod", 2))) io_die(S(p), errno); return V(); }
inline V m_File_symlink(const V& t, const V& l) { if (symlink(ns(t, "File.symlink", 1).c_str(), ns(l, "File.symlink", 2).c_str())) io_die(S(l), errno); return V(); }
inline V m_File_readlink(const V& p) { char b[PATH_MAX]; ssize_t n = readlink(ns(p, "File.readlink", 1).c_str(), b, sizeof b - 1); if (n < 0) io_die(S(p), errno); return mkstr(std::string(b, (size_t)n)); }
inline V m_Directory_create(const V& p) { std::string t = ns(p, "Directory.create", 1); for (size_t i = 1; i < t.size(); i++) if (t[i] == '/') { std::string q = t.substr(0, i); mkdir(q.c_str(), 0777); } if (mkdir(t.c_str(), 0777) && errno != EEXIST) io_die(t, errno); return V(); }
inline V m_Directory_exists(const V& p) { return V(is_dir_p(ns(p, "Directory.exists", 1))); }
inline V m_Directory_list(const V& p) {
    DIR* d = opendir(ns(p, "Directory.list", 1).c_str()); if (!d) io_die(S(p), errno); std::vector<std::string> n; while (dirent* e = readdir(d)) { std::string s = e->d_name; if (s != "." && s != "..") n.push_back(s); } closedir(d);
    std::sort(n.begin(), n.end()); std::vector<V> r; for (auto& s : n) r.push_back(mkstr(s)); return from_vec(r);
}
inline V m_Directory_remove(const V& p) { if (rmdir(ns(p, "Directory.remove", 1).c_str())) io_die(S(p), errno); return V(); }
inline std::vector<std::string> argv_of(const V& cmd, const V& args, const char* f) { std::vector<std::string> a{ns(cmd, f, 1)}; for (auto& v : na(args, f, 2).vals) a.push_back(text(v)); return a; }
inline int spawn_run(const std::vector<std::string>& a, bool capture, std::string* so, std::string* se) {
    flush(); int po[2] = {-1, -1}, pe[2] = {-1, -1}; if (capture && (pipe(po) || pipe(pe))) io_die(a[0], errno);
    pid_t pid = fork(); if (pid < 0) io_die(a[0], errno);
    if (pid == 0) {
        if (capture) { dup2(po[1], 1); dup2(pe[1], 2); close(po[0]); close(pe[0]); close(po[1]); close(pe[1]); }
        std::vector<char*> av; for (auto& s : a) av.push_back(const_cast<char*>(s.c_str())); av.push_back(nullptr);
        execvp(av[0], av.data()); fprintf(stderr, "%s: %s\n", av[0], strerror(errno)); _exit(127);
    }
    if (capture) {
        close(po[1]); close(pe[1]); bool fo = true, fe = true; char t[8192];
        while (fo || fe) {
            fd_set fs; FD_ZERO(&fs); int mx = 0; if (fo) { FD_SET(po[0], &fs); mx = po[0]; } if (fe) { FD_SET(pe[0], &fs); if (pe[0] > mx) mx = pe[0]; }
            if (select(mx + 1, &fs, nullptr, nullptr, nullptr) < 0) { if (errno == EINTR) continue; break; }
            if (fo && FD_ISSET(po[0], &fs)) { ssize_t n = read(po[0], t, sizeof t); if (n <= 0) fo = false; else so->append(t, (size_t)n); }
            if (fe && FD_ISSET(pe[0], &fs)) { ssize_t n = read(pe[0], t, sizeof t); if (n <= 0) fe = false; else se->append(t, (size_t)n); }
        }
        close(po[0]); close(pe[0]);
    }
    int st; waitpid(pid, &st, 0); return WIFEXITED(st) ? WEXITSTATUS(st) : -1;
}
inline V m_Process_run(const V& c, const V& a) { std::string so, se; int rc = spawn_run(argv_of(c, a, "Process.run"), true, &so, &se); return assoc({{"status", V((int64_t)rc)}, {"stdout", mkstr(lossy_s(so))}, {"stderr", mkstr(lossy_s(se))}}); }
inline V m_Process_exec(const V& c, const V& a) { return V((int64_t)spawn_run(argv_of(c, a, "Process.exec"), false, nullptr, nullptr)); }
#define ZS_NOSOCK(n) inline V m_Socket_##n(const V& = V(), const V& = V(), const V& = V(), const V& = V()) { die("Socket." #n ": niedostępne w celu cpp (użyj celu c / zsc)"); }
ZS_NOSOCK(tcpConnect) ZS_NOSOCK(tcpListen) ZS_NOSOCK(accept) ZS_NOSOCK(read) ZS_NOSOCK(write) ZS_NOSOCK(close) ZS_NOSOCK(setTimeout) ZS_NOSOCK(udpBind) ZS_NOSOCK(sendTo) ZS_NOSOCK(recvFrom) ZS_NOSOCK(resolve) ZS_NOSOCK(localAddr) ZS_NOSOCK(peerAddr)

// ───── program ─────
template <class... Ar> inline V echo(const Ar&... a) { std::string o; bool first = true; ((o += (first ? "" : " "), first = false, text_into(a, o, false)), ...); o += '\n'; outb(o); return V(); }
template <class... Ar> inline V necho(const Ar&... a) { std::string o; (text_into(a, o, false), ...); outb(o); return V(); }
template <class... Ar> inline V eecho(const Ar&... a) { std::string o; bool first = true; ((o += (first ? "" : " "), first = false, text_into(a, o, false)), ...); o += '\n'; errb(o); return V(); }
[[noreturn]] inline V exit(const V& c = V()) { flush(); throw ExitCode{c.t == T::Int ? (int)c.i : 0}; }
inline int run(int argc, char** argv, void (*top)(), V (*mainf)(), bool calls_main) {
    for (int i = 0; i < argc; i++) g_args.push_back(argv[i]);
    int rc = 0;
    try { top(); if (mainf && !calls_main) { V r = mainf(); if (r.t == T::Int) rc = (int)r.i; } }
    catch (ExitCode& e) { rc = e.code; }
    catch (Throw& t) {
        flush(); const V& v = t.v;
        if (v.t == T::Obj) { Obj* o = static_cast<Obj*>(v.p.get()); V* l = o->f.find(K{false, 0, "line"}); V* m = o->f.find(K{false, 0, "message"});
            errb("Nieobsłużony wyjątek " + o->cls + ": " + (m ? text(*m) : "null") + " (linia " + std::to_string(l && l->t == T::Int ? l->i : 0) + ")\n"); }
        else { Exc* e = static_cast<Exc*>(v.p.get()); errb(std::string("Nieobsłużony wyjątek ") + (e->kind ? "Error" : "Exception") + ": " + text(e->msg) + " (linia " + std::to_string(e->line) + ")\n"); }
        rc = 1;
    }
    flush(); return rc;
}
}  // namespace zs
