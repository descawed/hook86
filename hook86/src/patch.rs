pub use hook86_core::PatchPlaceholder;
pub use hook86_macro::patch;

#[cfg(test)]
mod tests {
    use super::*;

    patch! {
        pub TestPatch = [
            0x29 0xD8
            0x38 0xF4 0x04
            jz equal_target
            jmp else_target
            0x68 imm32 push_value
        ];
    }

    #[test]
    fn test_patch_literals() {
        let mut test_patch = TestPatch::new();
        test_patch.bind(0x80000000, 0x80000080, 1234).unwrap();

        let buf = test_patch.buf();
        assert_eq!(buf[buf.len() - 5..], [0x68, 0xD2, 0x04, 0x00, 0x00]);
    }
}