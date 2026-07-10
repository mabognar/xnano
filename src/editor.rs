use std::collections::{HashSet, HashMap};
use std::time::{Duration, Instant};
use std::path::Path;
use std::fs::{self, File};
use std::env;
use std::io::{self, BufWriter};
use ropey::Rope;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use crossterm::{execute, terminal, event::{self, Event, KeyCode, KeyModifiers}};

use std::sync::mpsc::{self, Receiver};
use std::thread;

// Pull in the trait definitions so this file knows about draw_screen, spell_check, etc.
use crate::config::ConfigExt;
use crate::spell::SpellExt;
use crate::ui::UiExt;

#[derive(PartialEq)]
pub(crate) enum MenuState {
    Menu1,
    Menu2,
    Menu3,
    YesNoCancel,
    ReplaceAction,
    CancelOnly,
    PromptWithBrowser,
    SpellCheck,
}

pub struct Editor {
    pub(crate) buffer: Rope,
    pub(crate) cursor_x: usize,
    pub(crate) cursor_y: usize,
    pub(crate) desired_cursor_x: usize,
    pub(crate) mark: Option<usize>,
    pub(crate) row_offset: usize,
    pub(crate) col_offset: usize,
    pub(crate) filename: Option<String>,
    pub(crate) should_quit: bool,
    pub(crate) status_message: String,
    pub(crate) clipboard: String,
    pub(crate) dictionary: Option<HashSet<String>>,
    pub(crate) ignored_words: HashSet<String>,
    pub(crate) current_suggestions: Vec<String>,
    pub(crate) syntax_set: SyntaxSet,
    pub(crate) theme_set: ThemeSet,
    pub(crate) is_modified: bool,
    pub(crate) last_search: Option<String>,
    pub(crate) menu_state: MenuState,
    pub(crate) status_time: Option<Instant>,
    pub(crate) highlight_match: Option<(usize, usize, usize)>,
    pub(crate) highlight_cache: HashMap<usize, Vec<(Style, String)>>,
    pub(crate) current_theme: String,
    pub(crate) is_justified: bool,
    pub(crate) pre_justify_snapshot: Option<(Rope, usize, usize)>,
    pub(crate) show_line_numbers: bool,
    pub(crate) soft_wrap: bool,
    pub(crate) previous_action_was_cut: bool,
    pub(crate) escape_pending: bool, // Added escape tracker for macOS terminal fallback
    pub(crate) update_rx: Option<Receiver<String>>,
    pub(crate) update_version: Option<String>,
}

