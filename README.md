# smalloc -- a simple memory allocator

`smalloc` is a new memory allocator, suitable (I hope) as a drop-in
replacement for the glibc built-in memory allocator, the Macos
built-in memory allocator, `dlmalloc`, `jemalloc`, `mimalloc`,
`snmalloc`, `rpmalloc`, etc.

`smalloc` offers performance properties comparable to the other memory
managers, while being simpler. The current implementation is only 907
lines of Rust code (excluding comments, tests, benchmarks, etc).

# Caveats

No warranty! Not supported. Never been security audited. First time
Rust project (and since it is a low-level memory allocator, Jack
O'Connor gave it the award for "worst choice ever for a first-time
Rust project"). There is no security contact information nor anyone
you can contact for help using this code.

Also, it doesn't really have complete unit tests yet, hasn't been
subjected to code coverage analysis, hasn't been used in production,
etc.

Use at your own risk!

# Usage

Add it to your Cargo.toml by executing `cargo add smalloc`, then add
this to your code:

```
use smalloc::Smalloc;

#[global_allocator]
static SMALLOC: Smalloc = Smalloc::new();
```

See `src/bin/hellosmalloc.rs` for a test program that demonstrates how
to do this.

That's it! There are no other features you could consider using, no
other changes you need to make, no configuration options, no tuning
options, no nothing.

# How it works

To understand how it works, you need to know `smalloc`'s data model
and the algorithms.

## Data model

### Data Slots and Slabs, Simplified

All memory managed by `smalloc` is organized into "slabs". A slab is a
fixed-length array of fixed-length "slots" of bytes. Every pointer
returned by a call to `smalloc`'s `malloc()` or `free()` is a pointer
to the beginning of one of those slots, and that slot is used
exclusively for that memory allocation until it is `free()`'ed [*].

([*] Except for calls to `malloc()` or `realloc()` for sizes that are
too big to fit into even the biggest of `smalloc`'s slots, which
`smalloc` instead satisfies by falling back to `mmap()`.)

There are two types of slots: small and large. The small slots are in
11 slabs, and have these sizes:

small slots:
slabnum:       size:     numslots:
--------    --------     ---------
       0       1   B   220,000,000
       1       2   B   220,000,000
       2       3   B   220,000,000
       3       4   B   220,000,000
       4       5   B   220,000,000
       5       6   B   220,000,000
       6       8   B   220,000,000
       7       9   B   220,000,000
       8      10   B   220,000,000
       9      16   B   220,000,000
      10      32   B   220,000,000

The large slots are in 10 slabs, and have these sizes:

large slots:
slabnum:       size:     numslots:
--------    --------     ---------
       0      64   B   220,000,000
       1     128   B   220,000,000
       2     256   B   220,000,000
       3     512   B   220,000,000
       4    1024   B   220,000,000
       5    2048   B   220,000,000
       6    4096   B   220,000,000
       7    8192   B   220,000,000
       8   16384   B   220,000,000
       9       4 MiB    20,000,000 <-- the "huge-slots" slab

With the exception of the "huge-slots" slab (large slabnum 9), all
slabs have 220,000,000 slots. They are 0-indexed, so the largest slot
number in each slab is 219,999,999.

The "huge-slot" slab has slots that are 4,194,304 bytes (4 MiB) in
size. It has only 20,000,000 slots instead of 220,000,000. The largest
slot number in the huge-slots slab is slot number 19,999,999.

In Figure 1, `[data]` means a span of memory (of that slab's
slot-size), a pointer to which can be returned from `malloc()` or
`realloc()` for use by the caller:

```
Figure 1. Organization of data slots.

        slot # -> slot 0      slot 1      ... slot 219,999,999
                  ------      ------          ----------------
small slots:
slab #  slot size
------  ---------
                  .---------. .---------.     .---------. 
     0      1   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. 
     1      2   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. 
     2      3   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     3      4   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     4      5   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     5      6   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     6      8   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     7      9   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     8     10   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     9     16   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    10     32   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.

        slot # -> slot 0      slot 1      ... slot 219,999,999
                  ------      ------          ----------------
large slots:
slab #  slot size
------  ---------
                  .---------. .---------.     .---------.
     0     64   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     1    128   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     2    256   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     3    512   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     4   1024   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     5   2048   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     6   4096   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     7   8192   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     8  16384   B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     9      4 MiB | [data]  | | [data]  | ... <-- only 20M slots
                  .---------. .---------.
```

### Variables

For each slab, there are two associated variables: the index of the
most-recently freed slot, and the count of ever-allocated slots.

The index of the most-recently freed slot is also known as the
"free list head" and is abbreviated `flh`.

The count of ever-allocated slots (abbreviated `eac`) is the number of
slots in that slab have ever been allocated (i.e. has a pointer ever
been returned by `malloc()` or `realloc()` that points to that slot).
`flh` and `eac` are each 8 bytes in size.

```
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
              .-----. .-----.
     2        | flh | | eac |
              .-----. .-----.
   ...          ...     ...
              .-----. .-----.
    10        | flh | | eac |
              .-----. .-----.

large slots:
slab #        variable
------        --------
              .-----. .-----.
     0        | flh | | eac |
              .-----. .-----.
     1        | flh | | eac |
              .-----. .-----.
   ...          ...     ...
              .-----. .-----.
     9        | flh | | eac |
              .-----. .-----.
```

### Separate Free List Spaces (for Small-Slot Slabs [0-5])

For slabs 0, 1, 2, 3, 4, and 5, there is a "separate free list space"
large enough to hold 440,000,000 slot indexes, each slot index being 4
bytes in size. (We'll describe to how to manage the free lists for the
other slabs later.)

```
Figure 3. Organization of separate free list spaces for small-slot slabs [0-5]

small slots:
slab #     separate free list space
------     ------------------------
           slot 0         slot 1         ... slot 439,999,999
           .------------. .------------.     .------------.
     0     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
     1     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
     2     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
   ...          ...            ...                ...
           .------------. .------------.     .------------.
     5     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
```

That's it! Those are all the data elements in `smalloc`!

(Except for one more modification we'll describe below -- see
"Separate Area For Multiprocessing".)

## Algorithms, Simplified

Here is a first pass describing simplified versions of the
algorithms. After you learn these simple descriptions, keep reading
for additional detail.

* `malloc()`

To allocate space, we identify the first slab containing slots are
large enough to hold the requested size (and also satisfy the
requested alignment).

If the free list is non-empty, pop the head element from it and return
the pointer to that slot.

If the free list is empty, increment the ever-allocated-count, `eac`,
and return the pointer to the newly-allocated slot.

* `free()`

Push the newly-freed slot (the slot whose first byte is pointed to by
the pointer to be freed) onto the free list of its slab.

* `realloc()`

If the requested new size (aligned) requires a larger slot than the
allocation's current slot, then allocate a new slot (just like in
`malloc()`, above). Then `memcpy()` the contents of the current slot
into the beginning of that new slot, deallocate the current slot (just
like in `free()`, above) and return the pointer to the new slot.

