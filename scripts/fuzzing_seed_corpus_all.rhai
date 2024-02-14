// The purpose of this file is just to include all executable
// funcs into a single script for use as seed-corpus entry
// when fuzzing.

let answer = 42;

print(`The Answer is ${answer}`);

let answer = 42;

debug(answer);

let x = "hello, world!";

print(x.type_of());

let f = Fn("foo");

f.call(42);

let f = Fn("foo");

f.call(1, 2, 3);

fn add(x) {
    this + x
}

let f = Fn("add");

let x = 41;

let r = x.call(f, 1);

print(r);

fn foo(x, y, z) {
    x + y + z
}

let f = Fn("foo");

let g = f.curry(1, 2);

g.call(3);

fn foo(x) { }

print(is_def_fn("foo", 1));
print(is_def_fn("foo", 2));
print(is_def_fn("foo", 0));
print(is_def_fn("bar", 1));

let x = 42;

print(is_def_var("x"));
print(is_def_var("foo"));

{
    let y = 1;
    print(is_def_var("y"));
    }

print(is_def_var("y"));

let x = 42;

print(is_shared(x));

let f = || x;

print(is_shared(x));

let x = 42;

eval("let y = x; x = 123;");

print(x);
print(y);

let x = "hello world!";


if "world" in x {
    print("found!");
}

let x = "hello world!";


if 'w' in x {
    print("found!");
}

let r = 1..100;


if 42 in r {
    print("found!");
}

let r = 1..=100;


if 42 in r {
    print("found!");
}

let m = #{a:1, b:2, c:3};


if "c" in m {
    print("found!");
}

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;


if 3 in b {
    print("found!");
}

let x = [1, 2, 3, 4, 5];
let y = [1, 2, 3, 4, 5];
let z = [1, 2, 3, 4];

print(x != y);

print(x != z);

let m1 = #{a:1, b:2, c:3};
let m2 = #{a:1, b:2, c:3};
let m3 = #{a:1, c:3};

print(m1 != m2);

print(m1 != m3);

let x = [1, 2, 3];
let y = [true, 'x'];

print(x + y);

print(x);

let m = #{a:1, b:2, c:3};
let n = #{a: 42, d:0};

print(m + n);

print(m);

let m = #{a:1, b:2, c:3};
let n = #{a: 42, d:0};

m.mixin(n);

print(m);

let x = [1, 2, 3, 4, 5];
let y = [1, 2, 3, 4, 5];
let z = [1, 2, 3, 4];

print(x == y);

print(x == z);

let m1 = #{a:1, b:2, c:3};
let m2 = #{a:1, b:2, c:3};
let m3 = #{a:1, c:3};

print(m1 == m2);

print(m1 == m3);

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.all(|v| v > 3));

print(x.all(|v| v > 1));

print(x.all(|v, i| i > v));

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.all(|v| v > 3));

print(x.all(|v| v > 1));

print(x.all(|v, i| i > v));

let x = [1, 2, 3];
let y = [true, 'x'];

x.push(y);

print(x);

let b1 = blob(5, 0x42);
let b2 = blob(3, 0x11);

b1.push(b2);

print(b1);

let b = blob(5, 0x42);

b.append('!');

print(b);

let b = blob(5, 0x42);

b.append("hello");

print(b);

let b = blob();

b.push(0x42);

print(b);

let b = blob(5, 0x42);

let x = b.as_string();

print(x);

let x = 123456;

for bit in x.bits() {
    print(bit);
}

let x = 123456;

for bit in x.bits(10) {
    print(bit);
}

let x = 123456;

for bit in x.bits(10..24) {
    print(bit);
}

let x = 123456;

for bit in x.bits(10..=23) {
    print(bit);
}

let x = 123456;

for bit in x.bits(10, 8) {
    print(bit);
}

let b = blob(10);

print(b);

let b = blob(10, 0x42);

print(b);

let text = "朝には紅顔ありて夕べには白骨となる";

print(text.bytes);

for ch in "hello, world!".chars() {
    print(ch);
}

for ch in "hello, world!".chars(2) {
    print(ch);
}

for ch in "hello, world!".chars(2..5) {
    print(ch);
}

for ch in "hello, world!".chars(2..=6) {
    print(ch);
}

