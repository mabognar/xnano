use crate::editor::Editor;
use crate::editor::MovementExt;
use crate::ui::HelpExt;
use std::collections::BTreeSet;
use std::io;

pub trait MarkExt {
    fn adjust_marks_on_delete(&mut self, deleted_line: usize);
    fn goto_next_mark(&mut self) -> io::Result<()>;
    fn goto_prev_mark(&mut self) -> io::Result<()>;
    fn toggle_line_mark(&mut self);
    fn toggle_mark(&mut self);
}

impl MarkExt for Editor {

    fn adjust_marks_on_delete(&mut self, deleted_line: usize) {
        let mut new_marks = BTreeSet::new();

        for &mark in &self.line_marks {
            if mark < deleted_line {
                // marks above the cut line stay exactly where they are
                new_marks.insert(mark);
            } else if mark == deleted_line {
                // delete mark along with the line.
                continue;
            } else {
                // marks below the cut line shift up by 1 to follow their text
                new_marks.insert(mark - 1);
            }
        }

        self.line_marks = new_marks;
    }


    fn goto_next_mark(&mut self) -> std::io::Result<()> {
        if self.line_marks.is_empty() {
            self.set_status(String::from("No marks set"));
            return Ok(());
        }

        // find the smallest mark larger than our current line, or wrap around to the lowest mark
        let next = self.line_marks.range((self.cursor_y + 1)..).next()
            .or_else(|| self.line_marks.iter().next())
            .copied();

        if let Some(line) = next {
            self.cursor_y = line;
            self.cursor_x = 0;
            self.desired_cursor_x = self.cursor_x;
            self.scroll()?;
            self.set_status(String::from("Jumped to next mark"));
        }
        Ok(())
    }

    fn goto_prev_mark(&mut self) -> std::io::Result<()> {
        if self.line_marks.is_empty() {
            self.set_status(String::from("No marks set"));
            return Ok(());
        }

        // find the largest mark smaller than our current line, or wrap around to the highest mark
        let prev = self.line_marks.range(..self.cursor_y).next_back()
            .or_else(|| self.line_marks.iter().next_back())
            .copied();

        if let Some(line) = prev {
            self.cursor_y = line;
            self.cursor_x = 0;
            self.desired_cursor_x = self.cursor_x;
            self.scroll()?;
            self.set_status(String::from("Jumped to previous mark"));
        }
        Ok(())
    }

    fn toggle_line_mark(&mut self) {
        if self.line_marks.contains(&self.cursor_y) {
            self.line_marks.remove(&self.cursor_y);
            self.set_status(String::from("Mark removed"));
        } else {
            self.line_marks.insert(self.cursor_y);
            self.set_status(String::from("Mark set"));
        }
    }

    fn toggle_mark(&mut self) {
        if self.mark.is_some() {
            self.mark = None;
            self.set_status(String::from("Unselected text"));
        } else {
            self.mark = Some(self.get_cursor_char_idx());
            self.set_status(String::from("Mark Set"));
        }
    }

}