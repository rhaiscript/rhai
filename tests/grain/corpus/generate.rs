// Random rhai scripts, for comparing the VM against the walker on programs
// nobody wrote.
//
// The hand-written corpus proves the constructs someone thought to test. This
// covers the combinations they did not: a `try` inside a `for` inside a
// `switch` arm whose subject is an indexed chain, and the several thousand
// other shapes of that kind. Every one is run both ways and the results
// compared, so a divergence names a script that reproduces it.
//
// Two constraints follow from `fuzz/fuzz_targets/generated.rs` pulling this
// file in with `include!` rather than depending on it: nothing may be imported
// beyond `std`, and the header is `//` rather than `//!`, because an included
// file cannot open with an inner attribute.
//
// What is deliberately not generated:
//
//   - Recursion. A function may only call one defined before it, so no script
//     can recurse. Running out of call levels is a limit both sides enforce
//     but not in lockstep, and it would drown real findings.
//   - A closure handed to a native. `a.map(|x| ..)` has a known divergence in
//     argument order for captures, pinned in `tests/callback.rs`. Closures are
//     generated and called directly instead.
//   - An unbounded loop. Every `while` counts a guard variable the body cannot
//     name, so it always terminates.

/// A source of choices: xorshift64*, optionally front-loaded with bytes
/// somebody else picked.
///
/// The bytes are for `cargo fuzz`. Every grammar decision takes one, so
/// flipping byte *k* of an input changes decision *k* and leaves the rest of
/// the script alone — which is the locality a coverage-guided fuzzer needs to
/// make progress. From a plain seed, or once the bytes run out, it is an
/// ordinary reproducible PRNG.
pub struct Rng {
    state: u64,
    choices: Vec<u8>,
    at: usize,
}

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            // A zero state is a fixed point for xorshift, and zero is exactly
            // the seed somebody reaches for first.
            state: if seed == 0 { 0x9e37_79b9_7f4a_7c15 } else { seed },
            choices: Vec::new(),
            at: 0,
        }
    }

    /// Draws from `choices` until they are used up, then from the PRNG.
    ///
    /// Running out is not a failure and not a truncation: a script that stops
    /// being fuzzer-directed part way through is still a valid script, and
    /// still worth comparing.
    #[must_use]
    pub fn from_bytes(choices: &[u8]) -> Self {
        let mut seed = 0u64;
        for byte in choices.iter().take(8) {
            seed = (seed << 8) | u64::from(*byte);
        }
        Self { choices: choices.to_vec(), at: 0, ..Self::new(seed) }
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        if let Some(byte) = self.choices.get(self.at) {
            self.at += 1;
            return usize::from(*byte) % bound;
        }
        (self.next() % bound as u64) as usize
    }

    fn one_of<'a, T>(&mut self, choices: &'a [T]) -> &'a T {
        &choices[self.below(choices.len())]
    }

    fn chance(&mut self, n: usize) -> bool {
        self.below(n) == 0
    }
}

/// How deep an expression may nest.
///
/// Rhai's parser has its own limit and rejects anything past it. Staying well
/// under means a rejected parse is a bug in this generator rather than the
/// expected outcome for half the corpus.
const MAX_DEPTH: usize = 3;

/// How deep blocks and control flow may nest, for the same reason.
///
/// Rhai counts statements towards the same budget and allows a function body
/// half of what it allows the top level, so this is what keeps a script inside
/// both. Scripts are wide rather than deep as a result, which is the right
/// trade: a `try` inside a `for` finds things, a `try` inside a `for` inside a
/// `while` inside an `if` mostly finds the parser.
const MAX_NESTING: usize = 2;

/// Operators whose *result* is a boolean whatever their operands turn out to
/// be, so the parser accepts one as a condition.
///
/// It type-checks that much statically: `if 5 { }` is a parse error, not a
/// runtime one. Comparisons are safe to build out of anything because only the
/// operands can be wrong, and that is a runtime disagreement worth generating.
const COMPARISONS: &[&str] = &["==", "!=", "<", "<=", ">", ">="];

/// Operators that are defined for at least one pair of generated types. A pair
/// they are not defined for is a runtime error both sides have to agree on,
/// which is worth generating rather than worth avoiding.
///
/// `&&` and `||` are not here — they demand boolean operands at parse time, so
/// they belong to [`Generator::condition`] and nowhere else.
const BINARY: &[&str] = &["+", "-", "*", "/", "%", "==", "!=", "<", "<=", ">", ">=", "&", "|"];

