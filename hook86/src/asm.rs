use core::ffi::c_void;

use thiserror::Error;

/// The opcode of the nop instruction
pub const NOP: u8 = 0x90;

#[derive(Error, Debug)]
pub enum UnexpectedOpcodeError {
    #[error("Unexpected opcode {opcode:02X} at {ptr:p}")]
    SingleByteOpcode { ptr: *const c_void, opcode: u8 },
    #[error("Unexpected opcode {opcode1:02X} {opcode2:02X} at {ptr:p}")]
    DoubleByteOpcode { ptr: *const c_void, opcode1: u8, opcode2: u8 },
}

/// Get an absolute address from an instruction containing a 32-bit relative offset
///
/// # Arguments
///
/// * `ptr` - A pointer to the start of the instruction (NOT the relative offset within the instruction)
/// * `N` - Size of the instruction in bytes
pub const unsafe fn get_absolute_from_rel32<const N: isize>(ptr: *const c_void) -> *const c_void {
    unsafe {
        let original_jump_offset = std::ptr::read_unaligned(ptr.offset(N - 4) as *const isize) + N;
        ptr.offset(original_jump_offset)
    }
}

/// Get an absolute address from an instruction containing an 8-bit relative offset
///
/// # Arguments
///
/// * `ptr` - A pointer to the start of the instruction (NOT the relative offset within the instruction)
pub const unsafe fn get_absolute_from_rel8(ptr: *const c_void) -> *const c_void {
    unsafe {
        let original_jump_offset = std::ptr::read_unaligned((ptr as *const i8).offset(1)) as isize;
        ptr.offset(original_jump_offset)
    }
}

/// Get the absolute address of the destination of a branch instruction
///
/// "Branch instructions" include both conditional and unconditional jumps, as well as calls. Only
/// branch instructions with immediate operands are supported, not register or memory operands.
///
/// # Arguments
///
/// * `ptr` - A pointer to the start of the branch instruction
///
/// # Errors
///
/// An UnexpectedOpcodeError is returned if the opcode at the provided location does not correspond
/// to a supported branch instruction.
pub unsafe fn get_branch_target(ptr: *const c_void) -> Result<*const c_void, UnexpectedOpcodeError> {
    let byte_ptr = ptr as *const u8;
    unsafe {
        let opcode = *byte_ptr;
        Ok(match opcode {
            // call and jump
            0xE8 | 0xE9 => get_absolute_from_rel32::<5>(ptr),
            // far call and far jump use absolute addresses
            0x9A | 0xEA => std::ptr::read_unaligned(byte_ptr.offset(1) as *const *const c_void),
            // short jumps
            0xEB | 0x77 | 0x73 | 0x72 | 0x76 | 0xE3 | 0x74 | 0x7F | 0x7D | 0x7C | 0x71 | 0x7B | 0x79 | 0x75 | 0x70 | 0x7A | 0x78 => get_absolute_from_rel8(ptr),
            // conditional jumps
            0x0F => {
                let sub_opcode = *byte_ptr.offset(1);
                match sub_opcode {
                    0x84 | 0x8C | 0x8D | 0x87 | 0x83 | 0x82 | 0x86 | 0x8F | 0x8E | 0x85 | 0x8B | 0x81 | 0x89 | 0x80 | 0x8A | 0x88 => get_absolute_from_rel32::<6>(ptr),
                    _ => return Err(UnexpectedOpcodeError::DoubleByteOpcode { ptr, opcode1: opcode, opcode2: sub_opcode }),
                }
            }
            _ => return Err(UnexpectedOpcodeError::SingleByteOpcode { ptr, opcode }),
        })
    }
}

/// Get the relative offset between two addresses as a byte array
const fn addr_offset<const N: usize>(
    from: usize,
    to: usize,
) -> [u8; size_of::<usize>()] {
    to.overflowing_sub(from + N).0.to_le_bytes()
}

/// Get the bytes of a call instruction from one address to another
pub const fn call(from: usize, to: usize) -> [u8; 5] {
    let bytes = addr_offset::<5>(from, to);
    [0xE8, bytes[0], bytes[1], bytes[2], bytes[3]]
}

/// Get the bytes of an unconditional jump instruction from one address to another
///
/// The returned instruction always uses a 32-bit offset even if the displacement could fit in an
/// 8-bit offset.
pub const fn jmp(from: usize, to: usize) -> [u8; 5] {
    let bytes = addr_offset::<5>(from, to);
    [0xE9, bytes[0], bytes[1], bytes[2], bytes[3]]
}

const fn cond_jmp(from: usize, to: usize, cond: u8) -> [u8; 6] {
    let bytes = addr_offset::<6>(from, to);
    [0x0F, cond, bytes[0], bytes[1], bytes[2], bytes[3]]
}

/// Get the bytes of a jz instruction from one address to another
///
/// The returned instruction always uses a 32-bit offset even if the displacement could fit in an
/// 8-bit offset.
pub const fn jz(from: usize, to: usize) -> [u8; 6] {
    cond_jmp(from, to, 0x84)
}

/// Get the bytes of a jl instruction from one address to another
///
/// The returned instruction always uses a 32-bit offset even if the displacement could fit in an
/// 8-bit offset.
pub const fn jl(from: usize, to: usize) -> [u8; 6] {
    cond_jmp(from, to, 0x8C)
}

/// Get the bytes of a jge instruction from one address to another
///
/// The returned instruction always uses a 32-bit offset even if the displacement could fit in an
/// 8-bit offset.
pub const fn jge(from: usize, to: usize) -> [u8; 6] {
    cond_jmp(from, to, 0x8D)
}

/// Get the bytes of a push instruction that pushes the provided immediate value onto the stack
pub const fn push(imm: usize) -> [u8; 5] {
    let bytes = imm.to_le_bytes();
    [0x68, bytes[0], bytes[1], bytes[2], bytes[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calc_addr_offset() {
        assert_eq!(addr_offset::<3>(0x80000000, 0x80000010), 13isize.to_le_bytes());
        assert_eq!(
            addr_offset::<4>(0x80000000, 0x7FFFFF10),
            (-244isize).to_le_bytes()
        );
    }

    #[test]
    fn call_bytes() {
        assert_eq!(call(0x80000000, 0x80000010), [0xE8, 11, 0, 0, 0]);
    }

    #[test]
    fn jmp_bytes() {
        assert_eq!(jmp(0x80000000, 0x80000010), [0xE9, 11, 0, 0, 0]);
    }

    #[test]
    fn jl_bytes() {
        assert_eq!(jl(0x80000000, 0x800000F0), [0x0F, 0x8C, 0xEA, 0, 0, 0]);
    }

    #[test]
    fn jge_bytes() {
        assert_eq!(jge(0x80000000, 0x800000E0), [0x0F, 0x8D, 0xDA, 0, 0, 0]);
    }
}