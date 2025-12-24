# smalloc -- a simple memory allocator

`smalloc` is suitable as a drop-in replacement for `ptmalloc2` (the glibc memory allocator),
`libmalloc` (the Macos userspace memory allocator), `jemalloc`, `mimalloc`, `snmalloc`, `rpmalloc`,
etc.

`smalloc` performs comparably or even better than those other high-quality memory managers, while
being much simpler. The current implementation is only 288 lines of Rust code! The other
high-quality memory allocators range from 2553 lines of code (`rpmalloc`) to 27,256 lines of code
(`jemalloc`) [^1].

# Caveats

No warranty. Not supported. Never been security audited. First time Rust project (and Jack O'Connor
told me, laughing, that a low-level memory manager was "worst choice ever for a first-time Rust
project"). There is no security contact information nor anyone you can contact for help using this
code. Use at your own risk!

`smalloc` doesn't have any features for hardening your process against exploitation of memory
management bugs. It also doesn't have any features for profiling or generating statistics.

# Performance

To run benchmarks measuring the latency of `smalloc` operations compared to other memory managers,
build the `bench` executable and run `bench --compare`. On Apple Silicon M4 Max, `smalloc` takes
substantially less time for most or all of these operations than the other memory alloctors
[^2]. You can also pass `--thorough` to `bench` to test more edge cases.

To measure performance in real application code, get [this fork of
simd-json](https://github.com/zooko/simd-json) and pass one of `smalloc`, `jemalloc`, `snmalloc`,
`rpmalloc`, or `mimalloc` to `--features=` on the `cargo bench` command-line. For example `cargo
bench -- --save-baseline baseline-default && cargo bench --features=smalloc -- --baseline
baseline-default`. `smalloc` improves the performance of `simd_json` substantially in my experiments
[^3].

# Limitations

There are two limitations:

1. You can't allocate more than 2 GiB in a single `malloc()`, and you can only allocate at most 480
   allocations between 1 GiB and 2 GiB, and at most 960 allocations between 512 MiB and 1 GiB, and
   so on. (See below for complete details.)
   
2. You can't instantiate more than one instance of `smalloc` in a single process.

If you run into either of these limitations in practice, open an issue on the `smalloc` github
repository. It would be possible to lift these limitations, but I'd like to know if it is needed in
practice before complicating the code to do so.

# Usage

Add `smalloc` and `ctor` to your Cargo.toml by executing `cargo add smalloc ctor`, then add this to
your code:

```
use smalloc::Smalloc;
#[global_allocator]
static ALLOC: Smalloc = Smalloc::new();

#[ctor::ctor]
unsafe fn init_smalloc() {
    unsafe { ALLOC.init() };
}
```

See [./src/bin/hellosmalloc_ctor.rs](./src/bin/hellosmalloc_ctor.rs) for a sample program that
demonstrates how to do
this. [./src/bin/hellosmalloc_procmacro.rs](./src/bin/hellosmalloc_procmacro.rs) contains a sample
program that demonstrates an alternate way to do it if the `ctor` approach doesn't work for you (in
which case open an issue on github).

That's it! There are no other features you could consider using, no other changes you need to make,
no configuration options, no tuning options, no nothing.

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

This workspace contains seven packages:
 * _smalloc_: the core memory allocator
 * _plat_: interface to the operating system's `mmap` or equivalent system call to allocate virtual
   address space

These are usually the only two you need to use smalloc.

 * _smalloc-macros_: a proc macro to generate a `main()` function that initializes smalloc before the
   Rust runtime's initialization; You need this only if the `ctor`-based initialization doesn't work
   in your system.
 * _bench_: micro-benchmarking tool to measure latency of operations and compare to other memory
   allocators
 * _hellomalloc_: a pair of sample apps that show the two ways to initialize smalloc as the global
   allocator in Rust code
 * _find_max_vm_addresses_reservable_: a tool used in the development of smalloc to determine how much
   virtual address is allocatable on the current system
 * _devutils_: code used in both tests and benchmarks

## Organization of the core code

Within the smalloc package, there are three files:
 * _smalloc/src/lib.rs_: the core memory allocator
 * _smalloc/src/tests.rs_: transparent-box tests that use internals of the core to test it
 * _smalloc/tests/integration.rs_: opaque-box tests that use only the public API
 
# How it works

## The Big Idea

`smalloc`'s big idea is that while touching memory (i.e. reading or writing a specific memory
location) imposes costs on the virtual memory subsystem, reserving virtual memory address space does
not. Virtual memory addresses are a free and near-limitless resource. Use that big idea by reserving
a huge swathe of virtual memory addresses that you will use only sparsely. When allocating memory,
this sparseness enables you to efficiently find an unoccupied space big enough to hold the
request. When de-allocating, this sparseness enables you to leverage information encoded into the
pointer itself (the pointer to be de-allocated), and minimize the need to look up and compute upon
additional information beyond that.

