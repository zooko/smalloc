xyz make sure small slabs are now described as areas containing slots... err... or... whatever way makes most sense and is most consistent with the code . i guess stop calling it a "slabnum" and start calling it a "size class" or something?

# smalloc -- a simple memory allocator

`smalloc` is a memory allocator, suitable (I hope) as a drop-in replacement for `ptmalloc` (the
glibc memory allocator), `libmalloc` (the Macos userspace memory allocator), `jemalloc`, `mimalloc`,
`snmalloc`, `rpmalloc`, etc.

`smalloc` offers performance properties comparable to the other memory managers, while being
simpler. The current implementation is only 903 lines of Rust code (excluding comments, tests,
benchmarks, etc).

# Caveats

No warranty! Not supported. Never been security audited. First time Rust project (and Jack O'Connor
told me, laughing, that a low-level memory manager was "worst choice ever for a first-time Rust
project"). There is no security contact information nor anyone you can contact for help using this
code.

Use at your own risk!

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

To run the tests:

```text
cargo --frozen nextest run tests::
```

And look at stdout to see the test results.

To run the benchmarks:

```text
cargo --frozen nextest run --release benches::
```

And open the `index.html` files in `./target/criterion/*/report/` to view the benchmark results.

# How it works

To understand how it works, you need to know `smalloc`'s data model
and the algorithms.

## Data model

### Data Slots and Slabs, Simplified

All memory managed by `smalloc` is organized into "slabs". A slab is a fixed-length array of
fixed-length "slots" of bytes. Every pointer returned by a call to `smalloc`'s `malloc()` or
`free()` is a pointer to the beginning of one of those slots, and that slot is used exclusively for
that memory allocation until it is `free()`'ed[^1].

[^1]: Except for calls to `malloc()` or `realloc()` for sizes that are too big to fit into even the
    biggest of `smalloc`'s slots, which `smalloc` instead satisfies by falling back to the system
    allocation, i.e. `mmap()` on Linux, `vm_mach_allocate()` on Macos, etc.

There are three kinds of slots: "small", "medium", and "large". (See the "Rationales for Design
Decisions" section below for the motivation for having three different kinds.)

The small slots are in 5 slabs, each slot has this size, and each slab has this many slots:

```text
small slots:
slabnum:     size:     numslots:
--------    ------     ---------
       0       4 B    16,777,216
       1       8 B    16,777,216
       2      16 B    16,777,216
       3      32 B    16,777,216
       4      64 B    16,777,216
```

The medium slots are in 8 slabs, and have these sizes and numbers:

```text
medium slots:
slabnum:     size:     numslots:
--------    ------     ---------
       0     128 B   536,870,912
       1     256 B   536,870,912
       2     512 B   536,870,912
       3    1024 B   536,870,912
       4    2048 B   536,870,912
       5    4096 B   536,870,912
       6    8192 B   536,870,912
       7   16384 B   536,870,912
```

The large slots are in 1 slab, and are of this size and number:

```text
large slots:
slabnum:     size:     numslots:
--------    ------     ---------
     n/a    64 MiB     1,048,576
```

Slots are 0-indexed, so the largest slot number in each small-slots slab is 16,777,215, the largest
slot number in each medium-slots slab is 536,870,911, and the largest slot number in the large-slots
slab is 1,048,575.

In Figure 1, `[data]` means a span of memory (of that slab's slot-size), a pointer to which can be
returned from `malloc()` or `realloc()` for use by the caller:

```text
Figure 1. Organization of data slots.

       slot # -> slot 0      slot 1      ... slot 16,777,215
                 ------      ------          ---------------
small slots:
slab # slot size
------ ---------
                 .---------. .---------.     .---------. 
     0       4 B | [data]  | | [data]  | ... | [data]  |
                 .---------. .---------.     .---------.
     1       8 B | [data]  | | [data]  | ... | [data]  |
                 '---------' '---------'     '---------'
                     ...         ...             ...
                 .---------. .---------.     .---------.
     4      64 B | [data]  | | [data]  | ... | [data]  |
                 '---------' '---------'     '---------'

       slot # -> slot 0      slot 1      ... slot 536,870,911
                 ------      ------          ----------------
medium slots:
slab # slot size
------ ---------
                 .---------. .---------.     .---------.
     0     128 B | [data]  | | [data]  | ... | [data]  |
                 .---------. .---------.     .---------.
     1     256 B | [data]  | | [data]  | ... | [data]  |
                 '---------' '---------'     '---------'
                     ...         ...             ...
                 .---------. .---------.     .---------.
     7   16384 B | [data]  | | [data]  | ... | [data]  |
                 '---------' '---------'     '---------'

       slot # -> slot 0      slot 1      ... slot 1,048,575
                 ------      ------          --------------
large slots:
slab # slot size
------ ---------
                 .---------. .---------.     .---------.
   n/a     4 MiB | [data]  | | [data]  | ... | [data]  |
                 '---------' '---------'     '---------'
```

### Variables

For each slab, there are two associated variables: the index of the most-recently freed slot, and
the count of ever-allocated slots.

The index of the most-recently freed slot is also known as the "free list head" and is abbreviated
`flh`.

The count of ever-allocated slots (abbreviated `eac`) is the number of slots in that slab have ever
been allocated (i.e. has a pointer ever been returned by `malloc()` or `realloc()` that points to
that slot).  `flh` and `eac` are each 8 bytes in size.

```text
Figure 2. Organization of variables.

 * "flh" is "free-list head"
 * "eac" is "ever-allocated count"

small slots:
slab #        variable
------        --------
              .-----. .-----.
     0        | flh | | eac |
              .-----. .-----.
     1        | flh | | eac |
              '-----' '-----'
   ...          ...     ...
              .-----. .-----.
     4        | flh | | eac |
              '-----' '-----'

medium slots:
slab #        variable
------        --------
              .-----. .-----.
     0        | flh | | eac |
              .-----. .-----.
     1        | flh | | eac |
              '-----' '-----'
   ...          ...     ...
              .-----. .-----.
     7        | flh | | eac |
              '-----' '-----'

large slots:
slab #        variable
------        --------
              .-----. .-----.
   n/a        | flh | | eac |
              '-----' '-----'
```

That's it! Those are all the data elements in `smalloc`!

(Except for one more modification we'll describe below -- see "Separate Area For Multiprocessing".)

## Algorithms, Simplified

Here is a first pass describing simplified versions of the
algorithms. After you learn these simple descriptions, keep reading
for additional detail.

* `malloc()`

To allocate space, we identify the first slab containing slots large enough to hold the requested
size (and also satisfy the requested alignment -- see below).

If the free list is non-empty, pop the head element from it and return the pointer to that slot.

If the free list is empty, increment the ever-allocated-count, `eac`, and return the pointer to the
newly-allocated slot.

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

The `flh` for a given slab is either a sentinel value meaning that the free list is empty, or else
it is the index of the slot most recently pushed onto the free list.

To satisfy a `malloc()`, first check if the free list for your chosen slab is non-empty, i.e. if its
`flh` is not the sentinel value. If so, then the `flh` is the index in this slab of the current
most-recently-freed slot.

We need to pop the head item off of the free list, i.e. set the `flh` to point to the next item
instead of the head item.

But where is the pointer to the next item stored? The answer is we store the next-pointers in the
same space where the data goes when the slot is in use! Each data slot is either currently freed,
meaning we can use its space to hold the next-pointer, or currently allocated, meaning it is not in
the free list and doesn't need a next-pointer.

This technique is known as an "intrusive free list". Thanks to Andrew Reece and Sam Smith, my
colleagues at Shielded Labs, for explaining this to me.

In Figure 3, `[data or slot #]` means a span of memory (of that slab's slot-size) that can hold user
data *or* the first 4 bytes of which can hold the slot number of the next free slot.

```text
Figure 3. Intrusive free lists

       slot # -> slot 0             slot 1             ... slot 16,777,215
                 ------             ------                 ---------------
small slots:
slab # slot size
------ ---------
                 .----------------. .----------------.     .----------------.
     0       4 B | data or slot # | | data or slot # | ... | data or slot # |
                 .----------------. .----------------.     .----------------.
     1       8 B | data or slot # | | data or slot # | ... | data or slot # |
                 '----------------' '----------------'     '----------------'
    ...                  ...                ...                    ...
                 .----------------. .----------------.     .----------------.
     4      64 B | data or slot # | | data or slot # | ... | data or slot # |
                 '----------------' '----------------'     '----------------'

       slot # -> slot 0             slot 1             ... slot 536,870,911
                 ------             ------                 ----------------
medium slots:
slab # slot size
------ ---------
                 .----------------. .----------------.     .----------------.
     0     128 B | data or slot # | | data or slot # | ... | data or slot # |
                 .----------------. .----------------.     .----------------.
     1     256 B | data or slot # | | data or slot # | ... | data or slot # |
                 '----------------' '----------------'     '----------------'
    ...                  ...                ...                    ...
                 .----------------. .----------------.     .----------------.
     7   16384 B | data or slot # | | data or slot # | ... | data or slot # |
                 '----------------' '----------------'     '----------------'

       slot # -> slot 0             slot 1             ... slot 1,048,575
                 ------             ------                 --------------
large slots:
slab # slot size
------ ---------
                 .----------------. .----------------.     .----------------.
   n/a     4 MiB | data or slot # | | data or slot # | ... | data or slot # |
                 '----------------' '----------------'     '----------------'
```

So to satisfy a `malloc()` or `realloc()` by popping the head item from the free list, what you do
is take the `flh` and read the *contents* of the indicated slot to find the index of the *next* item
in the free list. Set `flh` equal to the index of that next item and you're done popping the head of
the free list.

To push an item onto the free list (in order to implement `free()`), you are given the slot number
of the item to be freed. Take the current `flh` and copy its value into the data slot of the item to
be freed. Now set the `flh` to be the index of the item to be freed. That item is now the new head
of the free list.

If there are no items on the free list when you are satisfying a `malloc()` or `realloc()`, then you
increment the ever-allocated-count, `eac`, and return a pointer to the next never-before-allocated
slot.

## Algorithms in More Detail -- Multiprocessing

To make `smalloc` perform well with multiple cores operating in parallel, we need to add only two
modifications to the design above.

### Separate Areas For Multiprocessing

Replicate the data structures for the small-slot slabs into 32 identical "areas" that'll each
(typically) be accessed by a different thread. This is not necessary for correctness, it is just a
performance optimization.

In Figure 4, `[data]` means the same thing as `[data or slot #]` does in Figure 3.

```text
Figure 4. Organization of data slots including areas.

            area 0                                        area 1                             ... areas 2-30 ... area 31
        /-----------------------------------------\   /-----------------------------------------\     |     /-----------------------------------------\
        slot 0      slot 1      ... slot 16,777,215   slot 0      slot 1      ... slot 16,777,215     |     slot 0      slot 1      ... slot 16,777,216 
        ------      ------          ---------------   ------      ------          ---------------     |     ------      ------          ---------------
small slots:                                                                                          v
slab #
------
        .---------. .---------.     .---------.       .---------. .---------.     .---------.               .---------. .---------.     .---------. 
     0  | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |        ...    | [data]  | | [data]  | ... | [data]  |
        .---------. .---------.     .---------.       .---------. .---------.     .---------.               .---------. .---------.     .---------. 
     1  | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |        ...    | [data]  | | [data]  | ... | [data]  |
        '---------' '---------'     '---------'       '---------' '---------'     '---------'               '---------' '---------'     '---------' 
            ...         ...             ...               ...         ...             ...                       ...         ...             ...
        .---------. .---------.     .---------.       .---------. .---------.     .---------.               .---------. .---------.     .---------. 
     4  | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |        ...    | [data]  | | [data]  | ... | [data]  |
        '---------' '---------'     '---------'       '---------' '---------'     '---------'               '---------' '---------'     '---------' 
          
        slot 0      slot 1      ... slot 536,870,911
        ------      ------          ----------------
medium slots:
slab #
------
        .---------. .---------.     .---------.
     0  | [data]  | | [data]  | ... | [data]  |
        .---------. .---------.     .---------.
     1  | [data]  | | [data]  | ... | [data]  |
        '---------' '---------'     '---------'
            ...         ...             ...
        .---------. .---------.     .---------.
     7  | [data]  | | [data]  | ... | [data]  |
        '---------' '---------'     '---------'

        slot 0      slot 1      ... slot 1,048,575
        ------      ------          --------------
large slots:
slab #
------
        .---------. .---------.     .---------.
   n/a  | [data]  | | [data]  | ... | [data]  |
        '---------' '---------'     '---------'
```

And add space for each area for the variables for the small-slot slabs.

```text
Figure 5. Organization of variables including areas.

 * "flh" is "free-list head"
 * "eac" is "ever-allocated count"

   area # ->      area 0           area 1      ...     area 31
                  ------           ------              -------
small slots:
slab #        variable
------        --------
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     0        | flh | | eac |  | flh | | eac | ... | flh | | eac |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     1        | flh | | eac |  | flh | | eac | ... | flh | | eac |
              '-----' '-----'  '-----' '-----'     '-----' '-----'
                ...     ...      ...     ...         ...     ...
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     4        | flh | | eac |  | flh | | eac | ... | flh | | eac |
              '-----' '-----'  '-----' '-----'     '-----' '-----'
          
medium slots:
              .-----. .-----.
     0        | flh | | eac |
              .-----. .-----.
     1        | flh | | eac |
              '-----' '-----'
   ...          ...     ...
              .-----. .-----.
     7        | flh | | eac |
              '-----' '-----'

large slots:
              .-----. .-----.
   n/a        | flh | | eac |
              '-----' '-----'
```

There is a global static variable named `GLOBAL_THREAD_AREANUM`, initialized to 0.

Each thread has a thread-local variable named `THREAD_AREANUM` which determines which area this
thread uses for the small-slots slabs.

Whenever you choose a slab for `malloc()` or `realloc()`, if it is going to use small slot, then use
this thread's `THREAD_AREANUM` to determine which area to use. If this thread's `THREAD_AREANUM`
isn't initialized, add 1 to `GLOBAL_THREAD_AREANUM` and set this thread's `THREAD_AREANUM` to one
less than the result, mod 64.

### Thread-Safe State Update

Use thread-safe algorithms to update the free lists and the ever-allocated-counts. This is necessary
and sufficient to ensure correctness of `smalloc`'s behavior under multiprocessing.

Specifically, we use a simple loop with atomic compare-and-exchange or fetch-and-add operations.

#### To pop an element to the free list:

1. Load the value from `flh` into a local variable/register, `firstindex`. This is the index of the
   first entry in the free list.
2. If it is the sentinel value, meaning that the free list is empty, return. (This
   `malloc()`/`realloc()` will then be satisfied from the never-yet-allocated slots instead.)
3. Load the value from the free list slot indexed by `firstindex` into a local variable/register
   `nextindex`. This is the index of the next entry in the free list (i.e. the second entry), or a
   sentinel value there is if none.
4. Atomically compare-and-exchange the value from `nextindex` into `flh` if `flh` still contains the
   value in `firstindex`.
5. If the compare-and-exchange failed (meaning the value of `flh` has changed), jump to step 1.

Now you've thread-safely popped the head of the free list into `firstindex`.

#### To push an element onto the free list, where `newindex` is the index to be added:

1. Load the value from `flh` into a local variable/register, `firstindex`.
2. Store the value from `firstindex` into the free list element with index `newindex`.
3. Atomically compare-and-exchange the index `newindex` into `flh` if `flh` still contains the value
   in `firstindex`.
4. If the compare-and-exchange failed (meaning that value of `flh` has changed), jump to step 1.

Now you've thread-safely pushed `i` onto the free list.

### To prevent ABA bugs in updates to the free list head

Store a counter in the most-significant 32-bits of the (64-bit) flh word. Increment that counter
each time you attempt a compare-and-exchange. This prevents ABA bugs in the updates of the `flh`.

#### To increment `eac`:

1. Fetch-and-add 1 to the value of `eac`.
2. If the result is greater than the max slot number for that slab, meaning that the slab was
   already full, then fetch-and-add -1. (This `malloc()`/`realloc()` will then be satisfied by
   falling back to the system allocator instead.)

Now you've thread-safely incremented `eac`.

Finally, whenever incrementing the global `GLOBAL_THREAD_AREANUM`, use the atomic `fetch_add(1)`.

Now you know the entire data model and all of the algorithms for `smalloc`!

Except for a few more details about the algorithms:

## Algorithms, More Detail -- Growers

Suppose the user calls `realloc()` and the new requested size is larger than the original
size. Allocations that ever get reallocated to larger sizes often, in practice, get reallocated over
and over again to larger and larger sizes. We call any allocation that has gotten reallocated to a
larger size a "grower".

If the user calls `realloc()` asking for a new larger size, and the new size still fits within the
current slot that the data is already occupying, then just be lazy and consider this `realloc()` a
success and return the current pointer as the return value.

If the new requested size doesn't fit into the current slot, then choose the smallest of the
following list that can hold the new requested size: 64 B (small-slots slab 4), 4096 B (medium-slots
slab 5), or 16384 B (medium-slots slab 7).

If the new requested size doesn't fit into 16384 bytes, then use a slot from the large-slots slab.

As always, if the requested size doesn't fit into a large slot, then fall back to the system
allocator.

## Algorithms, More Detail -- Overflowers

Suppose the user calls `malloc()` or `realloc()` and the slab we choose to allocate from is
full. That means the free list is empty, and the ever-allocated-count is greater than or equal to
the number of slots in that slab. This could happen only if there were that many allocations from
that slab alive simultaneously.

In that case, if this is a small-slots slab, then find the area whose slab of this same slab number
has the lowest `eac`. Loop over each other area, checking its `eac` and remembering the lowest one
you've found so far. In order check its `eac` you actually have to `fetch_add(1)` to it so that if
another thread is changing the `eac` at the same time, you'll actually have reserved that slot. This
also means you have to either use that slot or push it onto that slab's free list before you forget
about it.

If you find a slab with `eac` 0, short-circuit the loop and use that area.

Once you've either reserved slot 0 in a slab, or else completed the traversal of all the areas and
found the lowest-`eac` and reserved a slot in it, then set your `THREAD_AREANUM` to that area
number, and that slot of that area to satisfy this request.

When doing this, traverse the areas in a permutation by adding 19 mod 32 instead of adding 1 mod 32,
in order to reduce the chance of your search overlapping with operations of any other threads whose
first allocation was after your thread's first allocation (since they got `THREAD_AREANUM`'s
incrementally higher than yours).

If you've searched all areas and you weren't able to allocate any slot -- meaning that all slabs of
this slab number in all areas were full -- then increase the slab number and try again. If this was
already the largest small-slots slab, then switch to the smallest large-slots slab.

If the this is a large-slots slab and it is full, then overflow to the next larger slab. (Thanks to
Nate Wilcox for suggesting this technique to me.)

If all of the slabs you could overflow to are full, then fall back to using the system allocator to
request more memory from the operating system and return the pointer to that.

## The Nitty-Gritty

(The following details are probably not necessary for you to understand unless you're debugging or
modifying `smalloc` or implementing a similar library yourself.)

### Layout

The small-slots slabs are laid out in memory first, with slab number most significant, then area
number, then slot number.

Then the medium-slots slabs, with slab number most significant, then slot number.

Then large-slots slab.

Last comes the variables, starting with variables of the small slots slabs, with area number most
significant and then slab number.

Then the variables of the medium slots slabs, and finally the variables of the large slots slab.

Note that the layout of the data slots for the small slots slabs (slab number most significant --
row-wise when looking at Figure 4) is different than the layout of the variables for the small slots
slabs (area number most significant -- column-wise when looking at Figure 5).

### Alignment

There are several constraints on alignment.

1. All `eac`'s and all `flh`'s have to be 8-byte aligned, for atomic memory access.

2. Each data slab has to be aligned to page size (which is 4 KiB on Linux and Windows and 16 KiB on
   Macos), for efficient use of memory pages and to satisfy requested alignments (see below).

3. The large-slots slab is additionally aligned to 64 MiB to satisfy larger requested alignments
   (see below).

4. Requested alignments: Sometimes the caller of `malloc()` requires an alignment for the resulting
   memory, meaning that the pointer returned needs to point to an address which is an integer
   multiple of that alignment. Such caller-required alignments are always a power of 2. Because of
   the alignments of the data slabs, slots whose sizes are powers of 2 are always aligned to their
   own size.

In order to be able to satisfy all of these alignments, align `smalloc`'s base pointer to the
largest of the alignments above, which is 64 MiB. (By reserving 67,108,863 extra addresses in
addition to the ones needed for all the data structures above, and then scooting forward the base
pointer to the first 64 MiB boundary.)

Then the rest of the alignments occur "naturally", because all of the things laid out before each
thing that needs to be aligned are themselves a power-of-2 in size and are larger than the required
alignment of the next thing. See `Figure

Okay, now you know everything there is to know about `smalloc`'s data model and memory layout. Given
this information, you can calculate the exact address of every data element in `smalloc`! (Counting
from the `smalloc` base pointer, which is the address of the first byte in the layout described
above.)

```text
Figure 6. Layout and alignment

offset
------
0      <- system base pointer (returned from `sys_alloc()`)
...    <- up to (64 MiB minus 1 byte) padding if needed so that:
X      <- smalloc base pointer (first 64 MiB boundary)
.------------------------------.
| small-slots slabs            |
| .--------------------------. |
| | slab 0                   | |
| | .----------------------. | |
| | | area 0               | | | 
| | | .------------------. | | | 
| | | | slot 0           | | | | <-- page align
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,215  | | | |
| | | '------------------' | | |
| | | area 1               | | | 
| | | .------------------. | | | 
| | | | slot 0           | | | | <-- page align
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,215  | | | |
| | | '------------------' | | |
| | | ...                  | | |
| | | area 31              | | | 
| | | .------------------. | | | 
| | | | slot 0           | | | | <-- page align
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,215  | | | |
| | | '------------------' | | |
| | '----------------------' | |
| '--------------------------' |
| .--------------------------. |
| | slab 1                   | |
| | .----------------------. | |
| | | area 0               | | | 
| | | .------------------. | | | 
| | | | slot 0           | | | | <-- page align
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,215  | | | |
| | | '------------------' | | |
| | | area 1               | | | 
| | | .------------------. | | | 
| | | | slot 0           | | | | <-- page align
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,215  | | | |
| | | '------------------' | | |
| | | ...                  | | |
| | | area 31              | | | 
| | | .------------------. | | | 
| | | | slot 0           | | | | <-- page align
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,215  | | | |
| | | '------------------' | | |
| | '----------------------' | |
| '--------------------------' |
| ...                          |
| .--------------------------. |
| | slab 4                   | |
| | .----------------------. | |
| | | area 0               | | | 
| | | .------------------. | | | 
| | | | slot 0           | | | | <-- page align
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,215  | | | |
| | | '------------------' | | |
| | | area 1               | | | 
| | | .------------------. | | | 
| | | | slot 0           | | | | <-- page align
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,215  | | | |
| | | '------------------' | | |
| | | ...                  | | |
| | | area 31              | | | 
| | | .------------------. | | | 
| | | | slot 0           | | | | <-- page align
| | | | slot 1           | | | |
| | | | ...              | | | |
| | | | slot 16,777,215  | | | |
| | | '------------------' | | |
| | '----------------------' | |
| '--------------------------' |
'------------------------------'
.------------------------------.
| medium-slots slabs           |
| .--------------------------. |
| | slab 0                   | |
| | .----------------------. | |
| | | slot 0               | | | <-- align to larger of page or slot (128 B)
| | | slot 1               | | |
| | | ...                  | | |
| | | slot 536,870,911     | | |
| | '----------------------' | |
| '--------------------------' |
| .--------------------------. |
| | slab 1                   | |
| | .----------------------. | |
| | | slot 0               | | | <-- align to larger of page or slot (256 B)
| | | slot 1               | | |
| | | ...                  | | |
| | | slot 536,870,911     | | |
| | '----------------------' | |
| '--------------------------' |
| ...                          |
| .--------------------------. |
| | slab 7                   | |
| | .----------------------. | |
| | | slot 0               | | | <-- align to larger of page or slot (16 KiB)
| | | slot 1               | | |
| | | ...                  | | |
| | | slot 536,870,911     | | |
| | '----------------------' | |
| '--------------------------' |
'------------------------------'
.------------------------------.
| large-slots slab             |
| .--------------------------. |
| | slot 0                   | | <-- 64 MiB align
| | slot 1                   | |
| | ...                      | |
| | slot 1,048,575           | |
| '--------------------------' |
'------------------------------'
.------------------------------.
| small-slots slabs variables  |
| .--------------------------. |
| | area 0                   | |
| | .----------------------. | |
| | | slab 0: flh -- eac   | | | <-- 8-byte align
| | | slab 1: flh -- eac   | | |
| | | ...                  | | |
| | | slab 4: flh -- eac   | | |
| | '----------------------' | |
| '--------------------------' |
| .--------------------------. |
| | area 1                   | |
| | .----------------------. | |
| | | slab 0: flh -- eac   | | | <-- 8-byte align
| | | slab 1: flh -- eac   | | |
| | | ...                  | | |
| | | slab 4: flh -- eac   | | |
| | '----------------------' | |
| '--------------------------' |
| ...                          |
| .--------------------------. |
| | area 31                  | |
| | .----------------------. | |
| | | slab 0: flh -- eac   | | | <-- 8-byte align
| | | slab 1: flh -- eac   | | |
| | | ...                  | | |
| | | slab 4: flh -- eac   | | |
| | '----------------------' | |
| '--------------------------' |
'------------------------------'
.------------------------------.
| medium-slots slabs variables |
| .--------------------------. |
| | slab 0: flh -- eac       | | <-- 8-byte align
| | slab 1: flh -- eac       | |
| | ...                      | |
| | slab 7: flh -- eac       | |
| '--------------------------' |
'------------------------------'
.------------------------------.
| large-slots slab variables   |
| flh -- eac                   | <-- 8-byte align
'------------------------------'
```

### Sentinel Value for flh

The sentinel value is actually `0` so you have to add 1 to an index value before storing it in `flh`
and subtract 1 from `flh` before using it as an index.

# Philosophy -- Why `smalloc` is beautiful (in my eyes)

"Allocating" virtual memory (as it is called in Unix terminology) doesn't prevent any other code (in
this process or any other process) from being able to allocate or use memory. It also doesn't
increase cache pressure or cause any other negative effects.

And, allocating one span of virtual memory -- no matter how large -- imposes only a tiny bit of
additional work on the kernel's virtual-memory accounting logic -- a single additional virtual
memory map entry. It's best to think of "allocating" virtual memory as simply *reserving address
space* (which is what they call it in Windows terminology).

The kernel simply ensures that no other code in this process that requests address space will
receive address space that overlaps with this span that you just reserved. (The kernel needs to do
this only for other code running in *this* process -- other processes already have completely
separate address spaces which aren't affected by allocations in this process.)

Therefore, it can be useful to reserve one huge span of address space and then use only a small part
of it, because then you know memory at those addresses space is available to you without dynamically
allocating more and having to track the resulting separate address spaces.

This technique is used occasionally in scientific computing, such as to compute over large sparse
matrices, and a limited form of it is used in some of the best modern memory managers like
`mimalloc` and `rpmalloc`, but I'm not aware of this technique being used to the hilt like this in a
memory manager before.

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
       allocations are packed into a single cache line, then it can execute much faster due to
       having the memory already in cache and not having to load it from main memory. This can make
       the difference between a few cycles when the data is already in cache versus tens of cycles
       when it has to load it from main memory. (This is sometimes called "constructive
       interference" or "true sharing", to distinguish it from "destructive interference" or "false
       sharing" -- see below.)

    2. On the other hand, if multiple different CPU cores access different allocations in parallel,
       and the allocations are packed into the same cache line as each other, then this causes a
       substantial performance *degradation*, as the CPU has to stall the cores while propagating
       their accesses of the shared memory. This is called "false sharing" or "destructive cache
       interference". The magnitude of the performance impact is the similar to that in point (a.)
       above -- false sharing can impose tens of cycles of penalty on a single
       access. Worse--depending on the data access patterns across cores--that penalty might recur
       over and over on subsequent accesses.

    3. Suppose the program accesses multiple separate allocations in quick succession -- regardless
       of whether the accesses are by the same processor or from different processors. If the
       allocations are packed into the same memory page, this avoids a potentially costly page
       fault. Page faults can cost only a few CPU cycles in the best case, but in case of a TLB
       cache miss they could incur substantially more. In the worst case, the kernel has to load the
       data from swap, which could incur a performance penalty of hundreds of *thousands* of CPU
       cycles or even more!  Additionally, faulting in a page of memory increases the pressure on
       the TLB cache and the swap subsystem, thus potentially causing a performance degradation for
       other processes running on the same system.

   Note that these three goals cannot be fully optimized for by the memory manager, because they
   depend on how the user code accesses the memory. What `smalloc` does is use some simple
   heuristics intended to optimize the above goals under some assumptions about the behavior of the
   user code:

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
       to pack multiple into a single memory page, `smalloc` attempts to pack multiple into a single
       memory page. It doesn't separate allocations of these sizes by thread, the way it does for
       small allocations, because there's no performance penalty when multiple cores access the same
       memory page (but not the same cache line) in parallel. In fact it is a performance benefit
       for them to share caching of the memory page -- it is a form of "constructive cache
       interference" or "true cache sharing".

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
   without reading or writing any actual memory).
    
   Therefore, `smalloc` can calculate the location of a valid slab to serve this call to `malloc()`
   using only one or two data inputs: One, the requested size and alignment (which are on the stack
   in the function arguments and do not incur a potential-cache-miss) and two -- only in the case of
   allocations small enough to pack multiple of them into a cache line -- the thread number (which
   is in thread-local storage: one potential-cache-miss). Having computed the location of the slab,
   it can access the `flh` and `eac` from that slab (one potential-cache-miss), at which point it
   has all the data it needs to compute the exact location of the resulting slot and to update the
   free list. (See below about why we don't typically incur another potential-cache-miss when
   updating the free list.)

   For the implementation of `free()`, we need to use *only* the pointer to be freed (which is on
   the stack in an argument -- not a potential-cache-miss) in order to calculate the precise
   location of the slot and the slab to be freed. From there, it needs to access the `flh` for that
   slab (one potential-cache-miss).

   Why don't we have to pay the cost of one more potential-cache-miss to update the free list (in
   both `malloc()` and in `free()`)? There is a sweet optimization here that the next
   free-list-pointer and the memory allocation occupy the same memory! (Although not at the same
   time.) Therefore, if the user code accesses the memory returned from `malloc()` after `malloc()`
   returns, there is no additional cache-miss penalty from `malloc()` accessing it before
   returning. Likewise, if the user code has recently accessed the memory to be freed before calling
   `free()` on it, then `smalloc`'s access of the same space to store the next free-list pointer
   will incur no additional cache-miss. (Thanks to Sam Smith from Shielded Labs for teaching me
   this.)

   So to sum up, here are the counts of the potential-cache-line misses for the common cases:

   1. To `malloc()` and then write into the resulting memory:
      * If the allocation size is <= 64 bytes, then:
         * 🟠 one to access the `THREAD_AREANUM`
         * 🟠 one to access the `flh` and `eac`
         * 🟠 one to access the intrusive free list entry
         * 🟢 no additional cache-miss for the user code to access the data

      For a total of 3 potential-cache-misses.

      * If the allocation size is > 64, then:
         * 🟠 one to access the `flh` and `eac`
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
   
   And of course a similar property holds for the potential cache-miss of accessing the `flh` and/or
   `eac` -- if this thread (for small-slot slabs), or any thread (for large-slot slabs) has recently
   called `malloc()`, `free()`, or `realloc()` for an allocation of this size class or a nearby size
   class, then the `flh` and `eac` for this slab will already be in cache.

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
   exceed the size of `smalloc`'s large slots: 64 MiB).

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
   
   An additional narrowing, when pushing a slot onto the free list, `smalloc` first loads the `eac`
   and `assert!()`'s that the slot has previously been allocated. This costs a little efficiency
   [XXX BENCHMARK THIS] but it does not incur an extra potential cache line miss (since `eac` is
   stored immediately after `flh`), and it may be helpful at detecting bugs in `smalloc`'s code,
   detecting bugs in user code, and constraining an attacker's options. (To inform this decision, I
   read a couple of slide decks by offensive security researchers about heap exploitation:
   ([1](https://blackwinghq.com/blog/posts/playing-with-libmalloc/),
   [2](https://www.synacktiv.com/ressources/Sthack_2018_Heapple_Pie.pdf).)


# Rationales for Design Decisions

## Rationale for Three Different Kinds of Slots

xyz cache page catchall



## Rationale for Slot Sizes, Growers, and Overflowers

Rationale for the sizes of small slots: I had a more complicated design to pack more slots into each
cache line at the cost of more complex calculations to map from pointer to slabnum and vice
versa. Andrew Reece asked me some skeptical questions about that (thanks, Andrew!), and then I did
profiling and benchmarking and couldn't demonstrate much benefit from it, so the current sizes are
chosen for simplicity, including simple calculations to map from pointer to slabnum and back.

```text
xyz
large slots:
               number that fit
                      into one
           virtual memory page
slabnum:      size:    (4KiB):
--------   --------   --------
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

* Large-slot slab numbers 0-5 are chosen so you can fit multiple of them into a 4 KiB memory page
  (which is the default on Linux, Windows, and Android), while having a simple power-of-2
  distribution that is easy to compute.

* Large-slot slab number 6, the 4096 byte slots, to hold some realloc growers that aren't ever going
  to exceed 4096 bytes, and to be able to copy the data out without touching more than one memory
  page on systems with 4096-byte memory pages, when and if they do exceeed 4096 bytes.

* Large-slab slab number 7 -- 8192 bytes -- because you can fit two slots into a memory page on a 16
  KiB memory page system.

* Large-slab slab number 8 -- 16,384 bytes -- with the same rationale as for the 4096-byte slots,
  but in this case it helps if your system has 16 KiB memory pages. (Or, I suppose if you have
  hugepages/superpages enabled.)

* Another motivation to include large-slots slab numbers 6, 7, and 8, is that we can fit only xyz 20
  million huge slots into our virtual xyz large->medium, huge->large address space limitations, and
  the user code could conceivably allocate more than 20 million allocations too big to fit into the
  slots smaller than slab number 6.
  
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

# Open Issues / Future Work

* SIMPLIFYING SMALLOC (SMALLOC v4)
  * step 1: replace overflowers algorithm with simple recurse to double-size request
  * step 2: remove `eac` and replace with zero-initialized free list :-) :-)
  * step 3: change layout so that optimized bittwiddling works for decoding addresses to slots :-)
  
* add benchmarks of 1000-threads-colliding tests e.g. `threads_1000_large_alloc_dealloc_x()`

* experiment with std::intrinsics::likely/unlikely

* Port to Cheri, add capability-safety

* Try adding a dose of quint, VeriFast, *and* Miri! :-D

* And Loom! |-D

* And llvm-cov's Modified Condition/Decision Coverage analysis. :-)

* and cargo-mutants

* Benchmark replacing `compare_exchange_weak` with `compare_exchange`
  (on various platforms with various levels of multithreading
  contention).

* The current design and implementation of `smalloc` is "tuned" to
  64-byte cache lines and 4096-bit virtal memory pages, but with
  "tunings" that hopefully also make it perform well for 128-byte
  cache lines and 16 KiB pages.

  It works correctly with larger page tables but there might be
  performance problems with extremely large ones -- I'm not
  sure. Notably "huge pages" of 2 MiB or 1 GiB are sometimes
  configured in Linux especially in "server"-flavored configurations.

* If we could allocate even more virtual memory address space,
  `smalloc` could more scalable (eg huge slots could be larger than 4
  mebibytes, the number of per-thread areas could be greater than 64),
  it could be even simpler (eg maybe just remove the (quite complex!)
  overflow algorithm, and the special-casing of the number of slots
  for the huge-slots slab), and you could have more than one `smalloc`
  heap in a single process. Larger (than 48-bit) virtual memory
  addresses are already supported on some platforms/configurations,
  especially server-oriented ones, but are not widely supported on
  desktop and smartphone platforms. We could consider creating a
  variant of `smalloc` that works only platforms with larger (than
  48-bit) virtual memory addresses and offers these advantages. TODO:
  make an even simpler smalloc ("ssmalloc"??) for 5-level-page-table
  systems.


* Rewrite it in Zig. :-)

