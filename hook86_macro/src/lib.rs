extern crate proc_macro;
use proc_macro::TokenStream;

use quote::quote;
use syn::parse::{Parse, ParseStream, Result};
use syn::{bracketed, parse_macro_input, Error, Ident, LitInt, Token, Visibility};

macro_rules! byte {
    ($buf:expr, $byte:expr) => {
        $buf.push($byte);
        continue;
    }
}

#[derive(Debug)]
enum PatchComponent {
    Bytes(Vec<u8>),
    Rel32(Vec<u8>, Ident),
    Imm32(Ident),
}

impl PatchComponent {
    fn size(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::Rel32(opcode, _) => opcode.len() + 4,
            Self::Imm32(_) => 4,
        }
    }

    fn buf_tokens(&self) -> proc_macro2::TokenStream {
        match self {
            Self::Bytes(bytes) => quote! { #(#bytes,)* },
            Self::Rel32(opcode, _) => quote! { #(#opcode,)* 0, 0, 0, 0, },
            Self::Imm32(_) => quote! { 0, 0, 0, 0, },
        }
    }
}

struct Patch {
    visibility: Visibility,
    name: Ident,
    components: Vec<PatchComponent>,
}

impl Parse for Patch {
    fn parse(input: ParseStream) -> Result<Self> {
        let visibility: Visibility = input.parse()?;

        let name: Ident = input.parse()?;

        input.parse::<Token![=]>()?;

        let content;
        bracketed!(content in input);

        let mut components = vec![];
        let mut current_buf = vec![];

        while !content.is_empty() {
            if content.peek(LitInt) {
                let byte: LitInt = content.parse()?;
                current_buf.push(byte.base10_parse::<u8>()?);
            } else {
                let instruction: Ident = content.parse()?;
                let inst_string = instruction.to_string();
                match inst_string.as_str() {
                    "pushad" => {
                        byte!(current_buf, 0x60);
                    }
                    "popad" => {
                        byte!(current_buf, 0x61);
                    }
                    "ret" | "retn" => {
                        byte!(current_buf, 0xC3);
                    }
                    _ => (),
                }

                let target: Ident = content.parse()?;
                let component = match inst_string.as_str() {
                    "imm32" => PatchComponent::Imm32(target),
                    "rel32" => PatchComponent::Rel32(vec![], target),
                    "call" => PatchComponent::Rel32(vec![0xE8], target),
                    "jmp" => PatchComponent::Rel32(vec![0xE9], target),
                    "jz" | "je" => PatchComponent::Rel32(vec![0x0F, 0x84], target),
                    "jl" | "jnge" => PatchComponent::Rel32(vec![0x0F, 0x8C], target),
                    "jge" | "jnl" => PatchComponent::Rel32(vec![0x0F, 0x8D], target),
                    "ja" | "jnbe" => PatchComponent::Rel32(vec![0x0F, 0x87], target),
                    "jae" | "jnb" | "jnc" => PatchComponent::Rel32(vec![0x0F, 0x83], target),
                    "jb" | "jc" | "jnae" => PatchComponent::Rel32(vec![0x0F, 0x82], target),
                    "jbe" | "jna" => PatchComponent::Rel32(vec![0x0F, 0x86], target),
                    "jg" | "jnle" => PatchComponent::Rel32(vec![0x0F, 0x8F], target),
                    "jle" | "jng" => PatchComponent::Rel32(vec![0x0F, 0x8E], target),
                    "jne" | "jnz" => PatchComponent::Rel32(vec![0x0F, 0x85], target),
                    "jno" => PatchComponent::Rel32(vec![0x0F, 0x81], target),
                    "jnp" | "jpo" => PatchComponent::Rel32(vec![0x0F, 0x8B], target),
                    "jns" => PatchComponent::Rel32(vec![0x0F, 0x89], target),
                    "jo" => PatchComponent::Rel32(vec![0x0F, 0x80], target),
                    "jp" | "jpe" => PatchComponent::Rel32(vec![0x0F, 0x8A], target),
                    "js" => PatchComponent::Rel32(vec![0x0F, 0x88], target),
                    "push" => {
                        current_buf.push(0x68);
                        PatchComponent::Imm32(target)
                    }
                    _ => return Err(Error::new(instruction.span(), "Invalid or unsupported instruction")),
                };

                if !current_buf.is_empty() {
                    components.push(PatchComponent::Bytes(current_buf));
                    current_buf = vec![];
                }
                components.push(component);
            }

            // optionally allow commas between values
            if content.peek(Token![,]) {
                content.parse::<Token![,]>()?;
            }
        }

        if !current_buf.is_empty() {
            components.push(PatchComponent::Bytes(current_buf));
        }

        input.parse::<Token![;]>()?;

        Ok(Self {
            visibility,
            name,
            components,
        })
    }
}

/// A macro for managing assembly patches with placeholder values that must be filled in at runtime
///
/// The patch structure is shown by the following example:
/// ```no_run
/// patch! {
///     pub ExamplePatch = [
///         0x68 imm32 push_value // push push_value
///         0x29 0xD8 // sub eax, ebx
///         0x38 0xF4 0x04 // cmp eax, 4
///         jz equal_target
///         jmp else_target
///     ];
/// }
/// ```
///
/// `ExamplePatch` is the name of a new type that will be defined. The patch body, in brackets, is a
/// series of integers, keywords, and placeholders making up the bytes of the patch. Integer values
/// will be included in the patch bytes directly (values must be u8's; delimiting commas optional).
/// Placeholders consist of a keyword followed by a unique name identifying the placeholder. The
/// keyword can be `imm32`, indicating a 32-bit placeholder that will be filled in at runtime with
/// the exact provided value; `rel32`, indicating a 32-bit placeholder that will be filled in at
/// runtime with the relative offset from the end of the placeholder to the provided address; or one
/// of a supported set of assembly instruction names (including all branch instructions), which will
/// automatically fill in the appropriate opcode bytes and a placeholder of the appropriate type.
/// Placeholder bytes are initialized to zero. Integers and placeholders can be interspersed freely.
///
/// Once an instance of a patch type has been created with the `new` method and you've identified
/// the runtime values for the placeholders, you can call the instance's `bind` method, which takes
/// one argument per placeholder in the order the placeholders were defined. `bind` will fill in
/// the placeholder bytes with the appropriate values, mark the patch bytes as executable, and
/// return a pointer to the patch bytes (make sure the patch instance is in static/pinned memory!).
#[proc_macro]
pub fn patch(input: TokenStream) -> TokenStream {
    let Patch {
        visibility,
        name,
        components,
    } = parse_macro_input!(input as Patch);

    let patch_size = components.iter().map(PatchComponent::size).sum::<usize>();
    let buf_pieces: Vec<_> = components.iter().map(PatchComponent::buf_tokens).collect();
    let field_names: Vec<_> = components
        .iter()
        .filter_map(|component| match component {
            PatchComponent::Bytes(_) => None,
            PatchComponent::Rel32(_, name) => Some(name),
            PatchComponent::Imm32(name) => Some(name),
        })
        .collect();

    let mut field_offsets = Vec::with_capacity(field_names.len());
    let mut offset = 0;
    for component in &components {
        match component {
            PatchComponent::Bytes(_) => (),
            PatchComponent::Rel32(opcode, _) => field_offsets.push(offset + opcode.len()),
            PatchComponent::Imm32(_) => field_offsets.push(offset),
        }

        offset += component.size();
    }
    let field_offsets = field_offsets.into_iter();

    let field_relativity = components.iter().filter_map(|f| match f {
        PatchComponent::Bytes(_) => None,
        PatchComponent::Rel32(_, _) => Some(true),
        PatchComponent::Imm32(_) => Some(false),
    });

    let expanded = quote! {
        #visibility struct #name {
            __buf: [u8; #patch_size],
            #(#field_names: hook86::patch::PatchPlaceholder),*
        }

        impl #name {
            pub const fn new() -> Self {
                Self {
                    __buf: [#(#buf_pieces)*],
                    #(#field_names: hook86::patch::PatchPlaceholder::new(#field_offsets, #field_relativity)),*
                }
            }

            pub const fn buf(&self) -> &[u8] {
                self.__buf.as_slice()
            }

            pub const fn buf_raw(&self) -> *const u8 {
                self.buf().as_ptr()
            }

            pub fn bind(&mut self, #(#field_names: hook86::mem::IntPtr,)*) -> windows::core::Result<*const u8> {
                #(self.#field_names.set_value(&mut self.__buf, #field_names);)*
                hook86::mem::unprotect(self.buf_raw() as *const std::ffi::c_void, #patch_size).map(|_| self.buf_raw())
            }
        }
    };

    TokenStream::from(expanded)
}