## Data model

### Slots, Slabs, and Size Classes

All memory managed by `smalloc` is organized into "slabs". A slab is a fixed-length array of
fixed-length "slots" of bytes. Every pointer returned by a call to `malloc()` or `realloc()` is a
pointer to the beginning of one of those slots, and that slot is used exclusively for that memory
allocation until it is `free()`'ed.

Each slab holds slots of a specific fixed length, called a "size class". Size class 3 contains
8-byte slots, size class 4 contains 16-byte slots, and so on which each size class having slots
twice as long as the size class before.

Within each size class there are 32 separate slabs holding slots of that size. (See below for why.)

Sizeclasses 0, 1 and 2 are unused and the space reserved for them is repurposed to hold "free list
heads" (see below). Figure 1 shows the information encoded into memory addresses (shown in binary
notation) of slots and of bytes of data within each slot:

```text
Figure 1. Overview

slabs
                                       [sla][sc ][f]
   0   000000000000000000000000000000000000000000000       used for flh's

   1   unused
   
   2   unused
   
        slab  sc   slotnum                      data
       [   ][   ][                           ][    ]
  sc   address in binary                             slotsize slots slabs
  --   --------------------------------------------- -------- ----- -----
       [sla][sc ][slotnum                       ][d]
   3   000000001100000000000000000000000000000000000     2^ 3  2^32   2^5

       [sla][sc ][slotnum                      ][da]
   4   000000010000000000000000000000000000000000000     2^ 4  2^31   2^5

       [sla][sc ][slotnum                     ][dat]
   5   000000010100000000000000000000000000000000000     2^ 5  2^30   2^5

       [sla][sc ][slotnum                    ][data]
   6   000000011000000000000000000000000000000000000     2^ 6  2^29   2^5

       [sla][sc ][slotnum                   ][data ]
   7   000000011100000000000000000000000000000000000     2^ 7  2^28   2^5

       [sla][sc ][slotnum                  ][data  ]
   8   000000100000000000000000000000000000000000000     2^ 8  2^27   2^5

   9                                                     2^ 9  2^26   2^5
  10                                                     2^10  2^25   2^5
  11                                                     2^11  2^24   2^5
  12                                                     2^12  2^23   2^5
  13                                                     2^13  2^22   2^5
  14                                                     2^14  2^21   2^5
  15                                                     2^15  2^20   2^5
  16                                                     2^16  2^19   2^5
  17                                                     2^17  2^18   2^5
  18                                                     2^18  2^17   2^5
  19                                                     2^19  2^16   2^5
  20                                                     2^20  2^15   2^5
  21                                                     2^21  2^14   2^5
  22                                                     2^22  2^13   2^5
  23                                                     2^23  2^12   2^5
  24                                                     2^24  2^11   2^5
  25                                                     2^25  2^10   2^5
  26                                                     2^26  2^ 9   2^5
  27                                                     2^27  2^ 8   2^5
  28                                                     2^28  2^ 7   2^5
 
       [sla][sc ][slot][data                       ]
  29   000001110100000000000000000000000000000000000     2^29  2^ 6   2^5

       [sla][sc ][slo][data                        ]
  30   000001111000000000000000000000000000000000000     2^30  2^ 5   2^5

       [sla][sc ][sl][data                         ]
  31   000001111100000000000000000000000000000000000     2^31  2^ 4   2^5
```

### Free-Lists

