# smalloc -- a simple memory allocator

`smalloc` is suitable as a drop-in replacement for the glibc memory allocator, `jemalloc`,
`mimalloc`, `snmalloc`, `rpmalloc`, etc -- *except* for security-hardening features.

`smalloc` performs comparably or even better than those other memory managers, while being much
simpler. The current implementation is only 349 lines of Rust code. The other memory allocators
range from 2,509 lines of C code (`rpmalloc`) to 25,713 lines of C code (`jemalloc`).

Fewer lines of code means fewer bugs, and it also means more consistent and debuggable behavior.

# Caveats

No warranty. Use at your own risk.

`smalloc` doesn't have any features for hardening your process against exploitation of memory
management bugs.

# Performance

<a href="https://github.com/zooko/bench-allocators/blob/main/benchmark-results/AppleM4Max.darwin25/COMBINED-REPORT.md">
    <img src="https://raw.githubusercontent.com/zooko/bench-allocators/refs/heads/main/benchmark-results/AppleM4Max.darwin25/smalloc-mt.graph.svg" width="600">
</a>
    
(Click on the image.)

Benchmark results (difference in time elapsed — lower is better):

```text
Multi-Threaded smalloc vs others:
  smalloc vs default     : -89.7%
  smalloc vs jemalloc    : -98.8%
  smalloc vs snmalloc    : -91.3%
  smalloc vs mimalloc    : -84.0%
  smalloc vs rpmalloc    : -95.9%
```

