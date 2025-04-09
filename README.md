# smalloc -- a simple memory allocator

`smalloc` is a new memory allocator, suitable (I hope) as a drop-in
replacement for the glibc built-in memory allocator, `dlmalloc`,
`jemalloc`, `mimalloc`, `snmalloc`, etc.

I would *like* to say that it exhibits good performance properties
compared to those others, but I haven't actually implemented and
tested it yet, so I'll have to limit myself to saying it is simple.

Or... at least I can claim that it is simpler than the others!

A note on naming: One thing I've learned from decades of software
engineering is that "Simplicity is in the eye of the beholder.". If
the design described in this document makes sense to you, then great!
For you, the "s" in `smalloc` stands for "simple". If this design is
confusing -- no worries! For you, the "s" in `smalloc` stands for
"sparse". :-)

# How it works

To understand how it works, you need to know `smalloc`'s data model
and the algorithms.

## Data model

### Data Slots and Slabs

All memory managed by `smalloc` is organized into "slabs". A slab is a
fixed-length array of fixed-length "slots" of bytes. Every pointer
returned by a call to `smalloc`'s `malloc()` or `free()` is a pointer
to the beginning of one of those slots, and that slot is used
exclusively for that memory allocation until it is `free()`'ed [*].

([*] Except for calls to `malloc()` or `realloc()` for sizes that are
too big to fit into even the biggest of `smalloc`'s slots, which
`smalloc` instead satisfies by falling back to `mmap()`.)

All slabs have 20,971,520 slots (20 times 2^20). They are 0-indexed,
so the largest slot number in each slab is 20,971,519.

Each slab has slots of a different size. Slab 0 has slots that are 1
byte in size, slab 9 has slots that are 16 bytes in size, and slab 15
has slots that are 1024 bytes in size. The final slab, and the one
with the largest slots, is slab 17, which has slots that are 4,194,304
bytes (4 MiB) in size.

In Figure 1, `[data]` means an span of memory (of that slab's
slot-size), a pointer to which can be returned from `malloc()` or
`realloc()` for use by the caller:

```
Figure 1. Organization of data slots.

        slot # -> slot 0      slot 1      ... slot 20,971,519
                  ------      ------          ---------------
slab #  slot size
------  ---------
                  .---------. .---------.     .---------. 
     0       1  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. 
     1       2  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. 
     2       3  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     3       4  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     4       5  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     5       6  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     6       8  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     7       9  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     8      10  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
     9      16  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    10      32  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    11      64  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    12     128  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    13     256  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    14     512  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    15    1024  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    16    2048  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    17   4 MiB  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
```

### Variables

For each slab, there are two associated variables: the count of
ever-allocated slots, and the index of the most-recently freed slot.

The count of ever-allocated slots (abbreviated `eac`) is the number of
slots in that slab have ever been allocated (i.e. has a pointer ever
been returned by `malloc()` or `realloc()` that points to that
slot). The index of the most-recently freed slot is also known as the
"free list head" and is abbreviated `flh`. `eac` and `flh` are both 4
bytes in size.

```
Figure 2. Organization of variables.

 * "eac" is "ever-allocated count"
 * "flh" is "free-list head"

slab #        variable
------        --------
              .-----. .-----.
     0        | eac | | flh |
              .-----. .-----.
     1        | eac | | flh |
              .-----. .-----.
     2        | eac | | flh |
              .-----. .-----.
   ...          ...     ...
              .-----. .-----.
    17        | eac | | flh |
              .-----. .-----.
```

### Separate Free List Spaces (for Slab Numbers 0, 1, and 2)

For slabs 0, 1, and 2 there is a "separate free list space" large
enough to hold 20,971,520 slot indexes, each slot index being 4 bytes
in size. (We'll describe to how to manage the free list for the other
slabs later.)

