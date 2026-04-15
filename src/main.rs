// xnano - a text editor written in Rust inspired by nano
// Written by: Matt Bognar, https://github.com/mabognar
// Extended to include Soft-Wrap and Line Numbers

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{Color, Print, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use ropey::Rope;
use std::collections::{HashSet, HashMap};
use std::env;
use std::fs;
use std::fs::File;
use std::io::{self, stdout, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use include_dir::{include_dir, Dir};

static BUNDLED_THEMES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/themes");

#[derive(PartialEq)]
enum MenuState {
    Default,
    YesNoCancel,
    ReplaceAction,
    CancelOnly,
    PromptWithBrowser,
}

struct Editor {
    buffer: Rope,
    cursor_x: usize,
    cursor_y: usize,
    desired_cursor_x: usize,
    row_offset: usize,
    col_offset: usize,
    filename: Option<String>,
    should_quit: bool,
    status_message: String,
    clipboard: String,
    dictionary: Option<HashSet<String>>,
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    is_modified: bool,
    last_search: Option<String>,
    menu_state: MenuState,
    status_time: Option<Instant>,
    highlight_match: Option<(usize, usize, usize)>,
    highlight_cache: HashMap<usize, Vec<(Style, String)>>,
    current_theme: String,
    is_justified: bool,
    pre_justify_snapshot: Option<(Rope, usize, usize)>,

    // New persistent settings
    show_line_numbers: bool,
    soft_wrap: bool,
}

impl Editor {
    fn get_base_dir() -> Option<PathBuf> {
        let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).unwrap_or_default();
        if home.is_empty() {
            None
        } else {
            let path = Path::new(&home).join(".xnano");
            let _ = fs::create_dir_all(&path);
            Some(path)
        }
    }

    fn initialize_themes() -> io::Result<()> {
        if let Some(theme_dir) = Self::get_theme_dir() {
            if fs::read_dir(&theme_dir)?.next().is_none() {
                for file in BUNDLED_THEMES.files() {
                    let path = theme_dir.join(file.path());
                    fs::write(path, file.contents())?;
                }
            }
        }
        Ok(())
    }

    fn get_config_path() -> Option<PathBuf> {
        Self::get_base_dir().map(|p| p.join("xnanorc"))
    }

    fn get_theme_dir() -> Option<PathBuf> {
        Self::get_base_dir().map(|p| {
            let theme_path = p.join("themes");
            let _ = fs::create_dir_all(&theme_path);
            theme_path
        })
    }

    fn load_config() -> (String, bool, bool) {
        let mut theme = String::from("base16-ocean.dark");
        let mut line_numbers = false;
        let mut soft_wrap = false;

        if let Some(path) = Self::get_config_path() {
            if let Ok(contents) = fs::read_to_string(path) {
                for line in contents.lines() {
                    let parts: Vec<&str> = line.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        match parts[0].trim() {
                            "theme" => theme = parts[1].trim().to_string(),
                            "line_numbers" => line_numbers = parts[1].trim() == "true",
                            "soft_wrap" => soft_wrap = parts[1].trim() == "true",
                            _ => {}
                        }
                    } else if parts.len() == 1 && !line.trim().is_empty() {
                        // Backwards compatibility with plain theme string
                        if !line.contains('=') {
                            theme = line.trim().to_string();
                        }
                    }
                }
            }
        }
        (theme, line_numbers, soft_wrap)
    }

    fn save_config(&self) {
        if let Some(path) = Self::get_config_path() {
            let content = format!(
                "theme={}\nline_numbers={}\nsoft_wrap={}\n",
                self.current_theme, self.show_line_numbers, self.soft_wrap
            );
            let _ = fs::write(path, content);
        }
    }

    fn is_dark_theme(theme: &syntect::highlighting::Theme) -> bool {
        let bg = theme.settings.background.unwrap_or(syntect::highlighting::Color { r: 0, g: 0, b: 0, a: 255 });
        let luminance = 0.299 * (bg.r as f32) + 0.587 * (bg.g as f32) + 0.114 * (bg.b as f32);
        luminance < 128.0
    }

    fn derive_ui_color(bg: syntect::highlighting::Color, is_dark: bool) -> Color {
        let offset: i16 = if is_dark { 20 } else { -20 };
        let r = (bg.r as i16 + offset).clamp(0, 255) as u8;
        let g = (bg.g as i16 + offset).clamp(0, 255) as u8;
        let b = (bg.b as i16 + offset).clamp(0, 255) as u8;
        Color::Rgb { r, g, b }
    }

    fn new(filename: Option<String>) -> Self {
        let buffer = if let Some(ref fname) = filename {
            let expanded = Self::expand_tilde(fname);
            if let Ok(file) = File::open(&expanded) {
                Rope::from_reader(BufReader::new(file)).unwrap_or_default()
            } else {
                Rope::new()
            }
        } else {
            Rope::new()
        };

        let mut theme_set = ThemeSet::load_defaults();
        let mut themes_found = 0;
        let mut error_occurred = None;

        if let Some(theme_dir) = Self::get_theme_dir() {
            if let Ok(custom_themes) = ThemeSet::load_from_folder(&theme_dir) {
                themes_found += custom_themes.themes.len();
                theme_set.themes.extend(custom_themes.themes);
            }
        }

        match ThemeSet::load_from_folder("themes") {
            Ok(custom_themes) => {
                themes_found += custom_themes.themes.len();
                theme_set.themes.extend(custom_themes.themes);
            }
            Err(e) => {
                error_occurred = Some(format!("Local themes not found: {}", e));
            }
        }

        let initial_status = if themes_found > 0 {
            String::new()
        } else if let Some(err) = error_occurred {
            err
        } else {
            String::new()
        };

        let (mut starting_theme, line_numbers, soft_wrap) = Self::load_config();
        if !theme_set.themes.contains_key(&starting_theme) {
            starting_theme = String::from("base16-ocean.dark");
        }

        Self {
            buffer,
            cursor_x: 0,
            cursor_y: 0,
            desired_cursor_x: 0,
            row_offset: 0,
            col_offset: 0,
            filename,
            should_quit: false,
            status_message: initial_status,
            status_time: Some(Instant::now()),
            clipboard: String::new(),
            dictionary: None,
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set,
            is_modified: false,
            last_search: None,
            menu_state: MenuState::Default,
            highlight_match: None,
            highlight_cache: HashMap::new(),
            current_theme: starting_theme,
            is_justified: false,
            pre_justify_snapshot: None,
            show_line_numbers: line_numbers,
            soft_wrap,
        }
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

    fn clear_cache(&mut self) {
        self.highlight_cache.clear();
    }

    fn mark_modified(&mut self) {
        self.is_modified = true;
        self.clear_cache();
    }

    fn cycle_theme(&mut self) {
        let mut themes: Vec<String> = self.theme_set.themes.keys().cloned().collect();
        themes.sort();

        if let Some(current_idx) = themes.iter().position(|t| t == &self.current_theme) {
            let next_idx = (current_idx + 1) % themes.len();
            self.current_theme = themes[next_idx].clone();

            self.save_config();
            self.clear_cache();

            self.set_status(format!("Theme changed to: {}", self.current_theme));
        }
    }

    fn set_status(&mut self, message: String) {
        self.status_message = message;
        self.status_time = Some(Instant::now());
    }

    fn clear_status(&mut self) {
        self.status_message.clear();
        self.status_time = None;
    }

    fn run(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        let mut stdout = stdout();
        execute!(stdout, terminal::EnterAlternateScreen)?;

        loop {
            if let Some(time) = self.status_time {
                if time.elapsed() >= Duration::from_secs(3) {
                    self.clear_status();
                }
            }

            self.draw_screen()?;
            if self.should_quit {
                break;
            }

            let timeout = if let Some(time) = self.status_time {
                let elapsed = time.elapsed();
                if elapsed >= Duration::from_secs(3) {
                    Duration::from_millis(1)
                } else {
                    Duration::from_secs(3) - elapsed
                }
            } else {
                Duration::from_secs(3600)
            };

            if event::poll(timeout)? {
                self.process_keypress()?;
            } else {
                self.clear_status();
            }
        }

        execute!(stdout, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;
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
            }

            loop {
                let mut screen_y_offset = 0;
                for i in self.row_offset..self.cursor_y {
                    let w = self.get_visual_line_width(i);
                    screen_y_offset += if w == 0 { 1 } else { (w - 1) / available_width + 1 };
                }
                let cursor_visual = self.get_visual_cursor_x();
                screen_y_offset += cursor_visual / available_width;

                if screen_y_offset >= visible_rows && self.row_offset < self.cursor_y {
                    self.row_offset += 1;
                } else {
                    break;
                }
            }
        } else {
            if self.cursor_y < self.row_offset {
                self.row_offset = self.cursor_y;
            }
            if self.cursor_y >= self.row_offset + visible_rows {
                self.row_offset = self.cursor_y - visible_rows + 1;
            }

            let visual_x = self.get_visual_cursor_x();
            let left_bound = if self.col_offset > 0 { self.col_offset + 1 } else { 0 };

            if visual_x < left_bound {
                self.col_offset = visual_x.saturating_sub(available_width / 2);
            } else if visual_x >= self.col_offset + available_width.saturating_sub(1) {
                self.col_offset = visual_x.saturating_sub(available_width / 2);
            }
        }

        Ok(())
    }

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

        queue!(stdout, cursor::MoveTo(0, 0),
            SetBackgroundColor(ui_bg), SetForegroundColor(title_fg))?;
        let title = "   xnano";
        let file_display = self.filename.as_deref().unwrap_or("New Buffer");

        let center_start = (cols as usize).saturating_sub(file_display.len()) / 2;
        let pad1_len = center_start.saturating_sub(title.len());
        let pad1 = " ".repeat(pad1_len);
        let left_and_center = format!("{}{}{}", title, pad1, file_display);

        if self.is_modified {
            let right = "[ Modified ]   ";
            let pad2_len = (cols as usize).saturating_sub(left_and_center.len() + right.len());
            let pad2 = " ".repeat(pad2_len);

            queue!(
                stdout,
                Print(left_and_center),
                Print(pad2),
                SetForegroundColor(title_fg),
                Print(right),
                SetForegroundColor(Color::Reset),
                SetBackgroundColor(Color::Reset)
            )?;
        } else {
            let pad2_len = (cols as usize).saturating_sub(left_and_center.len());
            let pad2 = " ".repeat(pad2_len);
            queue!(
                stdout,
                Print(left_and_center),
                Print(pad2),
                SetForegroundColor(Color::Reset),
                SetBackgroundColor(Color::Reset)
            )?;
        }

        let syntax = if let Some(ref name) = self.filename {
            let path = Path::new(name);
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

        let mut terminal_y = 0;
        let mut file_y = self.row_offset;

        while terminal_y < visible_rows {
            if file_y < self.buffer.len_lines() {
                if !self.highlight_cache.contains_key(&file_y) {
                    let mut highlighter = HighlightLines::new(syntax, theme);
                    let line_str = self.buffer.line(file_y).to_string();
                    let ranges = highlighter.highlight_line(&line_str, &self.syntax_set).unwrap();
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
                    queue!(stdout, SetBackgroundColor(default_cross_bg), SetForegroundColor(menu_key_fg), Print(num_str))?;
                }

                let mut printed_on_current_line = 0;

                'char_loop: for (style, text) in ranges {
                    let syn_color = style.foreground;
                    let cross_color = Color::Rgb { r: syn_color.r, g: syn_color.g, b: syn_color.b };
                    let syn_bg = style.background;
                    let cross_bg = Color::Rgb { r: syn_bg.r, g: syn_bg.g, b: syn_bg.b };

                    queue!(stdout, SetForegroundColor(cross_color), SetBackgroundColor(cross_bg))?;

                    for ch in text.chars() {
                        if ch == '\n' || ch == '\r' {
                            line_char_idx += 1;
                            continue;
                        }

                        let is_highlighted = if line_has_search_highlight {
                            if let Some((_, h_start, h_end)) = self.highlight_match {
                                line_char_idx >= h_start && line_char_idx < h_end
                            } else { false }
                        } else { false };

                        let display_chars = if ch == '\t' { vec![' '; 4 - (visual_x % 4)] } else { vec![ch] };

                        for display_ch in display_chars {
                            if self.soft_wrap {
                                if printed_on_current_line >= available_width {
                                    queue!(stdout, SetBackgroundColor(default_cross_bg), terminal::Clear(ClearType::UntilNewLine))?;
                                    terminal_y += 1;
                                    if terminal_y >= visible_rows { break 'char_loop; }

                                    queue!(stdout, cursor::MoveTo(0, (terminal_y + 1) as u16))?;
                                    if self.show_line_numbers {
                                        queue!(stdout, SetBackgroundColor(default_cross_bg), Print(" ".repeat(gutter_width)))?;
                                    }
                                    queue!(stdout, SetForegroundColor(cross_color), SetBackgroundColor(cross_bg))?;
                                    printed_on_current_line = 0;
                                }

                                if is_highlighted { queue!(stdout, SetBackgroundColor(Color::Red), SetForegroundColor(Color::White))?; }
                                queue!(stdout, Print(display_ch))?;
                                if is_highlighted { queue!(stdout, SetBackgroundColor(cross_bg), SetForegroundColor(cross_color))?; }

                                printed_on_current_line += 1;
                                visual_x += 1;
                            } else {
                                if visual_x >= self.col_offset && printed_on_current_line < available_width {
                                    if is_highlighted { queue!(stdout, SetBackgroundColor(Color::Red), SetForegroundColor(Color::White))?; }
                                    queue!(stdout, Print(display_ch))?;
                                    if is_highlighted { queue!(stdout, SetBackgroundColor(cross_bg), SetForegroundColor(cross_color))?; }
                                    printed_on_current_line += 1;
                                }
                                visual_x += 1;
                            }
                        }
                        line_char_idx += 1;
                    }
                }

                queue!(stdout, SetBackgroundColor(default_cross_bg), terminal::Clear(ClearType::UntilNewLine))?;

                if !self.soft_wrap {
                    let needs_left_dollar = self.col_offset > 0;
                    let needs_right_dollar = visual_x > self.col_offset + available_width;

                    if needs_left_dollar {
                        queue!(stdout, cursor::MoveTo(gutter_width as u16, (terminal_y + 1) as u16), SetBackgroundColor(dollar_bg), SetForegroundColor(dollar_fg), Print('$'))?;
                    }
                    if needs_right_dollar {
                        queue!(stdout, cursor::MoveTo((cols - 1) as u16, (terminal_y + 1) as u16), SetBackgroundColor(dollar_bg), SetForegroundColor(dollar_fg), Print('$'))?;
                    }
                }

                queue!(stdout, SetBackgroundColor(default_cross_bg), SetForegroundColor(Color::Reset))?;

            } else {
                queue!(stdout, cursor::MoveTo(0, (terminal_y + 1) as u16))?;
                if self.show_line_numbers {
                    queue!(stdout, SetBackgroundColor(default_cross_bg), Print(" ".repeat(gutter_width)))?;
                }
                queue!(stdout, SetBackgroundColor(default_cross_bg), terminal::Clear(ClearType::UntilNewLine))?;
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

            let status_text = format!("{}", self.status_message);
            let status_fill = " ".repeat((cols as usize).saturating_sub(status_text.len()));

            queue!(
                stdout,
                Print(format!("{}{}", status_text, status_fill)),
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
        }

        let mut cursor_screen_y = 0;
        let mut cursor_screen_x = 0;

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

    fn get_cursor_char_idx(&self) -> usize {
        self.buffer.line_to_char(self.cursor_y) + self.cursor_x
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

    fn move_up(&mut self) {
        if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        }
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

    fn move_to_start_of_line(&mut self) {
        self.cursor_x = 0;
        self.desired_cursor_x = 0;
    }

    fn move_to_end_of_line(&mut self) {
        self.cursor_x = self.line_len(self.cursor_y);
        self.desired_cursor_x = self.cursor_x;
    }

    fn delete_char(&mut self) {
        let idx = self.get_cursor_char_idx();
        if idx < self.buffer.len_chars() {
            self.buffer.remove(idx..(idx + 1));
            self.mark_modified();
        }
    }

    fn insert_tab(&mut self) {
        let idx = self.get_cursor_char_idx();
        self.buffer.insert(idx, "    ");
        self.cursor_x += 4;
        self.desired_cursor_x = self.cursor_x;
        self.mark_modified();
    }

    fn page_up(&mut self) -> io::Result<()> {
        let (_, rows) = terminal::size()?;
        let visible_rows = rows.saturating_sub(4) as usize;
        self.cursor_y = self.cursor_y.saturating_sub(visible_rows);
        self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        Ok(())
    }

    fn page_down(&mut self) -> io::Result<()> {
        let (_, rows) = terminal::size()?;
        let visible_rows = rows.saturating_sub(4) as usize;
        let max_y = self.buffer.len_lines().saturating_sub(1);
        self.cursor_y = (self.cursor_y + visible_rows).min(max_y);
        self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        Ok(())
    }

    fn exit_editor(&mut self) -> io::Result<()> {
        if self.is_modified {
            match self.prompt_yn("Save modified buffer (ANSWERING \"No\" WILL DESTROY CHANGES) ?")? {
                Some(true) => {
                    self.save_file()?;
                    if !self.is_modified {
                        self.should_quit = true;
                    }
                }
                Some(false) => {
                    self.should_quit = true;
                }
                None => {}
            }
        } else {
            self.should_quit = true;
        }
        Ok(())
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

            if entries.is_empty() {
                entries.push((String::from("."), true));
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

                queue!(stdout, SetBackgroundColor(default_cross_bg), terminal::Clear(ClearType::All))?;

                queue!(stdout, cursor::MoveTo(0, 0), SetBackgroundColor(ui_bg), SetForegroundColor(title_fg))?;
                let title = " xnano File Browser ";
                let path_str = current_dir.to_string_lossy();
                let center_start = (cols as usize).saturating_sub(path_str.len()) / 2;
                let pad1_len = center_start.saturating_sub(title.len());
                let pad1 = " ".repeat(pad1_len);
                let top_line = format!("{}{}{}", title, pad1, path_str);
                let pad2_len = (cols as usize).saturating_sub(top_line.len());
                let pad2 = " ".repeat(pad2_len);
                queue!(stdout, Print(format!("{}{}", top_line, pad2)))?;

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

                let menu_key_fg = if is_dark { Color::Rgb { r: 0, g: 150, b: 200 } } else { Color::Rgb { r: 0, g: 100, b: 200 } };
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
                                if name == ".." {
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
            "     -Written entirely in Rust",
            "     -Fast",
            "     -Themes",
            "     -Syntax highlighting",
            "     -Spell checker",
            "     -Soft line-wrap & line numbers",
            "  * Themes & Configuration: ",
            "     -To cycle through the included themes, type Meta+Z (ALT+Z, Option+Z)",
            "     -On MacOS, make sure you have 'Use Option as Meta' selected ",
            "      in your terminal settings",
            "     -Line numbers and Soft wrap are stored across sessions",
            "     -Settings are stored in ~/.xnano/xnanorc",
            "     -Themes are stored in ~/.xnano/themes",
            "     -Additional .tmTheme themes can be added to ~/.xnano/themes",
            "",
            "  Movement:",
            "    ^P, Up       Move up one line",
            "    ^N, Down     Move down one line",
            "    ^B, Left     Move left one character",
            "    ^F, Right    Move right one character",
            "    ^A           Move to start of line",
            "    ^E           Move to end of line",
            "    ^Y, F7, PgUp Move up one page",
            "    ^V, F8, PgDn Move down one page",
            "",
            "  Editing:",
            "    ^K, F9       Cut current line into clipboard",
            "    ^U, F10      Paste contents of clipboard",
            "    ^D, Del      Delete character under cursor",
            "    Backspace    Delete character before cursor",
            "    ^J, F4       Justify current paragraph",
            "    ^I, Tab      Insert tab",
            "",
            "  Search & Replace:",
            "    ^W, F6       Where is (Search)",
            "    ^\\           Search and Replace",
            "",
            "  File & System:",
            "    ^O, F3       Write Out (Save)",
            "    ^R, F5       Read File (Insert)",
            "    ^G, F1       Get Help (this screen)",
            "    ^X, F2       Exit xnano",
            "",
            "  Tools:",
            "    ^C, F11      Current Position",
            "    ^T, F12      To Spell (Spell check)",
            "    ^L           Go to line number",
            "    Meta+T       Cycle Syntax Theme",
            "    Meta+L       Toggle Line Numbers",
            "    Meta+S       Toggle Soft Wrap",
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

    fn process_keypress(&mut self) -> io::Result<()> {
        if let Event::Key(key) = event::read()? {
            self.highlight_match = None;

            let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let is_alt = key.modifiers.contains(KeyModifiers::ALT); // Leaving ALT match to support raw fallbacks just in case, but mappings below use CTRL.

            let was_justified = self.is_justified;
            let mut keep_justified = false;

            match key.code {
                KeyCode::Char('g') if is_ctrl => self.show_help()?,
                KeyCode::F(1) => self.show_help()?,

                KeyCode::Char('x') if is_ctrl => self.exit_editor()?,
                KeyCode::F(2) => self.exit_editor()?,

                KeyCode::Char('o') if is_ctrl => self.save_file()?,
                KeyCode::F(3) => self.save_file()?,

                KeyCode::Char('r') if is_ctrl => self.read_file()?,
                KeyCode::F(5) => self.read_file()?,

                KeyCode::Char('w') if is_ctrl => self.where_is()?,
                KeyCode::F(6) => self.where_is()?,

                KeyCode::Char('\\') if is_ctrl => self.replace()?,
                KeyCode::Char('4') if is_ctrl => self.replace()?,

                KeyCode::Char('k') if is_ctrl => self.cut_line(),
                KeyCode::F(9) => self.cut_line(),

                KeyCode::Char('u') if is_ctrl => {
                    if was_justified { self.unjustify(); } else { self.paste_line(); }
                }
                KeyCode::F(10) => {
                    if was_justified { self.unjustify(); } else { self.paste_line(); }
                }

                KeyCode::Char('j') if is_ctrl => {
                    self.justify();
                    self.is_justified = true;
                    keep_justified = true;
                }
                KeyCode::F(4) => {
                    self.justify();
                    self.is_justified = true;
                    keep_justified = true;
                }

                KeyCode::Char('t') if is_ctrl => self.spell_check()?,
                KeyCode::F(12) => self.spell_check()?,

                KeyCode::Char('c') if is_ctrl => self.cur_pos(),
                KeyCode::F(11) => self.cur_pos(),

                KeyCode::Char('l') if is_ctrl => self.go_to_line()?,

                KeyCode::Char('t') if is_alt => self.cycle_theme(),

                KeyCode::Char('l') if is_alt => {
                    self.show_line_numbers = !self.show_line_numbers;
                    self.save_config();
                    self.set_status(if self.show_line_numbers { "Line numbers enabled".into() } else { "Line numbers disabled".into() });
                }

                KeyCode::Char('s') if is_alt => {
                    self.soft_wrap = !self.soft_wrap;
                    self.save_config();
                    self.set_status(if self.soft_wrap { "Soft wrap enabled".into() } else { "Soft wrap disabled".into() });
                }

                KeyCode::Char('y') if is_ctrl => self.page_up()?,
                KeyCode::F(7) | KeyCode::PageUp => self.page_up()?,

                KeyCode::Char('v') if is_ctrl => self.page_down()?,
                KeyCode::F(8) | KeyCode::PageDown => self.page_down()?,

                KeyCode::Char('b') if is_ctrl => self.move_left(),
                KeyCode::Char('f') if is_ctrl => self.move_right(),
                KeyCode::Char('p') if is_ctrl => self.move_up(),
                KeyCode::Char('n') if is_ctrl => self.move_down(),
                KeyCode::Char('a') if is_ctrl => self.move_to_start_of_line(),
                KeyCode::Char('e') if is_ctrl => self.move_to_end_of_line(),

                KeyCode::Char('d') if is_ctrl => self.delete_char(),
                KeyCode::Delete => self.delete_char(),

                KeyCode::Char('i') if is_ctrl => self.insert_tab(),
                KeyCode::Tab => self.insert_tab(),

                KeyCode::Up => self.move_up(),
                KeyCode::Down => self.move_down(),
                KeyCode::Left => self.move_left(),
                KeyCode::Right => self.move_right(),

                KeyCode::Char(c) if !is_ctrl && !is_alt => {
                    let idx = self.get_cursor_char_idx();
                    self.buffer.insert_char(idx, c);
                    self.cursor_x += 1;
                    self.desired_cursor_x = self.cursor_x;
                    self.mark_modified();
                }
                KeyCode::Enter => {
                    let idx = self.get_cursor_char_idx();
                    self.buffer.insert_char(idx, '\n');
                    self.cursor_y += 1;
                    self.cursor_x = 0;
                    self.desired_cursor_x = 0;
                    self.mark_modified();
                }
                KeyCode::Backspace => {
                    let idx = self.get_cursor_char_idx();
                    if idx > 0 {
                        self.buffer.remove((idx - 1)..idx);
                        self.cursor_y = self.buffer.char_to_line(idx - 1);
                        self.cursor_x = (idx - 1) - self.buffer.line_to_char(self.cursor_y);
                        self.desired_cursor_x = self.cursor_x;
                        self.mark_modified();
                    }
                }
                _ => { self.clear_status(); }
            }
            if !keep_justified {
                self.is_justified = false;
            }
        }

        self.scroll()?;
        Ok(())
    }

    fn where_is(&mut self) -> io::Result<()> {
        let prompt_text = if let Some(ref last) = self.last_search {
            format!("Search [{}]: ", last)
        } else {
            String::from("Search: ")
        };

        if let Some(mut query) = self.prompt(&prompt_text, false)? {
            if query.is_empty() {
                if let Some(ref last) = self.last_search {
                    query = last.clone();
                } else {
                    self.set_status(String::from("Cancelled"));
                    return Ok(());
                }
            } else {
                self.last_search = Some(query.clone());
            }

            let text = self.buffer.to_string();
            let mut start_char = self.get_cursor_char_idx();

            if text[start_char..].starts_with(&query) {
                start_char += 1;
            }

            if let Some(pos) = text[start_char..].find(&query) {
                let absolute_pos = start_char + pos;
                self.cursor_y = self.buffer.char_to_line(absolute_pos);
                self.cursor_x = absolute_pos - self.buffer.line_to_char(self.cursor_y);
                self.desired_cursor_x = self.cursor_x;

                let match_len = query.chars().count();
                self.highlight_match = Some((self.cursor_y, self.cursor_x, self.cursor_x + match_len));

                self.clear_status();
            } else {
                if let Some(pos) = text.find(&query) {
                    self.cursor_y = self.buffer.char_to_line(pos);
                    self.cursor_x = pos - self.buffer.line_to_char(self.cursor_y);
                    self.desired_cursor_x = self.cursor_x;

                    let match_len = query.chars().count();
                    self.highlight_match = Some((self.cursor_y, self.cursor_x, self.cursor_x + match_len));

                    self.set_status(String::from("Search wrapped to top"));
                } else {
                    self.set_status(format!("\"{}\" not found", query));
                }
            }
        }
        Ok(())
    }

    fn replace(&mut self) -> io::Result<()> {
        let prompt_text = if let Some(ref last) = self.last_search {
            format!("Search (to replace) [{}]: ", last)
        } else {
            String::from("Search (to replace): ")
        };

        if let Some(mut query) = self.prompt(&prompt_text, false)? {
            if query.is_empty() {
                if let Some(ref last) = self.last_search {
                    query = last.clone();
                } else {
                    self.set_status(String::from("Cancelled"));
                    return Ok(());
                }
            } else {
                self.last_search = Some(query.clone());
            }

            if let Some(replacement) = self.prompt("Replace with: ", false)? {
                let mut current_idx = self.get_cursor_char_idx();
                let mut changes_made = 0;
                let mut replace_all = false;
                let mut wrapped = false;

                loop {
                    let text = self.buffer.to_string();
                    if let Some(pos) = text[current_idx..].find(&query) {
                        let start_idx = current_idx + pos;
                        let end_idx = start_idx + query.chars().count();

                        self.cursor_y = self.buffer.char_to_line(start_idx);
                        self.cursor_x = start_idx - self.buffer.line_to_char(self.cursor_y);
                        self.desired_cursor_x = self.cursor_x;
                        self.scroll()?;

                        if replace_all {
                            self.buffer.remove(start_idx..end_idx);
                            self.buffer.insert(start_idx, &replacement);
                            current_idx = start_idx + replacement.chars().count();
                            changes_made += 1;
                            self.mark_modified();
                            continue;
                        }

                        let match_len = query.chars().count();
                        self.highlight_match = Some((self.cursor_y, self.cursor_x, self.cursor_x + match_len));

                        let prompt_result = self.prompt_replace("Replace this instance?");

                        self.highlight_match = None;

                        if let Some(action) = prompt_result? {
                            match action {
                                'y' => {
                                    self.buffer.remove(start_idx..end_idx);
                                    self.buffer.insert(start_idx, &replacement);
                                    current_idx = start_idx + replacement.chars().count();
                                    changes_made += 1;
                                    self.mark_modified();
                                }
                                'n' => {
                                    current_idx = end_idx;
                                }
                                'a' => {
                                    replace_all = true;
                                    self.buffer.remove(start_idx..end_idx);
                                    self.buffer.insert(start_idx, &replacement);
                                    current_idx = start_idx + replacement.chars().count();
                                    changes_made += 1;
                                    self.mark_modified();
                                }
                                _ => unreachable!()
                            }
                        } else {
                            self.set_status(String::from("Cancelled"));
                            return Ok(());
                        }
                    } else {
                        if current_idx > 0 && !wrapped {
                            current_idx = 0;
                            wrapped = true;
                        } else {
                            break;
                        }
                    }
                }

                if changes_made > 0 {
                    self.set_status(format!("Replaced {} occurrences", changes_made));
                } else {
                    self.set_status(String::from("No matches found"));
                }
            }
        }
        Ok(())
    }

    fn cur_pos(&mut self) {
        let line = self.cursor_y + 1;
        let total_lines = self.buffer.len_lines();
        let col = self.cursor_x + 1;
        let total_chars = self.buffer.len_chars();
        self.set_status(format!("line {}/{}, col {}, char {}", line, total_lines, col, total_chars));
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

    fn justify(&mut self) {
        self.pre_justify_snapshot = Some((self.buffer.clone(), self.cursor_x, self.cursor_y));

        let max_y = self.buffer.len_lines().saturating_sub(1);
        if max_y == 0 && self.buffer.len_chars() == 0 { return; }

        let mut start_line = self.cursor_y;
        while start_line > 0 && self.buffer.line(start_line - 1).chars().any(|c| !c.is_whitespace()) {
            start_line -= 1;
        }

        let mut end_line = self.cursor_y;
        while end_line < max_y && self.buffer.line(end_line).chars().any(|c| !c.is_whitespace()) {
            end_line += 1;
        }
        if start_line == end_line && !self.buffer.line(start_line).chars().any(|c| !c.is_whitespace()) {
            return;
        }

        let start_char = self.buffer.line_to_char(start_line);
        let end_char = if end_line + 1 < self.buffer.len_lines() {
            self.buffer.line_to_char(end_line + 1)
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

        self.is_justified = true;
        self.mark_modified();
        self.set_status(String::from("Justified paragraph"));
    }

    fn unjustify(&mut self) {
        if let Some((snapshot, x, y)) = self.pre_justify_snapshot.take() {
            self.buffer = snapshot;
            self.cursor_x = x;
            self.cursor_y = y;
            self.desired_cursor_x = x;

            self.is_justified = false;
            self.clear_cache();
            self.set_status(String::from("Unjustified"));
            self.mark_modified();
        }
    }

    fn cut_line(&mut self) {
        if self.buffer.len_chars() == 0 { return; }
        let start_char = self.buffer.line_to_char(self.cursor_y);
        let end_char = if self.cursor_y + 1 < self.buffer.len_lines() {
            self.buffer.line_to_char(self.cursor_y + 1)
        } else {
            self.buffer.len_chars()
        };

        self.clipboard = self.buffer.slice(start_char..end_char).to_string();
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

    fn paste_line(&mut self) {
        if self.clipboard.is_empty() { return; }
        let start_char = self.buffer.line_to_char(self.cursor_y);
        self.buffer.insert(start_char, &self.clipboard);

        let newlines_pasted = self.clipboard.chars().filter(|&c| c == '\n').count();
        self.cursor_y += newlines_pasted;
        self.cursor_x = 0;
        self.desired_cursor_x = 0;
        self.set_status(String::from("Pasted line"));
        self.mark_modified();
    }

    fn prompt(&mut self, prompt_text: &str, allow_browser: bool) -> io::Result<Option<String>> {
        self.menu_state = if allow_browser { MenuState::PromptWithBrowser } else { MenuState::CancelOnly };
        self.status_time = None;
        let mut input = String::new();

        loop {
            self.status_message = format!("{}{}", prompt_text, input);
            self.draw_screen()?;

            let (_, rows) = terminal::size()?;
            let mut stdout = stdout();

            let cursor_x = prompt_text.len() + input.len();
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

    fn load_dictionary() -> HashSet<String> {
        let mut dict = HashSet::new();
        let dict_paths = ["/usr/share/dict/words", "/usr/dict/words"];

        for path in dict_paths {
            if let Ok(file) = File::open(path) {
                let reader = BufReader::new(file);
                for line in reader.lines().map_while(Result::ok) {
                    dict.insert(line.trim().to_lowercase());
                }
                break;
            }
        }
        dict
    }

    fn find_next_misspelled(&self, start_idx: usize) -> Option<(String, usize, usize)> {
        let dict = self.dictionary.as_ref().unwrap();
        let mut in_word = false;
        let mut word_start = 0;
        let mut word = String::new();

        let chars = self.buffer.chars().skip(start_idx);
        for (i, c) in chars.enumerate() {
            let actual_idx = start_idx + i;
            if c.is_alphabetic() {
                if !in_word {
                    in_word = true;
                    word_start = actual_idx;
                }
                word.push(c);
            } else {
                if in_word {
                    if !dict.contains(&word.to_lowercase()) {
                        return Some((word, word_start, actual_idx));
                    }
                    in_word = false;
                    word.clear();
                }
            }
        }
        if in_word && !dict.contains(&word.to_lowercase()) {
            return Some((word, word_start, self.buffer.len_chars()));
        }
        None
    }

    fn spell_check(&mut self) -> io::Result<()> {
        if self.dictionary.is_none() {
            self.set_status(String::from("Loading dictionary..."));
            self.draw_screen()?;
            self.dictionary = Some(Self::load_dictionary());
        }

        if self.dictionary.as_ref().unwrap().is_empty() {
            self.set_status(String::from("Error: No dictionary found at /usr/share/dict/words"));
            return Ok(());
        }

        let mut current_idx = 0;
        let mut changes_made = 0;

        loop {
            if let Some((word, start_idx, end_idx)) = self.find_next_misspelled(current_idx) {
                self.cursor_y = self.buffer.char_to_line(start_idx);
                self.cursor_x = start_idx - self.buffer.line_to_char(self.cursor_y);
                self.desired_cursor_x = self.cursor_x;
                self.scroll()?;

                let prompt_text = format!("Misspelled: '{}'. Replace with (Enter to skip): ", word);
                if let Some(replacement) = self.prompt(&prompt_text, false)? {
                    if !replacement.is_empty() {
                        self.buffer.remove(start_idx..end_idx);
                        self.buffer.insert(start_idx, &replacement);
                        current_idx = start_idx + replacement.chars().count();
                        changes_made += 1;
                        self.mark_modified();
                        continue;
                    }
                } else {
                    self.set_status(String::from("Spell check cancelled."));
                    return Ok(());
                }
                current_idx = end_idx;
            } else {
                break;
            }
        }
        self.set_status(format!("Spell check complete. {} replacements made.", changes_made));
        Ok(())
    }

    fn expand_tilde(path: &str) -> String {
        if path.starts_with("~/") || path.starts_with("~\\") || path == "~" {
            let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).unwrap_or_default();
            if !home.is_empty() {
                return path.replacen('~', &home, 1);
            }
        }
        path.to_string()
    }

    fn read_file(&mut self) -> io::Result<()> {
        if let Some(filepath) = self.prompt("File to insert: ", true)? {
            if filepath.is_empty() {
                self.set_status(String::from("Read cancelled."));
                return Ok(());
            }
            let expanded_path = Self::expand_tilde(&filepath);
            match fs::read_to_string(&expanded_path) {
                Ok(contents) => {
                    let idx = self.get_cursor_char_idx();
                    self.buffer.insert(idx, &contents);
                    self.set_status(format!("Read {} lines", contents.lines().count()));
                    self.mark_modified();
                }
                Err(e) => self.set_status(format!("Error reading file: {}", e)),
            }
        }
        Ok(())
    }

    fn save_file(&mut self) -> io::Result<()> {
        let default_name = self.filename.clone().unwrap_or_default();
        let prompt_text = if default_name.is_empty() {
            String::from("File Name to Write: ")
        } else {
            format!("File Name to Write [{}]: ", default_name)
        };

        if let Some(mut new_name) = self.prompt(&prompt_text, true)? {
            if new_name.is_empty() {
                if !default_name.is_empty() {
                    new_name = default_name;
                } else {
                    self.set_status(String::from("Save cancelled: No filename provided."));
                    return Ok(());
                }
            }

            let expanded_path = Self::expand_tilde(&new_name);
            let path = Path::new(&expanded_path);

            if path.exists() && Some(&new_name) != self.filename.as_ref() {
                let warning = format!("File \"{}\" exists, OVERWRITE ?", new_name);
                match self.prompt_yn(&warning)? {
                    Some(true) => {}
                    _ => {
                        self.set_status(String::from("Save cancelled"));
                        return Ok(());
                    }
                }
            }

            match File::create(&expanded_path) {
                Ok(file) => {
                    if let Err(e) = self.buffer.write_to(BufWriter::new(file)) {
                        self.set_status(format!("Error writing file: {}", e));
                    } else {
                        self.filename = Some(new_name);
                        self.set_status(format!("Wrote {} lines", self.buffer.len_lines()));
                        self.is_modified = false;
                    }
                }
                Err(e) => self.set_status(format!("Error creating file: {}", e)),
            }
        }
        Ok(())
    }
}

fn main() -> io::Result<()> {
    let _ = Editor::initialize_themes();

    let args: Vec<String> = env::args().collect();
    let filename = args.get(1).cloned();

    let mut editor = Editor::new(filename);
    editor.run()
}

