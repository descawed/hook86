#![cfg(windows)]

extern crate self as hook86;

pub mod asm;
pub mod input;
pub mod mem;
pub mod patch;
pub mod crash;

pub use mem::{IntPtr, IntoAddress, PTR_SIZE};

pub use hook86_dll_main::dll_main;