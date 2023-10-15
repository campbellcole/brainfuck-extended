use ascii::AsciiString;
use color_eyre::eyre::Result;
use proc_macro2::TokenStream;
use quote::quote;
use syn::LitByte;
use typed_builder::TypedBuilder;

use crate::ast::{File, Segment, Token, TokenExt, Tokens};

#[derive(Default, Debug, Clone, Copy)]
pub enum CellSize {
    #[default]
    U8,
    U16,
    U32,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerSafety {
    Wrap,
    Clamp,
    #[default]
    None,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    Wrap,
    Abort,
    #[default]
    None,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EofBehavior {
    #[default]
    NoChange,
    Fixed(u8),
}

#[derive(Debug, TypedBuilder)]
pub struct BrainfuckToRust {
    pub memory_size: usize,
    #[builder(default)]
    pub pointer_safety: PointerSafety,
    #[builder(default)]
    pub overflow_behavior: OverflowBehavior,
    #[builder(default)]
    pub cell_size: CellSize,
    #[builder(default)]
    pub fixed_input: Option<AsciiString>,
    #[builder(default)]
    pub eof_behavior: EofBehavior,
}

impl BrainfuckToRust {
    pub fn generate<T: TokenExt>(&self, file: File<T>) -> Result<TokenStream> {
        let body = self.generate_body(&file.segments);

        let full = self.template(body, file.needs_input);

        Ok(full)
    }

    fn generate_body<T: TokenExt>(&self, segments: &Vec<Segment<T>>) -> TokenStream {
        let mut blocks = Vec::new();

        for segment in segments {
            match segment {
                Segment::Executable(code) => {
                    let segments = self.generate_statements(code);
                    blocks.push(quote! {
                        #(#segments)*
                    });
                }
                Segment::Loop(segments) => {
                    let body = self.generate_body(segments);

                    blocks.push(quote! {
                        while tape[pointer] != 0 {
                            #body
                        }
                    });
                }
            }
        }

        quote! {
            #(#blocks)*
        }
    }

    fn generate_statements<T: TokenExt>(&self, tokens: &Tokens<T>) -> Vec<TokenStream> {
        let mut statements = Vec::new();

        for token in &tokens.tokens {
            let count_u8 = token.count() as u8;
            let count_usize = token.count();

            let stmt = match token.token() {
                Token::PointerAdd => match self.pointer_safety {
                    PointerSafety::Wrap => {
                        quote! {
                            pointer = (pointer + #count_usize) % MEM_SIZE;
                        }
                    }
                    PointerSafety::Clamp => {
                        quote! {
                            pointer = (pointer + #count_usize).min(MEM_SIZE - 1);
                        }
                    }
                    PointerSafety::None => {
                        quote! {
                            pointer += #count_usize;
                        }
                    }
                },
                Token::PointerSub => match self.pointer_safety {
                    PointerSafety::Wrap => {
                        quote! {
                            pointer = if pointer < #count_usize {
                                MEM_SIZE - (#count_usize - pointer)
                            } else {
                                pointer - #count_usize
                            };
                        }
                    }
                    PointerSafety::Clamp => {
                        quote! {
                            pointer = pointer.max(#count_usize) - #count_usize;
                        }
                    }
                    PointerSafety::None => {
                        quote! {
                            pointer -= #count_usize;
                        }
                    }
                },
                Token::ValueAdd => match self.overflow_behavior {
                    OverflowBehavior::None => {
                        quote! {
                            tape[pointer] += #count_u8;
                        }
                    }
                    OverflowBehavior::Wrap => {
                        quote! {
                            tape[pointer] = tape[pointer].wrapping_add(#count_u8);
                        }
                    }
                    OverflowBehavior::Abort => {
                        quote! {
                            tape[pointer] = tape[pointer].checked_add(#count_u8).unwrap();
                        }
                    }
                },
                Token::ValueSub => match self.overflow_behavior {
                    OverflowBehavior::None => {
                        quote! {
                            tape[pointer] -= #count_u8;
                        }
                    }
                    OverflowBehavior::Wrap => {
                        quote! {
                            tape[pointer] = tape[pointer].wrapping_sub(#count_u8);
                        }
                    }
                    OverflowBehavior::Abort => {
                        quote! {
                            tape[pointer] = tape[pointer].checked_sub(#count_u8).unwrap();
                        }
                    }
                },
                Token::Read => {
                    if count_usize > 1 {
                        unimplemented!("sequential reads not implemented due to lack of utility")
                    }
                    match self.eof_behavior {
                        EofBehavior::NoChange => {
                            quote! {
                                if let Some(_c) = input.get(input_pos) {
                                    tape[pointer] = _c.as_byte();
                                    input_pos += #count_usize;
                                }
                            }
                        }
                        EofBehavior::Fixed(ch) => {
                            let lit = LitByte::new(ch, proc_macro2::Span::call_site());
                            quote! {
                                if let Some(_c) = input.get(input_pos) {
                                    tape[pointer] = _c.as_byte();
                                    input_pos += #count_usize;
                                } else {
                                    tape[pointer] = #lit;
                                }
                            }
                        }
                    }
                }
                Token::Write => {
                    quote! {
                        let __c = tape[pointer].to_ascii_char().unwrap().as_char();
                        for _ in 0..#count_usize {
                            print!("{}", __c);
                        }
                    }
                }
                _ => unreachable!("loop characters are not included in the tokenized code"),
            };

            statements.push(stmt);
        }

        statements
    }

    fn template(&self, body: TokenStream, needs_input: bool) -> TokenStream {
        let mem_size = self.memory_size;
        let cell_type = match self.cell_size {
            CellSize::U8 => quote! { u8 },
            CellSize::U16 => quote! { u16 },
            CellSize::U32 => quote! { u32 },
        };

        let input_def = if let Some(fixed_input) = &self.fixed_input {
            let fixed = fixed_input.as_str();
            quote! {
                let input = {
                    use ascii::AsAsciiStr;

                    let input = #fixed;
                    let input_ascii = input.as_ascii_str().expect("input is not ASCII");
                    input_ascii.chars().collect::<Vec<_>>()
                };

                let mut input_pos = 0usize;
            }
        } else if needs_input {
            quote! {
                use ascii::AsciiChar;

                let input = {
                    use std::io::Read;
                    use ascii::AsAsciiStr;

                    let mut stdin = std::io::stdin();
                    let mut input = String::new();

                    stdin.read_to_string(&mut input).expect("failed to read stdin");
                    let input_ascii = input.as_ascii_str().expect("input is not ASCII");
                    input_ascii.chars().collect::<Vec<_>>()
                };

                let mut input_pos = 0usize;
            }
        } else {
            quote! {}
        };

        quote! {
            use ascii::ToAsciiChar;

            fn main() {
                const MEM_SIZE: usize = #mem_size;

                let mut pointer = 0usize;
                let mut tape: [#cell_type; MEM_SIZE] = [0; MEM_SIZE];

                #input_def

                #body
            }
        }
    }
}
