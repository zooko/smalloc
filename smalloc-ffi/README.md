# smalloc-ffi -- a simple memory allocator for C/C++/native code

Build the `smalloc-ffi` crate, which produces shared and static libs. Arrange linking so that uses
of `malloc`, `free`, and `realloc` will link to those functions.

Dynamic linking on Linux:

```
LD_PRELOAD=./libsmalloc_ffi.so ./prog
```

Dynamic linking on macOS:

```
DYLD_INSERT_LIBRARIES=./libsmalloc_ffi.dylib ./prog
```

Note: it is safe to pass pointers allocated with the default/system allocator to `smalloc`'s `free`
or `realloc` or any other `smalloc` function that accepts a pointer to an allocation. `smalloc` will
detect that this pointer was not allocated by `smalloc` and will invoke the appropriate
default/system `free` or `realloc`. It was necessary to implement this because during the dynamic
loading process, allocations are made before `smalloc`'s functions get installed as `malloc`
etc. Then after `smalloc`'s functions have gotten installed, the allocations previously allocated
are passed to `free`, or `realloc`, which are `smalloc`'s implementations at that point.

It is not safe to pass pointers allocated with `smalloc` to the default/system allocator, but I
don't know of a way that would happen.
