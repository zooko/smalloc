The `smalloc` [core code](./smalloc/) was written almost entirely by me, a human. I started dreaming
about it and then coding it back in the dark ages before AI was capable of this kind of coding, and
in the last year or so I started asking AIs more and more questions like "How do Rust const fns
work", "How does Rust build and package tooling work", "What asm instructions result from shifts by
a constant on arm64", "Can these memory ordering constraints be loosened", and more and more. So I
was mainly just using AI as a tutor. I also got an AI to give me a line or two of code such as the
`gen_mask!`  macro, but basically I understand every line of code in the core package and could code
it again from scratch if I had to. Same for the [transparent-box tests](./smalloc/src/tests.rs) and
the [opaque-box tests](./tests/integration.rs)

I got some help on debugging a couple of times by talking to an AI, although it made mistakes that I
caught more often than the other way around. At one point I had an AI write a script to parse a log
of debug printouts and calculate where the inconsistency started, which resulted in [this bug
fix](https://github.com/zooko/smalloc/commit/69bf5d8f01e5c91af446dd06de1084af78d61105) (that I wrote
by hand myself).

The tooling and infrastructure around it is a different story. Once I wanted to generalize [the
benchmarks](./bench) to measure alternative memory allocators like `snmalloc`, `rpmalloc`,
`mimalloc`, and `jemalloc`, I started having to use Rust macros, which I understand and can read and
debug much less that normal Rust code. I started relying on AI (especially Claude Open 4.5) more and
more to extend and debug the benchmarks.

I also had AI write entire scripts for benchmarking such as
https://github.com/zooko/simd-json/blob/main/critcmp.py .

With [the ffi](./smalloc-ffi), I cut and pasted big pieces of code from AI, although I checked all
the work, and I understand it pretty well. It felt more like ["AI
piloting"](https://x.com/hosseeb/status/2010837906865430835) than "vibe coding" because I was taking
responsibility for being sure the results were safe for users. (I was less concerned about possible
bugs in the benchmarks, which couldn't really harm users, than bugs in the core or the ffi.)

I tried to leave records of when I was using AI-generated information (code or just information) in
the comments/docs and git commit messages.

Anyway, the core code is more or less an artifact of artisanal human-crafted source code. Perhaps it
will be one of the last such!
