// The script behind `tests/fixtures/golden.rgrn`. See
// `an_artifact_written_by_an_older_build_still_runs` in `tests/format.rs`.
//
// Deliberately wide rather than deliberately short: every encoder branch this
// reaches is a branch the golden artifact pins against silent drift. Editing it
// means regenerating the artifact, which is the point — the pair only proves
// anything while the artifact is older than the reader.
//
// No `switch`. A switch table carries hashes from rhai's per-process hasher
// seed, so an artifact holding one is refused by any process that did not set
// the same seed (`format/write.rs:237`). That is right for a real artifact and
// useless for a fixture, which has to load anywhere.

fn tally(items, bias) {
    let total = bias;
    for item in items {
        total += item;
    }
    total
}

const STRIDE = 2;
let counts = [1, 2, 3];
let label = "measure";
let ratio = 2.5;
let flags = #{ on: true, off: false, mark: 'x' };
// Not all-constant, so the optimizer leaves it to be built at run time.
let computed = [STRIDE, counts.len(), ratio.to_int()];

// A chain rooted at a slot, one on a temporary, and one on the name the caller
// supplied — the three `Root` variants.
counts.push(tally(counts, 10));
let widest = [9, 8].len();
supplied.push(counts.len());

// The function-call spelling of the same method, which takes its first
// argument by reference — once over a slot and once over the caller's name,
// where the receiver is resolved after the other argument and moved back under
// it.
push(counts, ratio.to_int());
push(supplied, STRIDE);

// A closure, which shares what it captures and curries it onto a pointer. Made
// and spent inside a block, so the scope this is compared on does not end up
// holding a pointer — the two sides render one differently on purpose, which
// `a_closure_pointer_is_late_bound` in `tests/scope.rs` pins.
let captured = 0;
{
    let capture = || supplied.len() + counts.len();
    captured = capture.call();
}

let doubled = 0;
let i = 0;
while i < counts.len() {
    doubled += counts[i];
    i += 1;
}

let caught = ();
try {
    if doubled > 0 || flags.off {
        throw "thrown";
    }
    caught = counts[99];
} catch (err) {
    caught = err;
}

let squares = [];
for n in 0..8 {
    if n > 3 {
        break;
    }
    squares.push(n * STRIDE);
}

#{
    total: doubled,
    text: `${label} of ${counts.len()}`,
    ratio: ratio * 2.0,
    flag: flags.on,
    mark: flags.mark,
    supplied: supplied.len(),
    widest: widest,
    caught: caught,
    captured: captured,
    computed: computed,
    squares: squares,
    head: label[0..2],
    tail: label[1..=3],
}
