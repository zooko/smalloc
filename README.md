# smalloc -- a simple memory allocator

`smalloc` ("Simple Memory ALLOCator") is a new memory allocator,
suitable (I hope) as a drop-in replacement for the glibc built-in
memory allocator, `jemalloc`, `mimalloc`, etc.

I would *like* to say that it exhibits good performance properties
compared to those others, but I haven't actually implemented and
tested it yet, so I'll have to limit myself to saying it is simple. 😂

Or... at least I can claim that it is simpler than the others! 😂😂

# How it works

To understand how `smalloc` works, you need to know the data model and
the algorithms that use it.

## Data model

Here is the basic data model: all memory managed by `smalloc` is
organized into "slabs". A slab is a fixed-length array of fixed-length
"slots". Every pointer returned by a call to `smalloc`'s `malloc()` or
`free()` is a pointer to the beginning of one of those slots, and that
slot is used exclusively for that memory allocation until it is
`free()`'ed [*].

([*] Except for calls to `malloc()` or `realloc()` with requested
sizes that are too big to fit into even the biggest of `smalloc`'s
slots, which `smalloc` instead satisfies by falling back to `mmap()`.)

Here is some ASCII art depicting the layout of the slabs and
slots. `[data]` means an area of memory that can be returned from
`malloc()` or `realloc()` for use by the caller. Figure 1:

```
Figure 1. Organization of data slots.

         slot # ->     slot 0      slot 1 ...    slot 254    slot 255 ... slot 65,534 slot 65,535 ... slot 16,777,214
                       ------      ------        --------    --------     ----------- -----------     ---------------
slab #  slot size
------  ---------
                  .---------. .---------.     .---------. 
     0       1  B | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------.
     1       2  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------. 
     2       3  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------. 
     3       4  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     4       5  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     5       6  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     6       7  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     7       8  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     8       9  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     9      10  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    10      12  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    11      16  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    12      21  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    13      32  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    14      64  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    15      85  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    16     113  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    17     151  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    18     204  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    19     273  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    20     372  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    21     512  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    22     682  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    23     1.0 KB | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    24     1.3 KB | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    25     2.0 KB | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    26     5.9 MB | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.

```

Slab 0 (which has slots of size 1) has only 255 slots (that's one less
than 2^8). The largest slot number in slab 0 is slot number 254. Slab
1 (which has slots of size 2 bytes) has 65,535 slots (one less than
2^16). The largest slot number in Slab 1 is slot number 65,534. All
the rest of the slabs have 16,777,215 slots (one less than 2^24).

Now there are only two more things you need to learn in order to fully
understand the complete `smalloc` data model.

The first is that for Slabs 0 through 13 (inclusive), there are
actually 256 "areas" each having this same layout.

Imagine slabs 0-13 (inclusive) from Figure 1 above replicated 256
times into 256 identically laid-out areas, extending into the
foreground in 3-D. (For slabs 14 and up, there is only one of each
slab.) See Figure 2:

```
Figure 2. Organization of data slots (for multiple areas).

                                                        AREA 2   slot # ->     slot 0      slot 1 ...    slot 254    slot 255 ... slot 65,534 slot 65,535 ... slot 16,777,214
                                                                               ------      ------        --------    --------     ----------- -----------     ---------------
                                                        slab #  slot size
                                                        ------  ---------
                                                                          .---------. .---------.     .---------.
                            AREA 1  slot # ->      slot      0       1  B | [data]  | | [data]  | ... | [data]  |
                                                   ----                   .---------. .---------.     .---------. .---------.     .---------.
                            slab #  slot size                1       2  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                            ------  ---------                             .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
                                              .--------      2       3  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
AREA 0  slot # ->      slot      0       1  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
                       ----                   .--------      3       4  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
slab #  slot size                1       2  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
------  ---------                             .--------      4       5  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------      2       3  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     0       1  B | [data]                    .--------      5       6  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------      3       4  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     1       2  B | [data]                    .--------      6       7  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------      4       5  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     2       3  B | [data]                    .--------      7       8  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------      5       6  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.     <-- AREA 2
     3       4  B | [data]                    .--------      8       9  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------      6       7  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     4       5  B | [data]                    .--------      9      10  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------      7       8  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     5       6  B | [data]                    .--------     10      12  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------      8       9  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     6       7  B | [data]                    .--------     11      16  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------      9      10  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     7       8  B | [data]                    .--------     12      21  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------     10      12  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     8       9  B | [data]                    .--------     13      32  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .--------     11      16  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
     9      10  B | [data]                    .--------     ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                  .--------     12      21  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
    10      12  B | [data]                    .--------. .---------.     .---------. .---------.     .---------. .---------.     .---------.      <-- AREA 1
                  .--------     13      32  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
    11      16  B | [data]                    .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
                  .--------     ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    12      21  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.      <-- AREA 0
    13      32  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    14      64  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.      <-- slabs in AREA 0 but not in any of the other areas
    15      85  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    16     113  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    17     151  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    18     204  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    19     273  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    20     372  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    21     512  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    22     682  B | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    23     1.0 KB | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    24     1.3 KB | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    25     2.0 KB | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
    26     5.9 MB | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  | | [data]  | ... | [data]  |
                  .---------. .---------.     .---------. .---------.     .---------. .---------.     .---------.
```

