# smalloc -- a simple memory allocator

`smalloc` ("Simple Memory ALLOCator") is a new memory allocator,
suitable (I hope) as a drop-in replacement for the glibc built-in
memory allocator, `jemalloc`, `mimalloc`, etc.

I would *like* to say that it exhibits good performance properties
compared to those others, but I haven't actually implemented and
tested it yet, so I'll have to limit myself to saying it is simple. 😂

Or... at least I can claim that it is simpler than the others! 😂😂

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

### Free List Spaces (for Slab Numbers 0 and 1)

For slabs 0, 1, and 2 there is a "free list space" large enough to
hold 20,971,520 slot indexes, each slot index being 4 bytes in
size. (We'll describe to how to manage the free list for the other
slabs later.)

```
Figure 3. Organization of free list spaces for slab numbers 0, 1, and 2.

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
Figure 4. Free lists for slabs 0, 1, and 2

data slots
----------
slot 0         slot 1         ... slot 20,971,519
.------------. .------------.     .------------.
| [data]     | | [data]     | ... | [data]     |
.------------. .------------.     .------------.

free list space
---------------
slot 0         slot 1         ... slot 20,971,519
.------------. .------------.     .------------.
| next slot# | | next slot# | ... | next slot# |
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

data slots
----------
slot 0                    slot 1                    ... slot 20,971,519
.-----------------------. .-----------------------.     .-----------------------.
| [data or next slot #] | | [data or next slot #] | ... | [data or next slot #] |
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

Specifically, we use simple spin-locks with atomic
compare-and-exchange or fetch-and-add operations.

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
   changed), jump to step 1. (This is a spin-lock.)

Now you've safely popped the head of the free list into `a`.

* To push an element onto the free list, where `i` is the index to be
freed:

1. Load the value from `flh` into a local variable/register, `a`.
2. Store the value from `a` into the free list element with index `i`.
3. Atomically compare-and-exchange the index `i` into `flh` if `flh`
   still contains the value in `a`.
4. If the compare-and-exchange failed (meaning that value of `flh` has
   changed), jump to step 1. (This is a spin-lock.)

Now you've safely pushed `i` onto the free list.

* To increment `eac`:

1. Fetch-and-add 1 to the value of `eac`.
2. If the result is `eac > 20,971,520`, meaning that the slab was
   already full, then fetch-and-add -1, and then return. (This
   `malloc()`/`realloc()` will then be satisfied by the next slab
   instead.)
   
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
    27        | eac | | flh |
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
number you choose is <= 11, then map the current processor/core/thread
onto one of the 64 areas. Multiple cores might still access the same
area at the same time, so you stil have to use the thread-safe state
update methods described above for correctness.

Now you know the entire data model and algorithms for `smalloc`!

## The Nitty-Gritty

### Layout

The variables are laid out in memory first. Area number is most
significant, followed by slab number. This is "column-wise" order when
looking at Figure 3 above. So in memory, first the `eac` and `flh` for
area 0 and slab 0 are laid out, and then the `eac` and `flh` for area
0 and slab 1, and so on.

Following the variables, the free lists for slabs 0 and 1 are laid
out, with area number most significant.

Following the free lists, the data slots are laid out in memory, with
area number most significant, then slab number, then slot number. This
is "column-wise" order when looking at Figure 2 above. So in
memory, first all the data slots for area 0, slab 0 are laid out, and
then all the data slots for area 0, slab 1, and so on. Since area 0 is
laid out first, all of its slabs, including slab 11 and beyond, are
laid out in memory before any of area 1's slabs are.

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
`smalloc` base pointer, which is the address of the first bytes in the
layout described above.)

### Mapping Processor/Core Numbers to Areas

Maintain an atomically-incrementing u8 in thread-local storage
xxx

Get the Rust ThreadID, call `.as_u64()` to convert it to a u64, and
take mod (`%`) 256 to take the least-significant 8 bits of it.

### Sentinel Value for flh

The sentinel value is actually `0` so you have to add 1 to an index
value before storing it in `flh` and subtract 1 from `flh` before
using it as an index.

# Rationale / Philosophy

to be added

# Open Issues / Future Work

* The current design and implementation of `smalloc` is "tuned" to
  64-byte cache line and 4096-bit virtal memory page. (See
  `MAX_SLABNUM_TO_PACK_INTO_CACHELINE` and
  `MAX_SLABNUM_TO_PACK_INTO_PAGE`.)
  
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
  be even *simpler* (eg apply the "areas" structure to all slabs and
  not just the ones whose slots fit into cache lines), more scalable
  (eg large slot-sizes could be larger than 6.1 million bytes), and
  more flexible (eg have more than one `smalloc` heap in a single
  process). Larger (than 48-bit) virtual memory addresses are already
  supported on some platforms/configurations, especially
  server-oriented ones, but are not widely supported on desktop and
  smartphone platforms. We could consider creating a variant of
  `smalloc` that works only platforms with larger (than 48-bit)
  virtual memory addresses and offers these advantages.

* The Rust ThreadId documentation specifically says not to rely on the
  values of resulting ThreadIds for anything other than equality
  testing. However, we are relying (for performance optimization, not
  for correctness) on the lowest 8 bits of the values not colliding
  with the lowest 8 bits of other ThreadIds (or at least not colliding
  *much*). The current implementation of Rust ThreadId just has an
  incrementing counter, which is perfect for us! Possible future work:
  find some way to get a contract from the underlying platform
  (whether Rust std lib, operating system, or CPU), not just an
  implementation detail, that allows us to spread
  `malloc()`/`realloc()` calls from different cores/threads out across
  the areas (probabilistically--just for performance optimization). (I
  looked into the `RDPID` instruction on x86 and the `MPIDR`
  instruction on ARM as a way to do this without the overhead of an
  operating system call, but using `MPIDR` resulted in an illegal
  instruction exception on MacOS on Apple M4, so I gave up on that
  approach.)

Notes:

Our variables need to be 4-byte aligned (for performance and
correctness of atomic operations, on some/all architectures). They
always will be, because the variables are laid out at the beginning of
the mmap'ed region, which is always page-aligned.



Things `smalloc` does not attempt to do:

* Try to prevent exploitation after a memory-usage bug in the user
  code.

* Try to minimize allocation of virtual memory.

