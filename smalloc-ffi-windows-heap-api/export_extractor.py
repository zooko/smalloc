#!/usr/bin/env python3

# Thanks to Claude Sonnet 4.5 for help ideating this approach and writing this file.

"""
Extract kernel32.dll exports and generate DEF file for smalloc interposition.

Usage:
    dumpbin /EXPORTS C:\Windows\System32\kernel32.dll | python export_extractor.py > kernel32.def
"""
import sys
import re

# Heap API functions implemented by smalloc
SMALLOC_HEAP_FUNCTIONS = {
    'GetProcessHeap',
    'GetProcessHeaps', 
    'HeapAlloc',
    'HeapFree',
    'HeapReAlloc',
    'HeapSize',
    'HeapCreate',
    'HeapDestroy',
    'HeapValidate',
    'HeapLock',
    'HeapUnlock',
    'HeapCompact',
    'HeapWalk',
    'HeapSetInformation',
    'HeapQueryInformation',
}

def main():
    lines = sys.stdin.readlines()

    print('LIBRARY kernel32')
    print('EXPORTS')

    for line in lines:
        # Match dumpbin export format: ordinal, hint, RVA, function_name
        match = re.match(r'\s+\d+\s+\w+\s+\w+\s+(\w+)', line)
        if match:
            func_name = match.group(1)

            if func_name in SMALLOC_HEAP_FUNCTIONS:
                # Smalloc implementation of this function, for code that invokes these `HeapAlloc`
                # API functions to get linked to:
                print(f'    {func_name} = smalloc_{func_name}')

                # Original system implementation of this function, for `smalloc` itself to invoke
                # when it needs to:
                print(f'    System{func_name} = KERNEL32_SYSTEM.{func_name}')
            else:
                # Forward everything else to original system kernel32
                print(f'    {func_name} = KERNEL32_SYSTEM.{func_name}')

if __name__ == '__main__':
    main()