const METHODS: &[&str] = &["len", "to_string", "abs", "is_empty", "floor", "to_upper"];

/// Natives called in *function-call* style, with their arity.
///
/// Rhai rewrites `f(x, ..)` into `x.f(..)` when the first argument is a plain
/// variable, so `push(a, 2)` mutates `a` and `push([1], 2)` mutates nothing —
/// the same syntax, and only the receiver decides. Half of these mutate and
/// half do not, because whether the write lands is the question.
///
/// Nothing here can grow a value without bound: an argument reaching one of
/// these is an arbitrary generated integer, and `pad` would turn that into an
/// allocation rather than into a divergence.
const NATIVES: &[(&str, usize)] = &[("push", 2), ("insert", 3), ("remove", 2), ("truncate", 2), ("reverse", 1), ("clear", 1), ("pop", 1), ("shift", 1), ("len", 1), ("is_empty", 1), ("abs", 1), ("to_upper", 1)];

pub struct Generator {
    rng: Rng,
    /// Variables that can be named, innermost last.
    vars: Vec<String>,
    /// Functions defined so far, with their arity. Only these can be called,
    /// and only from after their definition, which is what rules out recursion.
    functions: Vec<(String, usize)>,
    /// Loop nesting, so `break` and `continue` appear only where they parse.
    loops: usize,
    depth: usize,
    /// Nesting of blocks and control flow, capped for the same reason `depth`
    /// is: rhai counts statements towards the same complexity limit, so a
    /// `while` inside an `if` inside a `for` runs out of budget on its own.
    nesting: usize,
    /// Set while generating the inside of an interpolated string, where
    /// another one cannot go — the lexer ends the outer string on the inner
    /// one's opening backtick.
    interpolating: bool,
    /// Supplies names that cannot collide with each other.
    counter: usize,
}

