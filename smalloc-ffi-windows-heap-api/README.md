* go back and prune out unused System* functions that smalloc doesn't need to use after all

MUST implement (for correctness & safety):

☐ GetProcessHeap- return smalloc sentinel handle
☐ HeapAlloc- main allocation
   Return value has to be >=8-byte-aligned (https://learn.microsoft.com/en-us/windows/win32/api/heapapi/nf-heapapi-heapalloc).
☐ HeapFree- main deallocation
☐ HeapReAlloc- reallocation
☐ HeapSize- query allocation size
☐ HeapValidate- could be called with process heap handle
☐ HeapLock- could be called with process heap handle
☐ HeapUnlock- could be called with process heap handle
☐ HeapCompact- could be called with process heap handle
☐ HeapWalk- could be called with process heap handle
☐ HeapSetInformation- could be called with process heap handle
☐ HeapQueryInformation- could be called with process heap handle
☐ GetProcessHeaps- enumerates heaps (can forward)

Do NOT need to implement (just forward):

❌HeapCreate- returns a different heap, not ours
❌HeapDestroy- operates on different heaps -- will not be called on the default process heap (or if it is, the resulting corruption or crash is someone else's fault, and would have happened anyway without smalloc)
