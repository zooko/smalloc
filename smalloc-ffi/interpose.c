#include <stddef.h>
#include <stdlib.h>

extern void* smalloc_malloc(size_t);
extern void smalloc_free(void*);
extern void* smalloc_realloc(void*, size_t);

typedef struct { void* replacement; void* replacee; } interpose_t;

__attribute__((visibility("default"), section("__DATA,__interpose")))
interpose_t interpose_malloc = { 
    (void*)smalloc_malloc, 
    (void*)malloc 
};

__attribute__((visibility("default"), section("__DATA,__interpose")))
interpose_t interpose_free = { 
    (void*)smalloc_free, 
    (void*)free 
};

__attribute__((visibility("default"), section("__DATA,__interpose")))
interpose_t interpose_realloc = { 
    (void*)smalloc_realloc, 
    (void*)realloc 
};