```
Figure 3. Organization of separate free list spaces for slab numbers 0, 1, and 2.

slab #     free list space
------     ---------------
           slot 0         slot 1         ... slot 20,971,519
           .------------. .------------.     .------------.
     0     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
     1     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
     2     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
```

## Algorithms, Simplified

Here is a first pass describing simplified versions of the
algorithms. After you read these simple descriptions, keep reading for
additional detail.

* `malloc()`

To allocate space, we identify the first slab containing slots are
large enough to hold the requested size (and also satisfy the
requested alignment).

If the free list is non-empty, pop the head element from it and return
the pointer to that slot.

If the free list is empty, increment the ever-allocated-count, `eac`,
and return the pointer to the newly-allocated slot.

* `free()`

Push the newly-freed slot onto the free list of its slab.

* `realloc()`

If the requested new size (aligned) requires a larger slot than the
allocation's current slot, then allocate a new slot (just like in
`malloc()`, above). Then `memcpy()` the contents of the current slot
into the beginning of that new slot, deallocate the current slot (just
like in `free()`, above) and return the pointer to the new slot.

That's it! You could stop reading here and you'd have a basic
knowledge of the design of `smalloc`.

## Algorithms, More Detail -- the Free Lists

The `flh` for a given slab is either a sentinel value meaning that the
free list is empty, or else it is the index of the slot most recently
pushed onto the free list.

To satisfy a `malloc()`, first check if the free list for your chosen
slab is non-empty, i.e. if its `flh` is not the sentinel value. If so,
then the `flh` is the index in this slab of the current
most-recently-freed slot.

We need to pop the head item off of the free list, i.e. set the `flh`
to point to the next item instead of the head item.

But where is the pointer to the *next* item? For slabs numbered 0, 1,
and 2, there is a separate free list space. The elements in the free
list space are associated with the data slots of the same index in the
data space -- if the data slot is currently freed, then the associated
free list slot contains the index of the *next* free slot (or the
sentinel value if there is no next free slot).

```
Figure 4. Free list slots associated with data slots for slabs 0, 1, and 2

slab #     data slots
------     ----------
           slot 0         slot 1         ... slot 20,971,519
           .------------. .------------.     .------------.
     0     | [data]     | | [data]     | ... | [data]     |
           .------------. .------------.     .------------.
     1     | [data]     | | [data]     | ... | [data]     |
           .------------. .------------.     .------------.
     2     | [data]     | | [data]     | ... | [data]     |
           .------------. .------------.     .------------.

slab #     free list space
------     ---------------
           slot 0         slot 1         ... slot 20,971,519
           .------------. .------------.     .------------.
     0     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
     1     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
     2     | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.
```

So to pop the head of the free list, for `malloc()`, for slab 0, 1, or
2, you take the `flh`, look up the indicated element in the free list
space, and copy its value to the `flh`. Now the element that was
formerly the second item in the free list is the head of the free
list.

To push an item onto the free list (making it become the new head) (in
order to implement `free()`) you are given the slot number of the item
to be freed. Take the `flh` and copy it into the "next slot#" space of
this slot number in the free list space. Now this item points to
whatever was formerly the head of the free list. Now set the `flh` to
be the slot number of this item. Now this item is the new head of the
free list.

What about for slabs numbered 3 through 17? There is no associated
free list space to hold the next-pointers. The answer is we store the
next-pointers in the same space where the data goes when the slot is
in use! Each data slot is either currently freed, meaning we can use
its space to hold the next-pointer, or currently allocated, meaning it
is not in the free list and doesn't need a next-pointer.

This technique is known as an "intrusive free list". Thanks to Andrew
Reece and Sam Smith, my colleagues at Shielded Labs, for explaining
this to me.

```
Figure 5. Intrusive free lists for slabs 3 through 17

slab #     data/free-list slots
------     --------------------

           slot 0                    slot 1                    ... slot 20,971,519
           .-----------------------. .-----------------------.     .-----------------------.
     3     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.
     4     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.
     5     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.
    ...               ...                       ...                           ...
           .-----------------------. .-----------------------.     .-----------------------.
    17     | [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
           .-----------------------. .-----------------------.     .-----------------------.
```

