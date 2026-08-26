use super::BasisError;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Word(String),
    Text(String),
    Number(f64),
    Newline,
    Comma,
    LeftBracket,
    RightBracket,
    LeftParen,
    RightParen,
    Operator(String),
    Symbol(char),
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub start: usize,
    pub length: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(source: &str) -> Result<Vec<Token>, BasisError> {
    let mut characters = source.char_indices().peekable();
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut column = 1;

    while let Some((start, character)) = characters.next() {
        let start_line = line;
        let start_column = column;

        if character == '\n' {
            tokens.push(token(TokenKind::Newline, start, start + 1, start_line, start_column));
            line += 1;
            column = 1;
            continue;
        }
        if character == '\r' {
            let mut end = start + character.len_utf8();
            if matches!(characters.peek(), Some((_, '\n'))) {
                let (_, newline) = characters.next().expect("peeked newline");
                end += newline.len_utf8();
            }
            tokens.push(token(TokenKind::Newline, start, end, start_line, start_column));
            line += 1;
            column = 1;
            continue;
        }
        if character.is_whitespace() {
            column += 1;
            continue;
        }
        if character == '#' {
            column += 1;
            while let Some((_, next)) = characters.peek() {
                if *next == '\n' || *next == '\r' {
                    break;
                }
                characters.next();
                column += 1;
            }
            continue;
        }

        if character == '"' || character == '\'' {
            let quote = character;
            let mut value = String::new();
            let mut escaped = false;
            let mut end = start + quote.len_utf8();
            let mut closed = false;
            column += 1;

            while let Some((_, next)) = characters.next() {
                end += next.len_utf8();
                if next == '\n' || next == '\r' {
                    return Err(BasisError::at(start_line, start_column, "unterminated string literal"));
                }
                if escaped {
                    value.push(unescape_character(next));
                    escaped = false;
                    column += 1;
                    continue;
                }
                if next == '\\' {
                    escaped = true;
                    column += 1;
                    continue;
                }
                if next == quote {
                    closed = true;
                    column += 1;
                    break;
                }
                value.push(next);
                column += 1;
            }

            if !closed {
                return Err(BasisError::at(start_line, start_column, "unterminated string literal"));
            }
            tokens.push(token(TokenKind::Text(value), start, end, start_line, start_column));
            continue;
        }

        let starts_decimal = character == '.' && matches!(characters.peek(), Some((_, next)) if next.is_ascii_digit());
        if character.is_ascii_digit() || starts_decimal {
            let mut literal = String::from(character);
            let mut end = start + character.len_utf8();
            column += 1;
            while let Some((_, next)) = characters.peek() {
                if !next.is_ascii_digit() && *next != '.' {
                    break;
                }
                let (_, next) = characters.next().expect("peeked number character");
                literal.push(next);
                end += next.len_utf8();
                column += 1;
            }
            if let Some((_, next)) = characters.peek() {
                if *next == 'e' || *next == 'E' {
                    let (_, exponent) = characters.next().expect("peeked exponent marker");
                    literal.push(exponent);
                    end += exponent.len_utf8();
                    column += 1;
                    if let Some((_, sign)) = characters.peek() {
                        if *sign == '+' || *sign == '-' {
                            let (_, sign) = characters.next().expect("peeked exponent sign");
                            literal.push(sign);
                            end += sign.len_utf8();
                            column += 1;
                        }
                    }
                    while let Some((_, digit)) = characters.peek() {
                        if !digit.is_ascii_digit() {
                            break;
                        }
                        let (_, digit) = characters.next().expect("peeked exponent digit");
                        literal.push(digit);
                        end += digit.len_utf8();
                        column += 1;
                    }
                }
            }
            let number = literal.parse::<f64>().map_err(|_| BasisError::at(start_line, start_column, format!("invalid number `{literal}`")))?;
            tokens.push(token(TokenKind::Number(number), start, end, start_line, start_column));
            continue;
        }

        if character.is_alphanumeric() || character == '_' {
            let mut word = String::from(character);
            let mut end = start + character.len_utf8();
            column += 1;
            while let Some((_, next)) = characters.peek() {
                if !next.is_alphanumeric() && *next != '_' && *next != '-' {
                    break;
                }
                let (_, next) = characters.next().expect("peeked word character");
                word.push(next);
                end += next.len_utf8();
                column += 1;
            }
            tokens.push(token(TokenKind::Word(word), start, end, start_line, start_column));
            continue;
        }

        let kind = match character {
            ',' => TokenKind::Comma,
            '[' => TokenKind::LeftBracket,
            ']' => TokenKind::RightBracket,
            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' => TokenKind::Operator(character.to_string()),
            other => TokenKind::Symbol(other),
        };
        tokens.push(token(kind, start, start + character.len_utf8(), start_line, start_column));
        column += 1;
    }

    tokens.push(token(TokenKind::End, source.len(), source.len(), line, column));
    Ok(tokens)
}

fn token(kind: TokenKind, start: usize, end: usize, line: usize, column: usize) -> Token {
    Token { kind, span: Span { line, column, start, length: end - start } }
}

fn unescape_character(character: char) -> char {
    match character {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        '\\' => '\\',
        '"' => '"',
        '\'' => '\'',
        other => other,
    }
}