That's it! You could stop reading here and you'd have a basic
knowledge of the design of `smalloc`.

## Algorithms in More Detail -- the Free Lists

The `flh` for a given slab is either a sentinel value meaning that the
free list is empty, or else it is the index of the slot most recently
pushed onto the free list.

To satisfy a `malloc()`, first check if the free list for your chosen
slab is non-empty, i.e. if its `flh` is not the sentinel value. If so,
then the `flh` is the index in this slab of the current
most-recently-freed slot.

We need to pop the head item off of the free list, i.e. set the `flh`
to point to the next item instead of the head item.

But where is the pointer to the *next* item? For slabs numbered [0-5],
there is a separate free list space. The elements in the free list
space are associated with the data slots of the same index in the data
space -- if the data slot is currently freed, then the associated free
list slot contains the index of the *next* free slot (or the sentinel
value if there is no next free slot).

```
Figure 4. Separate free list slots associated with data slots for slabs 0, 1, and 2

small slots:
slab #     data slots
------     ----------
           slot 0         slot 1         ... slot 219,999,999
           .------------. .------------.     .------------.
     0     | [data]     | | [data]     | ... | [data]     |
           .------------. .------------.     .------------.
     1     | [data]     | | [data]     | ... | [data]     |
           .------------. .------------.     .------------.
   ...          ...            ...                ...
           .------------. .------------.     .------------.
     5     | [data]     | | [data]     | ... | [data]     |
           .------------. .------------.     .------------.

small slots:
slab #     separate free list space
------     ------------------------
           slot 0         slot 1         ... slot 219,999,999
           .------------. .------------.     .------------.
     0     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
     1     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
   ...          ...            ...                ...
           .------------. .------------.     .------------.
     5     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
```

So to pop the head of the free list, for `malloc()`, for slab 0, 1, 2,
3, 4, or 5, you take the `flh`, look up the indicated element in the
free list space, and copy its value to the `flh`. Now the element that
was formerly the second item in the free list is the head of the free
list.

To push an item onto the free list (making it become the new head) (in
order to implement `free()`) you are given the slot number of the item
to be freed. Take the `flh` and copy it into the "next slot#" space of
this slot number in the free list space. Now this item points to
whatever was formerly the head of the free list. Now set the `flh` to
be the slot number of this item. Now this item is the new head of the
free list.

What about for small-slot slabs numbered 6 through 10, and large-slot
slabs? There is no separate free list space to hold their
next-pointers. The answer is we store the next-pointers in the same
space where the data goes when the slot is in use! Each data slot is
either currently freed, meaning we can use its space to hold the
next-pointer, or currently allocated, meaning it is not in the free
list and doesn't need a next-pointer.

This technique is known as an "intrusive free list". Thanks to Andrew
Reece and Sam Smith, my colleagues at Shielded Labs, for explaining
this to me.

```
Figure 5. Intrusive free lists for slabs with slots big enough to hold links

slab #     data/free-list slots
------     --------------------

small slots:
           slot 0                    slot 1                    ... slot 219,999,999
           .-----------------------. .-----------------------.     .-----------------------.
     6     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.
     7     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.
    ...               ...                       ...                           ...
           .-----------------------. .-----------------------.     .-----------------------.
    10     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.

large slots:
           .-----------------------. .-----------------------.     .-----------------------.
     0     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.
     1     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.
    ...               ...                       ...                           ...
           .-----------------------. .-----------------------.     .-----------------------.
     9     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.
```

So for these slabs, to satisfy a `malloc()` or `realloc()` by popping
the head item from the free list, what you do is take the `flh` and
read the *contents* of the indicated slot to find the index of the
*next* item in the free list. Set `flh` equal to the index of that
next item and you're done popping the head of the free list.

To push an item onto the free list (in order to implement `free()`),
you are given the slot number of the item to be freed. Take the
current `flh` and copy its value into the data slot of the item to be
freed. Now set the `flh` to be the index of the item to be freed. That
item is now the new head of the free list.

If there are no items on the free list when you are satisfying a
`malloc()` or `realloc()`, then you increment the
ever-allocated-count, `eac`, and return a pointer to the next
never-before-allocated slot.

## Algorithms in More Detail -- Multiprocessing

To make `smalloc` perform well with multiple cores operating in
parallel, we need to add only two modifications to the design above.

### Separate Areas For Multiprocessing

Replicate the data structures for the small-slot slabs into 64
identical "areas" that'll each be (typically) accessed by a different
thread. This is not necessary for correctness, it is just a
performance optimization.

```
Figure 6. Organization of data slots including areas.

                area 0                                         area 1                              ... areas 2-62 ...   area 63
            /------------------------------------------\   /------------------------------------------\     |       /------------------------------------------\
            slot 0      slot 1      ... slot 219,999,999   slot 0      slot 1      ... slot 219,999,999     |       slot 0      slot 1      ... slot 219,999,999
            ------      ------          ----------------   ------      ------          ----------------     |       ------      ------          ----------------
small slots:                                                                                                v
slab #
------
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     0      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     1      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     2      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     3      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     4      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     5      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     6      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     7      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     8      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     9      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
    10      | [data]  | | [data]  | ... | [data]  |        | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.        .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
          
            slot 0      slot 1      ... slot 219,999,999
            ------      ------          ----------------
large slots:
slab #
------
            .---------. .---------.     .---------.
     0      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.
     1      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.
     2      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.
     3      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.
     4      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.
     5      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.
     6      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.
     7      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.
     8      | [data]  | | [data]  | ... | [data]  |
            .---------. .---------.     .---------.
     9      | [data]  | | [data]  | ... <-- only 20M slots
            .---------. .---------.
```

And add space for each area for the variables for the small-slot
slabs, and the separate free list spaces for the first 6 small-slot
slabs:

