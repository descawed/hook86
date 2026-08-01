use crate::{IntPtr, IntoAddress, PTR_SIZE};
use crate::asm::{call, jmp, jz, jge, jl, NOP};
use crate::mem;

pub use hook86_macro::patch;

#[derive(Debug)]
pub struct Hook {
    address: *mut u8,
    patch_bytes: Vec<u8>,
    expected_bytes: Vec<u8>,
    original_bytes: Vec<u8>,
}

impl Hook {
    // builder methods
    /// Create a new hook that will patch the provided bytes at the specified address
    pub fn new<T>(address: *mut T, patch_bytes: &[u8]) -> Self {
        Self {
            address: address as *mut u8,
            patch_bytes: patch_bytes.to_vec(),
            expected_bytes: Vec::new(),
            original_bytes: Vec::new(),
        }
    }

    /// Create a new hook that will patch a call to `target` at the specified address
    pub fn call<T>(address: *mut T, target: impl IntoAddress) -> Self {
        Self::new(address, &call(address.into_address(), target.into_address()))
    }

    /// Create a new hook that will patch a jump to `target` at the specified address
    pub fn jmp<T>(address: *mut T, target: impl IntoAddress) -> Self {
        Self::new(address, &jmp(address.into_address(), target.into_address()))
    }

    /// Create a new hook that will patch a jump-if-zero/equal to `target` at the specified address
    pub fn jz<T>(address: *mut T, target: impl IntoAddress) -> Self {
        Self::new(address, &jz(address.into_address(), target.into_address()))
    }

    /// Create a new hook that will patch a jump-if-greater-than-or-equal to `target` at the specified address
    pub fn jge<T>(address: *mut T, target: impl IntoAddress) -> Self {
        Self::new(address, &jge(address.into_address(), target.into_address()))
    }

    /// Create a new hook that will patch a jump-if-less-than to `target` at the specified address
    pub fn jl<T>(address: *mut T, target: impl IntoAddress) -> Self {
        Self::new(address, &jl(address.into_address(), target.into_address()))
    }

    /// Pad the patch with `count` nop instructions
    pub fn pad(mut self, count: usize) -> Self {
        self.patch_bytes.extend(std::iter::repeat_n(NOP, count));
        self
    }

    /// Pad the patch to the given `size` in bytes with nop instructions
    ///
    /// If the patch is already at least `size` bytes, the patch is not changed.
    pub fn pad_to(mut self, size: usize) -> Self {
        if self.patch_bytes.len() >= size {
            return self;
        }

        self.patch_bytes.resize(size, NOP);
        self
    }

    /// Expect the given bytes to be present at the hook address. If the bytes at the hook address
    /// do not match the expected bytes, hook installation will fail.
    pub fn expect_bytes(mut self, expected_bytes: &[u8]) -> Self {
        self.expected_bytes = expected_bytes.to_vec();
        self
    }

    /// Install the hook persistently, consuming the `Hook` instance in the process
    pub unsafe fn install_persistent(mut self) -> Result<(), mem::MemoryError> {
        unsafe { self.install() }?;
        self.persist();
        Ok(())
    }

    // instance methods
    /// Has the hook been installed?
    pub const fn is_installed(&self) -> bool {
        !self.original_bytes.is_empty()
    }

    /// Install the hook, patching the patch bytes into memory at the hook address
    pub unsafe fn install(&mut self) -> Result<(), mem::MemoryError> {
        if self.is_installed() {
            return Ok(());
        }

        if !self.expected_bytes.is_empty() {
            unsafe { mem::assert_bytes(self.address as *const u8, &self.expected_bytes) }?;
        }

        let hook_slice = unsafe { std::slice::from_raw_parts(self.address as *const u8, self.patch_bytes.len()) };
        let original_bytes = hook_slice.to_vec();

        unsafe { mem::patch(self.address, &self.patch_bytes) }?;

        self.original_bytes = original_bytes;
        Ok(())
    }