So for slabs 3 through 17, to satisfy a `malloc()` or `realloc()` by
popping the head item from the free list, what you do is take the
`flh` and read the *contents* of the indicated slot to find the index
of the *next* item in the free list. Set `flh` equal to the index of
that next item and you're done popping the head of the free list.

To push an item onto the free list (in order to implement `free()`),
you are given the slot number of the item to be freed. Take the
current `flh` and copy its value into the data slot of the item to be
freed. Now set the `flh` to be the index of the item to be
freed. That item is now the new head of the free list.

If there are no items on the free list when you are satisfying a
`malloc()` or `realloc()`, then you increment the
ever-allocated-count, `eac`, and return a pointer into the next,
never-before-allocated, slot.

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

If the new requested size doesn't fit into the current slot, and if it
is less than or equal to 64 bytes, allocate from the slab with 64 byte
slots (slab number 11).

If the new requested size doesn't fit into 64 bytes, then allocate
from the slab with 4 MiB slots (slab number 17).

(As always, if the requested size doesn't fit into 4 MiB, then fall
back to `mmap()`.)

## Algorithms, More Detail -- Overflowers

Suppose the user calls `malloc()` or `realloc()` and the slab we
choose to allocate from is full. That means the free list is empty,
and the ever-allocated-count is equal to 20,971,520 -- the total
number of slots in this slab. This could happen if there were
20,971,520 allocations from this slab alive simultaneously. In that
case, allocate from the next bigger slab, i.e. incremenent the slab
number and try again. (Thanks to Nate Wilcox, also my colleague at
Shielded Labs, for suggesting this technique.)

What if the slab you were trying to allocate from was the biggest slab
-- slab 17 -- and it was full? Then fall back to using `mmap()` to
request more memory from the operating system and just return the
pointer to that.

## Algorithms, *Even* More Detail -- Multiprocessing

To make `smalloc` perform well with multiple processors/cores
operating in parallel, we need to make two changes to the above.

### Thread-Safe State Changes

First, add thread-safe locks around the modifications of the free
lists and the ever-allocated-counts to ensure that concurrent updates
to them are valid. This is sufficient to ensure correctness of
`smalloc`'s behavior under multiprocessing.

Specifically, we use a simple loop with atomic compare-and-exchange or
fetch-and-add operations.

* To pop an element to the free list:

1. Load the value from `flh` into a local variable/register, `a`.
2. If it is the sentinel value, meaning that the free list is empty,
   return. (This `malloc()`/`realloc()` will then be satisfied from
   the never-yet-allocated slots instead.)
3. Load the value from the free list slot indexed by `a` into a local
   variable/register `b`.
4. Atomically compare-and-exchange the value from `b` into `flh` if
   `flh` still contains the value in `a`.
5. If the compare-and-exchange failed (meaning the value of `flh` has
   changed), jump to step 1.

Now you've safely popped the head of the free list into `a`.

* To push an element onto the free list, where `i` is the index to be
freed:

1. Load the value from `flh` into a local variable/register, `a`.
2. Store the value from `a` into the free list element with index `i`.
3. Atomically compare-and-exchange the index `i` into `flh` if `flh`
   still contains the value in `a`.
4. If the compare-and-exchange failed (meaning that value of `flh` has
   changed), jump to step 1.

Now you've safely pushed `i` onto the free list.

* To increment `eac`:

1. Fetch-and-add 1 to the value of `eac`.
2. If the result is `eac > 20,971,520`, meaning that the slab was
   already full, then fetch-and-add -1. (This `malloc()`/`realloc()`
   will then be satisfied by the next slab instead.)
   
Now you've safely incremented `eac`.