for ch in "hello, world!".chars(2, 4) {
    print(ch);
}

let x = [1, 2, 3, 4, 5];

x.chop(3);

print(x);

x.chop(10);

print(x);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

b.chop(3);

print(b);

b.chop(10);

print(b);

let x = [1, 2, 3, 4, 5];


if 4 in x {
    print("found!");
}

let text = "hello, world!";

text.crop(2..8);

print(text);

let text = "hello, world!";

text.crop(2..=8);

print(text);

let text = "hello, world!";

text.crop(5);

print(text);

text.crop(-3);

print(text);

let text = "hello, world!";

text.crop(2, 8);

print(text);

text.crop(-5, 3);

print(text);

let x = [1, 2, 2, 2, 3, 4, 3, 3, 2, 1];

x.dedup();

print(x);

fn declining(a, b) { a >= b }

let x = [1, 2, 2, 2, 3, 1, 2, 3, 4, 3, 3, 2, 1];

x.dedup("declining");

print(x);

let x = [1, 2, 2, 2, 3, 1, 2, 3, 4, 3, 3, 2, 1];

x.dedup(|a, b| a >= b);

print(x);

fn small(x) { x < 3 }

fn screen(x, i) { x + i > 5 }

let x = [1, 2, 3, 4, 5];

let y = x.drain("small");

print(x);

print(y);

let z = x.drain("screen");

print(x);

print(z);

let x = [1, 2, 3, 4, 5];

let y = x.drain(|v| v < 3);

print(x);

print(y);

let z = x.drain(|v, i| v + i > 5);

print(x);

print(z);

let x = [1, 2, 3, 4, 5];

let y = x.drain(1..3);

print(x);

print(y);

let z = x.drain(2..3);

print(x);

print(z);

let x = [1, 2, 3, 4, 5];

let y = x.drain(1..=2);

print(x);

print(y);

let z = x.drain(2..=2);

print(x);

print(z);

let b1 = blob();

b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;

let b2 = b1.drain(1..3);

print(b1);

print(b2);

let b3 = b1.drain(2..3);

print(b1);

print(b3);

let b1 = blob();

b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;

let b2 = b1.drain(1..=2);

print(b1);

print(b2);

let b3 = b1.drain(2..=2);

print(b1);

print(b3);

let x = [1, 2, 3, 4, 5];

let y = x.drain(1, 2);

print(x);

print(y);

let z = x.drain(-1, 1);

print(x);

print(z);

let b1 = blob();

b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;

let b2 = b1.drain(1, 2);

print(b1);

print(b2);

let b3 = b1.drain(-1, 1);

print(b3);

print(z);

let now = timestamp();

sleep(10.0);

print(now.elapsed);

let text = "hello, world!";

print(text.ends_with("world!"));

print(text.ends_with("hello"));

let x = [1, 2, 3, 4, 5];

print(x.extract(1..3));

print(x);

let x = [1, 2, 3, 4, 5];

print(x.extract(1..=3));

print(x);

let x = [1, 2, 3, 4, 5];

print(x.extract(2));

print(x.extract(-3));

print(x);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

print(b.extract(1..3));

print(b);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

print(b.extract(1..=3));

print(b);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

print(b.extract(2));

print(b.extract(-3));

print(b);

let x = [1, 2, 3, 4, 5];

print(x.extract(1, 3));

print(x.extract(-3, 2));

print(x);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

print(b.extract(1, 3));

print(b.extract(-3, 2));

print(b);

let m = #{a:1, b:2, c:3};
let n = #{a: 42, d:0};

m.fill_with(n);

print(m);

let x = [1, 2, 3, 4, 5];

let y = x.filter(|v| v >= 3);

print(y);

let y = x.filter(|v, i| v * i >= 10);

print(y);

fn screen(x, i) { x * i >= 10 }

let x = [1, 2, 3, 4, 5];

let y = x.filter("is_odd");

print(y);

let y = x.filter("screen");

print(y);

let x = [1, 2, 3];

print(x.get(0));

print(x.get(-1));

print(x.get(99));

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

print(b.get(0));

print(b.get(-1));

print(b.get(99));

let m = #{a: 1, b: 2, c: 3};

print(m.get("b"));

print(m.get("x"));

let text = "hello, world!";

print(text.get(0));

print(text.get(-1));

print(text.get(99));