* profiling (cachegrind?)

* use a profiler on the dealloc functions of smalloc-v2 and smalloc-v3

* CI for benchmarks? 🤔

* According to Dino Dai Zovi's 2009 presentation on Macos heap exploitation, `libmalloc`'s size classes at that time were: tiny: <= 496 bytes, 16-byte quantum, small: <= 15,360 bytes, 512-byte quantum, large: <= 16,773,120 bytes, 4 KiB pages, huge: > 16,773,120, 4 KiB pages; consider renaming `smalloc`'s small/large/huge to tiny/small/large!? (and "huge" for smalloc means fall back to system). According tyo Eloi Benoist-Vanderbeken's 2018 presentation it has been updated to: tiny: <= 1008 bytes/16 B quantums, small: if machine has < 1 GiB RAM then <= 15 KiB else <= 127 KiB; 512 B quantums, else large; There is another allocator, the undocumented "nano allocator". According to Josh Pitts's post "Playing with libmalloc in 2024", the nano allocator is for <= 255 bytes/16 B quantums, small is for [1009 B - 32 KiB]; 512 B quantums, Medium is (32 KiB - 8192 KiB]; 32 KiB quantums, large is for > 8192 KiB; According to wangshuo's article on "Glibc Malloc Principle" on OpenEuler from 2021, glibc fast-bin's manage chunks <= 160 B. glibc-`M_MMAP_THRESHOLD` is default 128 KiB but can dynamically grow when larger allocations are requested. But according to sploitfun's post "Understanding glibc malloc" from 2015, fast bins actually hold {16-64}. It says large is >?= 512 B, within large there are 512, 4096, 32768, and 262144 byte distinctions

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

* xor running counter into indexes in bench-marks to foil speculative pre-fetching! Thanks Chenyao Lou: https://lemire.me/blog/2021/01/06/memory-access-on-the-apple-m1-processor/#comment-565474

* add support for the [new experimental Rust Allocator
  API](https://doc.rust-lang.org/nightly/std/alloc/trait.Allocator.html)

* add initialized-to-zero alloc alternative, relying on kernel
  0-initialization when coming from eac

* make it usable as the implementation `malloc()`, `free()`, and
  `realloc()` for native code. :-) (Nate's suggestion.)

* Rewrite it in Odin. :-) (Sam and Andrew's recommendation -- for the
  programming language, not for the rewrite.)

# Acknowledgments

* Thanks to Andrew Reece and Sam Smith for some specific suggestions that I implemented (see notes
  in documentation above).

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

# Historical notes about lines of code of older versions

Smalloc v2 had the following lines counts (using tokei)

* docs and comments: 1641
* implementation loc: 779 (excluding debug_asserts)
* tests loc: 878
* benches loc: 507
* tools loc: 223

Smalloc v3 had the following lines counts

* docs and comments: 1665
* implementation loc: 867 (excluding debug_asserts)
* tests loc: 1302
* benches loc: 796
* tools loc: 123