### Separate Areas For Multiprocessing

The second thing to do for multiprocessing is replicate the data
structures for slabs 0 through 10 (inclusive) into 64 identical
"areas", so that separate cores will (typically) use separate areas
from one another. This is not necessary for correctness, it is just a
performance optimization.

```
Figure 6. Organization of data slots including areas.

                              area 0                                        area 1                      ... (areas 2-63) ...         area 63
                             ------                                        ------                                |                   -------
						                                                                                         v
        slot # -> slot 0      slot 1      ... slot 20,971,519   slot 0      slot 1      ... slot 20,971,519              slot 0      slot 1      ... slot 20,971,519
                  ------      ------          ---------------   ------      ------          ---------------              ------      ------          ---------------
slab #  slot size
------  ---------
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     0       1  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     1       2  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     2       3  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     3       4  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     4       5  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                 .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     5       6  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     6       8  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     7       9  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     8      10  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
     9      16  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
    10      32  B | [data]  | | [data]  | ... | [data]  |       | [data]  | | [data]  | ... | [data]  |         ...      | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.       .---------. .---------.     .---------.                  .---------. .---------.     .---------. 
    11      64  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    12     128  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    13     256  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    14     512  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    15    1024  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    16    2048  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
    17   4 MiB  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------.
```

And add space for each area for the variables for the first 11 slabs
and the separate free list spaces for the first 3 slabs:

```
Figure 7. Organization of variables including areas.

 * "eac" is "ever-allocated count"
 * "flh" is "free-list head"

   area # ->      area 0           area 1      ...     area 63
                  ------           ------              -------
slab #        variable
------        --------
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     0        | eac | | flh |  | eac | | flh | ... | eac | | flh |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     1        | eac | | flh |  | eac | | flh | ... | eac | | flh |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     2        | eac | | flh |  | eac | | flh | ... | eac | | flh |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
   ...          ...     ...      ...     ...         ...     ...
              .-----. .-----.  .-----. .-----.     .-----. .-----.
     9        | eac | | flh |  | eac | | flh | ... | eac | | flh |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
    10        | eac | | flh |  | eac | | flh | ... | eac | | flh |
              .-----. .-----.  .-----. .-----.     .-----. .-----.
    11        | eac | | flh |
              .-----. .-----.
    12        | eac | | flh |
              .-----. .-----.
   ...          ...     ...
              .-----. .-----.
    17        | eac | | flh |
              .-----. .-----.
```

```
Figure 8. Organization of free list spaces for slab numbers 0, 1, and 2 including areas.
                                                                                                        ... (areas 2-62) ... 
area # ->  area 0                                             area 1                                              |   area 63
           ------                                             ------                                              v   -------
slab #     free list space
------     ---------------
           slot 0         slot 1         ... slot 20,971,519  slot 0         slot 1         ... slot 20,971,519       slot 0         slot 1         ... slot 20,971,519
           .------------. .------------.     .------------.   .------------. .------------.     .------------.        .------------. .------------.     .------------.
     0     | next slot# | | next slot# | ... | next slot# |   | next slot# | | next slot# | ... | next slot# |        | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.   .------------. .------------.     .------------.        .------------. .------------.     .------------.
     1     | next slot# | | next slot# | ... | next slot# |   | next slot# | | next slot# | ... | next slot# |        | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.   .------------. .------------.     .------------.        .------------. .------------.     .------------.
     2     | next slot# | | next slot# | ... | next slot# |   | next slot# | | next slot# | ... | next slot# |        | next slot# | | next slot# | ... | next slot# |
           .------------. .------------.     .------------.   .------------. .------------.     .------------.        .------------. .------------.     .------------.
```

Whenever choosing a slab for `malloc()` or `realloc()`, if the slab
number you choose is <= 10, then map the current processor/core/thread
onto one of the 64 areas. Multiple cores might still access the same
area at the same time, so you stil have to use the thread-safe state
update methods described above for correctness.