```
Figure 7. Organization of variables including areas.

 * "flh" is "free-list head"
 * "eac" is "ever-allocated count"

   area # ->      area 0           area 1      ...     area 63
                  ------           ------              -------
small slots:
slab #        variable
------        --------
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     0        | flh | | eac |  | flh | | eac | ... | flh | | eac |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     1        | flh | | eac |  | flh | | eac | ... | flh | | eac |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     2        | flh | | eac |  | flh | | eac | ... | flh | | eac |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
   ...          ...     ...      ...     ...         ...     ...
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     9        | flh | | eac |  | flh | | eac | ... | flh | | eac |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
    10        | flh | | eac |  | flh | | eac | ... | flh | | eac |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
          
large slots:
              .-----. .-----.
     0        | flh | | eac |
              .-----. .-----.
     1        | flh | | eac |
              .-----. .-----.
   ...          ...     ...
              .-----. .-----.
     9        | flh | | eac |
              .-----. .-----.
```

```
Figure 8. Organization of separate free list spaces for small-slot slabs [0-5] including areas.

area # ->      area 0                                             area 1                                  ... areas 2-62 ... area 63
           /------------------------------------------------\ /------------------------------------------------\   |     /------------------------------------------------\
slab #     free list space                                                                                         |
------     ---------------                                                                                         |
           slot 0         slot 1         ... slot 219,999,999 slot 0         slot 1         ... slot 219,999,999   v     slot 0         slot 1         ... slot 219,999,999
           .------------. .------------.     .------------.   .------------. .------------.     .------------.           .------------. .------------.     .------------.
     0     | next slot# | | next slot# | ... | next slot# |   | next slot# | | next slot# | ... | next slot# |    ...    | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.   .------------. .------------.     .------------.           .------------. .------------.     .------------.
     1     | next slot# | | next slot# | ... | next slot# |   | next slot# | | next slot# | ... | next slot# |    ...    | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.   .------------. .------------.     .------------.           .------------. .------------.     .------------.
   ...          ...            ...                ...              ...            ...                ...                      ...            ...                ...
           .------------. .------------.     .------------.   .------------. .------------.     .------------.           .------------. .------------.     .------------.
     5     | next slot# | | next slot# | ... | next slot# |   | next slot# | | next slot# | ... | next slot# |    ...    | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.   .------------. .------------.     .------------.           .------------. .------------.     .------------.
```

There is a global static variable named `GLOBAL_THREAD_AREANUM`,
initialized to 0.

Each thread has a thread-local variable named `THREAD_AREANUM` which
determines which area this thread uses for the small-slots slabs.

Whenever you choose a slab for `malloc()` or `realloc()`, if it is
going to use small slot, then use this thread's `THREAD_AREANUM` to
determine which area to use. If this thread's `THREAD_AREANUM` isn't
initialized, add 1 to `GLOBAL_THREAD_AREANUM` and set this thread's
`THREAD_AREANUM` to one less than the result, mod 64.

### Thread-Safe State Update

Use thread-safe algorithms to update the free lists and the
ever-allocated-counts. This is necessary and sufficient to ensure
correctness of `smalloc`'s behavior under multiprocessing.

Specifically, we use a simple loop with atomic compare-and-exchange or
fetch-and-add operations.

#### To pop an element to the free list:

1. Load the value from `flh` into a local variable/register,
   `firstindex`. This is the index of the first entry in the free
   list.
2. If it is the sentinel value, meaning that the free list is empty,
   return. (This `malloc()`/`realloc()` will then be satisfied from
   the never-yet-allocated slots instead.)
3. Load the value from the free list slot indexed by `firstindex` into
   a local variable/register `nextindex`. This is the index of the
   next entry in the free list (i.e. the second entry), or a sentinel
   value there is if none.
4. Atomically compare-and-exchange the value from `nextindex` into
   `flh` if `flh` still contains the value in `firstindex`.
5. If the compare-and-exchange failed (meaning the value of `flh` has
   changed), jump to step 1.

Now you've thread-safely popped the head of the free list into
`firstindex`.

#### To push an element onto the free list, where `newindex` is the index to be added:

1. Load the value from `flh` into a local variable/register,
   `firstindex`.
2. Store the value from `firstindex` into the free list element with
   index `newindex`.
3. Atomically compare-and-exchange the index `newindex` into `flh` if
   `flh` still contains the value in `firstindex`.
4. If the compare-and-exchange failed (meaning that value of `flh` has
   changed), jump to step 1.

Now you've thread-safely pushed `i` onto the free list.

### To prevent ABA bugs in updates to the free list head

Store a counter in the most-significant 32-bits of the (64-bit) flh
word. Increment that counter each time you attempt a
compare-and-exchange. This prevents ABA bugs in the updates of the
`flh`.

#### To increment `eac`:

1. Fetch-and-add 1 to the value of `eac`.
2. If the result is `> 219,999,999` (or, for the huge-slots slab, `>
   19,999,999`), meaning that the slab was already full, then
   fetch-and-add -1. (This `malloc()`/`realloc()` will then be
   satisfied by falling back to `mmap` instead.)

Now you've thread-safely incremented `eac`.

Finally, whenever incrementing the global `GLOBAL_THREAD_AREANUM`, use
the atomic `fetch_add(1)` instead of a non-atomic add.

Now you know the entire data model and all of the algorithms for
`smalloc`!

Except for a few more details about the algorithms:

## Algorithms, More Detail -- Growers

Suppose the user calls `realloc()` and the new requested size is
larger than the original size. Allocations that ever get reallocated
to larger sizes often, in practice, get reallocated over and over
again to larger and larger sizes. We call any allocation that has
gotten reallocated to a larger size a "grower".

If the user calls `realloc()` asking for a new larger size, and the
new size still fits within the current slot that the data is already
occupying, then just be lazy and consider this `realloc()` a success
and return the current pointer as the return value.