See the [bench-allocators](https://github.com/zooko/bench-allocators/blob/main/README.md) repo for
more benchmark results and how to generate them yourself.

# Limitations

There are two limitations:

1. You can't allocate more than 2 GiB in a single `malloc()`. You can only have at most 192
   simultaneous allocations larger than 1 GiB, plus at most 448 simultaneous allocations larger than
   512 MiB, plus at most 960 simultaneous allocations larger than 256 MiB, plus at most 1,984
   simultaneous allocations larger than 128 MiB and so on (see `Figure 1` for details). If all of
   smalloc's slots are exhausted so that it cannot deliver a requested allocation, then it will
   return a null pointer.
   
   It would be possible to change smalloc to fall back to the default allocator or to `mmap` in that
   case (as some other memory allocators do), but that would result in performance degradation and
   possibly in less predictable failure modes. I want smalloc to have consistent performance and
   failure modes so I choose to return a null pointer in that case.

2. You can't instantiate more than one instance of `smalloc` in a single process.

If you run into either of these limitations in practice, please open an issue on the `smalloc`
github repository. It would be possible in theory to lift these limitations, but I'd like to know
more about the user's needs in practice before changing the code to do so.

# Usage in Rust Code

Add `smalloc` to your Cargo.toml by executing `cargo add smmalloc --rename smalloc`, then add this to your code:

```
use smalloc::Smalloc;
#[global_allocator]
static ALLOC: Smalloc = Smalloc::new();
```

That's it! There are no other features you could consider using, no other changes you need to make,
no configuration options, no tuning options, no nothing.

(Wait, why is the crate named `smmalloc` instead of `smalloc`? Because there was already a crate
named `smalloc` on crates.io and I couldn't bear to stop calling this code `smalloc` myself, because
I'm in love with it.)

# Usage in C/C++/native code

See [./smalloc-ffi/README.md](./smalloc-ffi/README.md).

# Tests

Tests are run using the `nextest` runner.

To install `nextest`:

```text
cargo install cargo-nextest
```

To run the tests:

```text
cargo nextest run
```

# Map of the Source Code

## Packages within the workspace

This workspace contains six packages:

 * _smalloc_: the core memory allocator. This package contains the only code you need to use smalloc
   as the global allocator in your Rust code.
 * _smalloc-ffi_: Foreign Function Interface to use smalloc from C/C++/native code.
 * _bench_: micro-benchmarking tool to measure latency of operations and compare to other memory
   allocators
 * _hellomalloc_: a sample app that shows how to make smalloc be the global allocator in Rust code
 * _find_max_vm_addresses_reservable_: a tool used in the development of smalloc to determine how much
   virtual address is allocatable on the current system
 * _devutils_: code used in both tests and benchmarks

## Organization of the core code

Within the smalloc package, there are four files:
 * _smalloc/src/lib.rs_: the core memory allocator
 * _smalloc/src/i/plat/mod.rs_: interface to the operating system's `mmap` or equivalent system call
   to reserve virtual address space

Those two files contain the only source code you are relying on if you use smalloc as the global
allocator in Rust.

 * _smalloc/src/tests.rs_: transparent-box tests that use internals of the core to test it
 * _smalloc/tests/integration.rs_: opaque-box tests that use only the public API
 
# How it works

## The Big Idea

`smalloc`'s big idea is that although touching memory (i.e. reading or writing a specific memory
location) imposes costs on the operating system's virtual memory subsystem, reserving virtual memory
address space does not. Virtual memory addresses are a free and near-limitless resource. Use that
big idea by reserving a huge swathe of virtual memory addresses that you will use only sparsely.
When allocating memory, this sparseness enables you to efficiently find an unoccupied space big
enough to hold the request. When de-allocating, this sparseness enables you to leverage information
encoded into the pointer itself (the pointer to be de-allocated), and minimize the need to look up
and compute upon additional information beyond that.

In addition, this sparseness allows the implementation to be simple in source code as well as
efficient in execution.

## Data model

### Slots, Slabs, and Size Classes

All memory managed by `smalloc` is organized into "slabs". A slab is a fixed-length array of
fixed-length "slots" of bytes. Every pointer returned by a call to `malloc()` or `realloc()` is a
pointer to the beginning of one of those slots, and that slot is used exclusively for that memory
allocation until it is `free()`'ed.

Each slab holds slots of a specific fixed length, called a "size class". Size class 2 contains
4-byte slots, size class 3 contains 8-byte slots, and so on with each size class having slots twice
as big as the size class before.

Within each size class there are 64 separate slabs holding slots of that size. (See below for why.)

Sizeclasses 0 and 1 are unused and the space reserved for them is repurposed to hold "free list
heads" (see below). Here is how the information is encoded into memory addresses of slots and of
bytes of data within a slot (memory addresses shown in binary notation).

```text
Figure 1: Memory layout of slots and slabs and free-list-heads

slabs
                                      slab sc   flh
                                      [sla][sc ][f]
   0   00000000000000000000000000000000000000000000       used for flh's

   1   unused

                  .- reserved for touched bit
        slab  sc  | slotnum                     data
  sc   [    ][   ]v[                          ][   ] slotsize slots slabs
  --   --------------------------------------------- -------- ----- -----
       [slab][sc ] [slotnum                      ][]
   2   000000000100000000000000000000000000000000000     2^ 2  2^31   2^6

       [slab][sc ] [slotnum                     ][d]
   3   000000000110000000000000000000000000000000000     2^ 3  2^30   2^6

       [slab][sc ] [slotnum                    ][da]
   4   000000001000000000000000000000000000000000000     2^ 4  2^29   2^6

       [slab][sc ] [slotnum                   ][dat]
   5   000000001010000000000000000000000000000000000     2^ 5  2^28   2^6

       [slab][sc ] [slotn                    ][data]
   6   000000001100000000000000000000000000000000000     2^ 6  2^27   2^6

       [slab][sc ] [slotnum                 ][data ]
   7   000000001110000000000000000000000000000000000     2^ 7  2^26   2^6

       [slab][sc ] [slotnum                ][data  ]
   8   000000010000000000000000000000000000000000000     2^ 8  2^25   2^6
   9                                                     2^ 9  2^24   2^6
  10                                                     2^10  2^23   2^6
  11                                                     2^11  2^22   2^6
  12                                                     2^12  2^21   2^6
  13                                                     2^13  2^20   2^6
  14                                                     2^14  2^19   2^6
  15                                                     2^15  2^18   2^6
  16                                                     2^16  2^17   2^6
  17                                                     2^17  2^16   2^6
  18                                                     2^18  2^15   2^6
  19                                                     2^19  2^14   2^6
  20                                                     2^20  2^13   2^6
  21                                                     2^21  2^12   2^6
  22                                                     2^22  2^11   2^6
  23                                                     2^23  2^10   2^6
  24                                                     2^24  2^ 9   2^6
  25                                                     2^25  2^ 8   2^6
  26                                                     2^26  2^ 7   2^6
  27                                                     2^27  2^ 6   2^6
  28                                                     2^28  2^ 5   2^6
 
       [slab][sc ] [sl][data                       ]
  29   000000111010000000000000000000000000000000000     2^29  2^ 4   2^6

       [slab][sc ] [s][data                        ]
  30   000000111100000000000000000000000000000000000     2^30  2^ 3   2^6

       [slab][sc ] [][data                         ]
  31   000000111110000000000000000000000000000000000     2^31  2^ 2   2^6
```

### Free-Lists

For each slab, there is a free list, which is a singly-linked list of slots that are not currently
in use (i.e. either they've never yet been `malloc()`'ed, or they've been `malloc()`'ed and then
subsequently `free()`'ed). When referring to a slot's fixed position within the slab, call that its
"slot number", and when referring to a slot's position within the free list (which can change over
time as slots get removed from and added to the free list), call that a "free list entry". A free
list entry contains the slot number of the next free list entry (or a sentinel value if there is no
next free list entry, i.e. this entry is the end of the free list).

For each slab there is one additional associated variable, which holds the slot number of the first
free list entry (or the sentinel value if there are no entries in the list). This variable is called
the "free-list head" and is abbreviated `flh`. The contents of the free list head is the only
additional information you need to read or write beside the information present in the pointers
themselves.

That's it! Those are all the data elements in `smalloc`.

## Algorithms, Simplified

Here is a first pass describing simplified versions of the algorithms. After you learn these simple
descriptions, keep reading for additional detail.

* `malloc()`

To allocate space, calculate the size class of the request. Now pick one of the slabs in that size
class (see below for how). Pop the head entry from the free list and return the pointer to that
slot.

* `free()`

Push the slot number of the slot to be freed -- the slot whose first byte is pointed to by the
pointer to be freed -- onto the free list of its slab.

* `realloc()`

If the requested new size (and alignment) requires a larger slot than the allocation's current slot,
then allocate a new slot (just like in `malloc()`, above). Then `memcpy()` the contents of the
current slot into the beginning of that new slot, deallocate the current slot (just like in
`free()`, above) and return the pointer to the new slot.

That's it! You could stop reading here and you'd have a basic knowledge of the design of `smalloc`.

## The Free Lists in More Detail

The `flh` for a given slab is either the sentinel value (meaning that the list is empty), or else it
contains the slot number of the slot which is the first entry in that slab's free list.

To pop the head entry off of the free list, set the `flh` to contain the slot number of the next
(second) entry instead of the first entry.

But where is the slot number of the next free list entry stored? The answer is: in the same space
where the data goes when the slot is in use! Each slot is either currently freed, meaning you can
use its space to hold the slot number of the next free list entry, or currently allocated, meaning
it is not in the free list and doesn't have any next free list entry.

(This is also why not to use size class 0 -- 1-byte slots -- or size class 1 -- 2-byte slots:
because you need at least 4 bytes in each slot to store the slot number of the next entry.)

This technique is known as an "intrusive free list". Thanks to Andrew Reece and Sam Smith, my
colleagues at Shielded Labs (makers of fine Zcash protocol upgrades), for explaining this to me.

So to satisfy a `malloc()` by popping the first entry from the free list, read the value from the
`flh`, which is the slot number of the first entry in the free list, and then read the *contents* of
that slot to get the slot number of the next entry. Overwrite the value in `flh` with the slot
number of that *next* entry and you're done popping the head of the free list.

```text
Figure 2: A free list pop

Before:
                                    .---------.     .---------.
                                .-> | entry a | --> | entry b |
                               /    '---------'     '---------'
                           .-----.
                           | flh |
                           '-----'

After:
                                    .---------.
  return to caller:             .-> | entry b |
        .---------.            /    '---------'
        | entry a |        .-----.
        '---------'        | flh |
                           '-----'
```
    
To push an slot onto the free list (in order to implement `free()`), you are given the pointer of
the memory allocation to be freed. Calculate from that pointer the size class, slab number, and slot
number. Read the `flh` to get the slot number of the current first entry, and set the contents of
*that* slot to contain the slot number of the slot to be pushed. Now update the `flh` to contain the
slot number of the slot to be pushed. That slot is now the new head entry of the free list, and the
previous first-entry in the free list is now its next-entry.

```text
Figure 3: A free list push

Before:
                                    .---------.
  passed in by caller:          .-> | entry c |
        .---------.            /    '---------'
        | entry d |        .-----.
        '---------'        | flh |
                           '-----'

After:
                                    .---------.     .---------.
                                .-> | entry d | --> | entry c |
                               /    '---------'     '---------'
                           .-----.
                           | flh |
                           '-----'
```
    
### Lazy Initialization of Next-Entries (and Other Things)

When popping a slot, you need to know if this is the first time it has ever been popped. If so, it
doesn't contain a next-entry slot number. Instead its next-entry will be the next slot in the slab.

So reserve one bit in the `flh` to indicate whether the entry that the `flh` points to has ever been
popped. Likewise, reserve one bit in each next-pointer to indicate whether the entry that it points
to has ever been popped. (This means we have only 31 bits instead of 32 bits to encode the slot
number for size class 2, reducing `smalloc` overall capacity by half.)

Now when popping a slot for the first time, there are two other things you need to do in addition to
initializing its next-entry slot number:

1. If the user code requested that the allocated memory be zeroed (`alloc_zeroed` in the Rust
   GlobalAlloc trait, `calloc` in the C/Unix API, etc.), and this is *not* the first time this slot
   has been popped, you need to write 0's into all of the slot's bytes.

2. On Windows, you have to "commit" a memory page before reading or writing any of its bytes. So on
   Windows, if this is the first time this slot has been popped, commit all of the memory pages that
   this slot covers.

## Thread-Safe `flh` Updates

To make `smalloc` behave correctly under multiprocessing, it is necessary and sufficient to perform
thread-safe updates to `flh`. Use a simple loop with atomic compare-and-exchange operations.

### To pop an entry from the free list:

1. Load the value from `flh` into a local variable/register, called `firstslotnum`. This is the slot
   number of the first entry in the free list ("entry a" in `Figure 2`).
2. If it is the sentinel value, meaning that the free list is empty, return. (See below about
   "Handling Overflows" for how this `malloc()` request will be handled in this case.)
3. Load the value from first entry into a local variable/register, called `nextslotnum`. This is the
   slot number of the next entry in the free list (i.e. the second free-list entry, "entry b" in
   `Figure 2`), or a sentinel value there is if none.
4. Atomically compare-and-exchange the value from `nextslotnum` into `flh` if `flh` still contains
   the value from `firstslotnum`.
5. If the compare-and-exchange failed (meaning the value of `flh` has changed since you read it in
   step 1), jump back to step 1.

Now you've thread-safely popped the head of the free list into `firstslotnum`.

### To push an entry onto the free list, where `newslotnum` is the number of the slot to push:

1. Load the value from `flh` (which is the slot number of "entry c" in `Figure 3`) into a local
   variable/register, `firstslotnum`.
2. Write that value into the slot with slot number `newslotnum` ("entry d").
3. Atomically compare-and-exchange the value from `newslotnum` into `flh` if `flh` still contains
   the value from `firstslotnum`.
4. If the compare-and-exchange failed (meaning that value of `flh` has changed since it was read in
   step 1), jump back to step 1.

Now you've thread-safely pushed `newslotnum` onto the free list.

### To prevent ABA errors in updates to the free list head

The test described above of whether the `flh` still contains its original value is actually not
enough to guarantee correctness under multithreading. The problem is that step 4 of the pop
algorithm above is assuming that if the `flh` still contains the original value, then it is valid to
write `nextslotnum` into `flh`, but it is possible that a series of pops and pushes happened on
another thread between your thread's step 1 and step 4, that resulted in the `flh` still containing
the original slotnum, but with that slot's next-entry pointing to a different slot than
`nextslotnum`. The way this could happen is if the original value got popped off, then another pop
occurred (removing `nextslotnum` from the free list entirely), then the original value got pushed
back on. In that case the `flh` would contain the original slot number, but that slot would have a
different next-entry.  This is a kind of "ABA problem".

```text
Figure 4: ABA problem

Before:
                                    .---------.     .---------.     .---------.
                                .-> | entry a | --> | entry b | --> | entry c |
                               /    '---------'     '---------'     '---------'
                           .-----.
                           | flh |
                           '-----'

After first pop:
                                    .---------.     .---------.
                                .-> | entry b | --> | entry c |
                               /    '---------'     '---------'
                           .-----.
                           | flh |
                           '-----'

After second pop:
                                    .---------.
                                .-> | entry c |
                               /    '---------'
                           .-----.
                           | flh |
                           '-----'

After push of a:
                                    .---------.     .---------.
                                .-> | entry a | --> | entry c |
                               /    '---------'     '---------'
                           .-----.
                           | flh |
                           '-----'
```
    
So to ensure that popping entry a should leave entry b as the new first entry, it is not enough to
check that entry a is still the current first entry, you also have to check that this ABA sequence
hasn't happened.

In order to do this, store a counter in the unused high-order 32 bits of the flh word. Increment
that counter each time you attempt a compare-and-exchange on a push (`free`). Now if there were any
pushes concurrently completed between step 1 of the pop algorithm on this thread and step 4, the
compare-and-exchange will fail.

Now you know the entire data model and almost all of the algorithms for `smalloc`! Read on for a few
more details.

## Separate Threads Use Separate Slabs

This is not necessary for correctness -- the algorithms described above are sufficient for
correctness. This is just a performance optimization. Arrange it so that (under typical usage
patterns), each active thread will use a different slab from the other active threads. This will
minimize `flh`-update collisions, and for slots small enough to pack into a cache line, this will
tend to increase "true-sharing" -- cache-line-sharing between multiple allocations accessed from the
same processor as each other.

To do this, define a global static variable named `GLOBAL_THREAD_NUM`, initialized to `0`. 

Give each thread a thread-local variable named `SLABNUM`. The first time `alloc()` is called from
within a given thread, use the atomic `fetch_add` operation to increment `GLOBAL_THREAD_NUM` and set
this thread's `SLABNUM` to the previous value of `GLOBAL_THREAD_NUM`.

Whenever allocating, allocate from the slab indicated by your thread's `SLABNUM`.

## Handling Overflows and Update-Collisions

Suppose the user calls `malloc()` and the slab (determined by the size class of the request and your
thread's `SLABNUM`) is exhausted, i.e. the free list is empty. This could happen only if there were
that many slots from that slab currently allocated.

Or, suppose the user calls `malloc()` and you encounter a free-list-head update collision, i.e. you
reach step 5 of the thread-safe algorithm for popping an entry from the free list (above).

In either of these cases, try allocating from a different slab in the same size class. If this
attempt, too, fails, for either of those two reasons, then try yet another different slab in the
same size class. If you've tried every slab in this size class, and they've all failed (whether due
to that slab being exhausted or due to encountering an `flh` update collision when trying to pop
from that slab's free list), then *if* at least one slab was exhausted, move to the next bigger size
class and continue trying. (Thanks to Nate Wilcox -- also my colleague at Shielded Labs -- for
suggesting this technique to me.) On the other hand, if none of the slabs were exhausted, then
continue cycling through them trying to allocate from one of them.

## Realloc Growers

Suppose the user calls `realloc()` and the new requested size is larger than the original
size. Allocations that get reallocated to larger sizes sometimes, in practice, get reallocated over
and over again to larger and larger sizes. Call any allocation that has gotten reallocated to a
larger size a "grower".

If the user calls `realloc()` asking for a new larger size, and the new size still fits within the
current slot that the data is already occupying, then just be lazy and consider this `realloc()` a
success and return the current pointer as the return value.

If the new requested size doesn't fit into the current slot, and the new requested size is small
enough that you could pack more than one of them into a virtual memory page (i.e. the new requested
size is <= 2048 bytes on Linux, or <= 8,192 bytes on Apple OS), then just return a slot of that
size.

If the new requested size is so large that you can't pack more than one of them into a virtual
memory page, then return a slot of a very large size. Currently that "very large size" is 4 MiB --
size class 22 -- because that is the largest size I can think of where I still optimistically hope
that this will not result in exhausting all of the larger slots. There are 261,568 slots in size
classes 22 and up. Also because when I profiled the memory usage of the Zcash "Zebra" server, I saw
that it often grew reallocations up to around 4 MiB -- I think it is processing blockchain blocks by
extending a vector as it receives more bytes of that block.

Why use a very large slot for this case? Think of the virtual memory space as a very long linear
address space -- stretched out in a line. If the allocation is too large to pack more than one of
them into a page, then there is no benefit to having the address of the allocation close to the
address of another allocation. Instead, you want their addresses far apart so that if the allocation
is subsequently grown by `realloc`, there will be plenty of room to grow without having to move to a
new starting adddress.

# Design Goals

Why `smalloc` is beautiful in my eyes.

If you accept the Big Idea that "avoiding reserving too much virtual address space" is not an
important goal for a memory manager, what *are* good goals? `smalloc` was designed with the
following goals, written here in roughly descending order of importance:

1. Be simple. This helps greatly to ensure correctness -- always a critical issue in
   computing. "Simplicity is the inevitable price that we must pay for correctness."--Tony Hoare
   (paraphrased)

   In addition to "correctness", simplicity also helps make the performance and the failure modes
   more consistent and debuggable, because there are fewer modes.

   Simplicity also facilitates making improvements to the codebase and learning from the codebase.

   I've tried to pay the price of keeping `smalloc` simple while designing and implementing it.

2. Place user data where it can benefit from caching.

   1. If a single CPU core accesses different allocations in quick succession, and those allocations
      are packed into a single cache line, then it can execute faster due to having the memory
      already in cache and not having to load it from main memory. This can make the difference
      between a few cycles when the data is already in cache versus tens or hundreds of cycles when
      it has to load it from main memory. (This is sometimes called "constructive interference" or
      "true sharing", to distinguish it from "destructive interference" or "false sharing" -- see
      below.)

   2. On the other hand, if multiple different CPU cores access different allocations in parallel,
      and the allocations are packed into the same cache line as each other, then this causes a
      substantial performance *degradation*, as the CPU has to stall the cores while propagating
      their accesses of the shared memory. This is called "false sharing" or "destructive cache
      interference". The magnitude of the performance impact is the similar to that of true sharing:
      false sharing can impose tens or hundreds of cycles of penalty on a single memory
      access. Worse, that penalty might recur over and over on subsequent accesses, depending on the
      data access patterns across cores.

   3. Suppose the program accesses multiple separate allocations in quick succession -- regardless
      of whether the accesses are by the same processor or from different processors. If the
      allocations are packed into the same memory page, this avoids potentially costly TLB cache
      misses and page faults. In the worst case, the kernel would have to load the data from swap,
      which could incur a performance penalty of hundreds of *thousands* of CPU cycles or even more,
      depending on the performance of the persistent storage. Additionally, faulting in a page of
      memory increases the pressure on the TLB cache and the swap subsystem, thus potentially
      causing a performance degradation for other processes running on the same system.

   Note that these three goals cannot be fully optimized by the memory manager, because they depend
   on how the user code accesses the memory. What `smalloc` does is use some simple heuristics
   intended to optimize the above goals under some reasonable assumptions about the behavior of the
   user code:

   1. Try to pack separate small allocations from a single thread together to optimize for
      (constructive) cache-line sharing.

   2. Place small allocations requested by separate threads in separate slabs, to minimize the risk
      of destructive ("false") cache-line sharing. This is heuristically assuming that successive
      allocations requested by a single thread are less likely to later be accessed simultaneously
      by multiple different threads. You can imagine user code which violates this assumption --
      having one thread allocate many small allocations and then handing them out to other
      threads/cores which then access them in parallel with one another. Under `smalloc`'s current
      design, this behavior could result in a lot of "destructive cache interference"/"false
      sharing". However, I can't think of a simple way to avoid this bad case without sacrificing
      the benefits of "constructive cache interference"/"true sharing" that we get by packing
      together allocations that then get accessed by the same core.

   3. When allocations are freed by the user code, `smalloc` pushes their slot to the front of a
      free list. When allocations are subsequently requested, the most recently free'd slots are
      returned first. This is a LIFO (stack) pattern, which means user code that tends to access its
      allocations in a stack-like way will enjoy improved caching. (Thanks to Andrew Reece from
      Shielded Labs for teaching me this.)

   4. The same strategies also tend to pack allocations together into pages of virtual memory.

3. Execute `malloc()`, `free()`, and `realloc()` as efficiently as possible. `smalloc` is great at
   this goal! The obvious reason for that is that the code implementing those three functions is
   *very simple* -- it needs to execute only a few CPU instructions to implement each of those
   functions.

   A perhaps less-obvious reason is that there is *minimal data-dependency* in those code paths.

   Think about how many loads of memory from different locations, and therefore
   potential-cache-misses, your process incurs to execute `malloc()` and then to write into the
   memory that `malloc()` returned. It has to be at least one, because you are eventually going to
   pay the cost of a potential-cache-miss to write into the memory that `malloc()` returned.

   To execute `smalloc`'s `malloc()` and then write into the resulting memory takes, in the common
   case, at most three cache misses.

   The main reason `smalloc` incurs so few potential-cache-misses in these code paths is the
   sparseness of the data layout. `smalloc` has pre-reserved a vast swathe of address space and
   "laid out" unique locations for all of its slabs, slots, and variables (but only virtually --
   "laying the locations out" in this way does not involve reading or writing any actual memory).
    
   Therefore, `smalloc` can calculate the location of a valid slab to serve this call to `malloc()`
   using only one or two data inputs: One, the requested size and alignment (which are on the stack
   in the function arguments and do not incur a potential-cache-miss) and two the slab number (which
   is in thread-local storage: one potential-cache-miss). Having computed the location of the slab,
   it can access the `flh` from that slab (one potential-cache-miss), at which point it has all the
   data it needs to compute the exact location of the resulting slot and to update the free
   list.

   For the implementation of `free()`, we need to use *only* the pointer to be freed (which is on
   the stack in an argument -- not a potential-cache-miss) in order to calculate the precise
   location of the slot and the slab to be freed. From there, it needs to access the `flh` for that
   slab (one potential-cache-miss).

   Why don't we have to pay the cost of one more potential-cache-miss to update the free list (in
   both `malloc()` and in `free()`)? It's due to the fact that the next free-list-pointer and the
   memory allocation occupy the same memory! (Although not at the same time.) Therefore, if the user
   code accesses the memory returned from `malloc()` after `malloc()` returns but before the cache
   line gets flushed from the cache, there is no additional cache-miss penalty from `malloc()`
   accessing it before returning. Likewise, if the user code has recently accessed the memory to be
   freed before calling `free()` on it, then `smalloc`'s access of the same space to store the next
   free-list pointer will incur no additional cache-miss. (Thanks to Sam Smith from Shielded Labs
   for telling me this.)

   So to sum up, here are the counts of the potential-cache-line misses for the common cases:

   1. To `malloc()` and then write into the resulting memory:
     * 🟠 one to access the thread's `SLABNUM`
     * 🟠 one to access the slab's `flh`
     * 🟠 one to access the intrusive free list entry
     * 🟢 no additional cache-miss for the user code to access the data

     For a total of 3 potential-cache-misses.

   2. To read from some memory and then `free()` it:
     * 🟠 one for the user code to read from the memory
     * 🟠 one to access the slab's `flh`
     * 🟢 no additional cache-miss for `free()` to access the intrusive free list entry

     For a total of 2 potential-cache-misses.

   3. To `free()` some memory without first reading it:
     * 🟢 no cache-miss for user code since it doesn't read the memory
     * 🟠 one to access the slab's `flh`
     * 🟠 one to access the intrusive free list entry

     For a total of 2 potential-cache-misses.

   Note that the above counts do not count a potential cache miss to access the base pointer. That's
   because the base pointer is fixed and shared -- every call by any thread to `malloc()`, `free()`,
   or `realloc()` accesses the base pointer, so it is more likely to be in cache.
   
   Similarly, for accessing the `SLABNUM`, if this thread has recently called `malloc()` then this
   thread's `SLABNUM` will likely already be in cache, but if this thread has not made such a call
   recently then it would likely cache-miss.
   
   And similarly for the potential cache-miss of accessing the `flh` -- if any thread using this
   slab has recently called `malloc()`, `free()`, or `realloc()` for an allocation of this size
   class, then the `flh` for this slab will already be in cache.

4. Be *consistently* efficient.

   I want to avoid unpredictable performance degradation, such as when your function takes little
   time to execute usually, but occasionally there is a latency spike when the function takes much
   longer to execute.

   I also want to minimize the number of scenarios in which `smalloc`'s performance degrades due to
   the user code's behavior triggering an "edge case" or a "worst case scenario" in `smalloc`'s
   design.
    
   The story sketched out above about user code allocating small allocations on one thread and then
   handing them out to other threads to access and potentially to `free()` is an example of how user
   code behavior could trigger a performance degradation in `smalloc`.

   On the bright side, I can't think of any *other* "worst case scenarios" for `smalloc` beyond that
   one. In particular, `smalloc` never has to "rebalance" or re-arrange its data structures, or do
   any "deferred accounting". This nicely eliminates some sources of intermittent performance
   degradation. See [this blog post](https://pwy.io/posts/mimalloc-cigarette/) and [this
   one](https://hackmd.io/sH315lO2RuicY-SEt7ynGA?view#jemalloc-purging-will-commence-in-ten-seconds)
   for cautionary tales of how some techniques can improve performance in the common case, but also
   occasionally degrade performance or cause confusing failure modes.

   There are no locks in `smalloc`. There are concurrent-update loops in `malloc` and `free` -- see
   the pseudo-code in "Thread-Safe State Changes" above -- but these are not locks. Whenever
   multiple threads are running that code, one of them will make progress (i.e. successfully update
   the `flh`) after only a few CPU cycles, regardless of what any other threads do. And, if any
   thread becomes suspended in that code, one of the *other*, still-running threads will be the one
   to make progress (update the `flh`). Therefore, these concurrent-update loops cannot cause a
   pile-up of threads waiting for a (possibly-suspended) thread to release a lock, nor can they
   suffer from priority inversion.

   For `malloc()` (but not for `free()`), if a thread experiences an update collision it will
   immediately switch over to a different slab, which will quickly avoid out any such contention
   unless all slabs are simultaneously occupied by more than one thread actively `malloc()`'ing or
   `free()`'ing.
   
   For `free()` it isn't possible to change slabs (the pointer to be freed needs to be pushed back
   onto this particular free list and no other), so multiple threads simultaneously attempting to
   free slots in the same slab is the worst-case-scenario for `smalloc`.

   See the benchmarks named `hs` (for "hotspot") and `fh` (for "free hotspot") for how `smalloc`
   currently performs in these worst-case-scenarios. It is less efficient than the best modern
   memory allocators (`mimalloc`, `snmalloc`, and `rpmalloc`) in the "free hotspot" scenario, but it
   is still very efficient, and in particular its performance is still consistent even in these
   worst-case-scenarios.

5. (Optional, provisional goal) Efficiently support using `realloc()` to extend vectors. `smalloc`'s
   initial target user is Rust code, and Rust code uses a lot of Vectors, and not uncommonly it
   grows those Vectors dynamically, which results in a call to `realloc()` in the underlying memory
   manager. I hypothesized that this could be a substantial performance cost in real Rust
   programs. I profiled a Rust application (the "Zebra" Zcash full node) and observed that it did
   indeed call `realloc()` quite often, to resize an existing allocation to larger, and in many
   cases it did so repeatedly in order to enlarge a Vector, then fill it with data until it was full
   again, and then enlarge it again, and so on. This can result in the underlying memory manager
   having to copy the contents of the Vector over and over. `smalloc()` optimizes out much of that
   copying of data -- see "Realloc Growers" above.

`smalloc` appears to have achieved all five of these goals. If so, it may turn out to be a very
useful tool!

# Open Issues / Future Work

* Port to iOS (you just need to give your app the entitlement named
  `com.apple.developer.kernel.extended-virtual-addressing`), Android

* Experiment with making it FIFO instead of LIFO -- this would potentially harden against bugs like
  double-frees and buffer overflows, it might improve multithreading performance (because pushes
  would be updating a different pointer than pops), it would maybe improved cache-friendliness for
  FIFO-oriented usage patterns, but it would potentially probably worse load on the virtual memory
  subsystem

* Port to Cheri, add capability-safety

* Try adding a dose of quint, VeriFast, *and* Miri! :-D

* And Loom! |-D

* And llvm-cov's Modified Condition/Decision Coverage analysis. :-)

* and cargo-mutants

* Try "tarpaulin" again HT Sean Bowe

* If we could allocate even more virtual memory address space, `smalloc` could more scalable
  (i.e. have more large slots, more per-thread slabs, etc). And you could have more than one
  `smalloc` heap in a single process. Larger (than 48-bit) virtual memory addresses are already
  supported on most platforms/configurations, including almost all Linux desktop and server
  platforms, and Windows, but not iOS or Android. We could consider creating a variant of `smalloc`
  that works only platforms with larger (than 48-bit) virtual memory addresses and offers these
  advantages.

* Rewrite it in Zig. :-)

* make it work with valgrind
  * per the valgrind manual:
    * smalloc should register the "pool anchor address" (in valgrind terminology) which is the smalloc base pointer, by calling `VALGRIND_CREATE_MEMPOOL()`.
      * What `rzB` should we use? *think* We *could* add redzones, by choosing bigger slots and sliding-forward the pointer that we return from `alloc()`, but this would require us (smalloc) to slide-backward when calculating the slot location from the pointer in `dealloc()`. Why not!? It reduces computation efficiency a teeeny bit, reduces virtual-memory-efficiency (i.e. not "overhead" as other people seem to think about it, but cache, TLB, and swap efficiency), and complicates the code a little bit
      * Should we use `is_zeroed`? I guess we can't because `is_zeroed` is, for valgrind, a flag that applies to an entire pool for its entire lifetime, and some smalloc allocations (`eac` ones) but not others (`flh` ones) are zeroed. Question: is there some kind of extension to valgrind through which we could mark only the non-zeroed ones as valgrind-`UNDEFINED`?
      * What about `flags` in `VALGRIND_CREATE_MEMPOOL_EXT()`?
    * smalloc should mark the data area (which in valgrind terminology is called a "superblock" as `VALGRIND_MAKE_MEM_NOACCESS`
    * Should we use the `VALGRIND_MEMPOOL_METAPOOL` construct, or not?
    * I *guess* we should use `VALGRIND_DESTROY_MEMPOOL()` at some kind of drop/tear-down/abort/unwind point? Or maybe not so that valgrind can complain to the user about so-called "leaks" from them not having `dealloc()`'ed all their `alloc()`'s?
    * We should definitely call `VALGRIND_MEMPOOL_ALLOC()` on `alloc()` and `VALGRIND_MEMPOOL_FREE()` on `dealloc()`.
    * ... xyz0
    
* add support for the [new experimental Rust Allocator
  API](https://doc.rust-lang.org/nightly/std/alloc/trait.Allocator.html)

* Rewrite it in Odin. :-) (Sam and Andrew's recommendation -- for the programming language, not for
  the rewrite.)

* Try madvise'ing to mark pages as reusable but only when we can mark a lot of pages at once (HT Sam
  Smith)
  
  A note on this: builders and users of memory allocators sometimes talk about "giving memory back
  to the operating system". I think some of them are misunderstanding this by thinking of virtual
  memory as a scarce resource, and memory allocators as using that resource, and "giving memory back
  to the operating system" as making more memory available for other processes. That was accurate 30
  years ago, but it is not really how it works nowadays with virtual memory. Instead, `madvise`'ing
  like this is only *hinting* to the kernel that these are among the first virtual memory pages it
  should choose to unmap in the case that it needs to give some physical memory frames to another
  process. This is merely heuristic — it is a bet that it is *less likely* that these pages will be
  re-used soon compared to other pages in this system. Probably more importantly, the allocator can
  inform the kernel that it can skip the expensive step of swapping the contents of those pages
  to/from persistent storage.

  So it is only a rarely-used efficiency improvement (mostly for those other "neighbor" processes on
  this system, and only occasionally for this process itself).
  
  I personally remain undecided on whether the efficiency benefits in the rare case are worth the
  added code complexity (even though it is just a few lines of code) and the added
  compare-and-branches in the common case (even though it is just one or two of them).

  * Relatedly, when doing `zeromem=true`, i.e. for `calloc` and when the Windows Heap API's
    `HEAP_ZERO_MEMORY` flag is set, there is a size of allocation at which point it is actually more
    efficient to madvise the kernel to drop that page and give us fresh zero-backed pages than to
    memset all of the bytes. This too remains an open question to me about whether the runtime in
    the common case (a single simple comparison and a branch) and the code complexity are actually
    worth it.

  Okay, having posted this and then thought about it a bit more, this is worth doing, because it can
  plausibly avoid a system getting into a swap-thrash and/or OOM-killer situation, and — critically
  — because of Sam Smith's suggestion to do it only for sufficiently large allocations! My [previous
  experiment](https://github.com/zooko/smalloc/blob/86c17f0849cc9debda67d101070f5c0fb687454b/src/lib.rs#L1189)
  was naive and paid the cost of a system call on every `free`, but Sam Smith's suggested
  optimization fixes that, and is also [how `smalloc` current manages memory commits on
  Windows](https://github.com/zooko/smalloc/blob/80c823a31c9e5c4a91748f1d44bfff673a47b506/smalloc/src/lib.rs#L283).

  P.S. Oh, I found my [notes from the previous
  experiment](https://github.com/zooko/smalloc/blob/ee85224083330771324ac2682f3a6c4a114bf0bf/README.md#things-smalloc-does-not-currently-attempt-to-do)
  and it was not quite as naive as I remembered. It paid the cost of a system call only for _large_
  free's or alloc's. I wrote that it increased the latency from 8.4 ns to 1.8 μs for those
  operations on large slots. (That 8.4 ns number is pretty consistent with [current
  benchmarks](https://github.com/zooko/bench-allocators/blob/8daac112202502fdd81316bb51435677a9233849/benchmark-results/AppleM4Max.darwin25/smalloc.result.txt)
  which say about 9.2 ns.) So the next question is, how large does a slot have to be before the cost
  of roughly 1 μs to do a system call is compensated for by the savings of not swapping nor
  memsetting that slot?

* port to WASM now that WASM apparently has grown virtual memory; Note: turns out web browsers still limit the *virtual* memory space to 16 GiB even after the new improved memory model, which kills smalloc in wasm in the web browser. What a shame! But non-web-browser-hosted WASM could still maybe use smalloc...

* Revisit whether we need to provide the C++ memory operators to avoid cross-allocator effects (i.e. a pointer allocated with `malloc`, as implemented by `smalloc`, getting passed to C++ `delete` or vice versa).

* put smalloc into xous instead of its current libmalloc: https://github.com/betrusted-io/xous-core

* read this https://jahej.com/alt/2011_05_28_implementing-a-true-realloc-in-cpp.html

* go back and *really* make it no_std this time

* Fun things that to learn about:
  * https://codeberg.org/ziglang/zig/src/commit/0.15.2/lib/std/heap/SmpAllocator.zig -- even fewer lines of code than smalloc!
  * https://github.com/GJDuck/GC -- uses large amount of vm addresses (3 TiB)
  * https://ckirsch.github.io/publications/conferences/OOPSLA15-Scalloc.pdf / https://github.com/cksystemsgroup/scalloc?tab=readme-ov-file -- using the "sparsely-used virtual memory" approach

# Acknowledgments

* Thanks to Andrew Reece and Sam Smith from Shielded Labs for some specific suggestions that I
  implemented (see notes in documentation above). Thanks also to Andrew Reece for suggesting (at the
  Shielded Labs team meeting in San Diego) to use multiple slabs for all size classes in order to
  reduce flh update conflicts. This suggestion forms a big part of smalloc v6 vs smalloc v5, which
  used multiple slabs for small size classes but not for larger ones.

* Thanks to Jack O'Connor, Nate Wilcox, Sean Bowe, and Brian Warner for advice and
  encouragement. Thanks to Nate Wilcox and Jack O'Connor for hands-on debugging help!

* Thanks to Nate Wilcox for suggesting that I study the results of offensive security researchers on
  heap exploitation as a way to understand how memory managers work. :-)

* Thanks to Kris Nuttycombe for suggesting the name "smalloc". :-)

* Thanks to Jason McGee--my boss at Shielded Labs--for being patient with me obsessively working on
  this when I could have been doing even more work for Shielded Labs instead.

* Thanks to my lovely girlfriend, Kelcie, for housewifing for me while I wrote this program. ♥️

* Thanks to pioneers, competitors, colleagues, and "the giants on whose shoulders I stand", from
  whom I have learned much: the makers of dlmalloc, jemalloc, mimalloc, snmalloc, rsbmalloc, ferroc,
  scudo, rpmalloc, ... and [Michael &
  Scott](https://web.archive.org/web/20241122100644/https://www.cs.rochester.edu/research/synchronization/pseudocode/queues.html),
  and Leo (the Brave Web Browser AI) for extensive and mostly correct answers to stupid Rust
  questions. And Donald Knuth, who gave an interview to Dr Dobbs Journal that I read as a young man
  and that still sticks with me. He said something to the effect that all algorithms *actually* run
  with specific finite resources, and perhaps should be optimized for a specific target size rather
  than for asymptotic complexity. I doubt he'll ever see `smalloc` or this note, but I'm really glad
  that he's still alive. :-)

* Thanks to fluidvanadium for the first PR from a contributor. :-)

* Thanks to Denis Bazhenov, author of the "Tango" benchmarking tool.

* Thanks to Grok 4 and Claude (Opus 4.5) for helping me out with a lot of thorough, detailed, and
  almost entirely accurate explanations of kernel/machine timekeeping issues, Rust language
  behavior, etc, etc.

# Historical notes about lines of code of older versions

Smalloc v2 had the following lines counts (counted by tokei):

* docs and comments: 1641
* implementation loc: 779 (excluding debug_asserts)
* tests loc: 878
* benches loc: 507
* tools loc: 223

Smalloc v3 had the following lines counts:

* docs and comments: 2347
* implementation loc: 867 (excluding debug_asserts)
* tests loc: 1302
* benches loc: 796
* tools loc: 123

Smalloc v4 has the following lines counts:
* docs and comments: 2217
* implementaton loc: 401 (excluding debug_asserts)
* tests loc: 977
* benches loc: 0 -- benchmarks are broken 😭

Smalloc v5 has the following lines counts:
* docs and comments: 2208
* implementaton loc: 395 (excluding debug_asserts)
* tests loc: 949
* benches loc: 84 -- benchmarks are still mostly broken 😭

Smalloc v6.0.4 has the following lines counts:
* docs and comments: 1198
* implementaton loc: 455 (excluding debug_asserts)
* tests loc: 618
* benches loc: 328

(I got those numbers for tests and benches by attributing 1/2 of the lines of code in devutils to
each of them.)

Smalloc v7.4.9 (git commit 6ed1ae401b0ff29df3e2b14d4e86448eec1b6c2f) has the following lines counts:
* docs and comments: 1568
* implementation loc: 286 (excluding debug_asserts)
* tests loc: 760
* benches loc: 669

This is the last version of `smalloc` before adding Windows support and it is probably the fewest
lines of code `smalloc` will ever be!

Smalloc v7.6.3 has the following lines counts:
* docs and comments: 1580
* implementation loc: 349 (excluding debug_asserts)
* tests loc: 804
* benches loc: 1246

(I got those numbers for tests and benches by attributing 1/2 of the lines of code in devutils to
each of them.)

## License

You may use `smalloc` under the terms of any of these four Free and Open Source Software licences:

* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* Transitive Grace Period Public License 1.0 ([LICENSE-TGPPL](LICENSE-TGPPL) or https://spdx.org/licenses/TGPPL-1.0.html)
* Bootstrap Open Source License v1.0 ([LICENSE-BOSL.txt](LICENSE-BOSL.txt))