Now you know the entire data model and algorithms for `smalloc`!

## The Nitty-Gritty

### Layout

The variables are laid out in memory first. Area number is most
significant, followed by slab number. This is "column-wise" order when
looking at Figure 7 above. So in memory, first the `eac` and `flh` for
area 0 and slab 0 are laid out, and then the `eac` and `flh` for area
0 and slab 1, and so on. Since area 0 is first, all of its variables
(up to and including slab 17 variables) are laid out in memory before
the first variables from area 1 are.

Following the variables, the free lists for slabs 0, 1, and 2 are laid
out, with area number most significant, then slab number, then slot
number. This is "column-wise" order when looking at Figure 8.

Following the free lists, the data slots are laid out in memory, with
area number most significant, then slab number, then slot number. This
is "column-wise" order when looking at Figure 6 above. Like with the
variables, this means that all of the slabs for area 0 (up to and
including slab 17) are laid out in memory before the first slab from
area 1.

### Alignment

Sometimes the caller of `malloc()` requires an alignment, meaning that
the pointer returned needs to point to an address which is an integer
multiple of that alignment. Required alignments are always a power of
2.

In order satisfy such requests we'll ensure that every slot whose size
is a power of 2 begins at a virtual memory address which is an integer
multiple of its slot size. Since slots are the least-significant part
of the virtual memory address layout and slabs are the
second-least-significant part, this mean that slabs with slots of
power-of-2 sizes have to begin at a virtual memory address which is an
integer multiple of the slot size.

Additionally, we want each slab to begin at an even multiple of 64
bytes to optimize cache line usage.

Okay, now you know everything there is to know about `smalloc`'s data
model and memory layout. Given this information, you can calculate the
exact address of every data element in `smalloc`! (Counting from the
`smalloc` base pointer, which is the address of the first byte in the
layout described above.)

### Mapping Processor/Core Numbers to Areas

Have an atomic static (global) integer. When a thread calls `malloc()`
for the first time, it atomically increments that static integer (with
`fetch_add()`), then stores one less than its resulting value in this
thread's thread-local storage. Whenever this thread subsequently calls
`malloc()`, if the requested size is small enough to pack multiple
allocations into one cache line, then it reads the thread-number from
its thread-local storage and uses the area indicated by that
thread-number mod 64.

### Sentinel Value for flh

The sentinel value is actually `0` so you have to add 1 to an index
value before storing it in `flh` and subtract 1 from `flh` before
using it as an index.

# Rationale / Philosophy -- Why `smalloc` is beautiful (in my eyes)

"Allocating" virtual memory doesn't prevent any other code (in this
process or any other process) from being able to allocate or use
memory [\*]. It also doesn't increase cache pressure or cause any
other negative effects.

And, allocating one span of virtual memory -- no matter
how large -- imposes no more than a tiny bit of additional work on the
kernel's virtual-memory accounting logic. It's best to think of
"allocating" virtual memory as simply *reserving address space*. The
kernel simply ensures that no other code (in this process) that
requests address space will receive address space that overlaps with
this span that you just reserved. (The kernel needs to do this only
for other code running in *this* process -- other processes already
have completely separate address spaces which aren't affected by
allocations in this process.)

Therefore, it can be useful to reserve one huge span of address space and
then use only a small part of it. This technique is used occasionally
in scientific computing, such as to compute over large sparse
matrices, but I'm not aware of it being exploited to the hilt in a
memory manager before.

So, if you accept that "avoiding reserving too much virtual address
space" is not an important goal for a memory manager, what *are* the
important goals? `smalloc` was designed with the following goals,
written here in roughly descending order of importance:

(Caveat: the following is all based on my own starry-eyed love of
`smalloc`'s design, but I haven't actually tested it yet!)