For each slab, there is a free list, which is a singly-linked list of slots that are not currently
in use (i.e. either they've never yet been `malloc()`'ed, or they've been `malloc()`'ed and then
subsequently `free()`'ed). When referring to a slot's fixed position within the slab, call that its
"slot number", and when referring to a slot's position within the free list (which can change over
time as slots get removed from and added to the free list), call that a "free list entry". A free
list entry contains a pointer to the next free list entry (or a sentinel value if there is no next
free list entry, i.e. this entry is the end of the free list).

For each slab there is one additional associated variable, which holds the pointer to the first free
list entry (or the sentinel value if there are no entries in the list). This variable is called the
"free-list head" and is abbreviated `flh`. The contents of the free list head is the only
additional information you need beside the information present in the pointers themselves.

That's it! Those are all the data elements in `smalloc`.

## Algorithms, Simplified

Here is a first pass describing simplified versions of the algorithms. After you learn these simple
descriptions, keep reading for additional detail.

The free list for each slab begins life fully populated -- its `flh` points to the first slot in its
slab, the first slot points to the second slot, and so forth until the last slot, whose pointer is a
sentinel value meaning that there are no more elements in the free list.

* `malloc()`

To allocate space, calculate the size class of the request (which includes the requested alignment
-- see below). Now pick one of the slabs in that size class (see below for how). Pop the head
element from the free list and return the pointer to that slot.

* `free()`

Push the slot to be freed (the slot whose first byte is pointed to by the pointer to be freed) onto
the free list of its slab.

* `realloc()`

If the requested new size (and alignment) requires a larger slot than the allocation's current slot,
then allocate a new slot (just like in `malloc()`, above). Then `memcpy()` the contents of the
current slot into the beginning of that new slot, deallocate the current slot (just like in
`free()`, above) and return the pointer to the new slot.

That's it! You could stop reading here and you'd have a basic knowledge of the design of `smalloc`.

## The Free Lists in More Detail

The `flh` for a given slab is either a sentinel value (meaning that the list is empty), or else it
points to the slot which is the first entry in that slab's free list.

To pop the head entry off of the free list, set the `flh` to point to the next (second) entry
instead of the first entry.

But where is the pointer to the next entry stored? The answer is: store the next-pointers in the
same space where the data goes when the slot is in use! Each data slot is either currently freed,
meaning you can use its space to hold the pointer to the next free list entry, or currently
allocated, meaning it is not in the free list and doesn't have a next-pointer.

This is also why not to use size classes 0 (for 1-byte slots), 1 (for 2-byte slots), or 2 (for
4-byte slots): because you need 8 bytes in each slot to store the next-entry link.

This technique is known as an "intrusive free list". Thanks to Andrew Reece and Sam Smith, my
colleagues at Shielded Labs (makers of fine Zcash protocol upgrades), for explaining this to me.

So to satisfy a `malloc()` by popping the head slot from the free list, take the value from the
`flh`, use that value as a pointer to a slot (which is the first entry in the free list), and then
read the *contents* of that slot as the pointer to the next entry in the free list. Overwrite the
value in `flh` with the pointer of that *next* entry and you're done popping the head of the free
list.

To push an slot onto the free list (in order to implement `free()`), you are given the pointer of
the memory allocation to be freed. Calculate from that pointer the size class, slab number, and slot
number. Set the contents of that slot to point to the free list entry that its `flh` currently
points to. Now update the `flh` to point to the new slot. That slot is now the new head entry of the
free list (and the previous first-entry in the free list is now its next-entry).

### Encoding Slot Numbers In The Free List Entries

When memory is first allocated all of its bits are `0`. Define an encoding from pointers to free
list entries such that when all of the bits of the `flh` and the slots are `0`, then it is a
completely populated free list -- the `flh` points to the first slot number as the first free list
entry, the first free list entry points to the second slot number as the second free list entry, and
so on until the last-numbered slot which points to nothing -- a sentinel value meaning "this points
to no slot". (This is an example of a design pattern called "Zero Is Initialization" -- "ZII".)

Here's how that encoding works:

The `flh` contains the slot number of the first free list entry. So, when it is all `0` bits, it is
pointing to the slot with slot number `0`.

To get the next-entry pointer of a slot, load `4` bytes from the slot, interpret them as a 32-bit
unsigned integer, add it to the slot number of the slot, and add `1`, mod the total number of slots
in that slab.

