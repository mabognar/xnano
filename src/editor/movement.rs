use crate::editor::Editor;
use crate::ui::{HelpExt, PromptExt};
use crossterm::terminal;
use std::io;

pub trait MovementExt {
    fn cur_pos(&mut self);
    fn get_cursor_char_idx(&self) -> usize;
    fn get_visual_cursor_x(&self) -> usize;
    fn get_visual_line_width(&self, y: usize) -> usize;
    fn go_to_line(&mut self) -> io::Result<()>;
    fn line_len(&self, y: usize) -> usize;
    fn move_down(&mut self);
    fn move_left(&mut self);
    fn move_right(&mut self);
    fn move_to_end_of_line(&mut self);
    fn move_to_start_of_line(&mut self);
    fn move_up(&mut self);
    fn move_word_left(&mut self);
    fn move_word_right(&mut self);
    fn page_down(&mut self) -> io::Result<()>;
    fn page_up(&mut self) -> io::Result<()>;
    fn scroll(&mut self) -> io::Result<()>;
}

impl MovementExt for Editor {
    fn cur_pos(&mut self) {
        let line = self.cursor_y + 1;
        let total_lines = self.buffer.len_lines();
        let col = self.cursor_x + 1;
        let total_chars = self.buffer.len_chars();
        self.set_status(format!("line {}/{}, col {}, char {}", line, total_lines, col, total_chars));
    }

    fn get_cursor_char_idx(&self) -> usize {
        self.buffer.line_to_char(self.cursor_y) + self.cursor_x
    }

    fn get_visual_cursor_x(&self) -> usize {
        if self.cursor_y >= self.buffer.len_lines() { return 0; }
        let line = self.buffer.line(self.cursor_y);
        let mut visual_x = 0;
        for ch in line.chars().take(self.cursor_x) {
            if ch == '\t' {
                visual_x += 4 - (visual_x % 4);
            } else {
                visual_x += 1;
            }
        }
        visual_x
    }


    fn get_visual_line_width(&self, y: usize) -> usize {
        if y >= self.buffer.len_lines() { return 0; }
        let mut w = 0;
        for ch in self.buffer.line(y).chars() {
            if ch == '\n' || ch == '\r' { continue; }
            if ch == '\t' { w += 4 - (w % 4); }
            else { w += 1; }
        }
        w
    }

    fn go_to_line(&mut self) -> io::Result<()> {
        if let Some(input) = self.prompt("Enter line number: ", false)? {
            if let Ok(line) = input.trim().parse::<usize>() {
                self.cursor_y = line.saturating_sub(1).min(self.buffer.len_lines().saturating_sub(1));
                self.cursor_x = 0;
                self.desired_cursor_x = 0;
                self.clear_status();
            } else {
                self.set_status(String::from("Invalid line number"));
            }
        }
        Ok(())
    }

    fn line_len(&self, y: usize) -> usize {
        if y >= self.buffer.len_lines() {
            return 0;
        }

        let line = self.buffer.line(y);
        let mut len = line.len_chars();
        if len > 0 && line.char(len - 1) == '\n' { len -= 1; }
        if len > 0 && line.char(len - 1) == '\r' { len -= 1; }
        len
    }

    fn move_down(&mut self) {
        if self.cursor_y < self.buffer.len_lines().saturating_sub(1) {
            self.cursor_y += 1;
            self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        }
    }

    fn move_left(&mut self) {
        let idx = self.get_cursor_char_idx();
        if idx > 0 {
            let new_idx = idx - 1;
            self.cursor_y = self.buffer.char_to_line(new_idx);
            self.cursor_x = new_idx - self.buffer.line_to_char(self.cursor_y);
            self.desired_cursor_x = self.cursor_x;
        }
    }

    fn move_right(&mut self) {
        let idx = self.get_cursor_char_idx();
        if idx < self.buffer.len_chars() {
            let new_idx = idx + 1;
            self.cursor_y = self.buffer.char_to_line(new_idx);
            self.cursor_x = new_idx - self.buffer.line_to_char(self.cursor_y);
            self.desired_cursor_x = self.cursor_x;
        }
    }

    fn move_to_end_of_line(&mut self) {
        self.cursor_x = self.line_len(self.cursor_y);
        self.desired_cursor_x = self.cursor_x;
    }