let x = 123456;

for bit in x.bits {
    print(bit);
}

let text = "朝には紅顔ありて夕べには白骨となる";

print(text.bytes);

for ch in "hello, world!".chars {
    print(ch);
}

let now = timestamp();

sleep(10.0);

print(now.elapsed);

let f = |x| x * 2;

print(f.is_anonymous);

let b = blob(10, 0x42);

print(b);

print(b.len());

let text = "朝には紅顔ありて夕べには白骨となる";

print(text.len);

fn double(x) { x * 2 }

let f = Fn("double");

print(f.name);

let x = "hello, world!";

x.tag = 42;

print(x.tag);

let x = 123456;

print(x.get_bit(5));

print(x.get_bit(6));

print(x.get_bit(-48));

let x = 123456;

print(x.get_bits(5..10));

let x = 123456;

print(x.get_bits(5..=9));

let x = 123456;

print(x.get_bits(5, 8));

fn is_special(x) { x > 3 }

fn is_dumb(x) { x > 8 }

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.index_of("is_special"));

print(x.index_of("is_dumb"));

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.index_of(|v| v > 3));

print(x.index_of(|v| v > 8));

print(x.index_of(|v, i| v * i > 20));

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.index_of(4));

print(x.index_of(9));

print(x.index_of("foo"));

let text = "hello, world!";

print(text.index_of('l'));

print(text.index_of('x'));

let text = "hello, world! hello, foobar!";

print(text.index_of("ll"));

print(text.index_of("xx:));

fn plural(x) { x > 1 }

fn singular(x) { x < 2 }

fn screen(x, i) { x * i > 20 }

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.index_of("plural", 3));

print(x.index_of("singular", 9));

print(x.index_of("plural", 15));

print(x.index_of("plural", -5));

print(x.index_of("plural", -99));

print(x.index_of("screen", 8));

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.index_of(|v| v > 1, 3));

print(x.index_of(|v| v < 2, 9));

print(x.index_of(|v| v > 1, 15));

print(x.index_of(|v| v > 1, -5));

print(x.index_of(|v| v > 1, -99));

print(x.index_of(|v, i| v * i > 20, 8));

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.index_of(4, 2));

print(x.index_of(4, 5));

print(x.index_of(4, 15));

print(x.index_of(4, -5));

print(x.index_of(9, 1));

print(x.index_of("foo", 1));

let text = "hello, world!";

print(text.index_of('l', 5));

print(text.index_of('o', -7));

print(text.index_of('x', 0));

let text = "hello, world! hello, foobar!";

print(text.index_of("ll", 5));

print(text.index_of("ll", -15));

print(text.index_of("xx", 0));

let x = [1, 2, 3];

x.insert(0, "hello");

x.insert(2, true);

x.insert(-2, 42);

print(x);

let b = blob(5, 0x42);

b.insert(2, 0x18);

print(b);

let f = |x| x * 2;

print(f.is_anonymous);

let m = #{a:1, b:2, c:3};

print(m.keys());

let b = blob(10, 0x42);

print(b);

print(b.len());

let text = "朝には紅顔ありて夕べには白骨となる";

print(text.len);

let ch = 'A';

ch.make_lower();

print(ch);

let text = "HELLO, WORLD!"

text.make_lower();

print(text);

let ch = 'a';

ch.make_upper();

print(ch);

let text = "hello, world!"

text.make_upper();

print(text);

fn square(x) { x * x }

fn multiply(x, i) { x * i }

let x = [1, 2, 3, 4, 5];

let y = x.map("square");

print(y);

let y = x.map("multiply");

print(y);

let x = [1, 2, 3, 4, 5];

let y = x.map(|v| v * v);

print(y);

let y = x.map(|v, i| v * i);

print(y);

let m = #{a:1, b:2, c:3};
let n = #{a: 42, d:0};

m.mixin(n);

print(m);

fn double(x) { x * 2 }

let f = Fn("double");

print(f.name);

let x = [1, 2, 3];

x.pad(5, 42);

print(x);

x.pad(3, 123);

print(x);

let b = blob(3, 0x42);

b.pad(5, 0x18)

print(b);

b.pad(3, 0xab)

print(b);

let text = "hello";

text.pad(8, '!');

print(text);

text.pad(5, '*');