This way, a slot that is initialized to all `0` bits, points to the next slot number as its next
free list entry. The final slot in the slab, when it is all `0` bits, points to no next entry,
because when its `4` bytes are interpreted as a next-entry pointer, it equals the highest possible
slot number, which is the "sentinel value" meaning no next entry.

## Thread-Safe `flh` Updates

To make `smalloc` behave correctly under multiprocessing, it is necessary and sufficient to perform
thread-safe updates to `flh`. Use a simple loop with atomic compare-and-exchange operations.

### To pop an entry from the free list:

1. Load the value from `flh` into a local variable/register, `firstslotnum`. This is the slot number
   of the first entry in the free list.
2. If it is the sentinel value, meaning that the free list is empty, return. (See below for how this
   `malloc()` request will be handled in this case.)
3. Load the value from first entry into a local variable/register, `nextslotnum`. This is the slot
   number of the next entry in the free list (i.e. the second free-list entry), or a sentinel value
   there is if none.
4. Atomically compare-and-exchange the value from `nextslotnum` into `flh` if `flh` still contains
   the value from `firstslotnum`.
5. If the compare-and-exchange failed (meaning the value of `flh` has changed since it was read in
   step 1), jump to step 1.

Now you've thread-safely popped the head of the free list into `firstslotnum`.

### To push an entry onto the free list, where `newslotnum` is the number of the slot to push:

1. Load the value from `flh` into a local variable/register, `firstslotnum`.
2. Store the value from `firstslotnum` (encoded as a next-entry pointer) into the slot with slot
   number `newslotnum`.
3. Atomically compare-and-exchange the value from `newslotnum` into `flh` if `flh` still contains
   the value from `firstslotnum`.
4. If the compare-and-exchange failed (meaning that value of `flh` has changed since it was read in
   step 1), jump to step 1.

Now you've thread-safely pushed `newslotnum` onto the free list.

### To prevent ABA errors in updates to the free list head

The test described above of whether the `flh` still contains its original value is actually not
enough to guarantee correctness under multithreading. The problem is that step 4 of the pop
algorithm above is assuming that if the `flh` still contains the original value, then it is valid to
write `nextslotnum` into `flh`, but it is possible that a concurrent series of pops and pushes could
result in the `flh` containing the original slotnum, but with that slot's next-entry slot pointing
to a different entry than `nextslotnum`. The way this could happen is if the original value got
popped off, then another pop occurred (removing `nextslotnum` from the free list entirely), then the
original value got pushed back on. In that case the `flh` would contain the original value but with
a different next-entry link. This is a kind of "ABA problem".

In order to prevent this, store a counter in the unused bits of the flh word. Increment that counter
each time you attempt a compare-and-exchange on a push (`dealloc`). Now if there were any pushes
concurrently completed between step 1 of the pop algorithm and step 4, the compare-and-exchange will
fail.

Now you know the entire data model and almost all of the algorithms for `smalloc`! Read on for a few
more details.

## Separate Threads Use Separate Slabs

This is not necessary for correctness -- the algorithms described above are sufficient for
correctness. This is just a performance optimization. Arrange it so that (under reasonable usage
patterns), each active thread will use a different slab from the other active threads. This will
minimize `flh`-update collisions, and for slots small enough to pack into a cache line, this will
tend to increase "true-sharing" -- cache-line-sharing between multiple allocations accessed from the
same processor as each other.

To do this, define a global static variable named `GLOBAL_THREAD_NUM`, initialized to `0`. 

Give each thread a thread-local variable named `THREAD_NUM`. The first time `alloc()` is called from
within a given thread, use the atomic `fetch_add` operation to increment `GLOBAL_THREAD_NUM` and set
this thread's `THREAD_NUM` to the previous value of `GLOBAL_THREAD_NUM`. Also give each thread
another thread-local variable named `SLAB_NUM`, and set it to `THREAD_NUM` mod 32.

Whenever allocating, allocate from the slab indicated by your thread's `SLAB_NUM`.

## Handling Overflows and Update-Collisions

