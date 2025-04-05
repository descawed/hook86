use crate::mem::{IntPtr, PTR_SIZE};

pub use hook86_macro::patch;

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

// commented out for now until I figure out how to have the macro refer to types in the crate::
// namespace here but the hook86:: namespace for external users
/*#[cfg(test)]
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

    #[test]
    fn test_patch_literals() {
        let mut test_patch = TestPatch::new();
        test_patch.bind(0x80000000, 0x80000080, 1234).unwrap();

        let buf = test_patch.buf();
        assert_eq!(buf[buf.len() - 5..], [0x68, 0xD2, 0x04, 0x00, 0x00]);
    }
}*/