1. Be simple, in both design and implementation. This helps greatly to
	ensure correctness -- always a critical issue in modern
	computing. "Simplicity is the inevitable price that we must pay
	for correctness."--Tony Hoare (paraphrased)

	Simplicity also eases making improvements to the codebase and
    learning from the codebase.

	I've tried to pay the price of keeping `smalloc` simple while
    designing and implementing it.

2. Place user data in locations that gain the benefits of
	caching. This comes into play when the user code actually reads
	from and writes to addresses that the memory manager returned to
	it.
   
   a. If a single CPU core accesses different allocations in quick
	   succession, and those allocations are packed into a single
	   cache line, then it can execute much faster due to having the
	   memory already in cache and not having to load it from main
	   memory. This can make the difference between a few cycles when
	   the data is already in cache versus tens or even hundreds of
	   cycles when it has to load it from main memory.
   
   b. On the downside, if multiple different CPU cores access
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
	
   c. Suppose the program accesses multiple allocations in quick
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

   Note that these three goals cannot be fully optimized by the memory
   manager, because they depend on how the user code accesses the
   memory. What `smalloc` does is use some simple heuristics intended
   to optimize the above goals under some assumptions about the
   behavior of the user code:

   i.  Pack as many separate allocations from a single thread into
		each cache line as possible to optimize for point (a.) --
		cache line sharing.

   ii. Place allocations requested by separate threads in separate
		areas, to minimize the risk of point (b.) -- false
		sharing. This is heuristically assuming that successive
		allocations requested by a single thread are less likely to
		later be accessed simultaneously by multiple different
		threads. You can imagine user code which violates this
		assumption -- having one thread allocate many small
		allocations and then handing them out to other threads/cores
		which then access them in parallel with one another. Under
		`smalloc`'s current design, this behavior could result in a
		lot of false-sharing. However, I can't think of a simple way
		to avoid this bad case without sacrificing the benefits of
		true sharing that we get by packing together allocations that
		then get accessed by the same core.
   
   iii. When allocations are freed by the user code, `smalloc` appends
		their slot to a free list. When allocations are subsequently
		requested, the most recently free'd slots are returned
		first. This is a LIFO (stack) pattern, which means user code
		that tends to access its allocations in a stack-like pattern
		will enjoy improved caching. (Thanks to Andrew Reece from
		Shielded Labs for teaching me this.)
   
   iv. For allocations too large to pack multiple into a single cache
		line, but small enough to pack multiple into a single memory
		page, `smalloc` attempts to pack multiple into a single memory
		page. It doesn't separate allocations of these sizes by
		thread, the way it does for small allocations, because there's
		no performance penalty when multiple cores access the same
		memory page (but not the same cache line) in parallel.
   
   Note that all four of these techniques are limited by the fact that
   all allocations in `smalloc` are in fixed-size slots in a
   contiguous slab of equal-sized slots. Therefore, only allocations
   of (almost) the same size as each other will get packed into a
   single cache line. So, for example, if the user code allocates one
   10-byte allocation and then one 32-byte allocation, those two will
   not share a cache line. On the other hand, if the user code does
   that over and over many times, then each batch of six of its
   10-byte allocations will share one (64-byte) cache line, and each
   batch of two of its 32-byte allocations will share one (64-byte)
   cache line. So maybe the user code will enjoy good caching benefits
   anyway--that remains to be tested. In any case, the way `smalloc`
   currently does this keeps things simple.

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
    
    `smalloc`'s implementation of `malloc()` incurs, in the common
    cases, only two, three, or four potential cache misses.
    
    The main reason `smalloc` incurs so few potential-cache-misses in
    these code paths is the sparseness of the data layout. `smalloc`
    has pre-reserved a vast swathe of address space and "laid out"
    unique locations for all of its slabs, slots, and variables (but
    only virtually -- without reading or writing any actual memory).
	
	Therefore, `smalloc` can calculate the location of a valid slab to
    serve this call to `malloc()` using only two data inputs: the
    requested size and alignment (which are on the stack in the
    function arguments and do not incur a potential-cache-miss) and --
    in the case of allocations small enough to pack multiple of them
    into a cache line -- the thread number (which is in thread-local
    storage: one potential-cache-miss). Having computed the location
    of the slab, it can access the `flh` and `eac` from that slab (one
    potential-cache-miss), at which point it has all the data it needs
    to compute the exact location of the resulting slot and to update
    the free list. (See below about why we don't typically incur
    another potential-cache-miss when updating the free list.)

    For the implementation of `free()`, we need to use *only* the
    pointer to be freed (which is on the stack in an argument -- not a
    potential-cache-miss) in order to calculate the precise location
    of the slot and the slab to be freed. From there, it needs to
    access the `flh` for that slab (one potential-cache-miss).

	Why don't we have to pay the cost of one more potential-cache-miss
    to update the free list (in both `malloc()` and in `free()`)?
    There is a sweet optimization here that the next free-list-pointer
    and the memory allocation occupy the same memory! (Although not at
    the same time.) Therefore, if the user code accesses the memory
    returned from `malloc()` after `malloc()` returns, there is no
    additional cache-miss penalty from `malloc()` accessing it before
    returning. Likewise, if the user code has recently accessed the
    memory to be freed before calling `free()` on it, then `smalloc`'s
    access of the same space to store the next free-list pointer will
    incur no additional cache-miss. (Thanks to Sam Smith from Shielded
    Labs for teaching me this.)
    
    This optimization doesn't apply to allocations too small to hold a
    next-free-list-pointer, i.e. to allocations of size 1, 2, or 3
    bytes. For those, `smalloc` can't store the next-pointer -- which
    is 4 bytes -- in the slot, and so has to store it in a separate
    location, and does incur one additional potential-cache-line miss
    in both `malloc()` and `free()`.

	Counts of the potential-cache-line misses for the common cases:
	
	* `malloc()`:
	  * If the allocation size <= 32, then: + 1 to access the `threadnum`
	  * + 1 to access the `flh` and `eac`
	  * If the allocation size >= 4, then: + 1 to access the intrusive
		  free list entry, in which case the user code accessing the
		  resulting memory doesn't incur an additional cache-miss (as
		  long as the user code accesses it before it falls out of
		  cache)
      * Else: + 1 to access the separate free list entry, and + 1 for
          the user code to access the resulting memory
	  
	  Total of 2, 3, or 4 potential-cache-misses.
	  
	* `free()`:
	  * + 1 to access the `flh`
	  * If the allocation size >= 4, then: + 1 to access the intrusive
		  free list entry, in which the user code accessing the
		  resulting memory doesn't incur an additional cache-miss (as
		  long as the user code accesses it before it falls out of
		  cache)
      * Else: + 1 to access the separate free, and + 1 for the user
          code to access the resulting memory

	  Total of 2 or 3 potential-cache-misses.
	  