impl Generator {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rng: Rng::new(seed),
            vars: Vec::new(),
            functions: Vec::new(),
            loops: 0,
            depth: 0,
            nesting: 0,
            interpolating: false,
            counter: 0,
        }
    }

    /// A generator whose choices are made by somebody else's bytes, for
    /// `cargo fuzz`. See [`Rng::from_bytes`].
    #[must_use]
    pub fn from_bytes(choices: &[u8]) -> Self {
        Self { rng: Rng::from_bytes(choices), ..Self::new(0) }
    }

    fn name(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}{}", self.counter)
    }

    /// One whole script.
    pub fn script(&mut self) -> String {
        let mut out = String::new();

        // `no_function` has no syntax for one, so a script carrying any would
        // not parse and the run would compare nothing.
        #[cfg(not(feature = "no_function"))]
        for _ in 0..self.rng.below(3) {
            out.push_str(&self.function());
            out.push(' ');
        }

        // A scope of its own per script, so nothing leaks between them.
        self.vars.clear();
        for _ in 0..1 + self.rng.below(5) {
            out.push_str(&self.statement());
            out.push(' ');
        }

        // Ending on an expression gives the script a value, so a divergence in
        // what it computed shows up and not only a divergence in what it left
        // behind.
        out.push_str(&self.expression());
        out
    }

    fn function(&mut self) -> String {
        let name = self.name("f");
        let arity = self.rng.below(3);
        let params: Vec<String> = (0..arity).map(|i| format!("p{i}")).collect();

        // A body sees its parameters and nothing else, which is also what rhai
        // gives it.
        let outer = std::mem::replace(&mut self.vars, params.clone());
        let outer_loops = std::mem::replace(&mut self.loops, 0);
        // Rhai allows a function body half the expression depth it allows the
        // top level (`MAX_FUNCTION_EXPR_DEPTH`), so a body starts one level in.
        let outer_nesting = std::mem::replace(&mut self.nesting, 1);
        let mut body = String::new();
        for _ in 0..1 + self.rng.below(3) {
            body.push_str(&self.statement());
            body.push(' ');
        }
        body.push_str(&self.expression());
        self.vars = outer;
        self.loops = outer_loops;
        self.nesting = outer_nesting;

        // Registered after the body is generated, so the body cannot call it.
        self.functions.push((name.clone(), arity));
        format!("fn {name}({}) {{ {body} }}", params.join(", "))
    }

    fn statement(&mut self) -> String {
        // Weighted by hand: `let` more often than anything else, because a
        // script with no variables cannot exercise slots, chains or capture.
        // Anything that nests is dropped from the choices once deep enough,
        // which is why the arms are ordered with those last.
        let nesting_kinds = if self.nesting < MAX_NESTING { 5 } else { 0 };
        let loop_kinds = if self.loops > 0 { 2 } else { 0 };
        match self.rng.below(5 + nesting_kinds + loop_kinds) {
            0..=2 => {
                let value = self.expression();
                let name = self.name("v");
                self.vars.push(name.clone());
                format!("let {name} = {value};")
            }
            3 => self.assignment(),
            4 => format!("{};", self.expression()),

            // Nesting, reachable only while there is budget for it.
            5 => {
                let cond = self.condition();
                self.nesting += 1;
                let then = self.block();
                let otherwise = self.rng.chance(2).then(|| self.block());
                self.nesting -= 1;
                match otherwise {
                    Some(otherwise) => format!("if {cond} {then} else {otherwise}"),
                    None => format!("if {cond} {then}"),
                }
            }
            6 => self.nested(Self::bounded_while),
            7 => self.nested(Self::for_loop),
            8 => self.nested(Self::switch),
            9 => self.nested(Self::try_catch),

            // Only reachable inside a loop, per `loop_kinds` above.
            10 => "break;".to_string(),
            _ => "continue;".to_string(),
        }
    }

    fn nested(&mut self, build: fn(&mut Self) -> String) -> String {
        self.nesting += 1;
        let out = build(self);
        self.nesting -= 1;
        out
    }

    /// A run of statements, with the scope they declared popped afterwards
    /// exactly as the VM pops it.
    ///
    /// Returned without braces so a caller can append to it. Appending to a
    /// finished block by trimming its `}` back off is how a body that happens
    /// to end in one gets silently truncated.
    fn statements(&mut self) -> String {
        let mark = self.vars.len();
        let mut out = String::new();
        for _ in 0..1 + self.rng.below(2) {
            out.push_str(&self.statement());
            out.push(' ');
        }
        self.vars.truncate(mark);
        out
    }

    fn block(&mut self) -> String {
        format!("{{ {} }}", self.statements())
    }

    fn assignment(&mut self) -> String {
        let Some(target) = self.variable() else {
            return format!("{};", self.expression());
        };
        let op = self.rng.one_of(&["=", "+=", "-=", "*="]).to_string();
        format!("{target} {op} {};", self.expression())
    }

    /// `while`, with a guard the body cannot reach.
    ///
    /// The guard is not pushed onto `vars`, so nothing generated inside can
    /// assign to it and the loop always ends.
    fn bounded_while(&mut self) -> String {
        let guard = self.name("g");
        let limit = 1 + self.rng.below(4);
        self.loops += 1;
        let body = self.statements();
        self.loops -= 1;
        format!("let {guard} = 0; while {guard} < {limit} {{ {body} {guard} += 1; }}")
    }

    fn for_loop(&mut self) -> String {
        let item = self.name("it");
        // A range is the one iterable every build has; an array literal is not
        // syntax under `no_index`.
        #[cfg(not(feature = "no_index"))]
        let iterable = if self.rng.chance(2) { format!("0..{}", 1 + self.rng.below(4)) } else { self.array() };
        #[cfg(feature = "no_index")]
        let iterable = format!("0..{}", 1 + self.rng.below(4));

        self.vars.push(item.clone());
        self.loops += 1;
        let body = self.block();
        self.loops -= 1;
        self.vars.pop();

        format!("for {item} in {iterable} {body}")
    }

    fn switch(&mut self) -> String {
        let subject = self.expression();
        let mut arms = Vec::new();
        for value in 0..1 + self.rng.below(3) {
            arms.push(format!("{value} => {}", self.expression()));
        }
        if self.rng.chance(3) {
            arms.push(format!("0..={} => {}", 9, self.expression()));
        }
        arms.push(format!("_ => {}", self.expression()));
        format!("switch {subject} {{ {} }};", arms.join(", "))
    }

    fn try_catch(&mut self) -> String {
        let mut body = self.statements();
        // A `throw` inside, most of the time: rhai's optimizer replaces a `try`
        // whose body is pure with a plain block, so a pure body would test the
        // optimizer rather than the handler.
        if !self.rng.chance(3) {
            let thrown = self.expression();
            body.push_str(&format!("throw {thrown};"));
        }
        let catch = self.block();
        if self.rng.chance(2) {
            format!("try {{ {body} }} catch (e) {catch}")
        } else {
            format!("try {{ {body} }} catch {catch}")
        }
    }

    fn variable(&mut self) -> Option<String> {
        if self.vars.is_empty() {
            return None;
        }
        Some(self.vars[self.rng.below(self.vars.len())].clone())
    }

    /// An array literal, empty about a quarter of the time.
    ///
    /// Empty matters on its own: it contributes no size check, so nesting one
    /// inside a literal that does is what catches a running total being popped
    /// by the wrong literal.
    #[cfg(not(feature = "no_index"))]
    fn array(&mut self) -> String {
        let items: Vec<String> = (0..self.rng.below(4)).map(|_| self.element()).collect();
        format!("[{}]", items.join(", "))
    }

    #[cfg(not(feature = "no_object"))]
    fn map(&mut self) -> String {
        let entries: Vec<String> = (0..self.rng.below(4)).map(|n| format!("k{n}: {}", self.element())).collect();
        format!("#{{ {} }}", entries.join(", "))
    }

    /// An array or a map, whichever kind of literal this build has syntax for.
    #[cfg(not(all(feature = "no_index", feature = "no_object")))]
    fn container(&mut self) -> String {
        #[cfg(not(any(feature = "no_index", feature = "no_object")))]
        if self.rng.chance(2) {
            return self.array();
        }
        #[cfg(not(feature = "no_object"))]
        return self.map();
        #[cfg(feature = "no_object")]
        return self.array();
    }

    /// What goes inside a literal.
    ///
    /// Sometimes another literal, so nesting is reached — an all-constant one
    /// is folded away by the optimizer before the compiler sees it, and only a
    /// computed element keeps the literal alive to run time.
    #[cfg(not(all(feature = "no_index", feature = "no_object")))]
    fn element(&mut self) -> String {
        if self.depth < MAX_DEPTH && self.rng.chance(4) {
            self.depth += 1;
            let out = self.container();
            self.depth -= 1;
            return out;
        }
        self.atom()
    }

    /// A leaf: cheap, and always available however deep the nesting has got.
    fn atom(&mut self) -> String {
        match self.rng.below(8) {
            0..=2 => format!("{}", self.rng.below(64)),
            // A float literal is not syntax under `no_float` — rhai reads the
            // `.` as a property access — so the whole script would fail to
            // parse and test nothing.
            #[cfg(not(feature = "no_float"))]
            3 => format!("{}.{}", self.rng.below(8), self.rng.below(8)),
            4 => format!("\"s{}\"", self.rng.below(8)),
            5 => self.rng.one_of(&["true", "false"]).to_string(),
            6 => "'c'".to_string(),
            _ => self.variable().unwrap_or_else(|| "1".to_string()),
        }
    }

    /// Something the parser will accept where a boolean is required.
    fn condition(&mut self) -> String {
        if self.depth >= MAX_DEPTH {
            return self.rng.one_of(&["true", "false"]).to_string();
        }
        self.depth += 1;
        let out = match self.rng.below(8) {
            0 => "true".to_string(),
            1 => "false".to_string(),
            2 => format!("(!{})", self.condition()),
            3 => {
                let lhs = self.condition();
                let op = self.rng.one_of(&["&&", "||"]);
                format!("({lhs} {op} {})", self.condition())
            }
            _ => {
                let lhs = self.expression();
                let op = self.rng.one_of(COMPARISONS);
                format!("({lhs} {op} {})", self.expression())
            }
        };
        self.depth -= 1;
        out
    }

    pub fn expression(&mut self) -> String {
        if self.depth >= MAX_DEPTH {
            return self.atom();
        }
        self.depth += 1;
        let out = self.compound();
        self.depth -= 1;
        out
    }

    fn compound(&mut self) -> String {
        match self.rng.below(14) {
            0..=3 => self.atom(),
            4 | 5 => {
                let lhs = self.expression();
                let op = self.rng.one_of(BINARY);
                format!("({lhs} {op} {})", self.expression())
            }
            6 => format!("(-{})", self.expression()),
            // `!` takes a condition rather than any expression: rhai rejects
            // `!9` at parse time the same way it rejects `if 9`.
            7 => format!("(!{})", self.condition()),
            // A literal is not syntax without the type behind it, and the arm
            // falls through to the last one, as the float atom does.
            #[cfg(not(feature = "no_index"))]
            8 => self.array(),
            #[cfg(not(feature = "no_object"))]
            9 => self.map(),
            // A chain over each of the three `Root` variants: a declared
            // variable, a name that is declared nowhere — which is what a
            // caller would have supplied, and here resolves to nothing — and a
            // literal. Between them `no_index` and `no_object` remove every
            // step a chain can take, so there is no chain left to generate.
            #[cfg(not(all(feature = "no_index", feature = "no_object")))]
            10 => {
                // An array root, because the index step below needs one — a map
                // refuses a numeric index at parse time. Where there is no
                // array, there is no index step either.
                let root = match self.variable() {
                    _ if self.rng.chance(6) => "absent".to_string(),
                    Some(var) if self.rng.chance(2) => var,
                    #[cfg(not(feature = "no_index"))]
                    _ => self.array(),
                    #[cfg(feature = "no_index")]
                    _ => self.map(),
                };
                match self.rng.below(3) {
                    #[cfg(not(feature = "no_index"))]
                    0 => format!("{root}[{}]", self.rng.below(4)),
                    #[cfg(not(feature = "no_object"))]
                    1 => format!("{root}.{}()", self.rng.one_of(METHODS)),
                    #[cfg(not(feature = "no_object"))]
                    _ => format!("{root}.a"),
                    #[cfg(feature = "no_object")]
                    _ => format!("{root}[{}]", self.rng.below(4)),
                }
            }
            11 if !self.interpolating => {
                self.interpolating = true;
                let segment = self.expression();
                self.interpolating = false;
                format!("`i{}{{{segment}}}`", self.rng.below(8))
            }
            11 => self.atom(),
            12 => self.call(),
            // A closure, called directly. Handing one to a native is the shape
            // with the known divergence, so it is not generated. The anonymous
            // form is `fn` in disguise, so `no_function` takes it too.
            #[cfg(not(feature = "no_function"))]
            _ => {
                let param = self.name("c");
                self.vars.push(param.clone());
                let body = self.expression();
                self.vars.pop();
                // `.call` is method syntax; `no_object` leaves the function-call
                // form of the same native, which is what it rewrites to anyway.
                #[cfg(not(feature = "no_object"))]
                {
                    format!("(|{param}| {body}).call({})", self.atom())
                }
                #[cfg(feature = "no_object")]
                {
                    format!("call(|{param}| {body}, {})", self.atom())
                }
            }
            #[cfg(feature = "no_function")]
            _ => self.atom(),
        }
    }

    fn call(&mut self) -> String {
        if self.rng.chance(4) {
            return self.native_call();
        }
        if self.functions.is_empty() {
            return self.atom();
        }
        let (name, arity) = self.functions[self.rng.below(self.functions.len())].clone();
        let args: Vec<String> = (0..arity).map(|_| self.atom()).collect();
        format!("{name}({})", args.join(", "))
    }

    /// A native in function-call style, with a variable in first position
    /// wherever there is one to use — see [`NATIVES`].
    fn native_call(&mut self) -> String {
        let (name, arity) = NATIVES[self.rng.below(NATIVES.len())];
        // Falling back to an atom rather than a literal where there is no
        // variable: most of `NATIVES` will not resolve against one, and a
        // native that is not found is an error both sides give alike.
        #[cfg(not(feature = "no_index"))]
        let receiver = self.variable().unwrap_or_else(|| self.array());
        #[cfg(feature = "no_index")]
        let receiver = self.variable().unwrap_or_else(|| self.atom());
        let rest: Vec<String> = (1..arity).map(|_| self.atom()).collect();

        if rest.is_empty() {
            return format!("{name}({receiver})");
        }
        format!("{name}({receiver}, {})", rest.join(", "))
    }
}