impl Editor {
    pub fn new(filename: Option<String>) -> Self {
        let buffer = if let Some(ref fname) = filename {
            let expanded = Self::expand_tilde(fname);
            if let Ok(file) = File::open(&expanded) {
                Rope::from_reader(io::BufReader::new(file)).unwrap_or_default()
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

        let (update_tx, update_rx) = mpsc::channel();
        thread::spawn(move || {
            let current_version = env!("CARGO_PKG_VERSION");
            // GitHub API requires a User-Agent header
            if let Ok(resp) = ureq::get("https://api.github.com/repos/mabognar/xnano/releases/latest")
                .set("User-Agent", "xnano-update-checker")
                .timeout(Duration::from_secs(3))
                .call()
            {
                if let Ok(json) = resp.into_json::<serde_json::Value>() {
                    if let Some(tag) = json["tag_name"].as_str() {
                        let latest_version = tag.trim_start_matches('v');
                        if latest_version != current_version {
                            let _ = update_tx.send(latest_version.to_string());
                        }
                    }
                }
            }
        });

        Self {
            buffer,
            cursor_x: 0,
            cursor_y: 0,
            desired_cursor_x: 0,
            mark: None,
            row_offset: 0,
            col_offset: 0,
            filename,
            should_quit: false,
            status_message: initial_status,
            status_time: Some(Instant::now()),
            clipboard: String::new(),
            dictionary: None,
            ignored_words: HashSet::new(),
            current_suggestions: Vec::new(),
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set,
            is_modified: false,
            last_search: None,
            menu_state: MenuState::Menu1,
            highlight_match: None,
            highlight_cache: HashMap::new(),
            current_theme: starting_theme,
            is_justified: false,
            pre_justify_snapshot: None,
            show_line_numbers: line_numbers,
            soft_wrap,
            previous_action_was_cut: false,
            escape_pending: false, // Initialize tracker
            update_rx: Some(update_rx),
            update_version: None,
        }
    }

    pub(crate) fn get_visual_line_width(&self, y: usize) -> usize {
        if y >= self.buffer.len_lines() { return 0; }
        let mut w = 0;
        for ch in self.buffer.line(y).chars() {
            if ch == '\n' || ch == '\r' { continue; }
            if ch == '\t' { w += 4 - (w % 4); }
            else { w += 1; }
        }
        w
    }

    pub(crate) fn get_visual_cursor_x(&self) -> usize {
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

    pub(crate) fn clear_cache(&mut self) {
        self.highlight_cache.clear();
    }

    pub(crate) fn mark_modified(&mut self) {
        self.is_modified = true;
        self.clear_cache();
    }

    pub fn run(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, terminal::EnterAlternateScreen)?;

        self.update_cursor_color();

        loop {
            // check for update in background
            if let Some(rx) = &self.update_rx {
                if let Ok(version) = rx.try_recv() {
                    self.update_version = Some(version.clone());
                    self.set_status(format!("Press Meta+U (Alt+U) to update xnano to version {}", version));
                    self.update_rx = None; // Stop checking
                }
            }

            // expire old status messages
            if let Some(time) = self.status_time {
                if time.elapsed() >= Duration::from_secs(10) {
                    self.clear_status();
                }
            }

            self.draw_screen()?;
            if self.should_quit {
                break;
            }

            // calculate how long to sleep before repolling
            let mut timeout = if let Some(time) = self.status_time {
                let elapsed = time.elapsed();
                if elapsed >= Duration::from_secs(3) {
                    Duration::from_millis(1)
                } else {
                    Duration::from_secs(3) - elapsed
                }
            } else {
                Duration::from_secs(3600)
            };

            // wake every 250ms to check the background thread
            if self.update_rx.is_some() {
                timeout = timeout.min(Duration::from_millis(250));
            }

            // wait for event
            if event::poll(timeout)? {
                self.process_keypress()?;
            } else {
                if let Some(time) = self.status_time {
                    if time.elapsed() >= Duration::from_secs(3) {
                        self.clear_status();
                    }
                }
            }
        }

        execute!(stdout, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;

        // Reset cursor color on exit
        print!("\x1b]112\x07");
        let _ = io::Write::flush(&mut io::stdout());

        Ok(())
    }

    pub(crate) fn scroll(&mut self) -> io::Result<()> {
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
            // --- Vertical Scrolling ---
            if self.cursor_y < self.row_offset {
                self.row_offset = self.cursor_y;
            } else if self.cursor_y >= self.row_offset + visible_rows {
                self.row_offset = self.cursor_y.saturating_sub(visible_rows.saturating_sub(1));
            }

            // --- Horizontal Scrolling (1/2 Page at a time) ---
            let visual_x = self.get_visual_cursor_x();
            let right_bound = self.col_offset + available_width;

            if visual_x < self.col_offset {
                // Cursor moved off the left edge, jump left by half a screen
                self.col_offset = visual_x.saturating_sub(available_width / 2);
            } else if visual_x >= right_bound {
                // Cursor moved off the right edge, jump right by half a screen
                self.col_offset = visual_x.saturating_sub(available_width / 2);
            }
        }

        Ok(())
    }

    pub(crate) fn get_cursor_char_idx(&self) -> usize {
        self.buffer.line_to_char(self.cursor_y) + self.cursor_x
    }

    pub(crate) fn line_len(&self, y: usize) -> usize {
        if y >= self.buffer.len_lines() {
            return 0;
        }

        let line = self.buffer.line(y);
        let mut len = line.len_chars();
        if len > 0 && line.char(len - 1) == '\n' { len -= 1; }
        if len > 0 && line.char(len - 1) == '\r' { len -= 1; }
        len
    }

    pub(crate) fn move_up(&mut self) {
        if self.cursor_y > 0 {
            self.cursor_y -= 1;
            self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        }
    }

    pub(crate) fn move_down(&mut self) {
        if self.cursor_y < self.buffer.len_lines().saturating_sub(1) {
            self.cursor_y += 1;
            self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        }
    }

    pub(crate) fn move_left(&mut self) {
        let idx = self.get_cursor_char_idx();
        if idx > 0 {
            let new_idx = idx - 1;
            self.cursor_y = self.buffer.char_to_line(new_idx);
            self.cursor_x = new_idx - self.buffer.line_to_char(self.cursor_y);
            self.desired_cursor_x = self.cursor_x;
        }
    }

    pub(crate) fn move_right(&mut self) {
        let idx = self.get_cursor_char_idx();
        if idx < self.buffer.len_chars() {
            let new_idx = idx + 1;
            self.cursor_y = self.buffer.char_to_line(new_idx);
            self.cursor_x = new_idx - self.buffer.line_to_char(self.cursor_y);
            self.desired_cursor_x = self.cursor_x;
        }
    }

    pub(crate) fn move_to_start_of_line(&mut self) {
        self.cursor_x = 0;
        self.desired_cursor_x = 0;
    }

    pub(crate) fn move_to_end_of_line(&mut self) {
        self.cursor_x = self.line_len(self.cursor_y);
        self.desired_cursor_x = self.cursor_x;
    }

    pub(crate) fn delete_selection(&mut self) -> bool {
        if let Some(mark_idx) = self.mark {
            let cursor_idx = self.get_cursor_char_idx();
            let start_char = mark_idx.min(cursor_idx);
            let end_char = mark_idx.max(cursor_idx);

            // Always unmark, whether we delete something or not
            self.mark = None;

            if start_char != end_char {
                self.buffer.remove(start_char..end_char);
                self.cursor_y = self.buffer.char_to_line(start_char);
                self.cursor_x = start_char - self.buffer.line_to_char(self.cursor_y);
                self.desired_cursor_x = self.cursor_x;
                self.mark_modified();
                return true; // Indicates text was successfully deleted
            }
        }
        false
    }

    pub(crate) fn delete_char(&mut self) {
        if self.delete_selection() {
            return;
        }

        let idx = self.get_cursor_char_idx();
        if idx < self.buffer.len_chars() {
            self.buffer.remove(idx..(idx + 1));
            self.mark_modified();
        }
    }

    pub(crate) fn insert_tab(&mut self) {
        self.delete_selection(); // delete selected text before

        let idx = self.get_cursor_char_idx();
        self.buffer.insert(idx, "    ");
        self.cursor_x += 4;
        self.desired_cursor_x = self.cursor_x;
        self.mark_modified();
    }

    pub(crate) fn page_up(&mut self) -> io::Result<()> {
        let (_, rows) = terminal::size()?;
        let visible_rows = rows.saturating_sub(4) as usize;
        self.cursor_y = self.cursor_y.saturating_sub(visible_rows);
        self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        Ok(())
    }

    pub(crate) fn page_down(&mut self) -> io::Result<()> {
        let (_, rows) = terminal::size()?;
        let visible_rows = rows.saturating_sub(4) as usize;
        let max_y = self.buffer.len_lines().saturating_sub(1);
        self.cursor_y = (self.cursor_y + visible_rows).min(max_y);
        self.cursor_x = self.desired_cursor_x.min(self.line_len(self.cursor_y));
        Ok(())
    }

    pub(crate) fn exit_editor(&mut self) -> io::Result<()> {
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

    pub(crate) fn toggle_mark(&mut self) {
        if self.mark.is_some() {
            self.mark = None;
            self.set_status(String::from("Unmark set"));
        } else {
            self.mark = Some(self.get_cursor_char_idx());
            self.set_status(String::from("Mark Set"));
        }
    }

    pub(crate) fn process_keypress(&mut self) -> io::Result<()> {
        if let Event::Key(key) = event::read()? {
            // Ignore key release events on Windows
            if key.kind != event::KeyEventKind::Press {
                return Ok(());
            }

            self.highlight_match = None;

            let is_ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let mut is_alt = key.modifiers.contains(KeyModifiers::ALT);

            // macos terminal fallback
            if self.escape_pending {
                is_alt = true;
                self.escape_pending = false;
            } else if key.code == KeyCode::Esc {
                self.escape_pending = true;
                return Ok(());
            }

            let was_justified = self.is_justified;
            let mut keep_justified = false;
            let mut current_action_is_cut = false;

            match key.code {
                KeyCode::Char('^') if is_ctrl => self.toggle_mark(),
                KeyCode::Char('6') if is_ctrl => self.toggle_mark(),
                KeyCode::Char('a') if is_alt => self.toggle_mark(),

                KeyCode::Char('h') if is_ctrl => self.show_help()?,
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

                KeyCode::Char('k') if is_ctrl => {
                    self.cut_line();
                    current_action_is_cut = true;
                }
                KeyCode::F(9) => {
                    self.cut_line();
                    current_action_is_cut = true;
                }

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

                KeyCode::Char('o') if is_alt => {
                    self.menu_state = match self.menu_state {
                        MenuState::Menu1 => MenuState::Menu2,
                        MenuState::Menu2 => MenuState::Menu3,
                        _ => MenuState::Menu1, // Loop back to page 1
                    };
                }
                KeyCode::Char('u') if is_alt => {
                    if self.update_version.take().is_some() {
                        let _ = webbrowser::open("https://github.com/mabognar/xnano/releases/latest");
                        self.set_status(String::from("Opened browser to download update."));
                    } else {
                        // Optional: Give feedback if they press it when no update is available
                        self.set_status(String::from("No update pending."));
                    }
                }

                KeyCode::Char('y') if is_ctrl => self.page_up()?,
                KeyCode::F(7) | KeyCode::PageUp => self.page_up()?,
                KeyCode::Char('p') if is_alt => self.page_up()?,

                KeyCode::Char('v') if is_ctrl => self.page_down()?,
                KeyCode::F(8) | KeyCode::PageDown => self.page_down()?,
                KeyCode::Char('n') if is_alt => self.page_down()?,

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

                KeyCode::Left if is_ctrl => self.move_word_left(),
                KeyCode::Right if is_ctrl => self.move_word_right(),
                KeyCode::Left if is_alt => self.move_word_left(),
                KeyCode::Right if is_alt => self.move_word_right(),
                KeyCode::Char('b') if is_alt => self.move_word_left(), // macOS Esc b
                KeyCode::Char('f') if is_alt => self.move_word_right(), // macOS Esc f

                KeyCode::Up => self.move_up(),
                KeyCode::Down => self.move_down(),
                KeyCode::Left => self.move_left(),
                KeyCode::Right => self.move_right(),

                KeyCode::Char(c) if !is_ctrl && !is_alt => {
                    self.delete_selection();
                    let idx = self.get_cursor_char_idx();
                    self.buffer.insert_char(idx, c);
                    self.cursor_x += 1;
                    self.desired_cursor_x = self.cursor_x;
                    self.mark_modified();
                }
                KeyCode::Enter => {
                    self.delete_selection();
                    let idx = self.get_cursor_char_idx();
                    self.buffer.insert_char(idx, '\n');
                    self.cursor_y += 1;
                    self.cursor_x = 0;
                    self.desired_cursor_x = 0;
                    self.mark_modified();
                }
                KeyCode::Backspace => {
                    if !self.delete_selection() {
                        let idx = self.get_cursor_char_idx();
                        if idx > 0 {
                            self.buffer.remove((idx - 1)..idx);
                            self.cursor_y = self.buffer.char_to_line(idx - 1);
                            self.cursor_x = (idx - 1) - self.buffer.line_to_char(self.cursor_y);
                            self.desired_cursor_x = self.cursor_x;
                            self.mark_modified();
                        }
                    }
                }
                _ => { self.clear_status(); }
            }
            if !keep_justified {
                self.is_justified = false;
            }
            self.previous_action_was_cut = current_action_is_cut;
        }

        self.scroll()?;
        Ok(())
    }

    pub(crate) fn where_is(&mut self) -> io::Result<()> {
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

    pub(crate) fn replace(&mut self) -> io::Result<()> {
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

    pub(crate) fn cur_pos(&mut self) {
        let line = self.cursor_y + 1;
        let total_lines = self.buffer.len_lines();
        let col = self.cursor_x + 1;
        let total_chars = self.buffer.len_chars();
        self.set_status(format!("line {}/{}, col {}, char {}", line, total_lines, col, total_chars));
    }

    pub(crate) fn go_to_line(&mut self) -> io::Result<()> {
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

    pub(crate) fn justify(&mut self) {
        self.pre_justify_snapshot = Some((self.buffer.clone(), self.cursor_x, self.cursor_y));

        let max_y = self.buffer.len_lines().saturating_sub(1);
        if max_y == 0 && self.buffer.len_chars() == 0 { return; }

        let mut start_line = self.cursor_y;
        while start_line > 0 && self.buffer.line(start_line - 1).chars().any(|c| !c.is_whitespace()) {
            start_line -= 1;
        }

        let mut end_line = self.cursor_y;
        // Fix 1: Use <= max_y so the very last line of the file can be evaluated
        while end_line <= max_y && self.buffer.line(end_line).chars().any(|c| !c.is_whitespace()) {
            end_line += 1;
        }

        if start_line == end_line && !self.buffer.line(start_line).chars().any(|c| !c.is_whitespace()) {
            return;
        }

        let start_char = self.buffer.line_to_char(start_line);
        // Fix 2: Do not add + 1 to end_line. This stops the removal range exactly AT
        // the start of the blank line, leaving the blank line perfectly intact.
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
        // This newline will correctly bridge the gap to the preserved blank line
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
        self.set_status(String::from("Justified --- Ctrl+U to undo"));
    }

    pub(crate) fn unjustify(&mut self) {
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

    pub(crate) fn cut_line(&mut self) {
        if self.buffer.len_chars() == 0 { return; }

        // If a mark is active, cut the selected region
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
            // No mark, do standard full-line cut
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
    }

    pub(crate) fn paste_line(&mut self) {
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

    pub(crate) fn expand_tilde(path: &str) -> String {
        if path.starts_with("~/") || path.starts_with("~\\") || path == "~" {
            let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).unwrap_or_default();
            if !home.is_empty() {
                return path.replacen('~', &home, 1);
            }
        }
        path.to_string()
    }

    pub(crate) fn read_file(&mut self) -> io::Result<()> {
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

    pub(crate) fn save_file(&mut self) -> io::Result<()> {
        let default_name = self.filename.clone().unwrap_or_default();
        let prompt_text = if default_name.is_empty() {
            String::from("File name to write: ")
        } else {
            format!("File name to write [{}]: ", default_name)
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

                        // --- NEW: Clear the cache so the UI applies the new syntax on next draw ---
                        self.highlight_cache.clear();

                        self.set_status(format!("Wrote {} lines", self.buffer.len_lines()));
                        self.is_modified = false;
                    }
                }
                Err(e) => self.set_status(format!("Error creating file: {}", e)),
            }

        }
        Ok(())
    }

    pub(crate) fn move_word_right(&mut self) {
        let mut idx = self.get_cursor_char_idx();
        let len = self.buffer.len_chars();
        if idx >= len { return; }

        let start_is_word = self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_';

        // 1. If we are on a word, skip the rest of the word characters
        if start_is_word {
            while idx < len && (self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
                idx += 1;
            }
        }

        // 2. Skip any spaces, punctuation, or newlines until the start of the next word
        while idx < len && !(self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
            idx += 1;
        }

        // Update cursor coordinates
        self.cursor_y = self.buffer.char_to_line(idx);
        self.cursor_x = idx - self.buffer.line_to_char(self.cursor_y);
        self.desired_cursor_x = self.cursor_x;
    }

    pub(crate) fn move_word_left(&mut self) {
        let mut idx = self.get_cursor_char_idx();
        if idx == 0 { return; }

        idx -= 1; // Step back one character to begin evaluating

        // 1. Skip backwards through any spaces, punctuation, or newlines
        while idx > 0 && !(self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
            idx -= 1;
        }

        // 2. Skip backwards through the word characters to find the start of the word
        while idx > 0 && (self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
            idx -= 1;
        }

        // If we didn't hit the absolute beginning of the file, step forward one
        // to land on the first letter of the word.
        if idx > 0 || !(self.buffer.char(idx).is_alphanumeric() || self.buffer.char(idx) == '_') {
            idx += 1;
        }

        // Update cursor coordinates
        self.cursor_y = self.buffer.char_to_line(idx);
        self.cursor_x = idx - self.buffer.line_to_char(self.cursor_y);
        self.desired_cursor_x = self.cursor_x;
    }
}

