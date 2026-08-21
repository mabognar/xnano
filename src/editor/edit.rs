use crate::editor::Editor;
use crate::editor::MovementExt;
use crate::ui::HelpExt;

pub trait EditExt {
    fn copy_selection(&mut self);
    fn cut_line(&mut self);
    fn delete_char(&mut self);
    fn delete_selection(&mut self) -> bool;
    fn insert_tab(&mut self);
    fn justify(&mut self);
    fn paste_line(&mut self);
    fn redo(&mut self);
    fn snapshot(&mut self);
    fn undo(&mut self);
}

impl EditExt for Editor {

    fn copy_selection(&mut self) {
        if self.buffer.len_chars() == 0 { return; }

        if let Some(mark_idx) = self.mark {
            let cursor_idx = self.get_cursor_char_idx();
            let start_char = mark_idx.min(cursor_idx);
            let end_char = mark_idx.max(cursor_idx);

            if start_char != end_char {
                let copy_text = self.buffer.slice(start_char..end_char).to_string();
                self.clipboard = copy_text;
                self.mark = None;
                self.set_status(String::from("Copied selection"));
            } else {
                self.set_status(String::from("Nothing to copy"));
            }
        } else {
            let start_char = self.buffer.line_to_char(self.cursor_y);
            let end_char = if self.cursor_y + 1 < self.buffer.len_lines() {
                self.buffer.line_to_char(self.cursor_y + 1)
            } else {
                self.buffer.len_chars()
            };
            let copy_text = self.buffer.slice(start_char..end_char).to_string();
            self.clipboard = copy_text;
            self.set_status(String::from("Copied line"));
        }

        // sync to the OS clipboard
        if let Ok(mut ctx) = arboard::Clipboard::new() {
            let _ = ctx.set_text(self.clipboard.clone());
        }
    }


    fn cut_line(&mut self) {
        self.snapshot();
        if self.buffer.len_chars() == 0 { return; }

        // if mark is active, cut the selected region
        if let Some(mark_idx) = self.mark {
            let cursor_idx = self.get_cursor_char_idx();
            let start_char = mark_idx.min(cursor_idx);
            let end_char = mark_idx.max(cursor_idx);

            if start_char != end_char {
                let cut_text = self.buffer.slice(start_char..end_char).to_string();
                if self.previous_action_was_cut {
                    self.clipboard.push_str(&cut_text);
                } else {
                    self.clipboard = cut_text;
                }

                self.buffer.remove(start_char..end_char);

                self.cursor_y = self.buffer.char_to_line(start_char);
                self.cursor_x = start_char - self.buffer.line_to_char(self.cursor_y);
                self.desired_cursor_x = self.cursor_x;

                self.mark = None; // Unmark after cutting
                self.set_status(String::from("Cut selection"));
                self.mark_modified();
            }
        } else {
            // no mark, do full-line cut
            let start_char = self.buffer.line_to_char(self.cursor_y);
            let end_char = if self.cursor_y + 1 < self.buffer.len_lines() {
                self.buffer.line_to_char(self.cursor_y + 1)
            } else {
                self.buffer.len_chars()
            };

            let cut_text = self.buffer.slice(start_char..end_char).to_string();
            if self.previous_action_was_cut {
                self.clipboard.push_str(&cut_text);
            } else {
                self.clipboard = cut_text;
            }

            self.buffer.remove(start_char..end_char);

            self.cursor_x = 0;
            self.desired_cursor_x = 0;
            let max_y = self.buffer.len_lines().saturating_sub(1);
            if self.cursor_y > max_y {
                self.cursor_y = max_y;
            }
            self.set_status(String::from("Cut line"));
            self.mark_modified();
        }

        if let Ok(mut ctx) = arboard::Clipboard::new() {
            let _ = ctx.set_text(self.clipboard.clone());
        }
    }


    fn delete_char(&mut self) {
        self.snapshot();
        if self.delete_selection() {
            return;
        }

        let idx = self.get_cursor_char_idx();
        if idx < self.buffer.len_chars() {
            self.buffer.remove(idx..(idx + 1));
            self.mark_modified();
        }
    }


    fn delete_selection(&mut self) -> bool {
        if let Some(mark_idx) = self.mark {
            let cursor_idx = self.get_cursor_char_idx();
            let start_char = mark_idx.min(cursor_idx);
            let end_char = mark_idx.max(cursor_idx);

            // always unmark, whether we delete or not
            self.mark = None;

            if start_char != end_char {
                self.buffer.remove(start_char..end_char);
                self.cursor_y = self.buffer.char_to_line(start_char);
                self.cursor_x = start_char - self.buffer.line_to_char(self.cursor_y);
                self.desired_cursor_x = self.cursor_x;
                self.mark_modified();
                return true; // text successfully deleted
            }
        }
        false
    }


