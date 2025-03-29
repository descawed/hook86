use std::ffi::c_void;

use windows::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS};

// currently we only support 32-bit x86, but I'd like to keep the flexibility to support x64 in the
// future, so we'll use this type alias and maybe change it to a usize once we're ready to support
// both architectures.
pub type IntPtr = u32;
pub const PTR_SIZE: usize = size_of::<IntPtr>();

/// Make a memory region readable, writable, and executable
pub fn unprotect(ptr: *const c_void, size: usize) -> windows::core::Result<PAGE_PROTECTION_FLAGS> {
    let mut old_protect = PAGE_PROTECTION_FLAGS::default();
    unsafe { VirtualProtect(ptr, size, PAGE_EXECUTE_READWRITE, &mut old_protect) }?;

    Ok(old_protect)
}

/// Set the memory protection on a memory region
pub fn protect(ptr: *const c_void, size: usize, protection: PAGE_PROTECTION_FLAGS) -> windows::core::Result<()> {
    let mut old_protect = PAGE_PROTECTION_FLAGS::default();
    unsafe { VirtualProtect(ptr, size, protection, &mut old_protect) }
}

/// Write the given data to the specified address within a protected memory region
///
/// The region containing the address will be unprotected prior to the write. After writing, the
/// original protection will be restored.
pub unsafe fn patch(addr: *const c_void, data: &[u8]) -> windows::core::Result<()> {
    let old_protect = unprotect(addr, data.len())?;
    unsafe { std::slice::from_raw_parts_mut(addr as *mut u8, data.len()).copy_from_slice(data) };
    protect(addr, data.len(), old_protect)
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
    pub fn set_value(&mut self, buf: &mut [u8], value: IntPtr) {
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