/// Hard maximum length for a single console input line in UTF-8 bytes (Amendment 13).
pub const MAX_CONSOLE_INPUT_BYTES: usize = 4096;

/// Parsed representation of a developer console command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub raw_input: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    InputTooLong { max: usize, actual: usize },
    UnmatchedQuote,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InputTooLong { max, actual } => {
                write!(
                    f,
                    "input exceeds maximum length ({} > {} bytes)",
                    actual, max
                )
            }
            Self::UnmatchedQuote => write!(f, "unmatched quotation mark in command input"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses a raw console input string into a structured `ParsedCommand`.
///
/// Returns `Ok(None)` if the input is empty or whitespace-only.
/// Returns `Err(ParseError)` if the input exceeds `MAX_CONSOLE_INPUT_BYTES` or has unclosed quotes.
///
/// INVARIANTS:
/// - UTF-8 safe: operates over Unicode scalar values (`char`), never arbitrary byte slicing.
/// - Deterministic: identical inputs always tokenize identically.
/// - Bounded: rejects inputs exceeding `MAX_CONSOLE_INPUT_BYTES`.
pub fn parse_command(input: &str) -> Result<Option<ParsedCommand>, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if trimmed.len() > MAX_CONSOLE_INPUT_BYTES {
        return Err(ParseError::InputTooLong {
            max: MAX_CONSOLE_INPUT_BYTES,
            actual: trimmed.len(),
        });
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = trimmed.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            '\\' if in_quotes => {
                if let Some(&next_ch) = chars.peek() {
                    if next_ch == '"' || next_ch == '\\' {
                        current.push(next_ch);
                        chars.next();
                    } else {
                        current.push(ch);
                    }
                } else {
                    current.push(ch);
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current);
                    current = String::new();
                }
            }
            c => {
                current.push(c);
            }
        }
    }

    if in_quotes {
        return Err(ParseError::UnmatchedQuote);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    if tokens.is_empty() {
        return Ok(None);
    }

    let command = tokens.remove(0).to_lowercase();
    Ok(Some(ParsedCommand {
        raw_input: trimmed.to_string(),
        command,
        args: tokens,
    }))
}
