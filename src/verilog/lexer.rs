#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Module,
    Endmodule,
    Input,
    Output,
    Reg,
    Wire,
    Always,
    Posedge,
    If,
    Else,
    Case,
    Endcase,
    Default,
    Assign,
    Begin,
    End,
    Or,

    // Literals
    Number(u64, Option<u32>), // value, optional width

    // Identifier
    Ident(String),

    // Operators
    Plus,
    Minus,
    Star,
    EqEq,
    Neq,
    Gte,
    Lte,
    Gt,
    Lt,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Bang,
    AmpAmp,
    PipePipe,
    Eq,
    Question,
    Colon,

    // Delimiters
    LParen,
    RParen,
    LBrack,
    RBrack,
    LBrace,
    RBrace,
    Semi,
    Comma,
    Dot,
    At,
    Hash,

    Eof,
}

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    pub line: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if let Some(c) = ch {
            if c == '\n' {
                self.line += 1;
            }
            self.pos += 1;
        }
        ch
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.input.get(self.pos + offset).copied()
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while let Some(c) = self.peek() {
                if c.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }

            // Skip // comment
            if self.peek() == Some('/') && self.peek_at(1) == Some('/') {
                while let Some(c) = self.advance() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }

            // Skip /* */ comment
            if self.peek() == Some('/') && self.peek_at(1) == Some('*') {
                self.advance();
                self.advance();
                loop {
                    match self.advance() {
                        Some('*') if self.peek() == Some('/') => {
                            self.advance();
                            break;
                        }
                        None => break,
                        _ => {}
                    }
                }
                continue;
            }

            // Skip `timescale and other compiler directives
            if self.peek() == Some('`') {
                while let Some(c) = self.advance() {
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }

            break;
        }
    }

    fn read_ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        s
    }

    fn read_number_digits(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_hexdigit() || c == '_' || c == 'x' || c == 'X' || c == 'z' || c == 'Z'
            {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        s
    }

    fn parse_number(&mut self, first_char: char) -> Token {
        // Read decimal part first
        let mut dec_str = String::new();
        dec_str.push(first_char);
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                dec_str.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // Check for width'base format
        if self.peek() == Some('\'') {
            let width: u32 = dec_str.replace('_', "").parse().unwrap_or(32);
            self.advance(); // consume '
            let base = self.advance().unwrap_or('d');
            let digits = self.read_number_digits();
            let clean = digits.replace('_', "").replace(['x', 'X', 'z', 'Z'], "0");
            let value = match base {
                'b' | 'B' => u64::from_str_radix(&clean, 2).unwrap_or(0),
                'o' | 'O' => u64::from_str_radix(&clean, 8).unwrap_or(0),
                'd' | 'D' => clean.parse::<u64>().unwrap_or(0),
                'h' | 'H' => u64::from_str_radix(&clean, 16).unwrap_or(0),
                _ => 0,
            };
            Token::Number(value, Some(width))
        } else {
            let value: u64 = dec_str.replace('_', "").parse().unwrap_or(0);
            Token::Number(value, None)
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();

        let ch = match self.advance() {
            Some(c) => c,
            None => return Token::Eof,
        };

        match ch {
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '^' => Token::Caret,
            '~' => Token::Tilde,
            '?' => Token::Question,
            ':' => Token::Colon,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '[' => Token::LBrack,
            ']' => Token::RBrack,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            ';' => Token::Semi,
            ',' => Token::Comma,
            '.' => Token::Dot,
            '@' => Token::At,
            '#' => Token::Hash,

            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Token::EqEq
                } else {
                    Token::Eq
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Token::Neq
                } else {
                    Token::Bang
                }
            }
            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Token::Lte
                } else {
                    Token::Lt
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Token::Gte
                } else {
                    Token::Gt
                }
            }
            '&' => {
                if self.peek() == Some('&') {
                    self.advance();
                    Token::AmpAmp
                } else {
                    Token::Amp
                }
            }
            '|' => {
                if self.peek() == Some('|') {
                    self.advance();
                    Token::PipePipe
                } else {
                    Token::Pipe
                }
            }

            c if c.is_ascii_digit() => self.parse_number(c),

            c if c.is_alphabetic() || c == '_' => {
                let mut ident = String::new();
                ident.push(c);
                ident.push_str(&self.read_ident());
                match ident.as_str() {
                    "module" => Token::Module,
                    "endmodule" => Token::Endmodule,
                    "input" => Token::Input,
                    "output" => Token::Output,
                    "reg" => Token::Reg,
                    "wire" => Token::Wire,
                    "always" => Token::Always,
                    "posedge" => Token::Posedge,
                    "if" => Token::If,
                    "else" => Token::Else,
                    "case" => Token::Case,
                    "endcase" => Token::Endcase,
                    "default" => Token::Default,
                    "assign" => Token::Assign,
                    "begin" => Token::Begin,
                    "end" => Token::End,
                    "or" => Token::Or,
                    // Keywords handled by parser (skipped as module items)
                    "initial" => Token::Ident("__initial__".to_string()),
                    // Skip simple declarations to semicolon
                    "parameter" | "integer" => {
                        while let Some(c) = self.advance() {
                            if c == ';' {
                                break;
                            }
                        }
                        self.next_token()
                    }
                    _ if ident.starts_with('$') => {
                        // System task - skip to semicolon
                        while let Some(c) = self.advance() {
                            if c == ';' {
                                break;
                            }
                        }
                        self.next_token()
                    }
                    _ => Token::Ident(ident),
                }
            }

            '$' => {
                // System task - skip to semicolon
                let _ = self.read_ident();
                while let Some(c) = self.advance() {
                    if c == ';' {
                        break;
                    }
                }
                self.next_token()
            }

            _ => self.next_token(), // skip unknown chars
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            if tok == Token::Eof {
                tokens.push(Token::Eof);
                break;
            }
            tokens.push(tok);
        }
        tokens
    }
}
