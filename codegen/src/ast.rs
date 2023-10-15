use std::str::FromStr;

use thiserror::Error;

pub trait TokenExt {
    fn token(&self) -> Token;

    fn count(&self) -> usize;

    fn tokenize(code: &str) -> Tokens<Self>
    where
        Self: Sized;
}

impl TokenExt for Token {
    fn token(&self) -> Token {
        *self
    }

    fn count(&self) -> usize {
        1
    }

    fn tokenize(code: &str) -> Tokens<Self> {
        let mut tokens = Vec::new();

        for c in code.chars() {
            if let Some(token) = Token::from_char(c) {
                tokens.push(token);
            }
        }

        trace!("tokenizer found {} tokens", tokens.len());

        Tokens { tokens }
    }
}

impl TokenExt for Repeated {
    fn token(&self) -> Token {
        self.token
    }

    fn count(&self) -> usize {
        self.count
    }

    fn tokenize(code: &str) -> Tokens<Self> {
        let unoptimized = Token::tokenize(code);

        let mut tokens = Vec::new();

        let mut iter = unoptimized.tokens.into_iter().peekable();

        while let Some(token) = iter.next() {
            let mut count = 1;

            while let Some(next) = iter.peek() {
                if !matches!(token, Token::LoopStart | Token::LoopEnd | Token::Read)
                    && next == &token
                {
                    count += 1;
                    iter.next();
                } else {
                    break;
                }
            }

            tokens.push(Repeated { token, count });
        }

        trace!("tokenizer optimized to {} tokens", tokens.len());

        Tokens { tokens }
    }
}

macro_rules! tokens {
    ($(
        $(#[$attr:meta])*
        $token:ident = $c:literal
    ),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum Token {
            $(
                $(#[$attr])*
                $token
            ),*
        }

        impl Token {
            pub fn as_char(&self) -> char {
                match self {
                    $(Self::$token => $c),*
                }
            }

            pub fn from_char(c: char) -> Option<Self> {
                match c {
                    $($c => Some(Self::$token)),*,
                    _ => None
                }
            }
        }
    };
}

tokens! {
    /// Move the pointer to the right.
    PointerAdd = '>',
    /// Move the pointer to the left.
    PointerSub = '<',
    /// Increment the memory cell under the pointer.
    ValueAdd = '+',
    /// Decrement the memory cell under the pointer.
    ValueSub = '-',
    /// Input a character and store it in the cell at the pointer.
    Read = ',',
    /// Output the character signified by the cell at the pointer.
    Write = '.',
    /// Mark the beginning of a loop.
    LoopStart = '[',
    /// Skip if the cell under the pointer is 0, otherwise jump back to the matching `[`.
    LoopEnd = ']',
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens<T> {
    pub tokens: Vec<T>,
}

impl<T: TokenExt> Tokens<T> {
    pub fn new(tokens: Vec<T>) -> Self {
        Self { tokens }
    }
}

impl<T: TokenExt + Clone> Tokens<T> {
    pub fn segment(self) -> Vec<Segment<T>> {
        let (segments, _) = Self::segment_inner(&self.tokens[..]);
        segments
    }

    /// Takes a slice of repeated tokens and outputs the contained segments,
    /// as well as the number of tokens that were consumed.
    fn segment_inner(slice: &[T]) -> (Vec<Segment<T>>, usize) {
        let mut segments = Vec::new();

        let mut iter = slice.iter().peekable().enumerate();

        let mut consumed = 0usize;
        let mut code = Vec::new();

        while let Some((idx, token)) = iter.next() {
            match token.token() {
                Token::LoopStart => {
                    if !code.is_empty() {
                        segments.push(Segment::Executable(Tokens::new(code)));
                        code = Vec::new();
                    }

                    let (inner, count) = Self::segment_inner(&slice[idx + 1..]);
                    segments.push(Segment::Loop(inner));
                    iter.nth(count);
                    consumed += count + 2;
                }
                Token::LoopEnd => {
                    if !code.is_empty() {
                        segments.push(Segment::Executable(Tokens::new(code)));
                    }

                    return (segments, consumed);
                }
                _ => {
                    consumed += 1;
                    code.push(token.clone());
                }
            }
        }

        (segments, consumed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repeated {
    pub token: Token,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Segment<T> {
    Executable(Tokens<T>),
    Loop(Vec<Segment<T>>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File<T> {
    pub segments: Vec<Segment<T>>,
    pub needs_input: bool,
}

#[derive(Debug, Error)]
pub enum ParseFileError {}

impl<T: TokenExt + Clone> FromStr for File<T> {
    type Err = ParseFileError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tokens = T::tokenize(s);

        let needs_input = tokens.tokens.iter().any(|t| t.token() == Token::Read);

        let segments = tokens.segment();
        Ok(Self {
            segments,
            needs_input,
        })
    }
}
