use std::collections::VecDeque;

use super::command::{CommandRegistry, CommandResult};
use super::context::DeveloperExecutionContext;
use super::parser::{parse_command, MAX_CONSOLE_INPUT_BYTES};

pub const MAX_HISTORY_ENTRIES: usize = 128;
pub const MAX_OUTPUT_LINES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLineKind {
    Input,
    Output,
    Error,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleLine {
    pub text: String,
    pub kind: ConsoleLineKind,
}

impl ConsoleLine {
    pub fn new(text: impl Into<String>, kind: ConsoleLineKind) -> Self {
        Self {
            text: text.into(),
            kind,
        }
    }
}

/// State container for developer console input, history, and scrollback.
pub struct ConsoleState {
    /// Whether the developer console overlay is open and capturing input.
    pub is_open: bool,
    /// Current editable command line buffer.
    pub input_buffer: String,
    /// Cursor position in `input_buffer` (in UTF-8 char offset).
    pub cursor_pos: usize,
    /// Bounded command history for Up/Down arrow navigation.
    pub history: VecDeque<String>,
    /// Active navigation index into `history` (None when editing new input).
    pub history_cursor: Option<usize>,
    /// Bounded scrollback output lines.
    pub output_lines: VecDeque<ConsoleLine>,
    /// Vertical scroll offset (0 = scrolled to bottom).
    pub scroll_offset: usize,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleState {
    pub fn new() -> Self {
        let mut state = Self {
            is_open: false,
            input_buffer: String::new(),
            cursor_pos: 0,
            history: VecDeque::with_capacity(MAX_HISTORY_ENTRIES),
            history_cursor: None,
            output_lines: VecDeque::with_capacity(MAX_OUTPUT_LINES),
            scroll_offset: 0,
        };
        state.print_info("Omnisia Developer Console initialized. Type 'help' for command list.");
        state
    }

    /// Toggles console open/closed state.
    #[inline]
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
        if self.is_open {
            self.scroll_offset = 0;
        }
    }

    #[inline]
    pub fn open(&mut self) {
        self.is_open = true;
        self.scroll_offset = 0;
    }

    #[inline]
    pub fn close(&mut self) {
        self.is_open = false;
    }

    #[inline]
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Appends an informational output line.
    pub fn print_info(&mut self, text: impl Into<String>) {
        self.push_line(ConsoleLine::new(text, ConsoleLineKind::Info));
    }

    /// Appends standard command output.
    pub fn print(&mut self, text: impl Into<String>) {
        self.push_line(ConsoleLine::new(text, ConsoleLineKind::Output));
    }

    /// Appends an error output line.
    pub fn print_error(&mut self, text: impl Into<String>) {
        self.push_line(ConsoleLine::new(text, ConsoleLineKind::Error));
    }

    fn push_line(&mut self, line: ConsoleLine) {
        // Multi-line split support so multiline strings format cleanly
        for single_line in line.text.split('\n') {
            if self.output_lines.len() >= MAX_OUTPUT_LINES {
                self.output_lines.pop_front();
            }
            self.output_lines
                .push_back(ConsoleLine::new(single_line, line.kind));
        }
    }

    /// Clears the scrollback output buffer (Amendment 10).
    pub fn clear(&mut self) {
        self.output_lines.clear();
        self.scroll_offset = 0;
    }

    /// Inserts a character at the cursor position (UTF-8 safe, bounded to 4096 bytes).
    pub fn insert_char(&mut self, c: char) {
        if c == '\n' || c == '\r' || c == '`' {
            return;
        }

        if self.input_buffer.len() + c.len_utf8() > MAX_CONSOLE_INPUT_BYTES {
            return;
        }

        let byte_idx = self
            .input_buffer
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(idx, _)| idx)
            .unwrap_or(self.input_buffer.len());

        self.input_buffer.insert(byte_idx, c);
        self.cursor_pos += 1;
        self.history_cursor = None;
    }

    /// Deletes the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            let byte_idx = self
                .input_buffer
                .char_indices()
                .nth(self.cursor_pos)
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            self.input_buffer.remove(byte_idx);
            self.history_cursor = None;
        }
    }

    /// Deletes the character at the cursor.
    pub fn delete(&mut self) {
        let char_count = self.input_buffer.chars().count();
        if self.cursor_pos < char_count {
            let byte_idx = self
                .input_buffer
                .char_indices()
                .nth(self.cursor_pos)
                .map(|(idx, _)| idx)
                .unwrap_or(self.input_buffer.len());
            self.input_buffer.remove(byte_idx);
            self.history_cursor = None;
        }
    }

    pub fn cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn cursor_right(&mut self) {
        let count = self.input_buffer.chars().count();
        if self.cursor_pos < count {
            self.cursor_pos += 1;
        }
    }

    pub fn cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn cursor_end(&mut self) {
        self.cursor_pos = self.input_buffer.chars().count();
    }

    /// Recalls previous command in history (Up arrow).
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        let next_cursor = match self.history_cursor {
            None => self.history.len().saturating_sub(1),
            Some(idx) => idx.saturating_sub(1),
        };

        self.history_cursor = Some(next_cursor);
        if let Some(cmd) = self.history.get(next_cursor) {
            self.input_buffer = cmd.clone();
            self.cursor_pos = self.input_buffer.chars().count();
        }
    }

    /// Recalls next command in history (Down arrow).
    pub fn history_next(&mut self) {
        if let Some(idx) = self.history_cursor {
            if idx + 1 < self.history.len() {
                let next_cursor = idx + 1;
                self.history_cursor = Some(next_cursor);
                if let Some(cmd) = self.history.get(next_cursor) {
                    self.input_buffer = cmd.clone();
                    self.cursor_pos = self.input_buffer.chars().count();
                }
            } else {
                self.history_cursor = None;
                self.input_buffer.clear();
                self.cursor_pos = 0;
            }
        }
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(lines)
            .min(self.output_lines.len().saturating_sub(10));
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Submits the current input line, dispatches it to the command registry, and updates history.
    pub fn submit(&mut self, registry: &CommandRegistry, ctx: &mut DeveloperExecutionContext) {
        let input = std::mem::take(&mut self.input_buffer);
        self.cursor_pos = 0;
        self.history_cursor = None;
        self.scroll_offset = 0;

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }

        // Echo submitted input line
        self.push_line(ConsoleLine::new(
            format!("> {}", trimmed),
            ConsoleLineKind::Input,
        ));

        // Add to history if not duplicate of latest
        if self.history.back().map(|s| s.as_str()) != Some(trimmed) {
            if self.history.len() >= MAX_HISTORY_ENTRIES {
                self.history.pop_front();
            }
            self.history.push_back(trimmed.to_string());
        }

        // Parse command line
        match parse_command(trimmed) {
            Ok(Some(parsed)) => {
                let result = registry.dispatch(&parsed, ctx);
                match result {
                    CommandResult::Success(msg) => {
                        self.print(msg);
                    }
                    CommandResult::Error(err) => {
                        self.print_error(format!("Error: {}", err));
                    }
                    CommandResult::Clear => {
                        self.clear();
                    }
                }
            }
            Ok(None) => {}
            Err(err) => {
                self.print_error(format!("Parse error: {}", err));
            }
        }
    }
}