If the new requested size doesn't fit into the current slot, then
choose the smallest of the following list that can hold the new
requested size: 64 B (large-slots slab 0, 4096 B (large-slots slab 6),
or 16384 B (large-slots slab 8).

If the new requested size doesn't fit into 16384 bytes, then use a
slot from the slab with 4 MiB slots (large-slots slab 9, a.k.a. the
huge-slots slab).

(As always, if the requested size doesn't fit into 4 MiB, then fall
back to `mmap()`.)

## Algorithms, More Detail -- Overflowers

Suppose the user calls `malloc()` or `realloc()` and the slab we
choose to allocate from is full. That means the free list is empty,
and the ever-allocated-count is greater than or equal to 219,999,999
(or 19,999,999 for the huge-slots slab) -- the total number of slots
in this slab. This could happen only if there were that many
allocations from one slab alive simultaneously.

In that case, if this is a small-slots slab, then find the area whose
slab of this same slab number has the lowest `eac`. Loop over each
other area, checking its `eac` and remembering the lowest one you've
found so far. In order check its `eac` you actually have to
`fetch_add(1)` to it so that if another thread is changing the `eac`
at the same time, you'll actually have reserved that slot. This also
means you have to either use that slot or push it onto that slab's
free list before you forget about it.

If you find a slab with `eac` 0, short-circuit the loop and use that
area.

Once you've either reserved slot 0 in a slab, or else completed the
traversal of all the areas and found the lowest-`eac` and reserved a
slot in it, then set your `THREAD_AREANUM` to that area number, and
that slot of that area to satisfy this request.

When doing this, traverse the areas in a permutation by adding 31 mod
64 instead of adding 1 mod 64, in order to reduce the chance of your
search overlapping with operations of any other threads whose first
allocation was after your thread's first allocation (since they got
`THREAD_AREANUM`'s incrementally higher than yours).

If you've searched all areas and you weren't able to allocate any slot
-- meaning that all slabs of this slab number in all areas were full
-- then increase the slab number and try again. If this was already
the largest small-slots slab, then switch to the smallest large-slots
slab.

If the this is a large-slots slab and it is full, then overflow to the
next larger slab. (Thanks to Nate Wilcox for suggesting this technique
to me.)

If all of the slabs you could overflow to are full, then fall back to
using the system allocator (e.g. `mmap()`) to request more memory from
the operating system and return the pointer to that.

## The Nitty-Gritty

(The following details are probably not necessary for you to understand
unless you're debugging or modifying `smalloc` or implementing a
similar library yourself.)

### Layout

The variables are laid out in memory first, starting with the
small-slots slabs.

Area number is most significant, followed by slab number. This is
"column-wise" order when looking at Figure 7 above. So in memory,
first the `flh` and `eac` for area 0 and slab 0 are laid out, and then
the `flh` and `eac` for area 0 and slab 1, and so on. Since area 0 is
first, all of its variables are laid out in memory before the first
variables from area 1 are.

Following all of the variables for all of the areas of small-slot
slabs, then the variables for the large-slot slabs are laid out in the
same manner.

Following all of the variables, the free lists for small-slot slabs
[0-5] are laid out, with area number most significant, then slab
number, then slot number. This is "column-wise" order when looking at
Figure 8.

Following the free lists, the small-slot slabs's data slots are laid
out in memory, with area number most significant, then slab number,
then slot number. This is "column-wise" order when looking at Figure 6
above.

Finally, the large-slot slabs's data slots are laid out in memory in
the same way.

### Alignment

There are several constraints on alignment, and the layout "scoots
forward" the starting location of each of the elements described above
in order to guarantee that all of the alignment requirements hold.

1. All `eac`'s and all `flh`'s have to be 8-byte aligned, for atomic
   memory access.

2. Each separate free list begins at 16 KiB alignment, for efficient
   use of memory pages.

3. Each data slab is aligned to 16 KiB bytes, for efficient use of
   memory pages and to satisfy requested alignments (see below).

4. The huge-slots slab is additionally aligned to 4 MiB to satisfy
   larger requested alignments (see below).

5. Requested alignments: Sometimes the caller of `malloc()` requires
   an alignment for the resulting memory, meaning that the pointer
   returned needs to point to an address which is an integer multiple
   of that alignment. Such caller-required alignments are always a
   power of 2. Because of the alignments of the data slabs, slots
   whose sizes are powers of 2 are always aligned to their own size.

In order to start `smalloc`'s base pointer with 4 MiB alignment, we
over-allocate 4 MiB - 1 byte and the scoot forward the base pointer to
the first 4 MiB boundary.

Okay, now you know everything there is to know about `smalloc`'s data
model and memory layout. Given this information, you can calculate the
exact address of every data element in `smalloc`! (Counting from the
`smalloc` base pointer, which is the address of the first byte in the
layout described above.)

### Sentinel Value for flh

The sentinel value is actually `0` so you have to add 1 to an index
value before storing it in `flh` and subtract 1 from `flh` before
using it as an index.

# Philosophy -- Why `smalloc` is beautiful (in my eyes)

"Allocating" virtual memory (as it is called in Unix terminology)
doesn't prevent any other code (in this process or any other process)
from being able to allocate or use memory. It also doesn't increase
cache pressure or cause any other negative effects.

And, allocating one span of virtual memory -- no matter how large --
imposes only a tiny bit of additional work on the kernel's
virtual-memory accounting logic -- a single additional virtual memory
map entry. It's best to think of "allocating" virtual memory as simply
*reserving address space* (which is what they call it in Windows
terminology).

The kernel simply ensures that no other code (in this process) that
requests address space will receive address space that overlaps with
this span that you just reserved. (The kernel needs to do this only
for other code running in *this* process -- other processes already
have completely separate address spaces which aren't affected by
allocations in this process.)

Therefore, it can be useful to reserve one huge span of address space
and then use only a small part of it, because then you know memory at
those addresses space is available to you without dynamically
allocating more and having to track and maintain the resulting
separate address spaces.

This technique is used occasionally in scientific computing, such as
to compute over large sparse matrices, and a limited form of it is
used in some of the best modern memory managers like `mimalloc` and
`rpmalloc`, but I'm not aware of this technique being exploited to the
hilt like this in a memory manager before.

So, if you accept that "avoiding reserving too much virtual address
space" is not an important goal for a memory manager, what *are* the
important goals? `smalloc` was designed with the following goals,
written here in roughly descending order of importance:

1. Be simple, in both design and implementation. This helps greatly to
   ensure correctness -- always a critical issue in modern
   computing. "Simplicity is the inevitable price that we must pay for
   correctness."--Tony Hoare (paraphrased)

   Simplicity also eases making improvements to the codebase and
   learning from the codebase.

   I've tried to pay the price of keeping `smalloc` simple while
   designing and implementing it.

2. Place user data where it can benefit from caching.

    1. If a single CPU core accesses different allocations in quick
       succession, and those allocations are packed into a single
       cache line, then it can execute much faster due to having the
       memory already in cache and not having to load it from main
       memory. This can make the difference between a few cycles when
       the data is already in cache versus tens or even hundreds of
       cycles when it has to load it from main memory. (This is
       sometimes called "constructive interference" or "true sharing",
       to distinguish it from "destructive interference" or "false
       sharing" -- see below.)

    2. On the other hand, if multiple different CPU cores access
       different allocations in parallel, and the allocations are
       packed into the same cache line as each other, then this causes
       a substantial performance *degradation*, as the CPU has to
       stall the cores while propagating their accesses of the shared
       memory. This is called "false sharing" or "destructive cache
       interference". The magnitude of the performance impact is the
       similar to that in point (a.) above -- false sharing can impose
       as much as hundreds of cycles of penalty on a single
       access. Worse is that--depending on the data access patterns
       across cores--that penalty might recur over and over on
       subsequent accesses.

    3. Suppose the program accesses multiple allocations in quick
       succession. If they are packed into the same memory page, this
       avoids a potentially costly page fault. Page faults can cost
       only a few CPU cycles in the best case, but in case of a TLB
       cache miss they could incur hundreds of CPU cycles of
       penalty. In the worst case, the kernel has to load the data
       from swap, which could incur a performance penalty of hundreds
       of *thousands* of CPU cycles or even more! Additionally,
       faulting in a page of memory increases the pressure on the TLB
       cache and the swap subsystem, thus potentially causing a
       performance degradation for other processes running on the same
       system.

   Note that these three goals cannot be fully optimized for by the
   memory manager, because they depend on how the user code accesses
   the memory. What `smalloc` does is use some simple heuristics
   intended to optimize the above goals under some assumptions about
   the behavior of the user code:

    1. Pack as many separate allocations from a single thread into
       each cache line as possible to optimize for (constructive)
       cache-line sharing.

    2. Place allocations requested by separate threads in separate
       areas, to minimize the risk of destructive ("false") cache-line
       sharing. This is heuristically assuming that successive
       allocations requested by a single thread are less likely to
       later be accessed simultaneously by multiple different
       threads. You can imagine user code which violates this
       assumption -- having one thread allocate many small allocations
       and then handing them out to other threads/cores which then
       access them in parallel with one another. Under `smalloc`'s
       current design, this behavior could result in a lot of
       "destructive cache interference"/"false sharing". However, I
       can't think of a simple way to avoid this bad case without
       sacrificing the benefits of "constructive cache
       interference"/"true sharing" that we get by packing together
       allocations that then get accessed by the same core.

    3. When allocations are freed by the user code, `smalloc` appends
       their slot to a free list. When allocations are subsequently
       requested, the most recently free'd slots are returned
       first. This is a LIFO (stack) pattern, which means user code
       that tends to access its allocations in a stack-like pattern
       will enjoy improved caching. (Thanks to Andrew Reece from
       Shielded Labs for teaching me this.)

    4. For allocations too large to pack multiple into a single cache
       line, but small enough to pack multiple into a single memory
       page, `smalloc` attempts to pack multiple into a single memory
       page. It doesn't separate allocations of these sizes by thread,
       the way it does for small allocations, because there's no
       performance penalty when multiple cores access the same memory
       page (but not the same cache line) in parallel (and in fact it
       is a performance benefit for them to share caching of the
       memory page -- it is a form of "constructive cache
       interference" or "true cache sharing").

  Note that all four of these techniques are limited by the fact that
  all allocations in `smalloc` are in fixed-size slots in a contiguous
  slab of equal-sized slots. Therefore, only allocations of (almost)
  the same size as each other will get packed into a single cache
  line. So, for example, if the user code allocates one 10-byte
  allocation and then one 32-byte allocation, those two will not share
  a cache line. On the other hand, if the user code does that over and
  over many times, then each batch of six of its 10-byte allocations
  will share one (64-byte) cache line, and each batch of two of its
  32-byte allocations will share one (64-byte) cache line. So maybe
  the user code will enjoy good caching benefits anyway--that remains
  to be tested. In any case, the way `smalloc` currently does this
  keeps things simple.

3. Be efficient when executing `malloc()`, `free()`, and
   `realloc()`. I want calls to those functions to execute in as few
   CPU cycles as possible. I optimistically think `smalloc` is going
   to be great at this goal! The obvious reason for that is that the
   code implementing those three functions is *very simple* -- it
   needs to execute only a few CPU instructions to implement each of
   those functions.

   A perhaps less-obvious reason is that there is *minimal
   data-dependency* in those code paths.

   Think about how many loads of memory from different locations, and
   therefore potential-cache-misses, your process incurs to execute
   `malloc()` and then to write into the memory that `malloc()`
   returned. It has to be at least one, because you are going to pay
   the cost of a potential-cache-miss to write into the memory that
   `malloc()` returned.

   To execute `smalloc`'s `malloc()` and then write into the resulting
   memory incurs, in the common cases, only two or three potential
   cache misses.

   The main reason `smalloc` incurs so few potential-cache-misses in
   these code paths is the sparseness of the data layout. `smalloc`
   has pre-reserved a vast swathe of address space and "laid out"
   unique locations for all of its slabs, slots, and variables (but
   only virtually -- without reading or writing any actual memory).
    
   Therefore, `smalloc` can calculate the location of a valid slab to
   serve this call to `malloc()` using only one or two data inputs:
   One, the requested size and alignment (which are on the stack in
   the function arguments and do not incur a potential-cache-miss) and
   two -- only in the case of allocations small enough to pack
   multiple of them into a cache line -- the thread number (which is
   in thread-local storage: one potential-cache-miss). Having computed
   the location of the slab, it can access the `flh` and `eac` from
   that slab (one potential-cache-miss), at which point it has all the
   data it needs to compute the exact location of the resulting slot
   and to update the free list. (See below about why we don't
   typically incur another potential-cache-miss when updating the free
   list.)

   For the implementation of `free()`, we need to use *only* the
   pointer to be freed (which is on the stack in an argument -- not a
   potential-cache-miss) in order to calculate the precise location of
   the slot and the slab to be freed. From there, it needs to access
   the `flh` for that slab (one potential-cache-miss).

   Why don't we have to pay the cost of one more potential-cache-miss
   to update the free list (in both `malloc()` and in `free()`)? There
   is a sweet optimization here that the next free-list-pointer and
   the memory allocation occupy the same memory! (Although not at the
   same time.) Therefore, if the user code accesses the memory
   returned from `malloc()` after `malloc()` returns, there is no
   additional cache-miss penalty from `malloc()` accessing it before
   returning. Likewise, if the user code has recently accessed the
   memory to be freed before calling `free()` on it, then `smalloc`'s
   access of the same space to store the next free-list pointer will
   incur no additional cache-miss. (Thanks to Sam Smith from Shielded
   Labs for teaching me this.)

   (This optimization doesn't apply to allocations too small to hold a
   next-free-list-pointer, i.e. to allocations of less than or equal
   to 7 bytes. For those, `smalloc` can't store the next-pointer --
   which is 4 bytes and requires 4 byte alignment -- in the slot, and
   so has to store it in a separate location, and does incur one
   additional potential-cache-line miss in both `malloc()` and
   `free()`.)

   So  to sum  up, here  are  the counts  of the  potential-cache-line
   misses for the common cases:

   1. To `malloc()` and then write into the resulting memory:
      * If the allocation size <= 8, then:
         * 🟠 one to access the `THREAD_AREANUM`
         * 🟠 one to access the `flh` and `eac`
         * 🟠 one to access the separate free list entry
         * 🟠 one for the user code to access the data

      For a total of 4 potential-cache-misses.

      * If the allocation size is > 8 but <= 32, then:
         * 🟠 one to access the `THREAD_AREANUM`
         * 🟠 one to access the `flh` and `eac`
         * 🟠 one to access the intrusive free list entry
         * 🟢 no additional cache-miss for the user code to access the
           data

      For a total of 3 potential-cache-misses.

      * If the allocation size is > 32, then:
         * 🟠 one to access the `flh` and `eac`
         * 🟠 one to access the intrusive free list entry
         * 🟢 no additional cache-miss for the user code to access the
           data
     
      For a total of 2 potential-cache-misses.

   2. To read from some memory and then `free()` it:
      * 🟠 one for the user code to read from the memory
      * 🟠 one to access the `flh`
      * 🟢 no additional cache-miss for `free()` to access the
        intrusive free list entry

      For a total of 2 potential-cache-misses.

   3. To `free()` some memory without first reading it:
      * 🟢 no cache-miss for user code since it doesn't read the
        memory
      * 🟠 one to access the `flh`
      * 🟠 one to access the intrusive free list entry

      For a total of 2 potential-cache-misses.

   Note that the above counts do not count a potential cache miss to
   access the base pointer. That's because the base pointer is fixed
   and shared -- every call (by any thread) to `malloc()`, `free()`,
   or `realloc()` accesses the base pointer, so it is more likely to
   be in cache. A similar property holds for the potential cache-miss
   of accessing the `THREAD_AREANUM` -- if this thread has recently
   called `malloc()`, `free()`, or `realloc()` for a small slot, then
   the `THREAD_AREANUM` will likely already be in cache, but if this
   thread has not made such a call recently then it would likely
   cache-miss. And of course a similar property holds for the
   potential cache-miss of accessing the `flh` and/or `eac` -- if this
   thread (for small-slot slabs), or any thread (for large-slot slabs)
   has recently called `malloc()`, `free()`, or `realloc()` for an
   allocation of this size class, then the `flh` and `eac` for this
   slab will already be in cache.

   It also does not count the potential-cache-miss of using a lookup
   table instead of a computation to map back and forth between
   areas/slabs/slots and addresses. (The current implementation of
   `smalloc` does indeed use such lookup tables because benchmarking
   shows it is faster than computation of the same mapping.)
   
4. Be *consistently* efficient.

   I want to avoid intermittent performance degradation, such as when
   your function takes little time to execute usually, but
   occasionally there is a latency spike when the function takes much
   longer to execute.

   I also want to minimize the number of scenarios in which
   `smalloc`'s performance degrades due to the user code's behavior
   triggering an "edge case" or a "worst case scenario" in `smalloc`'s
   design.
    
   The story sketched out above about user code allocating small
   allocations on one thread and then handing them out to other
   threads to access is an example of how user code behavior could
   trigger a performance degradation in `smalloc`. There is one other
   such scenario I can think of: there are only 64 allocation areas,
   so if the user code has more than 64 threads that perform
   allocation, then some of them will have to share an area. If two
   threads that are sharing an area were then to allocate multiple
   small allocations simultaneously, those allocations would then
   share cache lines. This could lead, again, to false-sharing.

   On the bright side, I can't think of any *other* "worst case
   scenarios" for `smalloc` beyond these two. In particular, `smalloc`
   never has to "rebalance" or re-arrange its data structures, or do
   any "deferred accounting" like some other memory managers do, which
   nicely eliminates some sources of intermittent performance
   degradation. (See [this blog
   post](https://pwy.io/posts/mimalloc-cigarette/) for a cautionary
   tale of how deferred accounting, while it can nicely improve
   performance in the common "hot paths", can also give rise to edge
   cases that can occasionally degrade performance in a way that
   causes problems.)

   There are no locks in `smalloc`[^1], so it will hopefully handle
   heavy multi-processing contention (i.e. many separate cores
   allocating and freeing memory simultaneously) with consistent
   performance.
   
   There *are* concurrent-update loops in `malloc` and `free` -- see
   the pseudo-code in "Thread-Safe State Changes" above -- but these
   are not locks. Whenever multiple threads are running that code, one
   of them will make progress (i.e. successfully update the `flh`)
   after it gets only a few CPU cycles, regardless of what any other
   threads do. And, if any thread becomes suspended in that code, one
   of the *other*, still-running threads will be the one to make
   progress (update the `flh`). Therefore, these concurrent-update
   loops cannot cause a pile-up of threads waiting for a
   (possibly-suspended) thread to release a lock, nor can they suffer
   from priority inversion.

    [^1] ... except in the initialization function that acquires the
    lock only one time -- the first time `alloc()` is called.
    
   So with the possible exception of the two "worst-case scenarios"
   described above, I optimistically expect that `smalloc` will show
   excellent and extremely consistent performance.

5. Efficiently support using `realloc()` to extend
   vectors. `smalloc`'s initial target user is Rust code, and Rust
   code uses a lot of Vectors, and not uncommonly it grows those
   Vectors dynamically, which results in a call to `realloc()` in the
   underlying memory manager. I hypothesized that this could be a
   substantial performance cost in real Rust programs. I profiled a
   Rust application (the "Zebra" Zcash full node) and observed that it
   did indeed call `realloc()` quite often, to resize an existing
   allocation to larger, and in many cases it did so repeatedly in
   order to enlarge a Vector, then fill it with data until it was full
   again, and then enlarge it again, and so on. This can result in the
   underlying memory manager having to copy the contents of the Vector
   over and over (in fact, in the worst case, this results in O(N^2)
   running time when appending N bytes to the Vector). `smalloc()`
   optimizes out almost all of that copying of data, with the simple
   expedient of jumping to a much larger slot size whenever
   `realloc()`'ing an allocation to a larger size (see "Algorithms,
   More Detail -- Growers", above). My profiling results indicate that
   this technique would indeed eliminate at least 90% of the
   memory-copying when extending Vectors, making it almost costless to
   extend a Vector any number of times (as long as the new size
   doesn't exceed the size of `smalloc`'s large slots: 4 MiB).

   I am hopeful that `smalloc` may achieve all five of these goals. If
   so, it may turn out to be a very useful tool!

# Rationales for Specific Design Decisions

## Rationale for Slot Sizes, Growers, and Overflowers

small slots:
             worst-case number
             that fit into one
                     cacheline
slabnum:      size:     (64B):
--------   --------   --------
       0       1  B         64
       1       2  B         32
       2       3  B         20
       3       4  B         16
       4       5  B         12
       5       6  B         10
       6       8  B          8
       7       9  B          6
       8      10  B          5
       9      16  B          4
      10      32  B          2

Rationale for the sizes of small slots: These were chosen by
calculating how many objects of this size would fit into the
least-well-packed 64-byte cache line when we lay out objects of these
size end-to-end over many successive 64-byte cache lines. If that
makes sense. The worst-case number of objects that can be packed into
a cache line can be up 2 fewer than the best-case, since the first
object in this cache line might cross the cache line boundary and only
the last part of the object is in this cache line, and the last object
in this cache line might similarly be unable to fit entirely in and
only the first part of it might be in this cache line. So this "how
many fit" number below counts only the ones that entirely fit in, even
when we are laying out objects of this size one after another (with no
padding) across many cache lines. So it can be 0, 1, or 2 fewer than
you get from just diving 64 by the size of the slot. (We also excluded
sizes which are smaller but still can't fit more -- in the worst case
-- than a larger size.)

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

Rationale for the sizes of large slots: 

* Large-slot slab numbers 0-5 are chosen so you can fit multiple of
  them into a 4 KiB memory page (which is the default on Linux,
  Windows, and Android), while having a simple power-of-2 distribution
  that is easy to compute.

* Large-slot slab number 6, the 4096 byte slots, to hold some realloc
  growers that aren't ever going to exceed 4096 bytes, and to cheaply
  copy their data out (without touching more than one memory page on
  4096-byte memory page systems) when and if they do exceeed 4096
  bytes.

* Large-slab slab number 7 -- 8192 bytes -- because you can fit two
  slots into a memory page on a 16 KiB memory page system.

* Large-slab slab number 8 -- 16,384 bytes -- with the same rationale
  as for the 4096-byte slots, but in this case it helps if your system
  has 16 KiB memory pages. (Or, I suppose if you have
  hugepages/superpages enabled.)

* Another motivation to include large-slots slab numbers 6, 7, and 8,
  is that we can fit only 20 million huge slots into our virtual
  address space limitations, and the user code could conceivable
  allocate more than 20 million allocations too big to fit into the
  smaller slots.
  
* The 4 MiB "huge" slots, because according to profiling the Zcash
  "zebrad" server, allocations of 2 MiB, 3 MiB, and even 4 MiB are not
  uncommon.

It's interesting to consider that, aside from the reasons above, there
are no other benefits to having more slabs with slots smaller than
"huge". That is, if a slot is too large to fit more than one into a
memory page, and if it isn't likely that we're going to need to copy
the entire contents out for a realloc-grow, then whether the slot is,
say, 128 KiB, or 8 MiB makes no difference to the behavior of the
system! The only difference in the behavior of the system is how the
virtual memory pages get touched, which is determined by the user
code's memory access patterns, not by the memory manager's
code. Except per the reasons listed above. If I'm wrong about that
please let me know.

Well, there is one more potential reason: if the user code has
billions of live large allocations and/or trillions of live small
allocations, you'll need to overflow allocations to other slabs, and
if all of the sufficiently-large slabs are full then you'll have to
fall back to the system allocator.

When running benchmarks on Macos 15.4.1 on arm64, before I implemented
the Overflowers feature, I accidentally did this. It turns out the
system allocator is substantially slower (at least on this machine),
and it eventually crashed the operating system with:

```panic(cpu 0 caller 0xfffffe004c83ec30): zalloc[3]: zone map exhausted while allocating from zone [VM map entries], likely due to memory leak in zone [VM map entries] (13G, 180906506 elements allocated) @zalloc.c:4560```

In practice this seems unlikely to occur in real systems, unless there
is a memory leak (i.e. allocations that are then forgotten about
rather than freed), in which case the Overflowers feature and the
additional slot sizes only delay rather than prevent the problem. (But
I suppose that could be good enough if the program finishes its work
before it crashes.)

But, in any case, in order to prevent or at least delay a slowdown or
a crash in this (rather extreme) case is the rationale for including
the Overflowers feature.

Rationale for promoting growers to 64-byte slots: that *might* be
sufficient -- they might stop growing before exceeding 64 bytes. And
if not, then it is going to require only a single cache line access to
copy the data out to the next location.

Rationale for promoting growers to 4096-byte slots: that *might* be
sufficient, and if not then it is going to touch only a single memory
page to copy the data out to the next location.

Rationale for promoting growers to 16,384-byte slots: and we don't
have as many huge slots as we do non-huge slots, and we don't want to
the huge-slots slab to fill up.

# Open Issues / Future Work

* Port to Cheri, add capability-safety

* Implement this load-balancing feature: If the resulting
  `THREAD_AREANUM` > 63, then scan all the areas, inspecting the `eac`
  of this slab in each area and choose the area with the lowest `eac`
  for this slab and set this thread's `areanum` equal to that.

* See if you can prove whether jumping straight to large slots on the
  first resize is or isn't a performance regression compared to the
  current technique of first jumping to the 32-byte slot size and only
  then jumping to the large slot size. If you can't prove that the
  current technique is substantially better in some real program, then
  switch to the "straight to large slots" alternative for simplicity.

* Try adding a dose of quint, VeriFast, *and* Miri! :-D

* And Loom! |-D

* And llvm-cov's Modified Condition/Decision Coverage analysis. :-)

* xyz1 xxx The whole overflow algorithm turned out to be complicated to
  implement in source code. Or, to put it another way the simpler
  algorithm that just overflows straight to the system allocator was
  *substantially* simpler to implement. So... XXX come back to this
  and see if we can enlarge the NUM_SLOTS a more to further reduce the
  chances of that overflow ever happening in practice? Or see if after
  having implemented the simpler algorithm, we can see a nice way to
  implement the original overflow algorithm. XXX enlarge small slabs
  to NUM_SLOTS = 2^31 or so

* See if the "tiny" slots (1-, 2-, and 3-byte slots) give a
  substantial performance improvement in any real program and measure
  their benefits and drawbacks in micro-benchmarks. Consider removing
  them for simplicity. Maybe remove them, see if that causes a
  substantial performance regression in any real programs, and if so
  put them back? >:-D

* See if the "pack multiple into a cache line" slots that aren't
  powers of two (sizes 5, 6, 9, and 10) are worth the complexity, in
  the same was as the previous TODO... (Without them we can use
  bittwiddling instead of a loop or a lookup to map size to
  slabnumber. :-))

* Consider the previous two items with an eye to removing unaligned
  free list entries for simplicity.

* Benchmark replacing `compare_exchange_weak` with `compare_exchange`
  (on various platforms with various levels of multithreading
  contention).

* The current design and implementation of `smalloc` is "tuned" to
  64-byte cache lines and 4096-bit virtal memory pages.

  The current version of `smalloc` works correctly with larger cache
  lines but there might be a performance improvement from a variant of
  `smalloc` tuned for 128-bit cache lines. Notably the new Apple ARM64
  chips have 128-bit cache lines in some cases, and modern Intel chips
  have a hardware cache prefetcher that sometimes fetches the next
  64-bit cache line.

  It works correctly with larger page tables but there might be
  performance problems with extremely large ones -- I'm not
  sure. Notably "huge pages" of 2 MiB or 1 GiB are sometimes
  configured in Linux especially in "server"-flavored configurations.

  There might also be a performance improvement from a variant of
  `smalloc` tuned to larger virtual memory pages. Notably virtual
  memory pages on modern MacOS and iOS are 16 KiB.

* If we could allocate even more virtual memory address space,
  `smalloc` could more scalable (eg huge slots could be larger than 4
  mebibytes, the number of per-thread areas could be greater than 64),
  it could be even simpler (eg remove the (quite complex!) overflow
  algorithm, and the special-casing of the number of slots for the
  huge slots slab), and you could have more than one `smalloc` heap in
  a single process. Larger (than 48-bit) virtual memory addresses are
  already supported on some platforms/configurations, especially
  server-oriented ones, but are not widely supported on desktop and
  smartphone platforms. We could consider creating a variant of
  `smalloc` that works only platforms with larger (than 48-bit)
  virtual memory addresses and offers these advantages. TODO: make an
  even simpler smalloc ("ssmalloc"??) for 5-level-page-table systems.

* I looked into the `RDPID` instruction on x86 and the `MPIDR`
  instruction on ARM as a way to get a differentiating number/ID for
  the current core without the overhead of an operating system call
  and without having to load the current thread number from memory,
  but using `MPIDR` resulted in an illegal instruction exception on
  MacOS on Apple M4, so I gave up on that approach. The current
  implementation assigns a unique integer "thread number" to each
  thread the first time it calls `malloc()`, which is stored in
  thread-local storage. It might be nice if we could figure out a way
  to get any kind of differentiating number/ID for different cores
  with a CPU instruction instead of by reading it from thread-local
  memory, which incurs a potential-cache-miss. Also, of course, we
  ought to benchmark whether it is actually more efficient to use one
  of these CPU instructions or to just load the thread number from
  thread-local storage. :-)

* Define some kind of Rust type to manage the add-1/sub-1 on the
  indexes of the free list? (Nate's suggestion)

* Rewrite it in Zig. :-)

* need more huge slots

* CI for benchmarks? 🤔

* Nate's idea to make the sentinel value be a large value, initialized
  upon first eac, instead of being 0. <3

* Benchmark no-pub, all-const, native arch, lto, no-idempotent-init

* the 4-byte slots can already be intrusive-free-list :-o

* add support for the new experimental alloc API

* add initialized-to-zero alloc alternative, relying on kernel
  0-initialization when coming from eac

* make it usable as the implementation `malloc()`, `free()`, and
  `realloc()` for native code. :-) (Nate's suggestion.)

* reimplement it in the Odin programming language


Notes:

Our variables need to be 4-byte aligned (for performance and
correctness of atomic operations, on some/all architectures). They
always will be, because the variables are laid out at the beginning of
the mmap'ed region, which is always page-aligned.



Things `smalloc` does not currently attempt to do:

* Try to mitigate malicious exploitation after a memory-usage bug in
  the user code.


# Acknowledgments

* Thanks to Andrew Reece and Sam Smith for some specific suggestions
  that I implemented (see notes in documentation above).

* Thanks to Jack O'Connor, Nate Wilcox, Sean Bowe, and Brian Warner
  for advice and encouragement. Thanks to Nate Wilcox especially for
  debugging help!

* Thanks to Kris Nuttycombe for suggesting the name "smalloc". :-)

* Thanks to Jason McGee--my boss at Shielded Labs--for being patient
  with me obsessively working on this when I could have been doing
  even more work for Shielded Labs instead.

* Thanks to my lovely girlfriend, Kelcie, for housewifing for me while
  I wrote this program. ♥️

* Thanks to pioneers/competitors/colleagues from whom I have learned
  much: the makers of dlmalloc, jemalloc, mimalloc, snmalloc,
  rsbmalloc, ferroc, scudo, rpmalloc, ... and Michael & Scott
  (https://web.archive.org/web/20241122100644/https://www.cs.rochester.edu/research/synchronization/pseudocode/queues.html),
  and Leo (the Brave Web Browser AI) for extensive and mostly correct
  answers to stupid Rust questions.

* Thanks to fluidvanadium for the first PR from a contributor. :-)