Suppose the user calls `malloc()` and the slab (determined by the size class of the request and your
thread's `SLAB_NUM`) is exhausted, i.e. the free list is empty. This could happen only if there were
that many allocations from that slab active simultaneously.

Or, suppose the user calls `malloc()` and you encounter a free-list-head update collision, i.e. you
reach step 5 of the thread-safe algorithm for popping an entry from the free list (shown above).

In either of these cases, try allocating from a different slab in the same size class. If it
succeeds, update your thread's `SLAB_NUM` to point to this new slab. If this attempt, too, fails,
for either of those two reasons, then try yet another different slab in the same size class. If
you've tried every slab in this size class, and they've all failed (whether due to that slab being
exhausted or due to encountering an `flh` update collision when trying to pop from that slab's free
list), then *if* at least one slab was exhausted, move to the next bigger size class and continue
trying. (Thanks to Nate Wilcox -- also my colleague at Shielded Labs -- for suggesting this
technique to me.) On the other hand, if none of the slabs were exhausted, then continue cycling
through them trying to allocate from one of them.

## Realloc Growers

Suppose the user calls `realloc()` and the new requested size is larger than the original
size. Allocations that ever get reallocated to larger sizes often, in practice, get reallocated over
and over again to larger and larger sizes. Call any allocation that has gotten reallocated to a
larger size a "grower".

If the user calls `realloc()` asking for a new larger size, and the new size still fits within the
current slot that the data is already occupying, then just be lazy and consider this `realloc()` a
success and return the current pointer as the return value.

If the new requested size doesn't fit into the current slot, then choose the smallest of the
following list that can hold the new requested size: 64 B, 128 B, 4096 B, 16 KiB, 64 KiB, 256 KiB,
or 2 MiB.

If the new requested size doesn't fit into 2 MiB then just use the smallest size class that can hold
it.

This reduces unnecessary `memcpy`'s when an allocation gets reallocated to a larger size repeatedly,
while trying to avoid using up the very large slots, which are not that plentiful.

# Design Goals

Why `smalloc` is beautiful in my eyes.

If you accept the Big Idea that "avoiding reserving too much virtual address space" is not an
important goal for a memory manager, what *are* good goals? `smalloc` was designed with the
following goals, written here in roughly descending order of importance:

1. Be simple, in both design and implementation. This helps greatly to ensure correctness -- always
   a critical issue in modern computing. "Simplicity is the inevitable price that we must pay for
   correctness."--Tony Hoare (paraphrased)

   Simplicity also eases making improvements to the codebase and learning from the codebase.

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
      allocations are packed into the same memory page, this avoids a potentially costly page
      fault. Page faults can cost only a few CPU cycles in the best case, but in case of a TLB cache
      miss they could incur substantially more. In the worst case, the kernel has to load the data
      from swap, which could incur a performance penalty of hundreds of *thousands* of CPU cycles or
      even more, depending on the performance of the persistent storage. Additionally, faulting in a
      page of memory increases the pressure on the TLB cache and the swap subsystem, thus
      potentially causing a performance degradation for other processes running on the same system.

   Note that these three goals cannot be fully optimized for by the memory manager, because they
   depend on how the user code accesses the memory. What `smalloc` does is use some simple
   heuristics intended to optimize the above goals under some reasonable assumptions about the
   behavior of the user code:

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

   4. The same strategies also tend to pack together allocations into pages of virtual memory.

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
   case, only three potential cache misses.

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
   list. (See below about why we don't typically incur another potential-cache-miss when updating
   the free list.)

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
     * 🟠 one to access the `THREAD_AREANUM`
     * 🟠 one to access the `flh`
     * 🟠 one to access the intrusive free list entry
     * 🟢 no additional cache-miss for the user code to access the data

     For a total of 3 potential-cache-misses.

   2. To read from some memory and then `free()` it:
     * 🟠 one for the user code to read from the memory
     * 🟠 one to access the `flh`
     * 🟢 no additional cache-miss for `free()` to access the intrusive free list entry

     For a total of 2 potential-cache-misses.

   3. To `free()` some memory without first reading it:
     * 🟢 no cache-miss for user code since it doesn't read the memory
     * 🟠 one to access the `flh`
     * 🟠 one to access the intrusive free list entry

     For a total of 2 potential-cache-misses.

   Note that the above counts do not count a potential cache miss to access the base pointer. That's
   because the base pointer is fixed and shared -- every call (by any thread) to `malloc()`,
   `free()`, or `realloc()` accesses the base pointer, so it is more likely to be in cache.
   
   A similar property holds for the potential cache-miss of accessing the `SLAB_NUM` -- if this
   thread has recently called `malloc()` then this thread's `SLAB_NUM` will likely already be in
   cache, but if this thread has not made such a call recently then it would likely cache-miss.
   
   And of course a similar property holds for the potential cache-miss of accessing the `flh` -- if
   this thread has recently called `malloc()`, `free()`, or `realloc()` for an allocation of this
   size class, then the `flh` for this slab will already be in cache.

4. Be *consistently* efficient.

   I want to avoid intermittent performance degradation, such as when your function takes little
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
   for cautionary tales of how deferred accounting, while it can improve performance in the "hot
   paths", can also give rise to edge cases that can occasionally degrade performance or cause other
   problems.

   There are no locks in `smalloc`. There are concurrent-update loops in `malloc` and `free` -- see
   the pseudo-code in "Thread-Safe State Changes" above -- but these are not locks. Whenever
   multiple threads are running that code, one of them will make progress (i.e. successfully update
   the `flh`) after it gets only a few CPU cycles, regardless of what any other threads do. And, if
   any thread becomes suspended in that code, one of the *other*, still-running threads will be the
   one to make progress (update the `flh`). Therefore, these concurrent-update loops cannot cause a
   pile-up of threads waiting for a (possibly-suspended) thread to release a lock, nor can they
   suffer from priority inversion.

   Also, for `malloc()` (but not for `free()`), if a thread experiences an update collision it will
   immediately switch over to a different slab, which will immediately clear out any such contention
   unless all slabs are simultaneously occupied by more than one thread actively
   `malloc()`'ing.
   
   For `free()` it isn't possible to change slabs (the pointer to be freed needs to be pushed back
   onto this particular free list and no other), so multiple threads simultaneously attempting to
   free slots in the same slab is the worst-case-scenario for `smalloc`.

   See the benchmarks named `hs` (for "hotspot") and `fh` (for "free hotspot") for how `smalloc`
   currently performs in these worst-case-scenarios. It is much less efficient than the best modern
   memory allocators (`mimalloc`, `snmalloc`, and `rpmalloc`) in these scenarios, but it is still
   very efficient, and in particular its performance is still bounded and consistent even in these
   worst-case-scenarios.