print(text);

let text = "hello";

text.pad(10, "(!)");

print(text);

text.pad(8, '***');

print(text);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

let x = b.parse_be_int(1..3);

print(x.to_hex());

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

let x = b.parse_be_int(1..=3);

print(x.to_hex());

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

let x = b.parse_be_int(1, 2);

print(x.to_hex());

let x = parse_decimal("123.456");

print(x);

let x = parse_int("123.456");

print(x);

let x = parse_int("123");

print(x);

let x = parse_int("123");

print(x);

let y = parse_int("123abc", 16);

print(y);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

let x = b.parse_le_int(1..3);

print(x.to_hex());

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

let x = b.parse_le_int(1..=3);

print(x.to_hex());

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

let x = b.parse_le_int(1, 2);

print(x.to_hex());

let x = [1, 2, 3];

print(x.pop());

print(x);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

print(b.pop());

print(b);

let text = "hello, world!";

print(text.pop());

print(text);

let text = "hello, world!";

print(text.pop(4));

print(text);

let x = [1, 2, 3];

x.push("hello");

print(x);

let b = blob();

b.push(0x42);

print(b);


for n in range(8, 18) {
    print(n);
}


for n in range(8, 18) {
    print(n);
}


for n in range(8, 18) {
    print(n);
}


for n in range(8, 18) {
    print(n);
}


for n in range(8, 18) {
    print(n);
}


for n in range(8, 18) {
    print(n);
}


for n in range(8, 18) {
    print(n);
}


for n in range(8, 18) {
    print(n);
}


for n in range(8, 18) {
    print(n);
}


for n in range(8, 18) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8..18, 3) {
    print(n);
}


for n in range(18..8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}


for n in range(8, 18, 3) {
    print(n);
}


for n in range(18, 8, -3) {
    print(n);
}

fn process(r, x) {
    x + (r ?? 0)
}
fn process_extra(r, x, i) {
    x + i + (r ?? 0)
}

let x = [1, 2, 3, 4, 5];

let y = x.reduce("process");

print(y);

let y = x.reduce("process_extra");

print(y);

let x = [1, 2, 3, 4, 5];

let y = x.reduce(|r, v| v + (r ?? 0));

print(y);

let y = x.reduce(|r, v, i| v + i + (r ?? 0));

print(y);

fn process(r, x) { x + r }

fn process_extra(r, x, i) { x + i + r }

let x = [1, 2, 3, 4, 5];

let y = x.reduce("process", 5);

print(y);

let y = x.reduce("process_extra", 5);

print(y);

let x = [1, 2, 3, 4, 5];

let y = x.reduce(|r, v| v + r, 5);

print(y);

let y = x.reduce(|r, v, i| v + i + r, 5);

print(y);

fn process(r, x) {
    x + (r ?? 0)
}
fn process_extra(r, x, i) {
    x + i + (r ?? 0)
}

let x = [1, 2, 3, 4, 5];

let y = x.reduce_rev("process");

print(y);

let y = x.reduce_rev("process_extra");

print(y);

let x = [1, 2, 3, 4, 5];

let y = x.reduce_rev(|r, v| v + (r ?? 0));

print(y);

let y = x.reduce_rev(|r, v, i| v + i + (r ?? 0));

print(y);

fn process(r, x) { x + r }

fn process_extra(r, x, i) { x + i + r }

let x = [1, 2, 3, 4, 5];

let y = x.reduce_rev("process", 5);

print(y);

let y = x.reduce_rev("process_extra", 5);

print(y);

let x = [1, 2, 3, 4, 5];

let y = x.reduce_rev(|r, v| v + r, 5);

print(y);

let y = x.reduce_rev(|r, v, i| v + i + r, 5);

print(y);

let x = [1, 2, 3];

print(x.remove(1));

print(x);

print(x.remove(-2));

print(x);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

print(x.remove(1));

print(x);

print(x.remove(-2));

print(x);

let m = #{a:1, b:2, c:3};

let x = m.remove("b");

print(x);

print(m);

let text = "hello, world! hello, foobar!";

text.remove("o");

print(text);

let text = "hello, world! hello, foobar!";

text.remove("hello");

print(text);

let text = "hello, world! hello, foobar!";

text.replace("l", '*');

print(text);

let text = "hello, world! hello, foobar!";