5. Be *consistently* efficient.

    I want to avoid intermittent performance degradation, such as when
    your function takes little time to execute usually, but
    occasionally there is a latency spike when the same function takes
    much longer to execute.
	
    I also want to minimize the number of scenarios in which
    `smalloc`'s performance degrades due to the user code's behavior
    triggering an "edge case" or a "worst case scenario" in
    `smalloc`'s design.
	
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
    scenarios" for `smalloc` beyond these two. In particular,
    `smalloc` never has to "rebalance" or re-arrange its data
    structures, or do any "deferred accounting" like some other memory
    managers do, which nicely eliminates some sources of intermittent
    performance degradation.
    
    Additionally, there are no locks in `smalloc`, so it will
    hopefully handle heavy multi-processing contention (i.e. many
    separate cores allocating and freeing memory simultaneously) with
    consistent performance. (There *are* concurrency-resolution loops
    in `malloc` and `free` -- see the pseudo-code in "Thread-Safe
    State Changes" above -- but these are not locks. Any thread that
    runs that code will make progress in only a few CPU cycles, so
    this cannot trigger priority inversion or a "pile-up" of threads
    waiting for a lock.)
    
    So with the possible exception of the two "worst-case scenarios"
    described above, I optimistically expect that `smalloc` will show
    excellent and extremely consistent performance.