I am hopeful that `smalloc` has achieved all four of these main goals. If so, it may turn out to be
a very useful tool!

5. (Optional, provisional goal) Efficiently support using `realloc()` to extend vectors. `smalloc`'s
   initial target user is Rust code, and Rust code uses a lot of Vectors, and not uncommonly it
   grows those Vectors dynamically, which results in a call to `realloc()` in the underlying memory
   manager. I hypothesized that this could be a substantial performance cost in real Rust
   programs. I profiled a Rust application (the "Zebra" Zcash full node) and observed that it did
   indeed call `realloc()` quite often, to resize an existing allocation to larger, and in many
   cases it did so repeatedly in order to enlarge a Vector, then fill it with data until it was full
   again, and then enlarge it again, and so on. This can result in the underlying memory manager
   having to copy the contents of the Vector over and over. `smalloc()` optimizes out much of that
   copying of data, with the simple expedient of jumping to a larger slot size whenever
   `realloc()`'ing an allocation to a larger size (see "Realloc Growers", above). My profiling
   results indicate that this technique would indeed eliminate most of the memory-copying when
   extending Vectors.

# Open Issues / Future Work

* make it FIFO instead of LIFO -- improved security (?), maybe improved multithreading, maybe
  improved cache-friendliness for FIFO-oriented usage patterns, probably worse load on the virtual
  memory subsystem

* Port to Cheri, add capability-safety

* Try adding a dose of quint, VeriFast, *and* Miri! :-D

* And Loom! |-D

* And llvm-cov's Modified Condition/Decision Coverage analysis. :-)

* and cargo-mutants

