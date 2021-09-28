// This script calculates the n-th Fibonacci number using a really dumb algorithm
// to test the speed of the scripting engine.

const TARGET = 28;
const REPEAT = 5;

fn fib(n) {
    if n < 2 {
        n
    } else {
        fib(n-1) + fib(n-2)
    }
}

print(`Running Fibonacci(${TARGET}) x ${REPEAT} times...`);
print("Ready... Go!");

let result;
let now = timestamp();

for n in range(0, REPEAT) {
    result = fib(TARGET);
}

print(`Finished. Run time = ${now.elapsed} seconds.`);

print(`Fibonacci number #${TARGET} = ${result}`);

if result != 317_811 {
    print("The answer is WRONG! Should be 317,811!");
}