text.replace('l', "(^)");

print(text);

let text = "hello, world! hello, foobar!";

text.replace("hello", '*');

print(text);

let text = "hello, world! hello, foobar!";

text.replace("hello", "hey");

print(text);

fn large(x) { x >= 3 }

fn screen(x, i) { x + i <= 5 }

let x = [1, 2, 3, 4, 5];

let y = x.retain("large");

print(x);

print(y);

let z = x.retain("screen");

print(x);

print(z);

let x = [1, 2, 3, 4, 5];

let y = x.retain(|v| v >= 3);

print(x);

print(y);

let z = x.retain(|v, i| v + i <= 5);

print(x);

print(z);

let x = [1, 2, 3, 4, 5];

let y = x.retain(1..4);

print(x);

print(y);

let z = x.retain(1..3);

print(x);

print(z);

let x = [1, 2, 3, 4, 5];

let y = x.retain(1..=3);

print(x);

print(y);

let z = x.retain(1..=2);

print(x);

print(z);

let b1 = blob();

b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;

let b2 = b1.retain(1..4);

print(b1);

print(b2);

let b3 = b1.retain(1..3);

print(b1);

print(b2);

let b1 = blob();

b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;

let b2 = b1.retain(1..=3);

print(b1);

print(b2);

let b3 = b1.retain(1..=2);

print(b1);

print(b2);

let x = [1, 2, 3, 4, 5];

let y = x.retain(1, 2);

print(x);

print(y);

let z = x.retain(-1, 1);

print(x);

print(z);

let b1 = blob();

b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;

let b2 = b1.retain(1, 2);

print(b1);

print(b2);

let b3 = b1.retain(-1, 1);

print(b1);

print(b3);

let x = [1, 2, 3, 4, 5];

x.reverse();

print(x);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

print(b);

b.reverse();

print(b);

let x = [1, 2, 3];

x.set(0, 42);

print(x);

x.set(-3, 0);

print(x);

x.set(99, 123);

print(x);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

b.set(0, 0x42);

print(b);

b.set(-3, 0);

print(b);

b.set(99, 123);

print(b);

let m = #{a: 1, b: 2, c: 3};

m.set("b", 42)'

print(m);

x.set("x", 0);

print(m);

let text = "hello, world!";

text.set(3, 'x');

print(text);

text.set(-3, 'x');

print(text);

text.set(99, 'x');

print(text);

let x = "hello, world!";

x.tag = 42;

print(x.tag);

let x = 123456;

x.set_bit(5, true);

print(x);

x.set_bit(6, false);

print(x);

x.set_bit(-48, false);

print(x);

let x = 123456;

x.set_bits(5..10, 42);

print(x);

let x = 123456;

x.set_bits(5..=9, 42);

print(x);

let x = 123456;

x.set_bits(5, 8, 42);

print(x);

x.set_bits(-16, 10, 42);

print(x);

let x = "hello, world!";

x.tag = 42;

print(x.tag);

let x = [1, 2, 3];

print(x.shift());

print(x);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

print(b.shift());

print(b);

fn large(x) { x > 3 }

fn huge(x) { x > 10 }

fn screen(x, i) { i > x }

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.some("large"));

print(x.some("huge"));

print(x.some("screen"));

let x = [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 5];

print(x.some(|v| v > 3));

print(x.some(|v| v > 10));

print(x.some(|v, i| i > v));

let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];

x.sort();

print(x);

fn reverse(a, b) {
    if a > b {
        -1
    } else if a < b {
        1
    } else {
        0
    }
}
let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];

x.sort("reverse");

print(x);

let x = [1, 3, 5, 7, 9, 2, 4, 6, 8, 10];


x.sort(|a, b| if a > b { -1 } else if a < b { 1 } else { 0 });

print(x);

let x = [1, 2, 3, 4, 5];
let y = [7, 8, 9, 10];

x.splice(1..3, y);

print(x);

let x = [1, 2, 3, 4, 5];
let y = [7, 8, 9, 10];

x.splice(1..=3, y);

print(x);

let b1 = blob(10, 0x42);
let b2 = blob(5, 0x18);

b1.splice(1..4, b2);

print(b1);

let b1 = blob(10, 0x42);
let b2 = blob(5, 0x18);

b1.splice(1..=4, b2);