* If we could allocate even more virtual memory address space, `smalloc` could more scalable (eg
  huge slots could be larger than 4 mebibytes, the number of per-thread areas could be greater than
  64), it could be even simpler (eg maybe just remove the (quite complex!)  overflow algorithm, and
  the special-casing of the number of slots for the huge-slots slab), and you could have more than
  one `smalloc` heap in a single process. Larger (than 48-bit) virtual memory addresses are already
  supported on some platforms/configurations, especially server-oriented ones, but are not widely
  supported on desktop and smartphone platforms. We could consider creating a variant of `smalloc`
  that works only platforms with larger (than 48-bit) virtual memory addresses and offers these
  advantages. TODO: make an even simpler smalloc ("ssmalloc"??) for 5-level-page-table systems.

* Crib from parking_lot and/or static_locks or something for how to ask the OS to put a thread to
  sleep when it has encountered an flh collision, and then wake it again the next time a different
  thread successfully updates the flh

* Rewrite it in Zig. :-)

* Get an AI to review the code. :-)

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
    
* try adding some newtypey goodness?

* add support for the [new experimental Rust Allocator
  API](https://doc.rust-lang.org/nightly/std/alloc/trait.Allocator.html)

* add initialized-to-zero alloc alternative, relying on kernel 0-initialization when coming from eac

* make it usable as the implementation `malloc()`, `free()`, and `realloc()` for native code. :-)
  (Nate's suggestion.)

* Rewrite it in Odin. :-) (Sam and Andrew's recommendation -- for the programming language, not for
  the rewrite.)

* Try "tarpaulin" again HT Sean Bowe

* Try madvise'ing to mark pages as reusable but only when we can mark a lot of pages at once (HT Sam Smith)

* Put back the fallback to mmap for requests that overflow.??? Or document why not.

* automatically detect and use 5lpt?

* make it run benchmarks when you do `cargo run -p bench`, like iroh quinn does?

# Acknowledgments

* Thanks to Andrew Reece and Sam Smith for some specific suggestions that I implemented (see notes
  in documentation above). Thanks also to Andrew Reece for suggesting (at the Shielded Labs team
  meeting in San Diego) that we use multiple slabs for all size classes in order to reduce
  multithreading write conflicts. This suggestion forms a big part of smalloc v6 vs smalloc v5,
  which used multiple slabs for small size classes but not for larger ones.

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

* Thanks to Chenyao Lou for suggesting in https://lemire.me/blog/2021/01/06/memory-access-on-the-apple-m1-processor/#comment-565474 xor'ing a counter into indexes in benchmarks to foil speculative pre-fetching.

* Thanks to Denis Bazhenov, author of the "Tango" benchmarking tool.

* Thanks to Grok 4 for helping me out with a lot of thorough, detailed, and almost entirely accurate explanations of kernel/machine timekeeping issues, Rust language behavior, etc, and thanks to 

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

## License

Licensed under any of:

* MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* Transitive Grace Period Public License 1.0 ([LICENSE-TGPPL](LICENSE-TGPPL) or https://spdx.org/licenses/TGPPL-1.0.html)
* Bootstrap Open Source License v1.0 ([LICENSE-BOSL.txt](LICENSE-BOSL.txt))

at your option.


[^1]: Lines of code of various memory allocators, with an attempt to exclude test code and
    platform-interface code:
    ```text
    % tokei smalloc-7.3/smalloc/src/lib.rs
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Language              Files        Lines         Code     Comments       Blanks
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Rust                      1          499          288           99          112
     |- Markdown               1           29            0           23            6
     (Total)                              528          288          122          118
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Total                     1          528          288          122          118
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    % tokei ./rpmalloc/rpmalloc/
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Language              Files        Lines         Code     Comments       Blanks
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     C                         2         2840         2270          295          275
     C Header                  2          521          283          159           79
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Total                     4         3361         2553          454          354
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    % tokei ./mimalloc/src --exclude='prim'
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Language              Files        Lines         Code     Comments       Blanks
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     C                        20        11386         8131         1861         1394
     C Header                  1          119           43           51           25
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Total                    21        11505         8174         1912         1419
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    % tokei glibc/malloc/ --exclude 'tst-*'
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Language              Files        Lines         Code     Comments       Blanks
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     C                        32        11863         7018         3249         1596
     C Header                  5          960          449          369          142
     Makefile                  1          533          383           80           70
     Perl                      1          254          211           26           17
     Shell                     1          273          217           35           21
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Total                    40        13883         8278         3759         1846
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    % tokei ./snmalloc/src/snmalloc --exclude='pal'
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Language              Files        Lines         Code     Comments       Blanks
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     C Header                103        17346        10745         4520         2081
     C++                       6          800          542          156          102
     Markdown                  4           51            0           42            9
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Total                   113        18197        11287         4718         2192
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    % tokei ./jemalloc/src
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Language              Files        Lines         Code     Comments       Blanks
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     C                        67        36039        27021         4822         4196
     C++                       1          308          224           14           70
     Python                    1           15           11            2            2
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Total                    69        36362        27256         4838         4268
    ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    ```


[^2]: Output of `bench --compare` on Macos 26.2/Apple Silicon M4 Max:
    ```text
    name:     de_mt_adr-32, threads:    32, iters:  50,000, ns:     23,301,750, ns/i:      466.0
    name:     mm_mt_adr-32, threads:    32, iters:  50,000, ns:      8,233,834, ns/i:      164.6
    name:     jm_mt_adr-32, threads:    32, iters:  50,000, ns:      2,602,667, ns/i:       52.0
    name:     nm_mt_adr-32, threads:    32, iters:  50,000, ns:      1,232,209, ns/i:       24.6
    name:     rp_mt_adr-32, threads:    32, iters:  50,000, ns:      1,259,667, ns/i:       25.1
    name:     sm_mt_adr-32, threads:    32, iters:  50,000, ns:        821,791, ns/i:       16.4
    smalloc diff from  default:  -96%
    smalloc diff from mimalloc:  -90%
    smalloc diff from jemalloc:  -68%
    smalloc diff from snmalloc:  -33%
    smalloc diff from rpmalloc:  -35%
    ```

[^3]:  Output of  `simd_json`'s  `cargo bench  -- --save-baseline  baseline-default  && cargo  bench
    --features=smalloc  -- --baseline  baseline-default |  grep -B  1 -Ee"within  noise|o change  in
    performance|egressed|mproved"` on Macos 26.2/Apple Silicon M4 Max:
    ```text
    thrpt:  [+16.580% +16.985% +17.379%]
    Performance has improved.
    --
    thrpt:  [+15.711% +15.945% +16.165%]
    Performance has improved.
    --
    thrpt:  [+61.962% +62.512% +63.092%]
    Performance has improved.
    --
    thrpt:  [+0.5093% +1.0261% +1.5016%]
    Change within noise threshold.
    --
    thrpt:  [−2.0851% −1.6801% −1.2499%]
    Performance has regressed.
    --
    thrpt:  [+10.138% +10.611% +11.073%]
    Performance has improved.
    --
    thrpt:  [+16.631% +17.563% +18.566%]
    Performance has improved.
    --
    thrpt:  [+10.697% +11.590% +12.583%]
    Performance has improved.
    --
    thrpt:  [+51.888% +52.659% +53.491%]
    Performance has improved.
    --
    thrpt:  [+37.046% +37.274% +37.512%]
    Performance has improved.
    --
    thrpt:  [+36.707% +36.979% +37.258%]
    Performance has improved.
    --
    thrpt:  [+39.052% +39.261% +39.459%]
    Performance has improved.
    --
    thrpt:  [+34.876% +35.583% +36.345%]
    Performance has improved.
    --
    thrpt:  [+34.649% +35.374% +36.129%]
    Performance has improved.
    --
    thrpt:  [+58.748% +59.603% +60.496%]
    Performance has improved.
    --
    thrpt:  [+9.4057% +9.8442% +10.360%]
    Performance has improved.
    --
    thrpt:  [−2.3211% −1.7501% −1.2120%]
    Performance has regressed.
    --
    thrpt:  [+47.005% +47.483% +47.968%]
    Performance has improved.
    --
    thrpt:  [+11.964% +12.281% +12.592%]
    Performance has improved.
    --
    thrpt:  [+11.828% +12.100% +12.385%]
    Performance has improved.
    --
    thrpt:  [+55.077% +55.446% +55.815%]
    Performance has improved.
    ```
