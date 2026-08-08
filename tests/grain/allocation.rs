//! What the AST costs, and what the VM costs instead.
//!
//! This is the instrument the project's premise rests on. Rhai's tree costs
//! ~24 bytes of device heap per minified source byte, and traffic-light's
//! `firmware/src/script.rs:88-92` records something the retained figure misses:
//! "rhai's parser allocates well past the size of the tree it finally keeps, so
//! the peak during `compile` is what has to fit, not the result." So this
//! tracks peak as well as retained — traffic-light's own harness tracks only
//! retained, and peak is the number that actually bounds script size.
//!
//! # Why this is one test in its own file
//!
//! The counters are process-global, and cargo runs tests on parallel threads.
//! Any concurrent allocation lands in the same counters and corrupts every
//! reading. One `#[test]`, in its own binary, is what keeps the numbers real.
//! traffic-light learned the same thing (commit bee5fb5, "Fold the allocation
//! measurement into the one test that owns the counters").
//!
//! # What these numbers are not
//!
//! Host figures. They do not transfer to a device by halving: traffic-light
//! measured the engine ratio at 0.71 rather than 0.5, and found the AST does
//! not shrink on 32-bit at all, because `Dynamic`, `i64` and `f32` fields are
//! the same width either way. Device numbers have to come from a device.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

use rhai::grain::Compiler;
use rhai::Engine;

static LIVE: AtomicIsize = AtomicIsize::new(0);
static COUNT: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

/// Live allocations by size class, `SIZE_CLASSES[i-1] < size <= SIZE_CLASSES[i]`.
/// The small classes are what a per-allocation header punishes, and an AST is
/// mostly small classes.
static BUCKETS: [AtomicIsize; 6] = [AtomicIsize::new(0), AtomicIsize::new(0), AtomicIsize::new(0), AtomicIsize::new(0), AtomicIsize::new(0), AtomicIsize::new(0)];
const SIZE_CLASSES: [usize; 6] = [8, 16, 32, 64, 256, usize::MAX];

fn bucket_of(size: usize) -> usize {
    SIZE_CLASSES.iter().position(|&c| size <= c).unwrap_or(5)
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let now = LIVE.fetch_add(l.size() as isize, Ordering::Relaxed) + l.size() as isize;
        PEAK.fetch_max(now, Ordering::Relaxed);
        COUNT.fetch_add(1, Ordering::Relaxed);
        BUCKETS[bucket_of(l.size())].fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as isize, Ordering::Relaxed);
        COUNT.fetch_sub(1, Ordering::Relaxed);
        BUCKETS[bucket_of(l.size())].fetch_sub(1, Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let now = LIVE.fetch_add(new as isize - l.size() as isize, Ordering::Relaxed) + new as isize - l.size() as isize;
        PEAK.fetch_max(now, Ordering::Relaxed);
        BUCKETS[bucket_of(l.size())].fetch_sub(1, Ordering::Relaxed);
        BUCKETS[bucket_of(new)].fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, new) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn live() -> isize {
    LIVE.load(Ordering::Relaxed)
}

fn count() -> isize {
    COUNT.load(Ordering::Relaxed)
}

fn buckets() -> [isize; 6] {
    std::array::from_fn(|i| BUCKETS[i].load(Ordering::Relaxed))
}

/// What producing `T` cost, and what it goes on costing while held.
struct Measured<T> {
    value: T,
    /// Still-live bytes attributable to the returned value.
    bytes: isize,
    /// Still-live allocations attributable to the returned value.
    count: isize,
    /// High-water mark during the call. Exceeds `bytes` by whatever the
    /// producer allocated and freed along the way — for rhai's parser, a lot.
    peak: isize,
    /// Still-live allocations by size class.
    buckets: [isize; 6],
}

fn measure<T>(f: impl FnOnce() -> T) -> Measured<T> {
    let base_bytes = live();
    let base_count = count();
    let base_buckets = buckets();
    PEAK.store(base_bytes, Ordering::Relaxed);

    let value = f();

    let peak = PEAK.load(Ordering::Relaxed) - base_bytes;
    let after = buckets();

    Measured {
        bytes: live() - base_bytes,
        count: count() - base_count,
        peak,
        buckets: std::array::from_fn(|i| after[i] - base_buckets[i]),
        value,
    }
}