6. Efficiently support using `realloc()` to extend
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
   `realloc()`'ing an allocation to a larger size. My profiling
   results indicate that this technique would indeed eliminate at
   least 90% of the unnecessary memory-copying when extending Vectors,
   making it almost costless to extend a Vector any number of times
   (as long as the new size is less than the size of `smalloc`'s large
   slots: 4 MiB).

XXX TODO: see if you can prove whether jumping straight to large slots on the first resize is or isn't a performance regression compared to the current technique of first jumping to the 32-byte slot size and only then jumping to the large slot size. If you can't prove that the current technique is substantially better in some real program, then switch to the "straight to large slots" alternative for simplicity.

[\*] Caveat: the *one* drawback of reserving a huge span of virtual
address space...

Although everything I wrote above is true, about how reserving huge
swathes of address space -- even reserving more than half of all of
the possible address space -- does not interfere with any other code's
use of memory... there is one thing that it does prevent other code
from doing: also reserving more than half of all possible address
space! Normally, no other code in your process wants to do that, so
it's no problem. One plausible use case that this does prevent,
though, is one process having multiple instances of `smalloc`, for
example to have multiple heaps for an execution environment. (Thanks
to Sam Smith for suggesting this use case to me.) An upcoming
technology upgrade called "5-level paging" increases the total usable
address space to 57 bits, which would be more than enough to have
multiple instances of `smalloc` in a single process.

# Open Issues / Future Work

* TODO: see if the "tiny" slots (1-, 2-, and 3-byte slots) give a
  substantial performance improvement in any real program. If not,
  remove them, for simplicity. I guess another way to say the same
  thing is: remove them, see if that causes a substantial performance
  regression in any real program, and if so put them back. >:-D

* The current design and implementation of `smalloc` is "tuned" to
  64-byte cache lines and 4096-bit virtal memory pages.
  
  The current version of `smalloc` works correctly with larger cache
  lines but there might be a performance improvement from a variant of
  `smalloc` tuned for 128-bit cache lines. Notably the new Apple ARM64
  chips have 128-bit cache lines in some cases, and modern Intel chips
  have a hardware cache prefetcher that sometimes fetches the next
  64-bit cache line.
 
  It works correctly with larger page tables but there might be
  performance problems with extremely large ones -- I'm not sure.
  Notably "huge pages" of 2 MiB or 1 GiB are sometimes configured in
  Linux especially in "server"-flavored configurations.
 
  There might also be a performance improvement from a variant of
  `smalloc` tuned to larger virtual memory pages. Notably virtual
  memory pages on modern MacOS and iOS are 16 KiB.

* If we could allocate even more virtual memory space, `smalloc` could
  more scalable (eg large slot-sizes could be larger than 4 mebibytes,
  the number of per-thread areas could be greater than 64), and you
  could have more than one `smalloc` heap in a single process. Larger
  (than 48-bit) virtual memory addresses are already supported on some
  platforms/configurations, especially server-oriented ones, but are
  not widely supported on desktop and smartphone platforms. We could
  consider creating a variant of `smalloc` that works only platforms
  with larger (than 48-bit) virtual memory addresses and offers these
  advantages.

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

Notes:

Our variables need to be 4-byte aligned (for performance and
correctness of atomic operations, on some/all architectures). They
always will be, because the variables are laid out at the beginning of
the mmap'ed region, which is always page-aligned.



Things `smalloc` does not currently attempt to do:

* Try to mitigate malicious exploitation after a memory-usage bug in
  the user code.
