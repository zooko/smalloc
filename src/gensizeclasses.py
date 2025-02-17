# Let's say there are the following sizes of spaces that we want to pack objects into in order to fit as many objects as we can into spaces of varying sizes.

# There are two types:

# Firstly, the small spaces: L1/L2 cache lines, L3 cache lines (which are apparently double the size of L1/L2 cache lines on some machines), and double-cache-lines (due to the "next cache line prefetcher that apparently can prefetch one or more cache lines on some machines in some conditions: https://community.intel.com/t5/Software-Tuning-Performance/When-L1-Adjacent-line-prefetchers-starts-prefetching-and-how/td-p/1166311). For these ones, the objects will be stored in a single slab (in successive slab items), and we want to fit as many of those objects into one of these cache lines (etc) as possible to maximize constructive interference ("true sharing").

# Secondly, the large spaces: virtual memory pages ; We want to fit as many objects as possible into one virtual memory page in order to reduce TLB cache pressure.

# L1/L2 cache lines are almost always 64 bytes (on the modern, 64-bit CPUs that we are targetting). The only exception I've heard about is the new Apple chips which have some (but not all) of their caches 128 bytes instead of 64 bytes): https://www.7-cpu.com/cpu/Apple_M1.html

# L3 cache lines are apparently sometimes 128 bytes? (Lost the reference to that claim, and it is contradicted by all reliable data I find, which says Intel and AMD always use 64b at every level.)

# Also, of course, remember about the potential cache-line-prefetching. So let's just include a space of 256 bytes to acconodate the possibility that there are 128-byte cachelines with one extra prefetch (or, I suppose, maybe 64-byte cache lines with 3 extra prefetches...)

# Now virtual memory pages are: 4 KiB (Linux, Windows, MacOSX), 16 KiB (64-bit ARM), 2 MiB (Linux--hugepages, whether configured by the sysadmin, requested by the process, or transparently activated by the kernel), 1 GiB (Linux--jumbo ?? Unclear on when this can happen!), 

# ... What about caches?? Does it makes sense to try to pack more objects into the total cache? Well, maybe in specific circumstances, but I imagine probably not, but it wouldn't hurt, but here's a question: do the resulting size classes even change when we add those? Well, to find out, let's compute the size classes with and without. Cache sizes (data cache) tend to be like these sorts of numbers: 32 KiB, 64 KiB, 128 KiB, 4 MiB, 8 MiB, 12 MiB, 16 MiB (https://news.ycombinator.com/item?id=25660467, https://www.7-cpu.com/cpu/Apple_M1.html, https://www.7-cpu.com/cpu/SiFive_U74.html)

# Okay, with all that background, let's pick some size classes!

# First of all what size classes pack the most sizes of objects into the small-sized spaces (cache lines)?

import math
spacebests = {}
prevspace = 0
# # for space in (64, 128, 256):
# for space in (64, 128, 256, 4*2**10, 16*2**10, 2*2**20, 1*2**30):
#     prevbest = 0
#     bests = set()
#     for size in range(space, prevspace, -1):
#         fitted = math.floor(space/size)
#         if fitted > prevbest:
#             # print("size: %2d, fitted: %2d" % (size, fitted))
#             prevbest = fitted
#             bests.add(size)
# 
# #    print(bests)
# 
#     spacebests[space] = bests
#     prevspace = space
        
# print(spacebests)

spacebests2 = {}
prevspace = 0
# for space in (64, 128, 256):
# for space in (64, 4*2**10):
for space in (64,):
    bests = set()
    for siz in range(prevspace+1, space+1):
        fitted = math.floor(space/siz)
        biggestfit = math.floor(space/fitted)
        bests.add(biggestfit)
        # print("space: %s, siz: %s, fitted: %s, biggestfit: %s" % (space, siz, fitted, biggestfit,))
        
    spacebests2[space] = bests
    prevspace = space


print(spacebests2)

# for space in spacebests.keys():
#     b1 = spacebests[space]
#     b2 = spacebests2[space]
#     if set(b1) != set(b2):
#         print("wtf b1-b2: %s, b2-b1: %s, b1: %s, b2: %s" % (b1-b2, b2-b1, b1, b2))
#         break

#    prevbest = 0
#    bests = []
#    for size in range(space, 0, -1):
#        fitted = math.floor(space/size)
#        if fitted > prevbest:
#            print("size: %2d, fitted: %2d" % (size, fitted))
#            prevbest = fitted
#            bests.append(size)

#     prevspace = space
#     print(bests)

#     spacebests[space] = bests

# Okay here are the size classes which can pack the most objects into spaces of size 256 bytes (as well as into spaces of 128 bytes and of 64 bytes, excluding, of course, the size classes too large for those smaller spaces):
# 256, 128, 85, 64, 51, 42, 36, 32, 28, 25, 23, 21, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1

def c(num_bytes):
    for unit in ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB', 'EiB']:
        if num_bytes < 1024:
            return f"{num_bytes:.2f} {unit}"
        num_bytes /= 1024


