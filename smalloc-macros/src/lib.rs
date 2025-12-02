use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro_attribute]
pub fn smalloc_main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let body = &input.block;

    let output = quote! {
        #[global_allocator]
        static SMALLOC: smalloc::Smalloc = smalloc::Smalloc::new();

        #[unsafe(no_mangle)]
        pub extern "C" fn main(_argc: i32, _argv: *const *const u8) -> i32 {
            unsafe { SMALLOC.init() };

            fn user_main() #body
            user_main();

            0
        }
    };

    output.into()
}
