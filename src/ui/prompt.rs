use crate::editor::{Editor, FileExt, MenuState};
use crate::config::ConfigExt;
use crate::ui::{BrowserExt};
use crate::ui::{RenderExt, HelpExt};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    queue,
    style::{Print, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::env;
use std::io::{self, stdout, Write};


pub trait PromptExt {
    fn inline_prompt(&self, prefix: &str, initial_input: &str) -> io::Result<Option<String>>;
    fn prompt(&mut self, prompt_text: &str, allow_browser: bool) -> io::Result<Option<String>>;
    fn prompt_yn(&mut self, prompt_text: &str) -> io::Result<Option<bool>>;
    fn prompt_replace(&mut self, prompt_text: &str) -> io::Result<Option<char>>;
}


impl PromptExt for Editor {

    fn inline_prompt(&self, prefix: &str, initial_input: &str) -> io::Result<Option<String>> {
        let mut chars: Vec<char> = initial_input.chars().collect();
        let mut cursor_idx = chars.len();
        let mut stdout = stdout();
        let theme = &self.theme_set.themes[&self.current_theme];
        let colors = Self::get_theme_colors(theme);

        // --- NEW: Tracks our horizontal scrolling window ---
        let mut view_start = 0;

        loop {
            // fetch terminal size dynamically in the loop in case the window resizes
            let (cols, rows) = terminal::size()?;

            // calculate exact space available for input
            let prompt_len = prefix.chars().count();
            let max_input_width = (cols as usize).saturating_sub(prompt_len + 1);

            // adjust the sliding window so the cursor is always visible
            if cursor_idx < view_start {
                view_start = cursor_idx;
            } else if cursor_idx >= view_start + max_input_width {
                view_start = cursor_idx.saturating_sub(max_input_width).saturating_add(1);
            }

            let mut display_input = String::new();
            let mut display_cursor_x = prompt_len;

            if view_start > 0 {
                display_input.push('…');
                display_cursor_x += 1;

                let skip_count = view_start + 1;
                let take_count = max_input_width.saturating_sub(1);
                display_input.extend(chars.iter().skip(skip_count).take(take_count));
                display_cursor_x += cursor_idx.saturating_sub(skip_count);
            } else {
                display_input.extend(chars.iter().take(max_input_width));
                display_cursor_x += cursor_idx;
            }

            // add right ellipsis if there is hidden text
            if chars.len() > view_start + max_input_width {
                if display_input.chars().count() >= max_input_width {
                    display_input.pop();
                }
                display_input.push('…');
            }

            queue!(stdout, cursor::MoveTo(0, rows - 3), SetBackgroundColor(colors.menu_bg))?;
            queue!(
                stdout,
                SetForegroundColor(colors.accent),
                Print(prefix),
                SetForegroundColor(colors.fg),
                Print(&display_input),
                terminal::Clear(ClearType::UntilNewLine)
            )?;

            queue!(stdout, cursor::MoveTo(display_cursor_x as u16, rows - 3))?;
            stdout.flush()?;

            if let Event::Key(k) = event::read()? {
                if k.kind != event::KeyEventKind::Press { continue; }
                match k.code {
                    KeyCode::Enter => {
                        let final_string: String = chars.iter().collect();
                        return Ok(if chars.is_empty() { None } else { Some(final_string) });
                    }
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => return Ok(None),
                    KeyCode::Left => cursor_idx = cursor_idx.saturating_sub(1),
                    KeyCode::Char('b') if k.modifiers.contains(KeyModifiers::CONTROL) => cursor_idx = cursor_idx.saturating_sub(1),
                    KeyCode::Right => {
                        if cursor_idx < chars.len() { cursor_idx += 1; }
                    }
                    KeyCode::Char('f') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        if cursor_idx < chars.len() { cursor_idx += 1; }
                    }
                    KeyCode::Backspace => {
                        if cursor_idx > 0 {
                            cursor_idx -= 1;
                            chars.remove(cursor_idx);
                        }
                    }
                    KeyCode::Delete => {
                        if cursor_idx < chars.len() {
                            chars.remove(cursor_idx);
                        }
                    }
                    KeyCode::Char('d') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        if cursor_idx < chars.len() {
                            chars.remove(cursor_idx);
                        }
                    }
                    // insert
                    KeyCode::Char(c) if !c.is_control() => {
                        chars.insert(cursor_idx, c);
                        cursor_idx += 1;
                    }
                    _ => {}
                }
            }
        }
    }


    fn prompt(&mut self, prompt_text: &str, allow_browser: bool) -> io::Result<Option<String>> {
        let original_menu = self.menu_state;

        if self.menu_state == MenuState::Menu1
            || self.menu_state == MenuState::Menu2
            || self.menu_state == MenuState::Menu3
            || self.menu_state == MenuState::Menu4
        {
            self.menu_state = if allow_browser { MenuState::PromptWithBrowser } else { MenuState::CancelOnly };
        }

        self.status_time = None;

        // strip out brackets
        let mut clean_prompt = prompt_text.to_string();
        if let Some(start) = clean_prompt.find('[') {
            if let Some(end) = clean_prompt.find(']') {
                if start < end {
                    clean_prompt.replace_range(start..=end, "");
                    clean_prompt = clean_prompt.replace(" :", ":").replace("  ", " ");
                }
            }
        }

        let mut input = String::new();

        let is_save_prompt = clean_prompt.to_lowercase().contains("name to write");

        // pre-fill for save prompt
        if is_save_prompt {
            if let Some(ref fname) = self.filename {
                // Extract just the file name to keep the prompt clean
                let path = std::path::Path::new(fname);
                input = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            } else if let Ok(cwd) = env::current_dir() {
                input = cwd.to_string_lossy().into_owned();
                if !input.ends_with(std::path::MAIN_SEPARATOR) {
                    input.push(std::path::MAIN_SEPARATOR);
                }
            }
        }

        let mut chars: Vec<char> = input.chars().collect();
        let mut cursor_idx = chars.len();

        // horizontal scrolling window
        let mut view_start = 0;

        loop {
            input = chars.iter().collect();
            self.status_message = clean_prompt.clone();
            self.draw_screen()?;

            let (cols, rows) = terminal::size()?;
            let mut stdout = stdout();

            // calculate exact space available for input
            let prompt_len = clean_prompt.chars().count();
            // Leave 1 character margin to prevent terminal auto-wrapping
            let max_input_width = (cols as usize).saturating_sub(prompt_len + 1);

            // adjust the sliding window so the cursor is always visible
            if cursor_idx < view_start {
                view_start = cursor_idx;
            } else if cursor_idx >= view_start + max_input_width {
                view_start = cursor_idx.saturating_sub(max_input_width).saturating_add(1);
            }

            let mut display_input = String::new();
            let mut display_cursor_x = prompt_len;

            if view_start > 0 {
                display_input.push('…');
                display_cursor_x += 1; // Account for the left ellipsis

                let skip_count = view_start + 1;
                let take_count = max_input_width.saturating_sub(1);
                display_input.extend(chars.iter().skip(skip_count).take(take_count));
                display_cursor_x += cursor_idx.saturating_sub(skip_count);
            } else {
                display_input.extend(chars.iter().take(max_input_width));
                display_cursor_x += cursor_idx;
            }

            // add right ellipsis if there is hidden text
            if chars.len() > view_start + max_input_width {
                if display_input.chars().count() >= max_input_width {
                    display_input.pop(); // Make room for '…'
                }
                display_input.push('…');
            }

            queue!(stdout, cursor::MoveTo(prompt_len as u16, rows - 3))?;
            let theme = &self.theme_set.themes[&self.current_theme];
            let colors = Self::get_theme_colors(theme);

            queue!(
                stdout,
                SetBackgroundColor(colors.menu_bg),
                SetForegroundColor(colors.fg),
                Print(&display_input),
                terminal::Clear(ClearType::UntilNewLine) // Clear leftover artifacts
            )?;

            queue!(stdout, cursor::MoveTo(display_cursor_x as u16, rows - 3))?;
            stdout.flush()?;

            let ev = event::read()?;
            match ev {
                Event::Key(key) => {
                    if key.kind != event::KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Enter => {
                            if is_save_prompt && !input.is_empty() {
                                // expand the tilde before validating directory
                                let expanded_input = Self::expand_tilde(&input);
                                let mut target = std::path::PathBuf::from(&expanded_input);

                                if let Some(ref fname) = self.filename {
                                    let orig_path = std::path::Path::new(fname);
                                    if let Some(parent) = orig_path.parent() {
                                        // if user typed a new path like file.txt (1 component), append it to the current dir
                                        if target.components().count() == 1 {
                                            target = parent.join(&expanded_input);
                                        }
                                    }
                                }

                                if let Some(parent) = target.parent() {
                                    // if directory doesn't exist, throw error
                                    if !parent.as_os_str().is_empty() && !parent.exists() {
                                        self.set_status(String::from("File not saved. Directory does not exist."));
                                        self.menu_state = MenuState::Menu1;
                                        return Ok(None);
                                    }
                                }

                                input = target.to_string_lossy().into_owned();
                            }

                            self.clear_status();
                            self.menu_state = original_menu;
                            return Ok(Some(input));
                        }
                        KeyCode::Esc => {
                            self.set_status(String::from("Cancelled."));
                            self.menu_state = original_menu;
                            return Ok(None);
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.set_status(String::from("Cancelled."));
                            self.menu_state = original_menu;
                            return Ok(None);
                        }
                        KeyCode::Char('t') if allow_browser && key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if let Some(selected_path) = self.run_file_browser(is_save_prompt)? {
                                self.clear_status();
                                self.menu_state = original_menu;
                                return Ok(Some(selected_path));
                            }
                            self.menu_state = MenuState::PromptWithBrowser;
                        }
                        KeyCode::Left => cursor_idx = cursor_idx.saturating_sub(1),
                        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => cursor_idx = cursor_idx.saturating_sub(1),
                        KeyCode::Right => {
                            if cursor_idx < chars.len() { cursor_idx += 1; }
                        }
                        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if cursor_idx < chars.len() { cursor_idx += 1; }
                        }
                        KeyCode::Backspace => {
                            if cursor_idx > 0 {
                                cursor_idx -= 1;
                                chars.remove(cursor_idx);
                            }
                        }
                        KeyCode::Delete => {
                            if cursor_idx < chars.len() {
                                chars.remove(cursor_idx);
                            }
                        }
                        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if cursor_idx < chars.len() {
                                chars.remove(cursor_idx);
                            }
                        }
                        KeyCode::Char(c) if !c.is_control() => {
                            chars.insert(cursor_idx, c);
                            cursor_idx += 1;
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    if mouse.kind == event::MouseEventKind::Down(event::MouseButton::Left) {
                        let (cols, rows) = terminal::size()?;

                        // check if bottom two rows were clicked
                        if mouse.row >= rows - 2 {
                            let is_top = mouse.row == rows - 2;

                            // spellcheck mouse menu logic
                            if self.menu_state == MenuState::SpellCheck {
                                let col_width = (cols as usize) / 5;
                                let col_idx = (mouse.column as usize) / col_width;

                                if is_top {
                                    // click a suggestion (1-5)
                                    if col_idx < 5 && col_idx < self.current_suggestions.len() {
                                        self.clear_status();
                                        self.menu_state = original_menu;
                                        return Ok(Some((col_idx + 1).to_string()));
                                    }
                                } else {
                                    match col_idx {
                                        0 => { // ^C Cancel
                                            self.set_status(String::from("Cancelled."));
                                            self.menu_state = original_menu;
                                            return Ok(None);
                                        }
                                        1 => { // I Ignore
                                            self.clear_status();
                                            self.menu_state = original_menu;
                                            return Ok(Some(String::from("i")));
                                        }
                                        2 => { // A Add to Dict
                                            self.clear_status();
                                            self.menu_state = original_menu;
                                            return Ok(Some(String::from("a")));
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            // standard prompt menu mouse logic
                            else {
                                let col_width = (cols as usize) / 6;
                                let col_idx = (mouse.column as usize) / col_width;

                                if col_idx == 0 {
                                    if is_top && allow_browser {
                                        // trigger File Browser (^T)
                                        if let Some(selected_path) = self.run_file_browser(is_save_prompt)? {
                                            self.clear_status();
                                            self.menu_state = original_menu;
                                            return Ok(Some(selected_path));
                                        }
                                        self.menu_state = MenuState::PromptWithBrowser;
                                    } else if !is_top {
                                        // Trigger Cancel (^C)
                                        self.set_status(String::from("Cancelled."));
                                        self.menu_state = original_menu;
                                        return Ok(None);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }


    fn prompt_yn(&mut self, prompt_text: &str) -> io::Result<Option<bool>> {
        let original_menu = self.menu_state; // Capture state to prevent jump-back bugs
        self.menu_state = MenuState::YesNoCancel;
        self.status_time = None;
        let mut result = None;

        loop {
            self.status_message = prompt_text.to_string();
            self.draw_screen()?;

            let (_, rows) = terminal::size()?;
            let mut stdout = io::stdout();
            let cursor_x = self.status_message.len();

            queue!(stdout, cursor::MoveTo(cursor_x as u16, rows - 3))?;
            stdout.flush()?;

            let ev = event::read()?;
            match ev {
                Event::Key(key) => {
                    if key.kind != event::KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                            self.clear_status();
                            result = Some(true);
                            break;
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            self.clear_status();
                            result = Some(false);
                            break;
                        }
                        KeyCode::Esc => {
                            self.set_status(String::from("Cancelled"));
                            break;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            self.set_status(String::from("Cancelled"));
                            break;
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    if mouse.kind == event::MouseEventKind::Down(event::MouseButton::Left) {
                        let (cols, rows) = terminal::size()?;
                        if mouse.row >= rows - 2 {
                            let is_top = mouse.row == rows - 2;
                            let col_width = (cols as usize) / 6;
                            let col_idx = (mouse.column as usize) / col_width;

                            if is_top {
                                if col_idx == 0 { // Y Yes
                                    self.clear_status();
                                    result = Some(true);
                                    break;
                                }
                            } else {
                                match col_idx {
                                    0 => { // N No
                                        self.clear_status();
                                        result = Some(false);
                                        break;
                                    }
                                    1 => { // ^C Cancel
                                        self.set_status(String::from("Cancelled"));
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        self.menu_state = original_menu; // Restore original state
        Ok(result)
    }


    fn prompt_replace(&mut self, prompt_text: &str) -> io::Result<Option<char>> {
        let original_menu = self.menu_state;
        self.menu_state = MenuState::ReplaceAction;
        self.status_time = None;
        let mut result = None;

        loop {
            self.status_message = prompt_text.to_string();
            self.draw_screen()?;

            let (_, rows) = terminal::size()?;
            let mut stdout = stdout();

            let cursor_x = self.status_message.len();
            queue!(stdout, cursor::MoveTo(cursor_x as u16, rows - 3))?;
            stdout.flush()?;

            let ev = event::read()?;
            match ev {
                Event::Key(key) => {
                    if key.kind != event::KeyEventKind::Press { continue; }
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => { self.clear_status(); result = Some('y'); break; }
                        KeyCode::Char('n') | KeyCode::Char('N') => { self.clear_status(); result = Some('n'); break; }
                        KeyCode::Char('a') | KeyCode::Char('A') => { self.clear_status(); result = Some('a'); break; }
                        KeyCode::Esc => { self.set_status(String::from("Cancelled")); break; }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => { self.set_status(String::from("Cancelled")); break; }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    if mouse.kind == event::MouseEventKind::Down(event::MouseButton::Left) {
                        let (cols, rows) = terminal::size()?;
                        if mouse.row >= rows - 2 {
                            let is_top = mouse.row == rows - 2;
                            let col_width = (cols as usize) / 6;
                            let col_idx = (mouse.column as usize) / col_width;

                            if is_top {
                                match col_idx {
                                    0 => { self.clear_status(); result = Some('y'); break; } // Y Yes
                                    1 => { self.clear_status(); result = Some('a'); break; } // A All
                                    _ => {}
                                }
                            } else {
                                match col_idx {
                                    0 => { self.clear_status(); result = Some('n'); break; } // N No
                                    1 => { self.set_status(String::from("Cancelled")); break; } // ^C Cancel
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        self.menu_state = MenuState::Menu1;
        self.menu_state = original_menu;
        Ok(result)
    }


}