print(b1);

let x = [1, 2, 3, 4, 5];
let y = [7, 8, 9, 10];

x.splice(1, 2, y);

print(x);

x.splice(-5, 4, y);

print(x);

let b1 = blob(10, 0x42);
let b2 = blob(5, 0x18);

b1.splice(1, 3, b2);

print(b1);

b1.splice(-5, 4, b2);

print(b1);

let text = "hello, world! hello, foo!";

print(text.split());

let x = [1, 2, 3, 4, 5];

let y = x.split(2);

print(y);

print(x);

let b1 = blob();

b1 += 1; b1 += 2; b1 += 3; b1 += 4; b1 += 5;

let b2 = b1.split(2);

print(b2);

print(b1);

let text = "hello, world!";

print(text.split(6));

print(text.split(13));

print(text.split(-6));

print(text.split(-99));

let text = "hello, world! hello, foo!";

print(text.split("ll"));

let text = "hello, world! hello, foo!";

print(text.split('l'));

let text = "hello, world! hello, foo!";

print(text.split("ll", 2));

let text = "hello, world! hello, foo!";

print(text.split('l', 3));

let text = "hello, world! hello, foo!";

print(text.split_rev("ll"));

let text = "hello, world! hello, foo!";

print(text.split_rev('l'));

let text = "hello, world! hello, foo!";

print(text.split_rev("ll", 2));

let text = "hello, world! hello, foo!";

print(text.split('l', 3));

let text = "hello, world!";

print(text.starts_with("hello"));

print(text.starts_with("world"));

let text = "hello, world!";

print(text.sub_string(3..7));

let text = "hello, world!";

print(text.sub_string(3..=7));

let text = "hello, world!";

print(text.sub_string(5));

print(text.sub_string(-5));

let text = "hello, world!";

print(text.sub_string(3, 4));

print(text.sub_string(-8, 3));

let x = "hello, world!";

x.tag = 42;

print(x.tag);

let now = timestamp();

sleep(10.0);

print(now.elapsed);

let b = blob(5, 0x42);

let x = b.to_array();

print(x);

let text = "朝には紅顔ありて夕べには白骨となる";

let bytes = text.to_blob();

print(bytes.len());

let text = "hello";

print(text.to_chars());

let m = #{a:1, b:2, c:3};

print(m.to_json());

let ch = 'A';

print(ch.to_lower());

print(ch);

let text = "HELLO, WORLD!"

print(text.to_lower());

print(text);

let ch = 'a';

print(ch.to_upper());

print(ch);

let text = "hello, world!"

print(text.to_upper());

print(text);

let text = "   hello     ";

text.trim();

print(text);

let x = [1, 2, 3, 4, 5];

x.truncate(3);

print(x);

x.truncate(10);

print(x);

let b = blob();

b += 1; b += 2; b += 3; b += 4; b += 5;

b.truncate(3);

print(b);

b.truncate(10);

print(b);

let text = "hello, world! hello, foobar!";

text.truncate(13);

print(text);

x.truncate(10);

print(text);

let m = #{a:1, b:2, c:3};

print(m.values());

let b = blob(8);

b.write_ascii(1..5, "hello, world!");

print(b);

let b = blob(8);

b.write_ascii(1..=5, "hello, world!");

print(b);

let b = blob(8);

b.write_ascii(1, 5, "hello, world!");

print(b);

let b = blob(8, 0x42);

b.write_be_int(1..3, 0x99);

print(b);

let b = blob(8, 0x42);

b.write_be_int(1..=3, 0x99);

print(b);

let b = blob(8, 0x42);

b.write_be_int(1, 3, 0x99);

print(b);

let b = blob(8);

b.write_le_int(1..3, 0x12345678);

print(b);

let b = blob(8);

b.write_le_int(1..=3, 0x12345678);

print(b);

let b = blob(8);

b.write_le_int(1, 3, 0x12345678);

print(b);

let b = blob(8);

b.write_utf8(1..5, "朝には紅顔ありて夕べには白骨となる");

print(b);

let b = blob(8);

b.write_utf8(1..=5, "朝には紅顔ありて夕べには白骨となる");

print(b);

let b = blob(8);

b.write_utf8(1, 5, "朝には紅顔ありて夕べには白骨となる");

print(b);