    /// Uninstall the hook, restoring the original bytes into memory at the hook address
    pub unsafe fn uninstall(&mut self) -> Result<(), mem::MemoryError> {
        if !self.is_installed() {
            return Ok(());
        }

        unsafe { mem::patch(self.address, &self.original_bytes) }?;
        self.original_bytes.clear();
        Ok(())
    }

    /// Consume this `Hook` without uninstalling it so that the patch will persist beyond the
    /// lifetime of the `Hook` instance.
    ///
    /// The hook should be installed before calling this method. Persisting a hook that has not
    /// been installed does nothing.
    pub fn persist(mut self) {
        // clear the original bytes so that the hook is not uninstalled when it's dropped
        self.original_bytes.clear();
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        unsafe { self.uninstall() }.unwrap();
    }
}

#[derive(Debug)]
pub struct PatchPlaceholder {
    offset: usize,
    is_relative: bool,
    value: Option<IntPtr>,
}

impl PatchPlaceholder {
    pub const fn new(offset: usize, is_relative: bool) -> Self {
        Self {
            offset,
            is_relative,
            value: None,
        }
    }

    /// Set the value of the placeholder and patch it into the buffer at the appropriate location
    ///
    /// If `value` is a memory address, it should be an absolute address, even if the placeholder is
    /// relative.
    pub fn set_value(&mut self, buf: &mut [u8], value: impl IntoAddress) {
        let value = value.into_address();
        self.value = Some(value);

        let value_bytes = if self.is_relative {
            let buf_addr = buf.as_mut_ptr() as usize;
            let from_addr = buf_addr + self.offset + PTR_SIZE;
            let rel = value.overflowing_sub(from_addr as IntPtr).0;
            rel.to_le_bytes()
        } else {
            value.to_le_bytes()
        };

        buf[self.offset..self.offset + PTR_SIZE].copy_from_slice(&value_bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    patch! {
        pub TestPatch = [
            0x29 0xD8
            0x38 0xF4 0x04
            jz equal_target
            jmp else_target
            push push_value
        ];
    }

    patch! {
        pub CallPatch = [
            call call_target
        ];
    }

    #[test]
    fn test_patch_literals() {
        let mut test_patch = TestPatch::new();
        unsafe { test_patch.bind(0x80000000u32, 0x80000080u32, 1234u32) }.unwrap();

        let buf = test_patch.buf();
        assert_eq!(buf[buf.len() - 5..], [0x68, 0xD2, 0x04, 0x00, 0x00]);
    }

    fn call_target(_: i32) {
        // do nothing
    }

    #[test]
    fn test_patch_call() {
        let mut call_patch = CallPatch::new();
        unsafe { call_patch.bind(call_target as *const ()) }.unwrap();
    }

    #[test]
    fn test_hook() {
        let mut buf = [1u8, 2u8, 3u8, 4u8];
        {
            let mut hook = Hook::new(&raw mut buf, &[5, 6, 7, 8]);
            unsafe { hook.install() }.unwrap();
            assert_eq!(buf, [5, 6, 7, 8]);
        }
        // original values should be restored after drop
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    #[test]
    fn test_hook_expect() {
        let mut buf = [1u8, 2u8, 3u8, 4u8];
        let mut hook = Hook::new(&raw mut buf, &[5, 6, 7, 8]).expect_bytes(&[1, 2, 3, 5]);
        let result = unsafe { hook.install() };
        assert!(result.is_err());
    }

    #[test]
    fn test_hook_pad() {
        let mut buf = [1u8, 2u8, 3u8, 4u8];
        let mut hook = Hook::new(&raw mut buf, &[0]).pad_to(4);
        unsafe { hook.install() }.unwrap();
        assert_eq!(buf, [0, NOP, NOP, NOP]);
    }

    #[test]
    fn test_hook_persist() {
        let mut buf = [1u8, 2u8, 3u8, 4u8];
        {
            let mut hook = Hook::new(&raw mut buf, &[5, 6, 7, 8]);
            unsafe { hook.install() }.unwrap();
            assert_eq!(buf, [5, 6, 7, 8]);
            hook.persist();
        }
        // original values should NOT be restored after drop
        assert_eq!(buf, [5, 6, 7, 8]);
    }
}