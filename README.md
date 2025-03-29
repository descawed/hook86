# hook86

A Rust library of utilities for DLL-injection hacks on 32-bit x86.

I've written this primarily for my personal use so I'm only briefly documenting it here. I could
try to clean it up a bit if anyone else has any interest in it.

## Overview

This library only supports 32-bit x86 at the moment. I expect I'll add x64 support at some point
when I have a project that requires it. I don't expect I'll ever support architectures other than
x86; that would probably require a different library. The focus is on games, so only Windows is
supported, as that's the platform that the overwhelming majority of games target. I have some
interest in adding Linux support, but it would require a big refactor and I also don't plan to do
it until I have a project that requires it.

The library includes a proc macro, so the repo is a workspace with three crates: `hook86_macro`,
the proc macro; `hook86`, the main library; and `hook86_core` for types and functions needed by
both the proc macro and the main library. For the purposes of this document, I'll only cover the
modules of the main library.

### asm

Functions for generating common branch instructions (e.g. call, jmp, jz, jle, etc.) from one
address to another. Also contains the `get_branch_target` function which will read a branch
instruction at the given address and return the absolute address that the branch targets.

### crash

Optional crash logging infrastructure for when the hacks are a little too hacky. Requires the
`crash_logging` feature to be enabled; logs via the `log` crate.

### mem

Contains utilities for manipulating memory - removing protection (i.e. enabling read, write, and
execute permissions), changing protection, patching game memory. Also includes the `ByteSearcher`
type which allows you to search for byte strings in program memory with optional filters for
where in memory or in what type of memory we should search. `ByteSearcher` can also verify that
provided addresses reside in a region of memory that matches certain filters

### patch

Contains the `patch!` macro for defining assembly patches containing placeholders. Each patch is
its own type. The generated `bind` method takes one argument per placeholder, which should be an
absolute address or immediate value. After you've determined the addresses/values that need to
be filled in at runtime, call the `bind` method to fill in the placeholders, mark the patch bytes
as executable, and receive a pointer to the patch bytes.