/// A synthetic control, shaped like a real script: helper functions, integer
/// and float arithmetic, arrays, and a loop driving them.
///
/// `FOLLOW` is the real one, and the figures that matter are measured against
/// it. This is here so the per-source-byte and peak/retained ratios have a
/// second shape to be read against.
const SCRIPT: &str = r#"
fn clamp(v, lo, hi) {
    if v < lo { lo } else if v > hi { hi } else { v }
}

fn ease(t) {
    let x = clamp(t, 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn blend(a, b, t) {
    let k = ease(t);
    (a * (1.0 - k) + b * k)
}

let channels = [0, 0, 0, 0, 0, 0, 0, 0];
let phase = 0.0;
let step = 0.125;

for frame in 0..64 {
    phase += step;
    if phase > 1.0 { phase -= 1.0; }

    for i in 0..channels.len {
        let target = if i % 2 == 0 { 255.0 } else { 64.0 };
        let level = blend(0.0, target, ease(phase));
        channels[i] = clamp(level.to_int(), 0, 255);
    }
}

channels
"#;

/// The script the premise is actually about — byte-identical to
/// traffic-light's `scripts/follow.rhai`, the one its 24 bytes/source-byte
/// figure was measured against.
const FOLLOW: &str = include_str!("fixtures/follow.rhai");

/// Something that lowers all the way, so it can be written and read back.
///
/// `follow.rhai` cannot yet: it still fragments on property reads and index
/// assignment, and a program holding a fragment refuses to serialize. Until
/// chained lvalues land, the load figures come from a script shaped like the
/// parts of it that do lower — locals, arithmetic, branches and a loop.
const LOADABLE: &str = r#"
let total = 0;
let phase = 0;
let step = 3;
let i = 0;
while i < 64 {
    phase += step;
    if phase > 100 { phase -= 100; }
    if i % 2 == 0 { total += phase * 2; } else { total -= phase; }
    i += 1;
}
total;
"#;

#[test]
fn allocation_footprint() {
    let engine = Engine::new();

    let engine_cost = measure(Engine::new);
    let ast = measure(|| engine.compile(SCRIPT).expect("script must compile"));
    let program = measure(|| Compiler::new().compile(&ast.value));

    let follow_ast = measure(|| engine.compile(FOLLOW).expect("follow.rhai must compile"));

    let source_bytes = SCRIPT.len() as isize;
    let per_source_byte = ast.bytes as f64 / source_bytes as f64;

    println!("\nsource                    {source_bytes} bytes");
    println!("\n{:<24} {:>10} {:>10} {:>10}", "", "bytes", "allocs", "peak");
    println!("{:<24} {:>10} {:>10} {:>10}", "Engine::new", engine_cost.bytes, engine_cost.count, engine_cost.peak);
    println!("{:<24} {:>10} {:>10} {:>10}", "engine.compile (AST)", ast.bytes, ast.count, ast.peak);
    println!("{:<24} {:>10} {:>10} {:>10}", "Compiler::compile", program.bytes, program.count, program.peak);

    // Not comparable to traffic-light's 24.0: that is a 32-bit device figure
    // measured against minified source, this is a host figure against
    // unminified source. Comparing them needs follow.rhai measured minified,
    // the way the server actually ships it.
    println!("\nAST bytes per source byte {per_source_byte:.1}  (host, unminified)");
    println!("AST parser peak / retained {:.2}x  (the peak is what has to fit)", ast.peak as f64 / ast.bytes as f64);

    println!("\nAST live allocations by size class");
    let labels = ["<=8", "<=16", "<=32", "<=64", "<=256", ">256"];
    for (label, n) in labels.iter().zip(ast.buckets) {
        println!("  {label:>6} {n:>8}");
    }

    // What the real script's tree costs, to set against what
    // tests/grain/projection.rs says a lowering of it would weigh.
    //
    // Larger than traffic-light's own 77968 for the same script because this
    // builds rhai with default features. Theirs sets `no_module`, which drops
    // a `Namespace` (an inline `StaticVec<Ident>` plus a hash) from every
    // `Expr::Variable` payload — and this script has 457 of them. A restricted
    // build is what 77968 should be compared against, not this one.
    println!("\nfollow.rhai — {} source bytes", FOLLOW.len());
    println!("{:<24} {:>10} {:>10} {:>10}", "engine.compile (AST)", follow_ast.bytes, follow_ast.count, follow_ast.peak);
    println!("{:<24} {:>10.1}", "bytes per source byte", follow_ast.bytes as f64 / FOLLOW.len() as f64);
    println!("{:<24} {:>10.2}x", "parser peak / retained", follow_ast.peak as f64 / follow_ast.bytes as f64);
    println!("\nfollow.rhai AST live allocations by size class");
    for (label, n) in labels.iter().zip(follow_ast.buckets) {
        println!("  {label:>6} {n:>8}");
    }

    // What loading an artifact costs, against parsing the same script.
    //
    // This is the claim the byte encoding exists for. `Program::read` borrows
    // its instructions from the buffer, so nothing it retains is proportional
    // to how long the script is — only to the distinct names and constants it
    // mentions. Parsing retains a node per node.
    // The measurement the whole project was for: the real script, the real
    // tree, and the artifact that replaces it.
    let follow_program = Compiler::new().compile(&follow_ast.value);
    assert_eq!(follow_program.residual_count(), 0, "follow.rhai must lower completely, or there is nothing to write",);
    let (follow_artifact, follow_table) = follow_program.write_stripped().expect("follow.rhai must be writable");
    let follow_loaded = measure(|| rhai::grain::Program::read(&follow_artifact).expect("must load"));

    println!(
        "\nfollow.rhai: {} source bytes\n  tree      {:>8} bytes retained, {:>5} allocs, {:>8} peak\n  \
         artifact  {:>8} bytes on the wire ({} of debug table kept off it)\n  \
         loaded    {:>8} bytes retained, {:>5} allocs",
        FOLLOW.len(),
        follow_ast.bytes,
        follow_ast.count,
        follow_ast.peak,
        follow_artifact.len(),
        follow_table.len(),
        follow_loaded.bytes,
        follow_loaded.count,
    );
    println!("  {:.1}x less retained, and no parser peak at all", follow_ast.bytes as f64 / follow_loaded.bytes as f64,);

    // Measured at several lengths, because the ratio at one length says almost
    // nothing. Repeating the same statements grows the instruction stream while
    // the set of distinct names stays put — which is exactly the shape the
    // claim is about, and the shape a real script has as it gets longer.
    println!("\nloading an artifact against parsing the same script");
    println!("{:>7} {:>9} {:>9} {:>9} {:>8} {:>7}", "source", "artifact", "tree", "loaded", "allocs", "ratio");

    let mut ratios = Vec::new();
    for repeats in [1usize, 4, 16] {
        let source = LOADABLE.repeat(repeats);

        let tree = measure(|| engine.compile(&source).expect("must compile"));
        let program = Compiler::new().compile(&tree.value);
        assert_eq!(program.residual_count(), 0, "LOADABLE must lower completely, or this measures the walker",);

        let (stripped, _) = program.write_stripped().expect("must be writable");
        let loaded = measure(|| rhai::grain::Program::read(&stripped).expect("must load"));

        // Borrowed, not copied: the code section contributes nothing at all to
        // what a load retains, which is the whole reason for the byte encoding.
        let code = loaded.value.code();
        assert!(
            code.as_ptr() >= stripped.as_ptr() && code.as_ptr() as usize <= stripped.as_ptr() as usize + stripped.len(),
            "the loaded chunk must point into the artifact, not into a copy of it",
        );

        let ratio = tree.bytes as f64 / loaded.bytes as f64;
        ratios.push(ratio);
        println!("{:>7} {:>9} {:>9} {:>9} {:>8} {:>6.1}x", source.len(), stripped.len(), tree.bytes, loaded.bytes, loaded.count, ratio,);
    }

    // The property, rather than a number that would need updating: what a tree
    // retains grows with the program, what a load retains does not. A ratio
    // that stopped climbing would mean something in the loader had started
    // scaling with length.
    assert!(ratios[2] > ratios[1] && ratios[1] > ratios[0], "the saving must grow with the program, got {ratios:?}",);

    // The instrument has to be working before anything can lean on it.
    assert!(ast.count > 0 && ast.bytes > 0, "the counters saw nothing; the global allocator is not installed",);
    assert!(ast.peak >= ast.bytes, "peak ({}) below retained ({}) means peak tracking is broken", ast.peak, ast.bytes,);

    // A lowered program retains its pools rather than a node per node, so it
    // costs a fraction of the tree it came from. A `Program` that cost about
    // what the tree cost would mean the script fell back to fragments, which
    // hold real `Expr` trees — the one case where compiling saves nothing.
    assert!(program.bytes > 0 && program.bytes < ast.bytes, "a lowered program must retain less than its tree, got {} against {}", program.bytes, ast.bytes,);

    drop(program);
    drop(ast);
    drop(engine_cost);
}
