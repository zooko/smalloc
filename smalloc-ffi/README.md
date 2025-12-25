# smalloc-ffi -- a simple memory allocator for C/C++/native code

Build the `smalloc-ffi` crate, which produces shared and static libs. Arrange linking so that uses
of `malloc`, `free`, and `realloc` will link to those functions from those libs.

Dynamic linking on Linux:

```
LD_PRELOAD=./libsmalloc_ffi.so ./your_c_program
```

Dynamic linking on macOS:

```
DYLD_INSERT_LIBRARIES=./libsmalloc_ffi.dylib ./prog
```

Static linking:

```
cc -o program program.c -L./target/release -lsmalloc_ffi
```