The last thing to know about the data model is that for each slab,
there are two associated variables: the count of ever-allocated slots,
and the index of the first freed slot.

The count of ever-allocated slots (abbreviated `eac` is the number of
slots in that slab have ever been allocated (i.e. has a pointer ever
been returned by `malloc()` or `realloc()` that points to that
slot). The ever-allocated count (`eac`) is 1 byte in size for each of
the Slab 0's, 2 bytes for each of the Slab 1's, and 3 bytes in size
for all other slabs.

The index of the first freed slot (abbreviated `ffs`) is also 1 byte
for Slab 0's, 2 bytes for Slab 1's, and 3 bytes for all other
slabs. See Figure 3:

```
Figure 3. Organization of variables.

 * "eac" is "ever-allocated count"
 * "ffs" is "free-list head"

   area # ->                 area 0                area 1 ...              area 255
                             ------                ------                  --------
slab #        variable and its size
------        ---------------------
              .--------. .--------. .--------. .--------.     .--------. .--------.
     0        | eac 1B | | ffs 1B | | eac 1B | | ffs 1B | ... | eac 1B | | ffs 1B |
              .--------. .--------. .--------. .--------.     .--------. .--------.
     1        | eac 2B | | ffs 2B | | eac 2B | | ffs 2B | ... | eac 2B | | ffs 2B |
              .--------. .--------. .--------. .--------.     .--------. .--------.
     2        | eac 3B | | ffs 3B | | eac 3B | | ffs 3B | ... | eac 3B | | ffs 3B |
              .--------. .--------. .--------. .--------.     .--------. .--------.
   ...            ...        ...        ...        ...            ...        ...
              .--------. .--------. .--------. .--------.     .--------. .--------.
    12        | eac 3B | | ffs 3B | | eac 3B | | ffs 3B | ... | eac 3B | | ffs 3B |
              .--------. .--------. .--------. .--------.     .--------. .--------.
    13        | eac 3B | | ffs 3B | | eac 3B | | ffs 3B | ... | eac 3B | | ffs 3B |
              .--------. .--------. .--------. .--------.     .--------. .--------.
    14        | eac 3B | | ffs 3B |
              .--------. .--------.
    15        | eac 3B | | ffs 3B |
              .--------. .--------.
   ...            ...        ...
              .--------. .--------.
    26        | eac 3B | | ffs 3B |
              .--------. .--------.
```

Now you know the entire data model. There are no more data elements in
`smalloc`!

## Layout

The variables are laid out in memory first. Area # is most
significant, followed by slab #. This is "column-wise" order when
looking at Figure 3 above. So in memory, first the `eac` and `flh` for
area 0 and slab 0 are laid out, and then the `eac` and `flh` for area
0 and slab 1, and so on.

Following the variables, the data slots are laid out in memory, with
Area # most significant, then Slab #, then Slot #. This is "layer-wise
first and then row-wise" order when looking at Figure 2 above. So in
memory, first all the data slots for area 0, slab 0 are laid out, and
then all the data slots for area 0, slab 1, and so on. Since area 0 is
laid out first, all of its slabs, including slab 14 and beyond, are
laid out in memory before any of area 1's slabs are.

## Alignment

Sometimes the caller of `malloc()` includes a required alignment,
meaning that the pointer returned needs to point to an address which
is an integer multiple of that alignment. Required alignments are
always a power of 2.

In order to be able to satisfy such requests, we need to ensure that
every slot whose size is a power of 2 begins at a virtual memory
address which is an integer multiple of its slot size. Since slots are
the least-significant part of the virtual memory address layout and
slabs are the second-least-significant part, this mean that slabs with
slots of power-of-2 sizes have to begin at a virtual memory address
which is an integer multiple of the slot size.

Okay, now you know everything there is to know about `smalloc`'s data
model and memory layout. Given this information, you can calculate the
exact address of every data element in `smalloc`! (Counting from the
`smalloc` base pointer, which is the address of the first variable in
the layout described above.)

# Algorithms, Simplified

Here is a first pass describing simplified versions of the
algorithms. After you read these simple descriptions, keep reading for
additional detail.

## `malloc()`

To allocate space, we identify a slab that contains slots that are
large enough to hold the requested size (and also satisfy the
requested alignment).

If the free list is non-empty, pop the head element from it and return
the pointer to that slot.

If the free list is empty, increment the allocated-count and return
the pointer to the newly-allocated slot.

## `malloc()`

Insert the newly-freed slot as the new head of the free list of its
slab.

## `realloc()`

If the requested new size (aligned) requires a larger slot than the
allocation's current slot, then allocate a new slot (just like in
`mealloc()`, above), `memcpy()` the contents of the current slot into
the beginning of that new slot, and return the pointer to the new
slot.

# Algorithms, More Detail

## `malloc()`

First, calculate the slab number whose slots are big enough to hold
the requested (aligned) space.

If the slab number is 13 or less, then get the processor number from
the current processor, and deterministically map it to one of the 256
areas. From here on, use the 




# Why it works -- rationale