    fn move_to_start_of_line(&mut self) {
        self.cursor_x = 0;
        self.desired_cursor_x = 0;
    }

    fn move_up(&mut self) {
        if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        }
    }
    fn move_word_left(&mut self) {
        let mut idx = self.get_cursor_char_idx();
        if idx == 0 { return; }

        idx -= 1; // move back one character to begin evaluating

        // skip backwards through any spaces, punctuation, or newlines
        while idx > 0 && !(self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
            idx -= 1;
        }

        // skip backwards through the word characters to find the start of the word
        while idx > 0 && (self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
            idx -= 1;
        }

        // if we do not hit beginning of file, step forward one
        // to land on the first letter of the word.
        if idx > 0 || !(self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
            idx += 1;
        }

        // update cursor coordinates
        self.cursor_y = self.buffer.char_to_line(idx);
        self.cursor_x = idx - self.buffer.line_to_char(self.cursor_y);
        self.desired_cursor_x = self.cursor_x;
    }

    fn move_word_right(&mut self) {
        let mut idx = self.get_cursor_char_idx();
        let len = self.buffer.len_chars();
        if idx >= len { return; }

        let start_is_word = self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_';

        // if we are on a word, skip the rest of the word characters
        if start_is_word {
            while idx < len && (self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
                idx += 1;
            }
        }

        // skip any spaces, punctuation, or newlines until the start of the next word
        while idx < len && !(self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
            idx += 1;
        }

        // update cursor coordinates
        self.cursor_y = self.buffer.char_to_line(idx);
        self.cursor_x = idx - self.buffer.line_to_char(self.cursor_y);
        self.desired_cursor_x = self.cursor_x;
    }

    fn page_down(&mut self) -> io::Result<()> {
        let (_, rows) = terminal::size()?;
        let visible_rows = rows.saturating_sub(4) as usize;
        let max_y = self.buffer.len_lines().saturating_sub(1);
        self.cursor_y = (self.cursor_y + visible_rows).min(max_y);
        self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        Ok(())
    }

    fn page_up(&mut self) -> io::Result<()> {
        let (_, rows) = terminal::size()?;
        let visible_rows = rows.saturating_sub(4) as usize;
        self.cursor_y = self.cursor_y.saturating_sub(visible_rows);
        self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        Ok(())
    }

    fn scroll(&mut self) -> io::Result<()> {
        let (cols, rows) = terminal::size()?;
        let visible_rows = rows.saturating_sub(4) as usize;
        let cols_u = cols as usize;

        let max_line_num_len = self.buffer.len_lines().to_string().len();
        let gutter_width = if self.show_line_numbers { max_line_num_len + 1 } else { 0 };
        let available_width = std::cmp::max(1, cols_u.saturating_sub(gutter_width));

        if self.soft_wrap {
            self.col_offset = 0;

            if self.cursor_y < self.row_offset {
                self.row_offset = self.cursor_y;
            } else {
                let mut screen_rows_used = self.get_visual_cursor_x() / available_width;
                let mut required_row_offset = self.cursor_y;

                while required_row_offset > 0 {
                    let prev_line = required_row_offset - 1;
                    let w = self.get_visual_line_width(prev_line);
                    let line_rows = if w == 0 { 1 } else { (w - 1) / available_width + 1 };

                    if screen_rows_used + line_rows >= visible_rows {
                        break;
                    }

                    screen_rows_used += line_rows;
                    required_row_offset -= 1;
                }

                if self.row_offset < required_row_offset {
                    self.row_offset = required_row_offset;
                }
            }
        } else {
            // vertical scrolling
            if self.cursor_y < self.row_offset {
                self.row_offset = self.cursor_y;
            } else if self.cursor_y >= self.row_offset + visible_rows {
                self.row_offset = self.cursor_y.saturating_sub(visible_rows.saturating_sub(1));
            }

            // horizontal scrolling --- 1/2 page at a time
            let visual_x = self.get_visual_cursor_x();
            let right_bound = self.col_offset + available_width;

            if visual_x < self.col_offset {
                // cursor moved off the left edge, jump left by half a screen
                self.col_offset = visual_x.saturating_sub(available_width / 2);
            } else if visual_x >= right_bound {
                // cursor moved off the right edge, jump right by half a screen
                self.col_offset = visual_x.saturating_sub(available_width / 2);
            }
        }

        Ok(())
    }


}