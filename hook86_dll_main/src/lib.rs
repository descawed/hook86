extern crate proc_macro;

use std::collections::BTreeSet;

use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, parse_quote, punctuated::Punctuated, FnArg, Ident, ItemFn, ReturnType, Token, Type};

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_PROCESS_DETACH: u32 = 0;
const DLL_THREAD_ATTACH: u32 = 2;
const DLL_THREAD_DETACH: u32 = 3;

fn handle_type(func: &ItemFn, index: usize) -> Result<Type, syn::Error> {
    match func.sig.inputs.get(index) {
        Some(FnArg::Typed(pat)) => Ok(*pat.ty.clone()),
        Some(arg) => Err(syn::Error::new_spanned(arg, "expected a typed argument")),
        _ => panic!("dll_main macro bug: handle_type called with an invalid index"),
    }
}

fn compile_error(object: &impl ToTokens, msg: &str) -> TokenStream {
    TokenStream::from(syn::Error::new_spanned(object, msg).into_compile_error())
}

#[proc_macro_attribute]
pub fn dll_main(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args with Punctuated<Ident, Token![,]>::parse_terminated);
    let input = parse_macro_input!(input as ItemFn);

    let (reason_filter, needs_reason) = if args.is_empty() {
        (quote!(), true)
    } else {
        let mut accepted_reasons = BTreeSet::new();
        for arg in args {
            match arg.to_string().as_str() {
                "process" => {
                    accepted_reasons.insert(DLL_PROCESS_ATTACH);
                    accepted_reasons.insert(DLL_PROCESS_DETACH);
                }
                "process_attach" => { accepted_reasons.insert(DLL_PROCESS_ATTACH); }
                "process_detach" => { accepted_reasons.insert(DLL_PROCESS_DETACH); }
                "thread" => {
                    accepted_reasons.insert(DLL_THREAD_ATTACH);
                    accepted_reasons.insert(DLL_THREAD_DETACH);
                }
                "thread_attach" => { accepted_reasons.insert(DLL_THREAD_ATTACH); }
                "thread_detach" => { accepted_reasons.insert(DLL_THREAD_DETACH); }
                _ => return compile_error(&arg, "Invalid call reason"),
            }
        }

        (
            quote! {
                if !matches!(reason, #(#accepted_reasons)|*) {
                    return 1;
                }
            },
            accepted_reasons.len() > 1,
        )
    };

    let name = &input.sig.ident;
    let inputs = &input.sig.inputs;

    if inputs.is_empty() && needs_reason {
        return compile_error(&inputs, "A dll_main function that accepts more than one call reason must take a reason argument");
    }

    if inputs.len() > 2 {
        return compile_error(&inputs, "A dll_main function must accept at most two arguments, the call reason and the DLL handle");
    }

    let mut instance_type: Type = parse_quote!(*mut ::std::ffi::c_void);
    let call = match inputs.len() {
        0 => quote!(#name()),
        1 => {
            // if there's only one argument, then it's the reason if the function accepts multiple
            // reasons, otherwise it's the handle
            if needs_reason {
                quote!(#name(reason))
            } else {
                instance_type = match handle_type(&input, 0) {
                    Ok(ty) => ty,
                    Err(e) => return TokenStream::from(e.into_compile_error()),
                };
                quote!(#name(instance))
            }
        }
        2 => {
            instance_type = match handle_type(&input, 0) {
                Ok(ty) => ty,
                Err(e) => return TokenStream::from(e.into_compile_error()),
            };
            quote!(#name(instance, reason))
        }
        _ => unreachable!(),
    };

    let call_and_return = if matches!(input.sig.output, ReturnType::Default) {
        quote! {
            #call;
            1
        }
    } else {
        quote! {
            match #call {
                Ok(_) => 1,
                Err(e) => {
                    ::log::error!("Fatal error: {e}");
                    ::log::logger().flush();
                    0
                }
            }
        }
    };

    quote! {
        #input // original function

        // ensure the instance type is the same size as a pointer
        const _: () = assert!(size_of::<#instance_type>() == size_of::<*mut ::std::ffi::c_void>());

        #[unsafe(no_mangle)]
        #[allow(non_snake_case, unused_variables)]
        pub extern "system" fn DllMain(instance: #instance_type, reason: ::std::primitive::u32, _reserved: *const ::std::ffi::c_void) -> ::std::primitive::i32 {
            #reason_filter

            #call_and_return
        }
    }.into()
}