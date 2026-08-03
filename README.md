# hook86

A Rust library of utilities for DLL-injection hacks on 32-bit x86.

I've written this primarily for my personal use, so I'm only briefly documenting it here. I could
try to clean it up a bit if anyone else has any interest in it.

## Overview

This library only supports 32-bit x86 at the moment. I expect I'll add x64 support at some point
when I have a project that requires it. I don't expect I'll ever support architectures other than
x86; that would probably require a different library. The focus is on games, so only Windows is
supported, as that's the platform that the overwhelming majority of games target. I have some
interest in adding Linux support, but it would require a big refactor, and I also don't plan to do
it until I have a project that requires it.

The library includes two proc macros, so the repo is a Cargo workspace with three crates:
- `hook86` - the main library
- `hook86_dll_main` - the `dll_main` proc macro
- `hook86_macro` - the `patch` proc macro

### asm

Functions for generating common branch instructions (e.g., call, jmp, jz, jle, etc.) from one
address to another. Also contains the `get_branch_target` function which will read a branch
instruction at the given address and return the absolute address that the branch targets.

### crash

Optional crash logging infrastructure for when the hacks are a little too hacky. Logs via the `log` crate.

### mem

Contains utilities for manipulating memory – removing protection (i.e., enabling read, write, and
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

Also contains the `Hook` type for installing hooks in the target process. `Hook` patches a particular address with the
given bytes, with convenience functions for creating branch instructions to a given destination address. When the `Hook`
is dropped, the original bytes are restored, unless the hook is made persistent with the `persist` or
`install_persistent` methods.

### dll_main

The `dll_main` macro can be applied to a function to generate the `DllMain` boilerplate. The macro can optionally take
arguments to subscribe your function to only certain call reasons:
- `process` - `DLL_PROCESS_ATTACH` and `DLL_PROCESS_DETACH`
- `thread` - `DLL_THREAD_ATTACH` and `DLL_THREAD_DETACH`
- `process_attach` - `DLL_PROCESS_ATTACH`
- `process_detach` - `DLL_PROCESS_DETACH`
- `thread_attach` - `DLL_THREAD_ATTACH`
- `thread_detach` - `DLL_THREAD_DETACH`

When `DllMain` is called for any reason you don't subscribe to, it will return `TRUE` and your function will not be
called. Passing no arguments to the macro is the same as subscribing to all call reasons.

Your function signature must be one of the following:
- If you subscribe to only a single call reason:
  - `fn()` 
  - `fn() -> Result<_>`
  - `fn(HINSTANCE)`
  - `fn(HINSTANCE) -> Result<_>`
- If you subscribe to multiple call reasons:
  - `fn(u32)` 
  - `fn(u32) -> Result<_>`
  - `fn(HINSTANCE, u32)`
  - `fn(HINSTANCE, u32) -> Result<_>`

The `u32` argument is the call reason (`fdwReason`) and the `HINSTANCE` argument is the module handle (`hinstDLL`).
The module handle type doesn't have to be `HINSTANCE` specifically; it can be any pointer-sized type.

If your function returns nothing, `DllMain` will always return `TRUE`. If it returns a `Result`, `DllMain` will return
`TRUE` if the `Result` is `Ok`, or log the error with the `log` crate and return `FALSE` if the `Result` is `Err` (so
your error type must implement `Display`).