    fn insert_tab(&mut self) {
        self.snapshot();
        self.delete_selection(); // delete selected text before

        let idx = self.get_cursor_char_idx();
        self.buffer.insert(idx, "    ");
        self.cursor_x += 4;
        self.desired_cursor_x = self.cursor_x;
        self.mark_modified();
    }


    fn justify(&mut self) {
        self.snapshot();

        let max_y = self.buffer.len_lines().saturating_sub(1);
        if max_y == 0 && self.buffer.len_chars() == 0 { return; }

        let mut start_line = self.cursor_y;
        while start_line > 0 && self.buffer.line(start_line - 1).chars().any(|c| !c.is_whitespace()) {
            start_line -= 1;
        }

        let mut end_line = self.cursor_y;
        while end_line <= max_y && self.buffer.line(end_line).chars().any(|c| !c.is_whitespace()) {
            end_line += 1;
        }

        if start_line == end_line && !self.buffer.line(start_line).chars().any(|c| !c.is_whitespace()) {
            return;
        }

        let start_char = self.buffer.line_to_char(start_line);
        let end_char = if end_line < self.buffer.len_lines() {
            self.buffer.line_to_char(end_line)
        } else {
            self.buffer.len_chars()
        };

        let text = self.buffer.slice(start_char..end_char).to_string();
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() { return; }

        let mut new_text = String::new();
        let mut current_line_len = 0;

        for word in words {
            if current_line_len + word.len() + 1 > 72 {
                new_text.push('\n');
                new_text.push_str(word);
                current_line_len = word.len();
            } else {
                if current_line_len > 0 {
                    new_text.push(' ');
                    current_line_len += 1;
                }
                new_text.push_str(word);
                current_line_len += word.len();
            }
        }
        new_text.push('\n');

        self.buffer.remove(start_char..end_char);
        self.buffer.insert(start_char, &new_text);

        let total_chars = self.buffer.len_chars();
        let safe_pos = (start_char + new_text.chars().count()).min(total_chars);
        let raw_y = self.buffer.char_to_line(safe_pos);

        self.cursor_y = raw_y.min(self.buffer.len_lines().saturating_sub(1));
        self.cursor_x = safe_pos - self.buffer.line_to_char(self.cursor_y);
        self.desired_cursor_x = self.cursor_x;

        self.mark_modified();
        self.set_status(String::from("Justified"));
    }


    fn paste_line(&mut self) {
        self.snapshot();

        // try to sync from the OS clipboard first
        if let Ok(mut ctx) = arboard::Clipboard::new() {
            if let Ok(os_text) = ctx.get_text() {
                self.clipboard = os_text;
            }
        }

        if self.clipboard.is_empty() { return; }

        self.delete_selection();
        let current_char = self.get_cursor_char_idx();
        self.buffer.insert(current_char, &self.clipboard);
        let new_idx = current_char + self.clipboard.chars().count();
        self.cursor_y = self.buffer.char_to_line(new_idx);
        self.cursor_x = new_idx - self.buffer.line_to_char(self.cursor_y);
        self.desired_cursor_x = self.cursor_x;
        self.set_status(String::from("Pasted text"));
        self.mark_modified();
    }


    fn redo(&mut self) {
        if let Some((new_buffer, new_x, new_y)) = self.redo_stack.pop() {
            // save current reality back to the undo stack
            self.undo_stack.push((self.buffer.clone(), self.cursor_x, self.cursor_y));

            // restore the alternate future
            self.buffer = new_buffer;
            self.cursor_x = new_x;
            self.cursor_y = new_y;
            self.desired_cursor_x = new_x;

            // update the UI
            self.mark_modified();
            self.set_status(String::from("Redo"));
        } else {
            self.set_status(String::from("Nothing to redo"));
        }
    }


    fn snapshot(&mut self) {
        // save the current state of the buffer and cursor positions
        self.undo_stack.push((self.buffer.clone(), self.cursor_x, self.cursor_y));

        // clear the redo stack because a new edit invalidates the alternate future
        self.redo_stack.clear();
    }

    
    fn undo(&mut self) {
        if let Some((old_buffer, old_x, old_y)) = self.undo_stack.pop() {
            // save our current reality to the redo stack
            self.redo_stack.push((self.buffer.clone(), self.cursor_x, self.cursor_y));

            // restore the past
            self.buffer = old_buffer;
            self.cursor_x = old_x;
            self.cursor_y = old_y;
            self.desired_cursor_x = old_x;

            // update the UI
            self.mark_modified();
            self.set_status(String::from("Undo"));
        } else {
            self.set_status(String::from("Nothing to undo"));
        }
    }

}