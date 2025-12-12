# smalloc -- a simple memory allocator

`smalloc` is a memory allocator, suitable (I hope) as a drop-in replacement for `ptmalloc` (the
glibc memory allocator), `libmalloc` (the Macos userspace memory allocator), `jemalloc`, `mimalloc`,
`snmalloc`, `rpmalloc`, etc.

`smalloc` offers performance properties comparable to the other memory managers, while being
simpler. The current implementation is only 455 lines of Rust code (excluding comments, tests,
benchmarks, etc).

# Caveats

No warranty! Not supported. Never been security audited. First time Rust project (and Jack O'Connor
told me, laughing, that a low-level memory manager was "worst choice ever for a first-time Rust
project"). There is no security contact information nor anyone you can contact for help using this
code. Use at your own risk!

# Usage

Add it to your Cargo.toml by executing `cargo add smalloc`, then add this to your code:

```
use smalloc::Smalloc;

#[global_allocator]
static SMALLOC: Smalloc = Smalloc::new();
```

See `./src/bin/hellosmalloc.rs` for a test program that demonstrates how to do this.

That's it! There are no other features you could consider using, no other changes you need to make,
no configuration options, no tuning options, no nothing.

# Tests and Benchmarks

Tests and benchmarks are run using the `nextest` runner.

To install `nextest`:

```text
cargo install cargo-nextest
```

To run the tests:

```text
cargo --frozen nextest run
```

To run the benchmarks:

```text
cargo --frozen build --release --package simplebench
./target/release/simplebench
```

```
              [unused                                    ]
pad         0 00000000000000000000000000000000000000000000                           ~2^45

slabs
   0   used for flhs

   1   unused
   
        sc   slab slotnum                   data
       [   ][   ][                        ][     ]
  sc                                 ... in binary slotsize slots slabs
  --                                 ------------- -------- ----- -----
       [sc ][slab][slotnum 0                     ][]
   2   000100000000000000000000000000000000000000000   2^ 2  2^32   2^6

       [sc ][slab][slotnum                      ][d]
   3   000110000000000000000000000000000000000000000   2^ 3  2^31   2^6

       [sc ][slab][slotnum                     ][ra]
   4   001000000000000000000000000000000000000000000   2^ 4  2^30   2^6

       [sc ][slab][slotnum                    ][dat]
   5   001010000000000000000000000000000000000000000   2^ 5  2^29   2^6

       [sc ][slab][slotnum                   ][data]
   6   001100000000000000000000000000000000000000000   2^ 6  2^28   2^6

       [sc ][slab][slotnum                  ][data ]
   7   001110000000000000000000000000000000000000000   2^ 7  2^27   2^6

       [sc ][slab][slotnum                 ][data  ]
   8   010000000000000000000000000000000000000000000   2^ 8  2^26   2^6

   9                                                   2^ 9  2^25   2^6
  10                                                   2^10  2^24   2^6
  11                                                   2^11  2^23   2^6
  12                                                   2^12  2^22   2^6
  13                                                   2^13  2^21   2^6
  14                                                   2^14  2^20   2^6
  15                                                   2^15  2^19   2^6
  16                                                   2^16  2^18   2^6
  17                                                   2^17  2^17   2^6
  18                                                   2^18  2^16   2^6
  19                                                   2^19  2^15   2^6
  20                                                   2^20  2^14   2^6
  21                                                   2^21  2^13   2^6
  22                                                   2^22  2^12   2^6
  23                                                   2^23  2^11   2^6
  24                                                   2^24  2^10   2^6
  25                                                   2^25  2^ 9   2^6
  26                                                   2^26  2^ 8   2^6
  27                                                   2^27  2^ 7   2^6
  28                                                   2^28  2^ 6   2^6

       [sc ][slab][slo][data                       ]
  29   111010000000000000000000000000000000000000000   2^29  2^ 5   2^6

       [sc ][slab][sl][data                        ]
  30   111100000000000000000000000000000000000000000   2^30  2^ 4   2^6

       [sc ][slab][s][data                         ]
  31   111110000000000000000000000000000000000000000   2^31  2^ 3   2^6
```


XXX EVERYTHING BELOW THIS LINE IS PROBABLY OUT OF DATE SORRY BRB

# How it works

To understand how it works, you need to know `smalloc`'s data model and the algorithms.

## Data model

### Data Slots and Slabs

All memory managed by `smalloc` is organized into "slabs". A slab is a fixed-length array of
fixed-length "slots" of bytes. Every pointer returned by a call to `smalloc`'s `malloc()` or
`free()` is a pointer to the beginning of one of those slots, and that slot is used exclusively for
that memory allocation until it is `free()`'ed[^1].

[^1]: Except for calls to `malloc()` or `realloc()` for sizes that are too big to fit into even the
    biggest of `smalloc`'s slots, which `smalloc` instead satisfies by falling back to the system
    allocation, i.e. `mmap()` on Linux, `vm_mach_allocate()` on Macos, etc.

Each slab holds slots of a specific fixed length, called a "size class". Size class 0 is 4-byte
slot, size class 1 is 8-byte slots, and so on which each size class having slots twice as long as
the size class before. The exception is the largest size class, size class 13, which instead of
having slots double the length of the previous size class, has extra-large slots -- 8 MiB.

There are three kinds of slots: "small", "medium", and "large". (See the "Rationales for Design
Decisions" section below for the motivation for having three different kinds.) Small slots are in
slabs of 16,777,214 slots per slab, and there are 32 slabs of each size class for small
slots. Medium slots are in slabs of 536,870,848 slots, with only one slab per size class. Large
slots are in a slab of 8,388,606 slots.

xxx new medium: same size classes 5 (128 B) - 9 (2048 B) inclusive, nummediumslots = 2^27 xxx new
small: still size classes 0-4, but now numsmallslabs = 256 and numsmallslots = 2^29 xxx new large:
size class 10 (4096 B) and up, starting at 2^26 slots.. doubling the slotsize and halving the
numslots in each subsequent size class.

xxx this tops out at ... numslots=1, sizeclass=274_877_906_944 (2^37) (27 total large size classes)

xxx this takes only a total of 24_979_529_793_536 bytes! which is < 2^45. So I can system-allocate
2^46-1 virtual addresses (around 70 terabytes, less than 70 tebibytes) and start the smalloc base
pointer at a multiple of 2^45, which enables bit-twiddly mappings from pointer to slot! :-)

```text
Figure 1. Overview

small slots:
size class:  slot size:  # slabs      # slots:
-----------  ----------  -------      --------
          0         4 B       32    16,777,214
          1         8 B       32    16,777,214
          2        16 B       32    16,777,214
          3        32 B       32    16,777,214
          4        64 B       32    16,777,214

medium slots:
size class:  slot size:  # slabs      # slots:
-----------  ----------  -------      --------
          5       128 B        1   536,870,911
          6       256 B        1   536,870,911
          7       512 B        1   536,870,911
          8      1024 B        1   536,870,911
          9      2048 B        1   536,870,911
         10      4096 B        1   536,870,911
         11      8192 B        1   536,870,911
         12     16384 B        1   536,870,911

large slots:
size class:  slot size:  # slabs      # slots:
-----------  ----------  -------      --------
         13       8 MiB        1     8,388,607
```

xxx larger slots? xxx once we get past the medium slots, switch to fixed total size instead of fixed numslots?
xxx slots that fit into Large Pages
xxx automatically detect and use 5lpt?

Slots are 0-indexed, so the largest slot number in each small-slots slab is 16,777,213, the largest
slot number in each medium-slots slab is 536,870,847, and the largest slot number in the large-slots
slab is 8,388,605.

### Free-List

For each slab, there is a free list, which is a singly-linked list of slots that are not currently
in use (i.e. either they've never yet been `malloc()`'ed, or they've been `malloc()`'ed and then
subsequently `free()`'ed). When we're referring to a slot's fixed position within the slab, we call
that its "slot number", and when we're referring to a slot's position within the free list (which
can change over time as slots get removed from and added to the free list), we call that a "free
list entry". A free list entry contains a pointer to the next free list entry (or a sentinel value
if there is no next free list entry, i.e. this entry is the end of the free list).

For each slab there is one associated variable, which holds the pointer to the first free list entry
(or the sentinel value if there are no entries in the list). This variable is called the "free-list
head" and is abbreviated `flh`.

That's it! Those are all the data elements in `smalloc`.

## Algorithms, Simplified

Here is a first pass describing simplified versions of the algorithms. After you learn these simple
descriptions, keep reading for additional detail.

The free list for each slab begins life fully populated -- its `flh` points to the first slot in its
slab, the first slot points to the second slot, and so forth until the last slot, whose pointer is a
sentinel value meaning that there are no more elements in the free list.

* `malloc()`

To allocate space, we identify the first slab containing slots large enough to hold the requested
size (and also satisfy the requested alignment -- see below).

Pop the head element from the free list and return the pointer to that slot.

* `free()`

Push the newly-freed slot (the slot whose first byte is pointed to by the pointer to be freed) onto
the free list of its slab.

* `realloc()`

If the requested new size (and alignment) requires a larger slot than the allocation's current slot,
then allocate a new slot (just like in `malloc()`, above). Then `memcpy()` the contents of the
current slot into the beginning of that new slot, deallocate the current slot (just like in
`free()`, above) and return the pointer to the new slot.

That's it! You could stop reading here and you'd have a basic knowledge of the design of `smalloc`.

## Algorithms in More Detail -- the Free Lists

The `flh` for a given slab is either a sentinel value (meaning that the list is empty), or else it
points to the slot which is the first entry in that slab's free list.

To pop the head entry off of the free list, set the `flh` to point to the next (second) entry
instead of the first entry.

But where is the pointer to the next entry stored? The answer is we store the next-pointers in the
same space where the data goes when the slot is in use! Each data slot is either currently freed,
meaning we can use its space to hold the pointer to the next free list entry, or currently
allocated, meaning it is not in the free list and doesn't have a next-pointer.

This technique is known as an "intrusive free list". Thanks to Andrew Reece and Sam Smith, my
colleagues at Shielded Labs (makers of fine Zcash upgrades), for explaining this to me.

So to satisfy a `malloc()` or `realloc()` by popping the head slot from the free list, take the
value from the `flh`, use that value as a pointer to a slot (the first entry in the free list), and
then read the *contents* of that slot as the pointer to the next entry in the free list. Overwrite
the value in `flh` with the pointer of that *next* entry and you're done popping the head of the
free list.

To push an slot onto the free list (in order to implement `free()`), you are given the slot number
of the slot to be freed. Set the contents of that slot to point to the free list entry that the
`flh` currently points to. Now set the `flh` to point to the new slot. That slot is now the new head
entry of the free list.

### Encoding Slot Numbers In The Free Lists

When memory is first allocated all of its bits are `0`. We use an encoding for pointers to free list
entries such that when all of the bits of the `flh` and all the slots are `0`, then it is a
completely populated free list -- the `flh` points to the first slot number as the first free list
entry, the first free list entry points to the second slot number as the second free list entry, and
so on until the last-numbered slot which points to nothing (a sentinel value meaning "this points to
no slot").

Here's how that encoding works:

The `flh` contains the slot number of the first free list entry. So, when it is all `0` bits, it is
pointing to the slot with slot number `0`.

To get the next-entry pointer of a slot, load the first `4` bytes of the slot, interpret it as a
32-bit unsigned integer, add it to the slot number of the slot, and add `1`, (mod the total number
of slots in that slab plus `1`).

This way, a slot that is initialized to all `0` bits, points to the next slot number as its next
free list entry. The final slot in the slab, when it is all `0` bits, points to no next entry,
because when its first `4` bytes are interpreted as a next-entry pointer, it equals the total number
of slots in that slab, which is the "sentinel value" meaning no next entry.

## Algorithms in More Detail -- Thread-Safe `flh` Update


To make `smalloc` behave correctly under multiprocessing, it is necessary and sufficient to perform
thread-safe updates to `flh`. We use a simple loop with atomic compare-and-exchange operations.

#### To pop an entry from the free list:

1. Load the value from `flh` into a local variable/register, `firstslotnum`. This is the slot number
   of the first entry in the free list.
2. If it is the sentinel value, meaning that the free list is empty, return. (See "Algorithms, More
   Detail -- Overflowers" for how this `malloc()`/`realloc()` request will be handled in this case.)
3. Load the value from first entry into a local variable/register `nextslotnum`. This is the slot
   number of the next entry in the free list (i.e. the second free-list entry), or a sentinel value
   there is if none.
4. Atomically compare-and-exchange the value from `nextslotnum` into `flh` if `flh` still contains
   the value from `firstslotnum`.
5. If the compare-and-exchange failed (meaning the value of `flh` has changed since it was read in
   step 1), jump to step 1.

Now you've thread-safely popped the head of the free list into `firstslotnum`.

#### To push an entry onto the free list, where `newslotnum` is the number of the slot to push:

1. Load the value from `flh` into a local variable/register, `firstslotnum`.
2. Store the value from `firstslotnum` (encoded as a next-entry pointer) into the slot with slot
   number `newslotnum`.
3. Atomically compare-and-exchange the value from `newslotnum` into `flh` if `flh` still contains
   the value from `firstslotnum`.
4. If the compare-and-exchange failed (meaning that value of `flh` has changed since it was read in
   step 1), jump to step 1.

Now you've thread-safely pushed `newslotnum` onto the free list.

### To prevent ABA bugs in updates to the free list head

Store a counter in the most-significant 32-bits of the (64-bit) flh word. Increment that counter
each time you attempt a compare-and-exchange. This prevents ABA bugs in the updates of the `flh`.

Now you know the entire data model and all of the algorithms for `smalloc`!

Except for a few more details about the algorithms:

### Separate Threads Use Separate Slabs

This is not necessary for correctness -- it is just a performance optimization.

There is a global static variable named `GLOBAL_THREAD_NUM`, initialized to `0`.

Each thread has a thread-local variable named `THREAD_NUM` which determines which slab this thread
uses for each size class.

Whenever you choose a slab for `malloc()` or `realloc()`, if it is going to use small slot, then use
this thread's `THREAD_NUM` to determine which slab to use. If this thread's `THREAD_NUM` isn't
initialized, add `1` to `GLOBAL_THREAD_NUM` (mod `64`) and set this thread's `THREAD_NUM` to one
less than the result (mod `64`). Whenever incrementing the `GLOBAL_THREAD_NUM`, use the atomic
`fetch_add(1)` operation for thread-safety.

## Algorithms, More Detail -- Growers

Suppose the user calls `realloc()` and the new requested size is larger than the original
size. Allocations that ever get reallocated to larger sizes often, in practice, get reallocated over
and over again to larger and larger sizes. We call any allocation that has gotten reallocated to a
larger size a "grower".

If the user calls `realloc()` asking for a new larger size, and the new size still fits within the
current slot that the data is already occupying, then just be lazy and consider this `realloc()` a
success and return the current pointer as the return value.

If the new requested size doesn't fit into the current slot, then choose the smallest of the
following list that can hold the new requested size: 64 B (size class 4), 4096 B (size class 10), or
16384 B (size class 12).

If the new requested size doesn't fit into 16384 bytes, then use a slot from the large-slots slab
(size class 13).

As always, if the requested size doesn't fit into a large slot, then fall back to the system
allocator. xxx replace this with larger and larger slots!

## Algorithms, More Detail -- Overflowers

Suppose the user calls `malloc()` or `realloc()` and the slab we choose to allocate from is full,
i.e. the free list is empty. This could happen only if there were that many allocations from that
slab alive simultaneously.

In that case, overflow to the next larger slab. (Thanks to Nate Wilcox for suggesting this technique
to me.) If the slab you are overflowing to is a small-slots slab, use the same thread num you're
currently using.

If all of the slabs you could overflow to are full, then fall back to using the system allocator to
request more memory from the operating system and return the pointer to that. xxx

## The Nitty-Gritty

(The following details are probably not necessary for you to understand unless you're debugging or
modifying `smalloc` or implementing a similar library yourself.)

### Layout

The small-slots slabs are laid out in memory first, with sizeclass most significant, then thread
number, then slot number. Then the medium-slots slabs, with sizeclass most significant, then slot
number. Then the large-slots slab.

For each slab, store its `flh` at the beginning of the slab, where slot number `0` would go, and
then store the actual slots, starting with slot number `0` after the `flh`.

For size class `0`, the `flh` takes up two slot's worth of space (since `flh`'s are `8` bytes and
size class `0` slots are `4` bytes). For all other size classes, there is one slot's worth of space
reserved before slot `0`, and the `flh` occupies the first `8` bytes of that space, and any other
bytes in that space are unused.

Therefore, the number of slots in size class `0` slabs is two fewer than a power of two
(`16,777,214`) and the number of slots in the other size classes is one fewer than a power of two
(`536,870,911` medium slots and `8,388,607` large slots).

### Alignment

There are several constraints on alignment.

1. All `flh`'s have to be 8-byte aligned, for atomic memory access.

2. All slot entries have to be 4-byte aligned, for efficient memory access.

3. Each data slab has to be aligned to page size (which is 4 KiB on Linux and Windows and 16 KiB on
   Macos), for efficient use of memory pages and to satisfy requested alignments (see below).

4. The large-slots slab is additionally aligned to 8 MiB to satisfy larger requested alignments (see
   below).

5. Requested alignments: Sometimes the caller of `malloc()` requires an alignment for the resulting
   memory, meaning that the pointer returned needs to point to an address which is an integer
   multiple of that alignment. Such caller-required alignments are always a power of 2. Because of
   the alignments of the data slabs, slots whose sizes are powers of 2 are always aligned to their
   own size.

In order to be able to satisfy all of these alignments, align `smalloc`'s base pointer to the
largest of the alignments above, which is 8 MiB. (By reserving 8,388,607 extra bytes of address
space in addition to the ones needed for all the data structures above, and then scooting forward
the base pointer to the first 8 MiB boundary.)

Each medium-slots slab, and each group of `32` small-slots slabs, has a total size of a power of
two, and its size is larger than the required alignment of each thing later than it in the layout,
therefore everything will be aligned to its required alignment.

See `Figure 2`:

```text
Figure 2. Layout

offset
------
0      <- system base pointer (returned from `sys_alloc()`)
...    <- up to (8 MiB minus 1 byte) padding if needed so that:
X      <- smalloc base pointer (first 8 MiB boundary)
.------------------------------.
| small-slots slabs            |
| .--------------------------. |
| | sizeclass 0              | |
| | .----------------------. | |
| | | slab 0               | | | 
| | | .------------------. | | | 
| | | | flh              | | | |
| | | | slot 0           | | | |
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,213  | | | |
| | | '------------------' | | |
| | | slab 1               | | | 
| | | .------------------. | | | 
| | | | flh              | | | |
| | | | slot 0           | | | |
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,213  | | | |
| | | '------------------' | | |
| | | ...                  | | |
| | | slab 31              | | | 
| | | .------------------. | | | 
| | | | flh              | | | |
| | | | slot 0           | | | |
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,213  | | | |
| | | '------------------' | | |
| | '----------------------' | |
| '--------------------------' |
| .--------------------------. |
| | sizeclass 1              | |
| | .----------------------. | |
| | | slab 0               | | | 
| | | .------------------. | | | 
| | | | flh              | | | |
| | | | slot 0           | | | |
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,213  | | | |
| | | '------------------' | | |
| | | slab 1               | | | 
| | | .------------------. | | | 
| | | | flh              | | | |
| | | | slot 0           | | | |
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,213  | | | |
| | | '------------------' | | |
| | | ...                  | | |
| | | slab 31              | | | 
| | | .------------------. | | | 
| | | | flh              | | | |
| | | | slot 0           | | | |
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,213  | | | |
| | | '------------------' | | |
| | '----------------------' | |
| '--------------------------' |
| ...                          |
| .--------------------------. |
| | sizeclass 2              | |
| | .----------------------. | |
| | | slab 0               | | | 
| | | .------------------. | | | 
| | | | flh              | | | |
| | | | slot 0           | | | |
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,213  | | | |
| | | '------------------' | | |
| | | slab 1               | | | 
| | | .------------------. | | | 
| | | | flh              | | | |
| | | | slot 0           | | | |
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,213  | | | |
| | | '------------------' | | |
| | | ...                  | | |
| | | slab 31              | | | 
| | | .------------------. | | | 
| | | | flh              | | | |
| | | | slot 0           | | | |
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,213  | | | |
| | | '------------------' | | |
| | '----------------------' | |
| '--------------------------' |
'------------------------------'
.------------------------------.
| medium-slots slabs           |
| .--------------------------. |
| | sizeclass 5              | |
| | .----------------------. | |
| | | flh                  | | |
| | | slot 0               | | |
| | | slot 1               | | |
| | | ...                  | | |
| | | slot 536,870,910     | | |
| | '----------------------' | |
| '--------------------------' |
| .--------------------------. |
| | sizeclass 6              | |
| | .----------------------. | |
| | | flh                  | | |
| | | slot 0               | | |
| | | slot 1               | | |
| | | ...                  | | |
| | | slot 536,870,910     | | |
| | '----------------------' | |
| '--------------------------' |
| ...                          |
| .--------------------------. |
| | sizeclass 12             | |
| | .----------------------. | |
| | | flh                  | | |
| | | slot 0               | | |
| | | slot 1               | | |
| | | ...                  | | |
| | | slot 536,870,910     | | |
| | '----------------------' | |
| '--------------------------' |
'------------------------------'
.------------------------------.
| large-slots slab / sc 13     |
| .--------------------------. |
| | flh                      | |
| | slot 0                   | |
| | slot 1                   | |
| | ...                      | |
| | slot 8,388,606           | |
| '--------------------------' |
'------------------------------'
```

Okay, now you know everything there is to know about `smalloc`'s data model and memory layout. Given
his information, you can calculate the exact location of every data element in `smalloc`! (Counting
from the `smalloc` base pointer, which is the address of the first byte in the layout described
above.)

Thus ends the tour of `smalloc`'s design. Keep reading if you're interested in the rationales for
these design decisions.

# Rationales for Design Decisions

## Rationale for Three Different Kinds of Slots (Small, Medium, Large)

The small slots are for packing multiple slots that are allocated by the same thread into a single
cacheline. The medium slots are for packing multiple slots (allocated by any thread) into a single
virtual memory page. The large slots are for everything that doesn't fit into the first two kinds.

## Rationale for Slot Sizes, Growers, and Overflowers

Rationale for the sizes of small slots: I had a more complicated design to pack more slots into each
cache line at the cost of more complex calculations to map from pointer to sizeclass and vice
versa. Andrew Reece asked me some skeptical questions about that (thanks, Andrew!), and then I did
profiling and benchmarking and couldn't demonstrate much benefit from it, so the current sizes are
chosen for simplicity, including simple calculations to map from pointer to sizeclass and back.

```text
xyz
large slots:
               number that fit
                      into one
           virtual memory page
sizeclass:    size:    (4KiB):
----------   --------   --------
         0     64   B         64
         1    128   B         32
         2    256   B         16
         3    512   B          8
         4   1024   B          4
         5   2048   B          2
         6   4096   B          1
         7   8192   B          0
         8  16384   B          0
         9      4 MiB          0
```

Rationale for the sizes of large slots:

* Large-slot sizeclasses 0-5 are chosen so you can fit multiple of them into a 4 KiB memory page
  (which is the default on Linux, Windows, and Android), while having a simple power-of-2
  distribution that is easy to compute.

* Large-slot sizeclass 6, the 4096 byte slots, is to hold some realloc growers that aren't ever
  going to exceed 4096 bytes, and to be able to copy the data out without touching more than one
  memory page on systems with 4096-byte memory pages, when and if they do exceeed 4096 bytes.

* Large-slab sizeclass 7 -- 8192 bytes -- because you can fit two slots into a memory page on a 16
  KiB memory page system.

* Large-slab sizeclass 8 -- 16,384 bytes -- with the same rationale as for the 4096-byte slots, but
  in this case it helps if your system has 16 KiB memory pages. (Or, I suppose if you have
  hugepages/superpages enabled.)

* Another motivation to include large-slots sizeclasses 6, 7, and 8, is that we can fit only xyz 20
  million huge slots into our virtual xyz large->medium, huge->large address space limitations, and
  the user code could conceivably allocate more than 20 million allocations too big to fit into the
  slots smaller than sizeclass 6.
  
xyz
* The 4 MiB "huge" slots, because according to profiling the Zcash "zebrad" server, allocations of 2
  MiB, 3 MiB, and even 4 MiB are not uncommon.

It's interesting to consider that, aside from the reasons above, there are no other benefits to
having more slabs with slots smaller than "huge". That is, if a slot is too large to fit more than
one into a memory page, and if it isn't likely that we're going to need to copy the entire contents
out for a realloc-grow, then whether the slot is, say, 128 KiB, or 8 MiB makes no difference to the
behavior of the system! The only difference in the behavior of the system is how the virtual memory
pages get touched, which is determined by the user code's memory access patterns, not by the memory
manager. Except per the reasons listed above. If I'm wrong about that please let me know.

Well, there is one more potential reason: if the user code has billions of live large allocations
and/or trillions of live small allocations, you'll need to overflow allocations to other slabs, and
if all of the sufficiently-large slabs are full then you'll have to fall back to the system
allocator.

In practice this seems unlikely to occur in real systems, unless there is a memory leak
(i.e. allocations that are then forgotten about rather than freed), in which case the Overflowers
feature and the additional slot sizes only delay rather than prevent the problem. (But I suppose
that could be good enough if the program finishes its work before it crashes.)

But, in any case, in order to prevent or at least delay a slowdown or a crash in this (rather
extreme) case is the rationale for including the Overflowers feature.

Rationale for promoting growers to 64-byte slots instead of just promoting them directly to the
ultimate huge (4 MiB) slot: 64 bytes *might* be sufficient -- they might stop growing before
exceeding 64 bytes. And if not, then it is going to require only a single cache line access to copy
the data out to the next location.

Rationale for promoting growers to 4096-byte slots: that *might* be sufficient, and if not then it
is going to touch only a single memory page to copy the data out to the next location.

Rationale for promoting growers to 16,384-byte slots: that *might* be sufficient, and we don't have
as many huge slots as we do non-huge slots, and we don't want to the huge-slots slab to fill up.

# Things `smalloc` does not currently attempt to do:

* "Give memory back to the operating system." I don't think that's a real thing. You can't "Give
  memory back to the operating system.". You could release your reservation of some span of virtual
  memory address space, but that wouldn't change anything. It wouldn't make it "easier" or faster
  for any other process to allocate virtual address space -- your process has a separate virtual
  address space from those other processes!
  
  The thing you *can* do, that I think people are getting at when they say that, is you can
  communicate to the operating system that certain virtual memory pages are not in use, so that the
  operating system can choose those pages first when looking for pages to unmap to free up physical
  memory, and it can skip the costly task of writing the contents of those pages out to persistent
  storage (swapping) before reusing their physical memory page frames. (Also, on XNU/Macos, the
  operating system can skip the step of zero'ing out the contents of those pages when and if you
  later start using them again.)
  
  I implemented this in `smalloc` -- communicating this to the operating system through
  `madvise()`/`mach_vm_behavior_set()` -- and benchmarked the time it took to `alloc()` and then
  `dealloc()` a large slot. This behavior of marking virtual memory pages as currently not in use
  increased the "upper bound mean" time as reported by Criterion from 8.4 ns to 1.8 μs -- 200X worse
  latency. That seems like too high a cost to pay on every single call to `alloc()` or `dealloc()` a
  large slot, for the benefit of *occasionally* avoiding unnecessary swaps of a few pages. So I
  removed the implementation of that. You can find [it in the git
  history](https://github.com/zooko/smalloc/blob/86c17f0849cc9debda67d101070f5c0fb687454b/src/lib.rs#L1189).

# Philosophy -- Why `smalloc` is beautiful (in my eyes)

"Allocating" virtual memory (as it is called in Unix terminology) doesn't prevent any other code (in
this process or any other process) from being able to allocate or use memory. It also doesn't
increase cache pressure or cause any other negative effects.

And, allocating one span of virtual memory -- no matter how large -- imposes only a tiny bit of
additional work on the kernel's virtual-memory accounting logic -- a single additional virtual
memory map entry. It's best to think of "allocating" virtual memory as simply *reserving address
space* (which is what they call it in Windows terminology).

The kernel simply ensures that no other code in this process that requests address space will
receive address space that overlaps with this span. (The kernel needs to do this only for other code
running in *this* process -- other processes already have completely separate address spaces which
aren't affected by allocations in this process.)

Therefore, it can be useful to reserve one huge span of address space and then use only a small part
of it, because then you know memory at those addresses space is available to you without dynamically
allocating/reserving more and having to track the resulting separate spans of address space.

This technique is used occasionally in scientific computing, such as to compute over large sparse
matrices, and a limited form of it is used in some of the best modern memory managers like
`mimalloc` and `rpmalloc`, but I'm not aware of this technique being used to the extreme like this
in a memory manager before.

So, if you accept that "avoiding reserving too much virtual address space" is not an important goal
for a memory manager, what *are* the important goals? `smalloc` was designed with the following
goals, written here in roughly descending order of importance:

1. Be simple, in both design and implementation. This helps greatly to ensure correctness -- always
   a critical issue in modern computing. "Simplicity is the inevitable price that we must pay for
   correctness."--Tony Hoare (paraphrased)

   Simplicity also eases making improvements to the codebase and learning from the codebase.

   I've tried to pay the price of keeping `smalloc` simple while designing and implementing it.

2. Place user data where it can benefit from caching.

    1. If a single CPU core accesses different allocations in quick succession, and those
       allocations are packed into a single cache line, then it can execute faster due to having the
       memory already in cache and not having to load it from main memory. This can make the
       difference between a few cycles when the data is already in cache versus tens of cycles when
       it has to load it from main memory. (This is sometimes called "constructive interference" or
       "true sharing", to distinguish it from "destructive interference" or "false sharing" -- see
       below.)

    2. On the other hand, if multiple different CPU cores access different allocations in parallel,
       and the allocations are packed into the same cache line as each other, then this causes a
       substantial performance *degradation*, as the CPU has to stall the cores while propagating
       their accesses of the shared memory. This is called "false sharing" or "destructive cache
       interference". The magnitude of the performance impact is the similar to that of true
       sharing: false sharing can impose tens of cycles of penalty on a single memory access. Worse,
       that penalty might recur over and over on subsequent accesses, depending on the data access
       patterns across cores.

    3. Suppose the program accesses multiple separate allocations in quick succession -- regardless
       of whether the accesses are by the same processor or from different processors. If the
       allocations are packed into the same memory page, this avoids a potentially costly page
       fault. Page faults can cost only a few CPU cycles in the best case, but in case of a TLB
       cache miss they could incur substantially more. In the worst case, the kernel has to load the
       data from swap, which could incur a performance penalty of hundreds of *thousands* of CPU
       cycles or even more, depending on the performance of the persistent storage. Additionally,
       faulting in a page of memory increases the pressure on the TLB cache and the swap subsystem,
       thus potentially causing a performance degradation for other processes running on the same
       system.

   Note that these three goals cannot be fully optimized for by the memory manager, because they
   depend on how the user code accesses the memory. What `smalloc` does is use some simple
   heuristics intended to optimize the above goals under some reasonable assumptions about the
   behavior of the user code:

    1. Try to pack separate small allocations from a single thread together to optimize for
       (constructive) cache-line sharing.

    2. Place small allocations requested by separate threads in separate areas, to minimize the risk
       of destructive ("false") cache-line sharing. This is heuristically assuming that successive
       allocations requested by a single thread are less likely to later be accessed simultaneously
       by multiple different threads. You can imagine user code which violates this assumption --
       having one thread allocate many small allocations and then handing them out to other
       threads/cores which then access them in parallel with one another. Under `smalloc`'s current
       design, this behavior could result in a lot of "destructive cache interference"/"false
       sharing". However, I can't think of a simple way to avoid this bad case without sacrificing
       the benefits of "constructive cache interference"/"true sharing" that we get by packing
       together allocations that then get accessed by the same core.

    3. When allocations are freed by the user code, `smalloc` appends their slot to a free
       list. When allocations are subsequently requested, the most recently free'd slots are
       returned first. This is a LIFO (stack) pattern, which means user code that tends to access
       its allocations in a stack-like way will enjoy improved caching. (Thanks to Andrew Reece from
       Shielded Labs for teaching me this.)

    4. For allocations too large to pack multiple of them into a single cache line, but small enough
       to pack multiple into a single virtual memory page, `smalloc` attempts to pack multiple into
       a single memory page. It doesn't separate allocations of these sizes by thread, the way it
       does for small allocations, because there's no performance penalty when multiple cores access
       the same memory page (but not the same cache line) in parallel. In fact it is a performance
       benefit for them to share caching of the virtual memory page -- it is a form of "constructive
       cache interference" or "true cache sharing".

3. Be efficient when executing `malloc()`, `free()`, and `realloc()`. I want calls to those
   functions to execute in as few CPU cycles as possible. I optimistically think `smalloc` is going
   to be great at this goal! The obvious reason for that is that the code implementing those three
   functions is *very simple* -- it needs to execute only a few XYZ MEASURE THIS CPU instructions to
   implement each of those functions.

   A perhaps less-obvious reason is that there is *minimal data-dependency* in those code paths.

   Think about how many loads of memory from different locations, and therefore
   potential-cache-misses, your process incurs to execute `malloc()` and then to write into the
   memory that `malloc()` returned. It has to be at least one, because you are going to pay the cost
   of a potential-cache-miss to write into the memory that `malloc()` returned.

   To execute `smalloc`'s `malloc()` and then write into the resulting memory incurs, in the common
   cases, only two or three potential cache misses.

   The main reason `smalloc` incurs so few potential-cache-misses in these code paths is the
   sparseness of the data layout. `smalloc` has pre-reserved a vast swathe of address space and
   "laid out" unique locations for all of its slabs, slots, and variables (but only virtually --
   "laying the locations out" in this way does not involve reading or writing any actual memory).
    
   Therefore, `smalloc` can calculate the location of a valid slab to serve this call to `malloc()`
   using only one or two data inputs: One, the requested size and alignment (which are on the stack
   in the function arguments and do not incur a potential-cache-miss) and two -- only in the case of
   allocations small enough to pack multiple of them into a cache line -- the thread number (which
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
   code accesses the memory returned from `malloc()` after `malloc()` returns, there is no
   additional cache-miss penalty from `malloc()` accessing it before returning. Likewise, if the
   user code has recently accessed the memory to be freed before calling `free()` on it, then
   `smalloc`'s access of the same space to store the next free-list pointer will incur no additional
   cache-miss. (Thanks to Sam Smith from Shielded Labs for teaching me this.)

   So to sum up, here are the counts of the potential-cache-line misses for the common cases:

   1. To `malloc()` and then write into the resulting memory:
      * If the allocation size is <= 64 bytes, then:
         * 🟠 one to access the `THREAD_AREANUM`
         * 🟠 one to access the `flh`
         * 🟠 one to access the intrusive free list entry
         * 🟢 no additional cache-miss for the user code to access the data

      For a total of 3 potential-cache-misses.

      * If the allocation size is > 64, then:
         * 🟠 one to access the `flh`
         * 🟠 one to access the intrusive free list entry
         * 🟢 no additional cache-miss for the user code to access the data
     
      For a total of 2 potential-cache-misses.

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
   
   A similar property holds for the potential cache-miss of accessing the `THREAD_AREANUM` -- if
   this thread has recently called `malloc()`, `free()`, or `realloc()` for a small slot, then the
   `THREAD_AREANUM` will likely already be in cache, but if this thread has not made such a call
   recently then it would likely cache-miss.
   
   And of course a similar property holds for the potential cache-miss of accessing the `flh` -- if
   this thread (for small-slot slabs), or any thread (for medium- or large-slot slabs) has recently
   called `malloc()`, `free()`, or `realloc()` for an allocation of this size class, then the `flh`
   for this slab will already be in cache.

4. Be *consistently* efficient.

   I want to avoid intermittent performance degradation, such as when your function takes little
   time to execute usually, but occasionally there is a latency spike when the function takes much
   longer to execute.

   I also want to minimize the number of scenarios in which `smalloc`'s performance degrades due to
   the user code's behavior triggering an "edge case" or a "worst case scenario" in `smalloc`'s
   design.
    
   The story sketched out above about user code allocating small allocations on one thread and then
   handing them out to other threads to access is an example of how user code behavior could trigger
   a performance degradation in `smalloc`.

   xxx other problem: heavy multithreading contention -- currently it is inefficient at this

   On the bright side, I can't think of any *other* "worst case scenarios" for `smalloc` beyond that
   one. In particular, `smalloc` never has to "rebalance" or re-arrange its data structures, or do
   any "deferred accounting". This nicely eliminates some sources of intermittent performance
   degradation. (See [this blog post](https://pwy.io/posts/mimalloc-cigarette/) and [this
   one](https://hackmd.io/sH315lO2RuicY-SEt7ynGA?view#jemalloc-purging-will-commence-in-ten-seconds)
   for cautionary tales of how deferred accounting, while it can improve performance in the "hot
   paths", can also give rise to edge cases that can occasionally degrade performance or cause other
   problems.)

   There are no locks in `smalloc`[^2], so it will hopefully handle heavy multi-processing
   contention (i.e. many separate cores allocating and freeing memory simultaneously) with
   consistent performance.
   
   There *are* concurrent-update loops in `malloc` and `free` -- see the pseudo-code in "Thread-Safe
   State Changes" above -- but these are not locks. Whenever multiple threads are running that code,
   one of them will make progress (i.e. successfully update the `flh`) after it gets only a few CPU
   cycles, regardless of what any other threads do. And, if any thread becomes suspended in that
   code, one of the *other*, still-running threads will be the one to make progress (update the
   `flh`). Therefore, these concurrent-update loops cannot cause a pile-up of threads waiting for a
   (possibly-suspended) thread to release a lock, nor can they suffer from priority inversion.

    [^2]: ... except in the initialization function that acquires the lock only one time -- the
    first time `alloc()` is called.
    
   So with the possible exception of the (hopefully rare) "worst-case scenario" described above, I
   optimistically expect that `smalloc` will show excellent and extremely consistent performance.

   XXX This bit about the lock-free approach seems to have been a bit over-optimistic -- if you run
   one of the unit tests which spawns 1000 threads and has all of them hammer on the same `flh` as
   fast as they can, it takes a heck of a long time to complete. :-{ Like 500 seconds.

5. Efficiently support using `realloc()` to extend vectors. `smalloc`'s initial target user is Rust
   code, and Rust code uses a lot of Vectors, and not uncommonly it grows those Vectors dynamically,
   which results in a call to `realloc()` in the underlying memory manager. I hypothesized that this
   could be a substantial performance cost in real Rust programs. I profiled a Rust application (the
   "Zebra" Zcash full node) and observed that it did indeed call `realloc()` quite often, to resize
   an existing allocation to larger, and in many cases it did so repeatedly in order to enlarge a
   Vector, then fill it with data until it was full again, and then enlarge it again, and so
   on. This can result in the underlying memory manager having to copy the contents of the Vector
   over and over. `smalloc()` optimizes out almost all of that copying of data, with the simple
   expedient of jumping to a much larger slot size whenever `realloc()`'ing an allocation to a
   larger size (see "Algorithms, More Detail -- Growers", above). My profiling results indicate that
   this technique would indeed eliminate more than 90% of the memory-copying when extending Vectors,
   making it almost costless to extend a Vector any number of times (as long as the new size doesn't
   exceed the size of `smalloc`'s large slots: 8 MiB. In the profiling of Zcash Zebra, most of the
   large vectors topped out at 4 MiB, except for a few that topped out at 5 MiB).

   I am hopeful that `smalloc` may achieve all five of these goals. If so, it may turn out to be a
   very useful tool!

6. Where possible without sacrificing the higher priorities listed above, I'd like `smalloc` to be
   at least *somewhat* helpful to the author of the calling code, and at least *somewhat* difficult
   for an attacker to exploit, if there are bugs in that code.
   
   The main line of defense here is `smalloc`'s simplicity (priority 1, above). Simplicity can help
   people detect and fix bugs in the code of `smalloc` itself, which are more dangerous! And its
   simplicity and predictability can make it easier for the authors of user code to detect and fix
   bugs in their code. It also makes it easier for security researchers to determine the conditions
   under which a program would be exploitable.
   
   Another line of defense is an accidental side-effect of `smalloc`'s performance-oriented
   design. It groups allocations into size classes and it uses indexes instead of pointers
   internally. These two factors can make some kinds of heap exploits more difficult. The difference
   between indexes and pointers is that indexes are more narrowly-scoped -- only into that one slab
   and only aligned for slots of that size.
   
   That narrowing of pointers into indexes is `assert()`'ed in `dealloc()` and `realloc()` before
   using the resulting indexes.
   
   (To inform this decision, I read
   [1](https://security.apple.com/blog/towards-the-next-generation-of-xnu-memory-safety/),
   [2](https://googleprojectzero.blogspot.com/2025/03/blasting-past-webp.html), and a couple of
   slide decks by offensive security researchers about heap exploitation:
   [3](https://blackwinghq.com/blog/posts/playing-with-libmalloc/),
   [4](https://www.synacktiv.com/ressources/Sthack_2018_Heapple_Pie.pdf).)


# Open Issues / Future Work

* add benchmarks of 1000-threads-colliding tests e.g. `threads_1000_large_alloc_dealloc_x()`

* experiment with std::intrinsics::likely/unlikely

* Port to Cheri, add capability-safety

* Try adding a dose of quint, VeriFast, *and* Miri! :-D

* And Loom! |-D

* And llvm-cov's Modified Condition/Decision Coverage analysis. :-)

* and cargo-mutants

* Benchmark replacing `compare_exchange_weak` with `compare_exchange` (on various platforms with
  various levels of multithreading contention).

* The current design and implementation of `smalloc` is "tuned" to 64-byte cache lines and 4096-bit
  virtal memory pages, but with "tunings" that hopefully also make it perform well for 128-byte
  cache lines and 16 KiB pages.

  It works correctly with larger page tables but there might be performance problems with extremely
  large ones -- I'm not sure. Notably "huge pages" of 2 MiB or 1 GiB are sometimes configured in
  Linux especially in "server"-flavored configurations.

* If we could allocate even more virtual memory address space, `smalloc` could more scalable (eg
  huge slots could be larger than 4 mebibytes, the number of per-thread areas could be greater than
  64), it could be even simpler (eg maybe just remove the (quite complex!)  overflow algorithm, and
  the special-casing of the number of slots for the huge-slots slab), and you could have more than
  one `smalloc` heap in a single process. Larger (than 48-bit) virtual memory addresses are already
  supported on some platforms/configurations, especially server-oriented ones, but are not widely
  supported on desktop and smartphone platforms. We could consider creating a variant of `smalloc`
  that works only platforms with larger (than 48-bit) virtual memory addresses and offers these
  advantages. TODO: make an even simpler smalloc ("ssmalloc"??) for 5-level-page-table systems.

* ? Stop trying to use the `mach_vm_alloc()`/`mach_vm_remap()` API on Macos and switch to using the
  `libmalloc` API? (There are numerous problems with our current attempts to use the former. Unclear
  if this is due to bugs in the kernel or our misunderstandings of how to use that API.)

* Crib from parking_lot and/or static_locks or something for how to ask the OS to put a thread to
  sleep when it has encountered an flh collision, and then wake it again the next time a different
  thread successfully updates the flh

* Rewrite it in Zig. :-)

* profiling (cachegrind?)

* use a profiler on the dealloc functions of smalloc-v2 and smalloc-v3

* Try bma_benchmark, tango_bench, yab, iai-callgrind instead of Criterion

* Get an AI to review the code. :-)

* CI for benchmarks? 🤔

* According to Dino Dai Zovi's 2009 presentation on Macos heap exploitation, `libmalloc`'s size
  classes at that time were: tiny: <= 496 bytes, 16-byte quantum, small: <= 15,360 bytes, 512-byte
  quantum, large: <= 16,773,120 bytes, 4 KiB pages, huge: > 16,773,120, 4 KiB pages; consider
  renaming `smalloc`'s small/large/huge to tiny/small/large!? (and "huge" for smalloc means fall
  back to system). According tyo Eloi Benoist-Vanderbeken's 2018 presentation it has been updated
  to: tiny: <= 1008 bytes/16 B quantums, small: if machine has < 1 GiB RAM then <= 15 KiB else <=
  127 KiB; 512 B quantums, else large; There is another allocator, the undocumented "nano
  allocator". According to Josh Pitts's post "Playing with libmalloc in 2024", the nano allocator is
  for <= 255 bytes/16 B quantums, small is for [1009 B - 32 KiB]; 512 B quantums, Medium is (32
  KiB - 8192 KiB]; 32 KiB quantums, large is for > 8192 KiB; According to wangshuo's article on
  "Glibc Malloc Principle" on OpenEuler from 2021, glibc fast-bin's manage chunks <= 160
  B. glibc-`M_MMAP_THRESHOLD` is default 128 KiB but can dynamically grow when larger allocations
  are requested. But according to sploitfun's post "Understanding glibc malloc" from 2015, fast bins
  actually hold {16-64}. It says large is >?= 512 B, within large there are 512, 4096, 32768, and
  262144 byte distinctions

* document why note try falling back to system `malloc()` instead of system `mach_vm_alloc()`/`mmap()`

* configure rustfmt and emacs to do what i want with formatting

* Iai

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
    
* try adding some newtypey goodness! (mediumslabnum/smallslabnum)

* add support for the [new experimental Rust Allocator
  API](https://doc.rust-lang.org/nightly/std/alloc/trait.Allocator.html)

* add initialized-to-zero alloc alternative, relying on kernel 0-initialization when coming from eac

* make it usable as the implementation `malloc()`, `free()`, and `realloc()` for native code. :-)
  (Nate's suggestion.)

* Rewrite it in Odin. :-) (Sam and Andrew's recommendation -- for the programming language, not for
  the rewrite.)

* investigate using the #[noalias] annotation

* Try "tarpaulin" again HT Sean Bowe

* Try madvise'ing to mark pages as reusable but only when we can mark a lot of pages at once (HT Sam Smith)

* Put back the fallback to mmap for requests that overflow.??? Or document why not.

* try again to get the debug/test/benchmark stuff out of the end of $WORKSPACE/smalloc/src/lib.rs

* automatically detect and use 5lpt?

* make it run benchmarks when you do `cargo run -p bench`, like iroh quinn does?

* recreate the "inner_alloc" function which takes a sizeclass instead of a Layout, and let realloc and the overflow behavior of alloc use inner_alloc instead of alloc. (And benchmark it of course.)

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
