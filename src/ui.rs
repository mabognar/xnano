use crate::editor::{Editor, MenuState};
use crate::config::ConfigExt;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    queue,
    style::{Color, Print, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::Style;
use std::io::{self, stdout, Write};
use std::env;
use std::fs;
use std::path::PathBuf;

pub trait UiExt {
    fn draw_menu_line(writer: &mut io::Stdout, row: u16, cols: u16, col_width: usize, items: &[(&str, &str)], ui_bg: Color, key_fg: Color, text_fg: Color) -> io::Result<()>;
    fn draw_screen(&mut self) -> io::Result<()>;
    fn prompt(&mut self, prompt_text: &str, allow_browser: bool) -> io::Result<Option<String>>;
    fn prompt_yn(&mut self, prompt_text: &str) -> io::Result<Option<bool>>;
    fn prompt_replace(&mut self, prompt_text: &str) -> io::Result<Option<char>>;
    fn run_file_browser(&mut self) -> io::Result<Option<String>>;
    fn show_help(&mut self) -> io::Result<()>;
    fn set_status(&mut self, message: String);
    fn clear_status(&mut self);
}

impl UiExt for Editor {
    fn draw_menu_line(
        writer: &mut io::Stdout,
        row: u16,
        cols: u16,
        col_width: usize,
        items: &[(&str, &str)],
        ui_bg: Color,
        key_fg: Color,
        text_fg: Color
    ) -> io::Result<()> {
        queue!(writer, cursor::MoveTo(0, row), SetBackgroundColor(ui_bg))?;
        let mut printed = 0;

        for (cmd, desc) in items.iter() {
            let cmd_chars = cmd.chars().count();
            let desc_chars = desc.chars().count();
            let total_chars = cmd_chars + desc_chars;

            if total_chars <= col_width {
                let padding = " ".repeat(col_width - total_chars);
                queue!(
                    writer,
                    SetForegroundColor(key_fg), Print(cmd),
                    SetForegroundColor(text_fg), Print(format!("{}{}", desc, padding))
                )?;
            } else {
                let max_desc = col_width.saturating_sub(cmd_chars);
                let truncated_desc: String = desc.chars().take(max_desc).collect();
                queue!(
                    writer,
                    SetForegroundColor(key_fg), Print(cmd),
                    SetForegroundColor(text_fg), Print(truncated_desc)
                )?;
            }
            printed += col_width;
        }

        let end_pad = " ".repeat((cols as usize).saturating_sub(printed));
        queue!(writer, Print(end_pad), SetBackgroundColor(Color::Reset))?;
        Ok(())
    }

    fn draw_screen(&mut self) -> io::Result<()> {
        let mut stdout = stdout();

        let (cols, rows) = terminal::size()?;
        let visible_rows = rows.saturating_sub(4) as usize;

        let theme = &self.theme_set.themes[&self.current_theme];
        let is_dark = Self::is_dark_theme(theme);

        let raw_theme_bg = theme.settings.background.unwrap_or(syntect::highlighting::Color { r: 0, g: 0, b: 0, a: 255 });
        let ui_bg = Self::derive_ui_color(raw_theme_bg, is_dark);

        let title_fg = if is_dark { Color::Reset } else { Color::Rgb { r: 0, g: 50, b: 150 } };
        let menu_key_fg = if is_dark { Color::Rgb { r: 0, g: 150, b: 200 } } else { Color::Rgb { r: 0, g: 100, b: 200 } };
        let menu_text_fg = if is_dark { Color::Reset } else { Color::Black };

        let dollar_bg = if is_dark { Color::Rgb { r: 180, g: 180, b: 180 } } else { Color::Rgb { r: 80, g: 80, b: 80 } };
        let dollar_fg = if is_dark { Color::Black } else { Color::White };

        queue!(stdout, cursor::MoveTo(0, 0), SetBackgroundColor(ui_bg))?;

        let title = "   xnano";

        let file_display_string = match self.filename.as_deref() {
            Some(name) => {
                let path = std::path::Path::new(name);
                if path.is_absolute() {
                    name.to_string()
                } else if let Ok(cwd) = std::env::current_dir() {
                    let full_path = cwd.join(path);
                    // Canonicalize resolves '..' and symlinks if the file exists on disk.
                    // If the file is new and doesn't exist yet, fallback to the basic joined path.
                    std::fs::canonicalize(&full_path)
                        .unwrap_or(full_path)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    name.to_string()
                }
            }
            None => String::from("New Buffer"),
        };

        // Format the spacing and the filename independently
        let file_section = format!("     {}", file_display_string);

        let right_indicator_len = if self.is_modified { "[ Modified ]   ".len() } else { 0 };
        let max_allowable_len = (cols as usize).saturating_sub(right_indicator_len);
        let full_len = title.chars().count() + file_section.chars().count();

        // Safeguard: Truncate only the file path side if it overflows
        let mut final_file_section = file_section.clone();
        if full_len > max_allowable_len {
            let allowed_file_len = max_allowable_len.saturating_sub(title.chars().count());
            if allowed_file_len > 3 {
                final_file_section = file_section.chars().take(allowed_file_len.saturating_sub(3)).collect();
                final_file_section.push_str("...");
            } else {
                final_file_section = String::new();
            }
        }

        let printed_left_len = title.chars().count() + final_file_section.chars().count();

        if self.is_modified {
            let right = "[ Modified ]   ";
            let pad2_len = (cols as usize).saturating_sub(printed_left_len + right.len());
            let pad2 = " ".repeat(pad2_len);

            queue!(
                stdout,
                SetForegroundColor(menu_key_fg), // Color "xnano"
                Print(title),
                SetForegroundColor(title_fg),    // Color filename
                Print(&final_file_section),
                Print(pad2),
                SetForegroundColor(title_fg),
                Print(right),
                SetForegroundColor(Color::Reset),
                SetBackgroundColor(Color::Reset)
            )?;
        } else {
            let pad2_len = (cols as usize).saturating_sub(printed_left_len);
            let pad2 = " ".repeat(pad2_len);
            queue!(
                stdout,
                SetForegroundColor(menu_key_fg), // Color "xnano"
                Print(title),
                SetForegroundColor(title_fg),    // Color filename
                Print(&final_file_section),
                Print(pad2),
                SetForegroundColor(Color::Reset),
                SetBackgroundColor(Color::Reset)
            )?;
        }

        let syntax = if let Some(ref name) = self.filename {
            let path = std::path::Path::new(name);
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                self.syntax_set.find_syntax_by_extension(ext).unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
            } else {
                self.syntax_set.find_syntax_plain_text()
            }
        } else {
            self.syntax_set.find_syntax_plain_text()
        };

        let theme_bg_raw = theme.settings.background.unwrap_or(syntect::highlighting::Color { r: 0, g: 0, b: 0, a: 255 });
        let default_cross_bg = Color::Rgb { r: theme_bg_raw.r, g: theme_bg_raw.g, b: theme_bg_raw.b };

        let max_line_num_len = self.buffer.len_lines().to_string().len();
        let gutter_width = if self.show_line_numbers { max_line_num_len + 1 } else { 0 };
        let available_width = std::cmp::max(1, (cols as usize).saturating_sub(gutter_width));

        let cursor_absolute = self.get_cursor_char_idx();
        let mark_range = self.mark.map(|m| {
            if m < cursor_absolute { (m, cursor_absolute) } else { (cursor_absolute, m) }
        });

        // --- FIX 1: ANSI state tracking and Hoisted Parser ---
        let mut last_fg: Option<Color> = None;
        let mut last_bg: Option<Color> = None;
        let mut fallback_highlighter = None;

        let mut terminal_y = 0;
        let mut file_y = self.row_offset;

        while terminal_y < visible_rows {
            if file_y < self.buffer.len_lines() {
                // Instantiating HighlightLines is expensive. Do ONCE per frame
                // and reuse it for any uncached lines we encounter.
                if !self.highlight_cache.contains_key(&file_y) {
                    if fallback_highlighter.is_none() {
                        fallback_highlighter = Some(HighlightLines::new(syntax, theme));
                    }
                    let line_str = self.buffer.line(file_y).to_string();
                    let ranges = fallback_highlighter.as_mut().unwrap().highlight_line(&line_str, &self.syntax_set).unwrap();
                    let owned_ranges: Vec<(Style, String)> = ranges.into_iter().map(|(s, t)| (s, t.to_string())).collect();
                    self.highlight_cache.insert(file_y, owned_ranges);
                }

                let ranges = self.highlight_cache.get(&file_y).unwrap();

                let mut visual_x = 0;
                let mut line_char_idx = 0;
                let line_has_search_highlight = self.highlight_match.map_or(false, |(h_y, _, _)| h_y == file_y);

                queue!(stdout, cursor::MoveTo(0, (terminal_y + 1) as u16))?;
                if self.show_line_numbers {
                    let num_str = format!("{:>width$} ", file_y + 1, width = max_line_num_len);
                    if last_bg != Some(default_cross_bg) { queue!(stdout, SetBackgroundColor(default_cross_bg))?; last_bg = Some(default_cross_bg); }
                    if last_fg != Some(menu_key_fg) { queue!(stdout, SetForegroundColor(menu_key_fg))?; last_fg = Some(menu_key_fg); }
                    queue!(stdout, Print(num_str))?;
                }

                let mut printed_on_current_line = 0;

                'char_loop: for (style, text) in ranges {
                    let syn_color = style.foreground;
                    let cross_color = Color::Rgb { r: syn_color.r, g: syn_color.g, b: syn_color.b };
                    let syn_bg = style.background;
                    let cross_bg = Color::Rgb { r: syn_bg.r, g: syn_bg.g, b: syn_bg.b };

                    // --- FIX 2: Only issue ANSI codes if the color actually changes ---
                    if last_fg != Some(cross_color) {
                        queue!(stdout, SetForegroundColor(cross_color))?;
                        last_fg = Some(cross_color);
                    }
                    if last_bg != Some(cross_bg) {
                        queue!(stdout, SetBackgroundColor(cross_bg))?;
                        last_bg = Some(cross_bg);
                    }

                    for ch in text.chars() {
                        if ch == '\n' || ch == '\r' {
                            line_char_idx += 1;
                            continue;
                        }

                        let absolute_char_idx = self.buffer.line_to_char(file_y) + line_char_idx;

                        let is_highlighted = if line_has_search_highlight {
                            if let Some((_, h_start, h_end)) = self.highlight_match {
                                line_char_idx >= h_start && line_char_idx < h_end
                            } else { false }
                        } else if let Some((m_start, m_end)) = mark_range {
                            absolute_char_idx >= m_start && absolute_char_idx < m_end
                        } else {
                            false
                        };

                        let display_chars = if ch == '\t' { vec![' '; 4 - (visual_x % 4)] } else { vec![ch] };

                        for display_ch in display_chars {
                            if self.soft_wrap {
                                if printed_on_current_line >= available_width {
                                    if last_bg != Some(default_cross_bg) { queue!(stdout, SetBackgroundColor(default_cross_bg))?; last_bg = Some(default_cross_bg); }
                                    queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
                                    terminal_y += 1;
                                    if terminal_y >= visible_rows { break 'char_loop; }

                                    queue!(stdout, cursor::MoveTo(0, (terminal_y + 1) as u16))?;
                                    if self.show_line_numbers {
                                        queue!(stdout, Print(" ".repeat(gutter_width)))?;
                                    }
                                    // Re-apply styles after clearing the line
                                    if last_fg != Some(cross_color) { queue!(stdout, SetForegroundColor(cross_color))?; last_fg = Some(cross_color); }
                                    if last_bg != Some(cross_bg) { queue!(stdout, SetBackgroundColor(cross_bg))?; last_bg = Some(cross_bg); }
                                    printed_on_current_line = 0;
                                }

                                if is_highlighted {
                                    if last_bg != Some(Color::Red) { queue!(stdout, SetBackgroundColor(Color::Red))?; last_bg = Some(Color::Red); }
                                    if last_fg != Some(Color::White) { queue!(stdout, SetForegroundColor(Color::White))?; last_fg = Some(Color::White); }
                                }
                                queue!(stdout, Print(display_ch))?;
                                if is_highlighted {
                                    // Revert immediately back to the current token's syntax color
                                    if last_bg != Some(cross_bg) { queue!(stdout, SetBackgroundColor(cross_bg))?; last_bg = Some(cross_bg); }
                                    if last_fg != Some(cross_color) { queue!(stdout, SetForegroundColor(cross_color))?; last_fg = Some(cross_color); }
                                }

                                printed_on_current_line += 1;
                                visual_x += 1;
                            } else {
                                if visual_x >= self.col_offset && printed_on_current_line < available_width {
                                    if is_highlighted {
                                        if last_bg != Some(Color::Red) { queue!(stdout, SetBackgroundColor(Color::Red))?; last_bg = Some(Color::Red); }
                                        if last_fg != Some(Color::White) { queue!(stdout, SetForegroundColor(Color::White))?; last_fg = Some(Color::White); }
                                    }
                                    queue!(stdout, Print(display_ch))?;
                                    if is_highlighted {
                                        if last_bg != Some(cross_bg) { queue!(stdout, SetBackgroundColor(cross_bg))?; last_bg = Some(cross_bg); }
                                        if last_fg != Some(cross_color) { queue!(stdout, SetForegroundColor(cross_color))?; last_fg = Some(cross_color); }
                                    }
                                    printed_on_current_line += 1;
                                }
                                visual_x += 1;
                            }
                        }
                        line_char_idx += 1;
                    }
                }

                if last_bg != Some(default_cross_bg) { queue!(stdout, SetBackgroundColor(default_cross_bg))?; last_bg = Some(default_cross_bg); }
                queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;

                if !self.soft_wrap {
                    let needs_left_dollar = self.col_offset > 0;
                    let needs_right_dollar = visual_x > self.col_offset + available_width;

                    if needs_left_dollar {
                        if last_bg != Some(dollar_bg) { queue!(stdout, SetBackgroundColor(dollar_bg))?; last_bg = Some(dollar_bg); }
                        if last_fg != Some(dollar_fg) { queue!(stdout, SetForegroundColor(dollar_fg))?; last_fg = Some(dollar_fg); }
                        queue!(stdout, cursor::MoveTo(gutter_width as u16, (terminal_y + 1) as u16), Print('$'))?;
                    }
                    if needs_right_dollar {
                        if last_bg != Some(dollar_bg) { queue!(stdout, SetBackgroundColor(dollar_bg))?; last_bg = Some(dollar_bg); }
                        if last_fg != Some(dollar_fg) { queue!(stdout, SetForegroundColor(dollar_fg))?; last_fg = Some(dollar_fg); }
                        queue!(stdout, cursor::MoveTo((cols - 1) as u16, (terminal_y + 1) as u16), Print('$'))?;
                    }
                }

                // Reset before moving to the next line
                if last_bg != Some(default_cross_bg) { queue!(stdout, SetBackgroundColor(default_cross_bg))?; last_bg = Some(default_cross_bg); }
                if last_fg != Some(Color::Reset) { queue!(stdout, SetForegroundColor(Color::Reset))?; last_fg = Some(Color::Reset); }

            } else {
                queue!(stdout, cursor::MoveTo(0, (terminal_y + 1) as u16))?;
                if self.show_line_numbers {
                    if last_bg != Some(default_cross_bg) { queue!(stdout, SetBackgroundColor(default_cross_bg))?; last_bg = Some(default_cross_bg); }
                    queue!(stdout, Print(" ".repeat(gutter_width)))?;
                }
                if last_bg != Some(default_cross_bg) { queue!(stdout, SetBackgroundColor(default_cross_bg))?; last_bg = Some(default_cross_bg); }
                queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
            }

            terminal_y += 1;
            file_y += 1;
        }

        queue!(stdout, cursor::MoveTo(0, rows - 3))?;

        if !self.status_message.is_empty() {
            queue!(
                stdout,
                SetBackgroundColor(ui_bg),
                SetForegroundColor(title_fg)
            )?;

            let mut printed_len = 0;

            if self.menu_state == MenuState::SpellCheck {
                if !self.current_suggestions.is_empty() {
                    for (i, sug) in self.current_suggestions.iter().enumerate() {
                        let num_str = format!("{}", i + 1);
                        queue!(
                            stdout,
                            SetForegroundColor(menu_key_fg),
                            Print(&num_str),
                            SetForegroundColor(title_fg),
                            Print(format!(" {}   ", sug))
                        )?;
                        printed_len += num_str.len() + 1 + sug.len() + 3;
                    }
                } else {
                    queue!(stdout, Print("No suggestions   "))?;
                    printed_len += "No suggestions   ".len();
                }
            }

            let status_text = format!("{}", self.status_message);
            queue!(stdout, Print(&status_text))?;
            printed_len += status_text.len();

            let status_fill = " ".repeat((cols as usize).saturating_sub(printed_len));
            queue!(
                stdout,
                Print(status_fill),
                SetBackgroundColor(Color::Reset),
                SetForegroundColor(Color::Reset)
            )?;
        } else {
            queue!(stdout, SetBackgroundColor(default_cross_bg), terminal::Clear(ClearType::CurrentLine))?;
        }

        let col_width = (cols as usize) / 6;

        match self.menu_state {
            MenuState::Default => {
                let menu1 = [
                    ("^G", " Get Help"), ("^O", " Write Out"), ("^R", " Read File"),
                    ("^Y", " Prev Pg"), ("^K", " Cut Txt"), ("^C", " Cur Pos")
                ];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

                let u_label = if self.is_justified { " Unjustify" } else { " UnCut Txt" };

                let menu2 = [
                    ("^X", " Exit"), ("^J", " Justify"), ("^W", " Where Is"),
                    ("^V", " Next Pg"), ("^U", u_label), ("^T", " To Spell")
                ];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
            MenuState::YesNoCancel => {
                let menu1 = [(" Y", " Yes")];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

                let menu2 = [(" N", " No"), ("^C", " Cancel")];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
            MenuState::ReplaceAction => {
                let menu1 = [(" Y", " Yes"), (" A", " All")];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

                let menu2 = [(" N", " No"), ("^C", " Cancel")];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
            MenuState::CancelOnly => {
                let menu1 = [];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

                let menu2 = [("^C", " Cancel")];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
            MenuState::PromptWithBrowser => {
                let menu1 = [("^T", " To Files")];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

                let menu2 = [("^C", " Cancel")];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
            MenuState::SpellCheck => {
                let menu1 = [("i", "gnore"), ("a", "dd to dict")];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

                let menu2 = [("^C", " Cancel")];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
        }

        let cursor_screen_y;
        let cursor_screen_x;

        if self.soft_wrap {
            let mut temp_screen_y = 0;
            for i in self.row_offset..self.cursor_y {
                let w = self.get_visual_line_width(i);
                temp_screen_y += if w == 0 { 1 } else { (w - 1) / available_width + 1 };
            }
            let cursor_visual = self.get_visual_cursor_x();
            temp_screen_y += cursor_visual / available_width;
            cursor_screen_x = gutter_width + (cursor_visual % available_width);
            cursor_screen_y = temp_screen_y;
        } else {
            cursor_screen_y = self.cursor_y.saturating_sub(self.row_offset);
            cursor_screen_x = gutter_width + self.get_visual_cursor_x().saturating_sub(self.col_offset);
        }

        let safe_screen_y = cursor_screen_y.min(visible_rows.saturating_sub(1)) + 1;
        let safe_screen_x = cursor_screen_x.min((cols as usize).saturating_sub(1));

        queue!(stdout, cursor::MoveTo(safe_screen_x as u16, safe_screen_y as u16))?;
        stdout.flush()?;
        Ok(())
    }

    fn prompt(&mut self, prompt_text: &str, allow_browser: bool) -> io::Result<Option<String>> {
        if self.menu_state == MenuState::Default {
            self.menu_state = if allow_browser { MenuState::PromptWithBrowser } else { MenuState::CancelOnly };
        }

        self.status_time = None;
        let mut input = String::new();

        loop {
            self.status_message = format!("{}{}", prompt_text, input);
            self.draw_screen()?;

            let (_, rows) = terminal::size()?;
            let mut stdout = stdout();

            let mut cursor_x = prompt_text.len() + input.len();

            if self.menu_state == MenuState::SpellCheck {
                if !self.current_suggestions.is_empty() {
                    for (i, sug) in self.current_suggestions.iter().enumerate() {
                        let num_str = format!("{}", i + 1);
                        cursor_x += num_str.len() + 1 + sug.len() + 3;
                    }
                } else {
                    cursor_x += "No suggestions   ".len();
                }
            }

            queue!(stdout, cursor::MoveTo(cursor_x as u16, rows - 3))?;
            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter => {
                        self.clear_status();
                        self.menu_state = MenuState::Default;
                        return Ok(Some(input));
                    }
                    KeyCode::Esc => {
                        self.set_status(String::from("Cancelled."));
                        self.menu_state = MenuState::Default;
                        return Ok(None);
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.set_status(String::from("Cancelled."));
                        self.menu_state = MenuState::Default;
                        return Ok(None);
                    }
                    KeyCode::Char('t') if allow_browser && key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(selected_path) = self.run_file_browser()? {
                            self.clear_status();
                            self.menu_state = MenuState::Default;
                            return Ok(Some(selected_path));
                        }
                        self.menu_state = MenuState::PromptWithBrowser;
                    }
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    KeyCode::Char(c) => {
                        if !c.is_control() {
                            input.push(c);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn prompt_yn(&mut self, prompt_text: &str) -> io::Result<Option<bool>> {
        self.menu_state = MenuState::YesNoCancel;
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

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
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
        }

        self.menu_state = MenuState::Default;
        Ok(result)
    }

    fn prompt_replace(&mut self, prompt_text: &str) -> io::Result<Option<char>> {
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

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.clear_status();
                        result = Some('y');
                        break;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') => {
                        self.clear_status();
                        result = Some('n');
                        break;
                    }
                    KeyCode::Char('a') | KeyCode::Char('A') => {
                        self.clear_status();
                        result = Some('a');
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
        }

        self.menu_state = MenuState::Default;
        Ok(result)
    }

    fn run_file_browser(&mut self) -> io::Result<Option<String>> {
        let mut current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        if let Ok(canon) = current_dir.canonicalize() {
            current_dir = canon;
        }
        let mut selected = 0;
        let mut scroll = 0;

        loop {
            let mut entries: Vec<(String, bool)> = Vec::new();

            entries.push((String::from("."), true));

            if current_dir.parent().is_some() {
                entries.push((String::from(".."), true));
            }

            if let Ok(read_dir) = fs::read_dir(&current_dir) {
                let mut dirs = Vec::new();
                let mut dot_dirs = Vec::new();
                let mut files = Vec::new();
                let mut dot_files = Vec::new();

                for entry in read_dir.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let is_dir = path.is_dir();
                    let is_dot = name.starts_with('.');

                    if is_dir {
                        if is_dot { dot_dirs.push((name, true)); }
                        else { dirs.push((name, true)); }
                    } else {
                        if is_dot { dot_files.push((name, false)); }
                        else { files.push((name, false)); }
                    }
                }

                dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                dot_dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                dot_files.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

                entries.extend(dirs);
                entries.extend(files);
                entries.extend(dot_dirs);
                entries.extend(dot_files);
            }

            if selected >= entries.len() {
                selected = entries.len().saturating_sub(1);
            }

            loop {
                let mut stdout = stdout();
                let (cols, rows) = terminal::size()?;
                let visible_rows = rows.saturating_sub(4) as usize;

                if selected < scroll { scroll = selected; }
                if selected >= scroll + visible_rows { scroll = selected - visible_rows + 1; }

                let theme = &self.theme_set.themes[&self.current_theme];
                let is_dark = Self::is_dark_theme(theme);
                let theme_bg_raw = theme.settings.background.unwrap_or(syntect::highlighting::Color { r: 0, g: 0, b: 0, a: 255 });
                let default_cross_bg = Color::Rgb { r: theme_bg_raw.r, g: theme_bg_raw.g, b: theme_bg_raw.b };
                let default_cross_fg = if is_dark { Color::White } else { Color::Black };

                let ui_bg = Self::derive_ui_color(theme_bg_raw, is_dark);
                let title_fg = if is_dark { Color::Reset } else { Color::Rgb { r: 0, g: 50, b: 150 } };
                let menu_key_fg = if is_dark { Color::Rgb { r: 0, g: 150, b: 200 } } else { Color::Rgb { r: 0, g: 100, b: 200 } };

                queue!(stdout, SetBackgroundColor(default_cross_bg), terminal::Clear(ClearType::All))?;

                queue!(stdout, cursor::MoveTo(0, 0), SetBackgroundColor(ui_bg))?;
                let title = " xnano File Browser ";
                let path_str = current_dir.to_string_lossy();
                let center_start = (cols as usize).saturating_sub(path_str.len()) / 2;
                let pad1_len = center_start.saturating_sub(title.len());
                let pad1 = " ".repeat(pad1_len);

                let combined_len = title.len() + pad1.len() + path_str.len();
                let pad2_len = (cols as usize).saturating_sub(combined_len);
                let pad2 = " ".repeat(pad2_len);

                queue!(
                    stdout,
                    SetForegroundColor(menu_key_fg),
                    Print(title),
                    SetForegroundColor(title_fg),
                    Print(format!("{}{}{}", pad1, path_str, pad2))
                )?;

                for i in 0..visible_rows {
                    queue!(stdout, cursor::MoveTo(0, (i + 1) as u16))?;
                    let idx = scroll + i;

                    if idx < entries.len() {
                        let (name, is_dir) = &entries[idx];
                        let is_selected = idx == selected;

                        let display_name = if *is_dir { format!("(dir)  {}", name) } else { format!("       {}", name) };
                        let mut truncated = display_name;
                        if truncated.len() > cols as usize {
                            truncated.truncate(cols as usize);
                        }
                        let padding = " ".repeat((cols as usize).saturating_sub(truncated.len()));

                        if is_selected {
                            queue!(stdout, SetBackgroundColor( Color::Rgb { r: 0, g: 150, b: 200} ), SetForegroundColor(Color::White))?;
                        } else {
                            queue!(stdout, SetBackgroundColor(default_cross_bg), SetForegroundColor(default_cross_fg))?;
                        }

                        queue!(stdout, Print(format!("{}{}", truncated, padding)))?;
                    } else {
                        queue!(stdout, SetBackgroundColor(default_cross_bg), terminal::Clear(ClearType::UntilNewLine))?;
                    }
                }

                let menu_text_fg = if is_dark { Color::Reset } else { Color::Black };
                let col_width = (cols as usize) / 6;

                let menu1 = [("", ""), ("^Y", " Prev Pg")];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

                let menu2 = [("^C", " Cancel"), ("^V", " Next Pg"), ("Enter", " Select")];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;

                stdout.flush()?;

                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Esc => return Ok(None),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Ok(None),

                        KeyCode::Up => {
                            selected = selected.saturating_sub(1);
                        }
                        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            selected = selected.saturating_sub(1);
                        }
                        KeyCode::Down => {
                            if selected + 1 < entries.len() {
                                selected += 1;
                            }
                        }
                        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if selected + 1 < entries.len() {
                                selected += 1;
                            }
                        }
                        KeyCode::PageUp | KeyCode::F(7) => {
                            selected = selected.saturating_sub(visible_rows);
                        }
                        KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            selected = selected.saturating_sub(visible_rows);
                        }
                        KeyCode::PageDown | KeyCode::F(8) => {
                            let max_offset = entries.len().saturating_sub(1);
                            selected = (selected + visible_rows).min(max_offset);
                        }
                        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            let max_offset = entries.len().saturating_sub(1);
                            selected = (selected + visible_rows).min(max_offset);
                        }

                        KeyCode::Enter => {
                            let (name, is_dir) = &entries[selected];
                            if *is_dir {
                                if name == "." {
                                    let mut input = String::new();
                                    let prompt_prefix = " File name to write: ";

                                    loop {
                                        queue!(stdout, cursor::MoveTo(0, rows - 3), SetBackgroundColor(ui_bg), SetForegroundColor(title_fg))?;

                                        let prompt_str = format!("{}{}", prompt_prefix, input);
                                        queue!(stdout, Print(&prompt_str), terminal::Clear(ClearType::UntilNewLine))?;

                                        let cursor_x = prompt_prefix.len() + input.len();
                                        queue!(stdout, cursor::MoveTo(cursor_x as u16, rows - 3))?;

                                        stdout.flush()?;

                                        if let Event::Key(k) = event::read()? {
                                            match k.code {
                                                KeyCode::Enter => {
                                                    if !input.is_empty() {
                                                        let target = current_dir.join(&input);
                                                        return Ok(Some(target.to_string_lossy().into_owned()));
                                                    }
                                                    break;
                                                }
                                                KeyCode::Esc => break,
                                                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                                                KeyCode::Backspace => { input.pop(); }
                                                KeyCode::Char(c) if !c.is_control() => { input.push(c); }
                                                _ => {}
                                            }
                                        }
                                    }
                                    continue;
                                } else if name == ".." {
                                    if let Some(parent) = current_dir.parent() {
                                        current_dir = parent.to_path_buf();
                                    }
                                } else {
                                    current_dir = current_dir.join(name);
                                    if let Ok(canon) = current_dir.canonicalize() {
                                        current_dir = canon;
                                    }
                                }
                                selected = 0;
                                scroll = 0;
                                break;
                            } else {
                                let target = current_dir.join(name);
                                return Ok(Some(target.to_string_lossy().into_owned()));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn show_help(&mut self) -> io::Result<()> {
        let help_lines = [
            "  xnano is a text editor inspired by nano",
            "  ---------------------------------------",
            "  * Features: ",
            "     - Written entirely in Rust",
            "     - Fast",
            "     - Themes, syntax highlighting",
            "     - Spell checker",
            "     - Soft line-wrap & line numbers",
            "  * Themes & Configuration: ",
            "     - To cycle through the included themes, type Meta+T (ALT+T,",
            "       Option+T) when in editor",
            "     - On MacOS, make sure you have 'Use Option as Meta' selected ",
            "       in your terminal settings",
            "     - Theme, line numbers, soft wrap are persistent",
            "     - Settings are stored in ~/.xnano/xnanorc",
            "     - Themes are stored in ~/.xnano/themes",
            "     - Additional .tmTheme themes can be added to ~/.xnano/themes",
            "",
            "  Movement:",
            "    Ctrl+P, Up       Move up one line",
            "    Ctrl+N, Down     Move down one line",
            "    Ctrl+B, Left     Move left one character",
            "    Ctrl+F, Right    Move right one character",
            "    Ctrl+A           Move to start of line",
            "    Ctrl+E           Move to end of line",
            "    Ctrl+Y, F7, PgUp Move up one page",
            "    Ctrl+V, F8, PgDn Move down one page",
            "",
            "  Editing:",
            "    Ctrl+K, F9       Cut current line into clipboard",
            "    Ctrl+U, F10      Paste contents of clipboard",
            "    Ctrl+D, Del      Delete character under cursor",
            "    Backspace        Delete character before cursor",
            "    Ctrl+J, F4       Justify current paragraph",
            "    Ctrl+I, Tab      Insert tab",
            "    Ctrl+^, Meta+A   Mark beginning of selected text.",
            "                     This key also unselects text.",
            "                     Note: Ctrl+^ = Ctrl+Shift+6",
            "",
            "  Search & Replace:",
            "    Ctrl+W, F6       Where is (Search)",
            "    Ctrl+\\           Search and Replace",
            "",
            "  File & System:",
            "    Ctrl+O, F3       Write Out (Save)",
            "    Ctrl+R, F5       Read File (Insert)",
            "    Ctrl+G, F1       Get Help (this screen)",
            "    Ctrl+X, F2       Exit xnano",
            "",
            "  Tools:",
            "    Ctrl+C, F11      Current Position",
            "    Ctrl+T, F12      To Spell (Spell check)",
            "    Ctrl+L           Go to line number",
            "    Meta+T           Cycle Syntax Theme",
            "    Meta+L           Toggle Line Numbers",
            "    Meta+S           Toggle Soft Wrap",
            " ",
            "  Written by: Matt Bognar, https://github.com/mabognar",
            " ",
        ];

        let mut scroll_offset = 0;

        let theme = &self.theme_set.themes[&self.current_theme];
        let bg = theme.settings.background.unwrap_or(syntect::highlighting::Color { r: 0, g: 0, b: 0, a: 255 });
        let fg = theme.settings.foreground.unwrap_or(syntect::highlighting::Color { r: 255, g: 255, b: 255, a: 255 });

        let theme_bg = Color::Rgb { r: bg.r, g: bg.g, b: bg.b };
        let theme_fg = Color::Rgb { r: fg.r, g: fg.g, b: fg.b };

        let is_dark = Self::is_dark_theme(theme);
        let ui_bg = Self::derive_ui_color(bg, is_dark);
        let menu_key_fg = if is_dark { Color::Rgb { r: 0, g: 150, b: 200 } } else { Color::Rgb { r: 0, g: 100, b: 200 } };
        let menu_text_fg = if is_dark { Color::Reset } else { Color::Black };

        loop {
            let mut stdout = stdout();
            let (cols, rows) = terminal::size()?;
            let visible_rows = rows.saturating_sub(3) as usize;

            queue!(stdout, SetBackgroundColor(theme_bg), terminal::Clear(ClearType::All))?;

            queue!(stdout, cursor::MoveTo(0, 0),
                SetBackgroundColor(ui_bg), SetForegroundColor( Color::Rgb{r:0,g:150,b:200} ))?;

            let title = " xnano Help Viewer ";
            let pad_len = (cols as usize).saturating_sub(title.len()) / 2;
            let pad1 = " ".repeat(pad_len);
            let pad2 = " ".repeat((cols as usize).saturating_sub(title.len() + pad_len));

            queue!(stdout, Print(format!("{}{}{}", pad1, title, pad2)),
                SetBackgroundColor(theme_bg), SetForegroundColor(theme_fg))?;

            for i in 0..visible_rows {
                queue!(stdout, cursor::MoveTo(0, (i + 1) as u16))?;
                let line_idx = scroll_offset + i;
                if line_idx < help_lines.len() {
                    let line = help_lines[line_idx];
                    let truncated = if line.len() > cols as usize { &line[..(cols as usize)] } else { line };
                    queue!(stdout, Print(truncated))?;
                }

                queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
            }

            let col_width = (cols as usize) / 6;

            let menu1 = [("",""), ("^Y", " Prev Pg")];
            Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

            let menu2 = [("^X", " Exit Help"), ("^V", " Next Pg")];
            Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;

            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::F(2) => break,
                    KeyCode::Esc => break,

                    KeyCode::Up => {
                        scroll_offset = scroll_offset.saturating_sub(1);
                    }
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        scroll_offset = scroll_offset.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        if scroll_offset + visible_rows < help_lines.len() {
                            scroll_offset += 1;
                        }
                    }
                    KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if scroll_offset + visible_rows < help_lines.len() {
                            scroll_offset += 1;
                        }
                    }
                    KeyCode::PageUp | KeyCode::F(7) => {
                        scroll_offset = scroll_offset.saturating_sub(visible_rows);
                    }
                    KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        scroll_offset = scroll_offset.saturating_sub(visible_rows);
                    }
                    KeyCode::PageDown | KeyCode::F(8) => {
                        let max_offset = help_lines.len().saturating_sub(visible_rows);
                        scroll_offset = (scroll_offset + visible_rows).min(max_offset);
                    }
                    KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let max_offset = help_lines.len().saturating_sub(visible_rows);
                        scroll_offset = (scroll_offset + visible_rows).min(max_offset);
                    }
                    _ => {}
                }
            }
        }

        self.clear_status();
        self.draw_screen()?;
        Ok(())
    }

    fn set_status(&mut self, message: String) {
        self.status_message = message;
        self.status_time = Some(std::time::Instant::now());
    }

    fn clear_status(&mut self) {
        self.status_message.clear();
        self.status_time = None;
    }
}
