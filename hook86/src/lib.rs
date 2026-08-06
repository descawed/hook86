#![cfg(windows)]

extern crate self as hook86;

pub mod asm;
pub mod crash;
pub mod dll;
pub mod input;
pub mod mem;
pub mod patch;

pub use mem::{IntPtr, IntoAddress, PTR_SIZE};

// used by the dll_main macro to ensure log is available
pub use log;