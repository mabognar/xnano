mod file;
mod movement;
mod edit;
mod buffer;
mod search;
mod marks;

use crate::config::ConfigExt;
use crate::spell::SpellExt;
use crate::ui::{RenderExt, HelpExt};

pub use file::FileExt;
pub use movement::MovementExt;
pub use edit::EditExt;
pub use buffer::BufferManagerExt;
pub use search::SearchExt;
pub use marks::MarkExt;


use std::collections::{HashSet, HashMap, VecDeque};
use std::time::{Duration, Instant};
use std::path::Path;
use std::fs::{self, File};
use std::env;
use std::io::{self};
use ropey::Rope;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use crossterm::{
    execute, terminal,
    event::{self, Event, KeyCode, KeyModifiers, EnableMouseCapture, DisableMouseCapture, MouseEventKind, MouseButton}
};
use std::collections::BTreeSet;
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[derive(PartialEq, Clone, Copy)]
pub(crate) enum MenuState {
    Menu1,
    Menu2,
    Menu3,
    Menu4,
    YesNoCancel,
    ReplaceAction,
    CancelOnly,
    PromptWithBrowser,
    SpellCheck,
}

pub(crate) struct BufferState {
    pub buffer: Rope, pub cursor_x: usize, pub cursor_y: usize,
    pub desired_cursor_x: usize, pub undo_stack: Vec<(Rope, usize, usize)>,
    pub redo_stack: Vec<(Rope, usize, usize)>, pub mark: Option<usize>,
    pub row_offset: usize, pub col_offset: usize, pub filename: Option<String>,
    pub is_modified: bool, pub highlight_cache: HashMap<usize, Vec<(Style, String)>>,
    pub line_marks: BTreeSet<usize>,
}

pub struct Editor {
    pub(crate) buffer: Rope,
    pub(crate) cursor_x: usize,
    pub(crate) cursor_y: usize,
    pub(crate) desired_cursor_x: usize,
    pub(crate) undo_stack: Vec<(Rope, usize, usize)>,
    pub(crate) redo_stack: Vec<(Rope, usize, usize)>,
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
    pub(crate) show_line_numbers: bool,
    pub(crate) soft_wrap: bool,
    pub(crate) previous_action_was_cut: bool,
    pub(crate) escape_pending: bool,
    pub(crate) update_rx: Option<Receiver<String>>,
    pub(crate) update_version: Option<String>,
    pub(crate) line_marks: BTreeSet<usize>,
    pub(crate) mouse_enabled: bool,
    pub(crate) inactive_buffers: VecDeque<BufferState>,
    pub(crate) active_buffer_index: usize,
}

impl BufferState {
    pub fn new(filename: Option<String>) -> Self {
        let buffer = if let Some(ref fname) = filename {
            let expanded = Editor::expand_tilde(fname);
            if let Ok(file) = File::open(&expanded) { ropey::Rope::from_reader(io::BufReader::new(file)).unwrap_or_default() }
            else { ropey::Rope::new() }
        } else { ropey::Rope::new() };
        Self { buffer, cursor_x: 0, cursor_y: 0, desired_cursor_x: 0, undo_stack: Vec::new(),
            redo_stack: Vec::new(), mark: None, row_offset: 0, col_offset: 0, filename,
            is_modified: false, highlight_cache: HashMap::new(), line_marks: BTreeSet::new() }
    }
}


impl Editor {

    pub fn new(filenames: Vec<String>) -> Self {
        // deduplicate incoming files using canonical paths
        let mut unique_files = Vec::new();
        let mut seen_paths = HashSet::new();

        for fname in filenames {
            let expanded = Self::expand_tilde(&fname);
            let target = Path::new(&expanded);
            let canon = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());

            if seen_paths.insert(canon) {
                unique_files.push(fname);
            }
        }

        // load the first file into the active buffer
        let first_file = unique_files.get(0).cloned();
        let buffer = if let Some(ref fname) = first_file {
            let expanded = Self::expand_tilde(fname);
            if let Ok(file) = File::open(&expanded) {
                Rope::from_reader(io::BufReader::new(file)).unwrap_or_default()
            } else {
                Rope::new()
            }
        } else {
            Rope::new()
        };

        // load remaining files into the background queue
        let mut inactive_buffers = VecDeque::new();
        for fname in unique_files.into_iter().skip(1) {
            inactive_buffers.push_back(BufferState::new(Some(fname)));
        }

        let mut theme_set = ThemeSet::load_defaults();
        let mut themes_found = 0;
        let mut error_occurred = None;

        if let Some(theme_dir) = Self::get_themes_dir() {
            if let Ok(custom_themes) = ThemeSet::load_from_folder(&theme_dir) {
                themes_found += custom_themes.themes.len();
                theme_set.themes.extend(custom_themes.themes);
            }
        }

        match ThemeSet::load_from_folder("../../themes") {
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

        let (mut starting_theme, line_numbers, soft_wrap, mouse_enabled) = Self::load_config();

        if !theme_set.themes.contains_key(&starting_theme) {
            starting_theme = String::from("base16-ocean.dark");
        }

        let (update_tx, update_rx) = mpsc::channel();
        thread::spawn(move || {
            let current_version = env!("CARGO_PKG_VERSION");
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
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            mark: None,
            row_offset: 0,
            col_offset: 0,
            filename: first_file, // This handles the scoping correctly
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
            show_line_numbers: line_numbers,
            soft_wrap,
            previous_action_was_cut: false,
            escape_pending: false,
            update_rx: Some(update_rx),
            update_version: None,
            line_marks: BTreeSet::new(),
            mouse_enabled,
            inactive_buffers,
            active_buffer_index: 0,
        }
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

        // Split the execute macro into two parts
        execute!(stdout, terminal::EnterAlternateScreen)?;
        if self.mouse_enabled {
            execute!(stdout, EnableMouseCapture)?;
        }

        loop {
            // check for update in background
            if let Some(rx) = &self.update_rx {
                if let Ok(version) = rx.try_recv() {
                    self.update_version = Some(version.clone());
                    self.set_status(format!("Press Meta+V (Alt+V) to update xnano to version {}", version));
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

        // flush any pending terminal events before exiting
        while event::poll(Duration::from_millis(0))? {
            let _ = event::read()?;
        }

        execute!(stdout, DisableMouseCapture, terminal::LeaveAlternateScreen)?;
        terminal::disable_raw_mode()?;

        // reset cursor color on exit
        print!("\x1b]112\x07");
        let _ = io::Write::flush(&mut io::stdout());

        Ok(())
    }

    pub(crate) fn process_keypress(&mut self) -> io::Result<()> {
        let event = event::read()?;
        match event {
            Event::Key(key) => {
                // ignore key release events on Windows
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
                    if self.mark.is_some() {
                        self.mark = None;
                        self.set_status(String::from("Unselected text"));
                        return Ok(());
                    }

                    self.escape_pending = true;
                    return Ok(());
                }

                let mut current_action_is_cut = false;

                match key.code {
                    KeyCode::Char('^') if is_ctrl => self.toggle_mark(),
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
                        self.adjust_marks_on_delete(self.cursor_y);
                    }
                    KeyCode::F(9) => {
                        self.cut_line();
                        current_action_is_cut = true;
                        self.adjust_marks_on_delete(self.cursor_y);
                    }

                    KeyCode::Char('u') | KeyCode::Char('v') if is_ctrl => self.paste_line(),
                    KeyCode::F(10) => self.paste_line(),

                    KeyCode::Char('j') if is_ctrl => self.justify(),
                    KeyCode::F(4) => self.justify(),

                    KeyCode::Char('t') if is_ctrl => self.spell_check()?,
                    KeyCode::F(12) if key.modifiers.contains(KeyModifiers::NONE) => self.spell_check()?,

                    KeyCode::Char('c') if is_ctrl => self.copy_selection(),
                    KeyCode::Char('c') if is_alt => self.cur_pos(),

                    KeyCode::F(11) if key.modifiers.contains(KeyModifiers::NONE) => self.cur_pos(),

                    KeyCode::Char('l') if is_ctrl => self.go_to_line()?,

                    KeyCode::Char('t') if is_alt => self.cycle_theme(),

                    KeyCode::Char('l') if is_alt => {
                        self.show_line_numbers = !self.show_line_numbers;
                        self.save_config();
                        self.set_status(if self.show_line_numbers { "Line numbers enabled".into() } else { "Line numbers disabled".into() });
                    }
                    KeyCode::Char('e') if is_alt => self.toggle_mouse(),

                    KeyCode::Char('s') if is_alt => {
                        self.soft_wrap = !self.soft_wrap;
                        self.save_config();
                        self.set_status(if self.soft_wrap { "Soft wrap enabled".into() } else { "Soft wrap disabled".into() });
                    }

                    KeyCode::Char('o') if is_alt => {
                        self.menu_state = match self.menu_state {
                            MenuState::Menu1 => MenuState::Menu2,
                            MenuState::Menu2 => MenuState::Menu3,
                            MenuState::Menu3 => MenuState::Menu4,
                            _ => MenuState::Menu1,
                        };
                    }
                    KeyCode::Char('v') if is_alt => {
                        if self.update_version.is_some() {
                            let _ = webbrowser::open("https://github.com/mabognar/xnano/releases/latest");
                            self.set_status(String::from("Opened browser to download update."));
                        } else {
                            self.set_status(String::from("xnano is the latest version"));
                        }
                    }

                    KeyCode::Char('y') if is_ctrl => self.page_up()?,
                    KeyCode::F(7) | KeyCode::PageUp => self.page_up()?,
                    KeyCode::Char('p') if is_alt => self.page_up()?,

                    KeyCode::Char(' ') if is_ctrl => self.page_down()?,
                    // KeyCode::Char('v') if is_ctrl => self.page_down()?,
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
                        let idx = self.get_cursor_char_idx();

                        // step-over --- if we type a closing char and it matches what's directly to the right, just step over it
                        let is_step_over = (c == ')' || c == ']' || c == '}' || c == '"' || c == '\'')
                            && idx < self.buffer.len_chars()
                            && self.buffer.char(idx) == c;

                        if is_step_over {
                            self.move_right();
                        } else {
                            self.snapshot();
                            self.delete_selection();

                            // electric character logic
                            if c == '}' || c == ']' || c == ')' {
                                let line_start_char = self.buffer.line_to_char(self.cursor_y);
                                let current_idx = self.get_cursor_char_idx();
                                let text_before_cursor = self.buffer.slice(line_start_char..current_idx).to_string();

                                if text_before_cursor.trim().is_empty() && !text_before_cursor.is_empty() {
                                    let spaces_to_remove = text_before_cursor
                                        .chars()
                                        .rev()
                                        .take(4)
                                        .take_while(|&ch| ch == ' ')
                                        .count();

                                    if spaces_to_remove > 0 {
                                        self.buffer.remove((current_idx - spaces_to_remove)..current_idx);
                                        self.cursor_x -= spaces_to_remove;
                                    }
                                }
                            }

                            let final_idx = self.get_cursor_char_idx();
                            self.buffer.insert_char(final_idx, c);
                            self.cursor_x += 1;
                            self.desired_cursor_x = self.cursor_x;

                            // auto-close
                            let closer = match c {
                                '(' => Some(')'),
                                '[' => Some(']'),
                                '{' => Some('}'),
                                '"' => Some('"'),
                                '\'' => Some('\''),
                                _ => None,
                            };

                            if let Some(close_char) = closer {
                                // only auto-close if the next character is whitespace, punctuation, or EOF
                                let next_idx = final_idx + 1;
                                let next_char = if next_idx < self.buffer.len_chars() {
                                    self.buffer.char(next_idx)
                                } else {
                                    ' ' // Treat End-Of-File as safe
                                };

                                let is_safe_to_close = next_char.is_whitespace() || next_char.is_ascii_punctuation();

                                if is_safe_to_close {
                                    self.buffer.insert_char(next_idx, close_char);
                                }
                            }

                            self.mark_modified();
                        }
                    }

                    KeyCode::Enter => {
                        self.snapshot();
                        self.delete_selection();

                        let idx = self.get_cursor_char_idx();

                        // indent code --- read current line up to cursor
                        let line_start_char = self.buffer.line_to_char(self.cursor_y);
                        let text_before_cursor = self.buffer.slice(line_start_char..idx).to_string();

                        // extract leading whitespace
                        let mut whitespace: String = text_before_cursor
                            .chars()
                            .take_while(|c| *c == ' ' || *c == '\t')
                            .collect();

                        // check for a block opener at the end of the string
                        if let Some(last_char) = text_before_cursor.trim_end().chars().last() {
                            if last_char == '{' || last_char == '[' || last_char == '(' || last_char == ':' {
                                whitespace.push_str("    "); // Add 4 spaces of indentation
                            }
                        }

                        // build the final string to insert
                        let mut payload = String::from('\n');
                        payload.push_str(&whitespace);

                        // insert and update cursor
                        self.buffer.insert(idx, &payload);
                        self.cursor_y += 1;
                        self.cursor_x = whitespace.chars().count();
                        self.desired_cursor_x = self.cursor_x;

                        self.mark_modified();
                    }

                    KeyCode::Backspace => {
                        self.snapshot();
                        if !self.delete_selection() {
                            let idx = self.get_cursor_char_idx();
                            if idx > 0 {
                                // pair delete
                                let char_behind = self.buffer.char(idx - 1);
                                let char_at_cursor = if idx < self.buffer.len_chars() {
                                    Some(self.buffer.char(idx))
                                } else {
                                    None
                                };

                                // check if the cursor is sandwiched between an empty pair
                                let is_empty_pair = match (char_behind, char_at_cursor) {
                                    ('(', Some(')')) | ('[', Some(']')) | ('{', Some('}')) | ('"', Some('"')) | ('\'', Some('\'')) => true,
                                    _ => false,
                                };

                                if is_empty_pair {
                                    // delete both the open and the close using a multi-character range
                                    self.buffer.remove((idx - 1)..=idx);
                                } else {
                                    // single-character delete
                                    self.buffer.remove((idx - 1)..idx);
                                }

                                // adjust the cursor exactly one space backward regardless of whether we deleted 1 or 2 chars
                                self.cursor_y = self.buffer.char_to_line(idx - 1);
                                self.cursor_x = (idx - 1) - self.buffer.line_to_char(self.cursor_y);
                                self.desired_cursor_x = self.cursor_x;
                                self.mark_modified();
                            }
                        }
                    }
                    KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::ALT) => {
                        self.toggle_line_mark();
                    }
                    KeyCode::F(11) | KeyCode::Char(',') if key.modifiers.contains(KeyModifiers::ALT) => {
                        self.goto_prev_mark()?;
                    }
                    KeyCode::F(12) | KeyCode::Char('.') if key.modifiers.contains(KeyModifiers::ALT) => {
                        self.goto_next_mark()?;
                    }
                    KeyCode::Char('u') if is_alt => self.undo(),
                    KeyCode::Char('r') if is_alt => self.redo(),
                    KeyCode::Char('<') if is_alt => self.cycle_prev_buffer(),
                    KeyCode::Char('>') if is_alt => self.cycle_next_buffer(),
                    KeyCode::Char('i') if is_alt => self.open_new_buffer()?,
                    _ => { self.clear_status(); }
                }
                self.previous_action_was_cut = current_action_is_cut;
            }
            Event::Mouse(mouse) => {
                match mouse.kind {
                    // mouse wheel scrolling
                    MouseEventKind::ScrollUp => {
                        for _ in 0..3 { self.move_up(); }
                    }
                    MouseEventKind::ScrollDown => {
                        for _ in 0..3 { self.move_down(); }
                    }
                    // left click action
                    MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                        let (cols, rows) = terminal::size()?;

                        // if clicked on the bottom two rows (where the menu lives)
                        if mouse.row >= rows.saturating_sub(2) {

                            // only want to trigger a menu action on the initial click
                            if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                                let is_top_row = mouse.row == rows.saturating_sub(2);
                                let col_width = (cols as usize) / 6;
                                let col_idx = (mouse.column as usize) / col_width;

                                // reset the cut tracker for all buttons by default
                                self.previous_action_was_cut = false;

                                if col_idx < 6 {
                                    match self.menu_state {
                                        MenuState::Menu1 => {
                                            if is_top_row {
                                                match col_idx {
                                                    0 => { self.save_file()?; }           // ^O
                                                    1 => { self.read_file()?; }           // ^R
                                                    2 => { self.justify(); }              // ^J
                                                    3 => {                                        // ^K
                                                        self.cut_line();
                                                        self.previous_action_was_cut = true;
                                                        self.adjust_marks_on_delete(self.cursor_y);
                                                    }
                                                    4 => { self.toggle_mark(); }          // M+A
                                                    5 => { self.menu_state = MenuState::Menu2; }  // M+O
                                                    _ => {}
                                                }
                                            } else {
                                                match col_idx {
                                                    0 => { self.exit_editor()?; }    // ^X
                                                    1 => { self.where_is()?; }       // ^W
                                                    2 => { self.replace()?; }        // ^\
                                                    3 => { self.copy_selection(); }  // ^L
                                                    4 => { self.paste_line(); }      // ^U
                                                    5 => { self.show_help()?; }      // ^H
                                                    _ => {}
                                                }
                                            }
                                        }
                                        MenuState::Menu2 => {
                                            if is_top_row {
                                                match col_idx {
                                                    0 => { self.undo(); }                // M+U
                                                    1 => { self.cur_pos(); }             // ^C
                                                    2 => { self.spell_check()?; }        // ^T
                                                    3 => { self.toggle_line_mark(); }    // M+M
                                                    4 => { self.goto_prev_mark()?; }     // M+,
                                                    5 => { self.menu_state = MenuState::Menu4; } // M+O
                                                    _ => {}
                                                }
                                            } else {
                                                match col_idx {
                                                    0 => { self.redo(); }              // M+R
                                                    1 => { self.go_to_line()?; }       // ^L
                                                    2 => { self.delete_char(); }       // ^D
                                                    4 => { self.goto_next_mark()?; }   // M+.
                                                    5 => { self.show_help()?; }        // ^H
                                                    _ => {}
                                                }
                                            }
                                        }
                                        MenuState::Menu3 => {
                                            if is_top_row {
                                                match col_idx {
                                                    0 => { self.move_up(); }               // ^P
                                                    1 => { self.page_up()?; }              // ^Y
                                                    2 => { self.move_left(); }             // ^B
                                                    3 => { self.move_word_left(); }        // M+B
                                                    4 => { self.move_to_start_of_line(); } // ^A
                                                    5 => { self.menu_state = MenuState::Menu3; }   // M+O
                                                    _ => {}
                                                }
                                            } else {
                                                match col_idx {
                                                    0 => { self.move_down(); }           // ^N
                                                    1 => { self.page_down()?; }          // ^V
                                                    2 => { self.move_right(); }          // ^F
                                                    3 => { self.move_word_right(); }     // M+F
                                                    4 => { self.move_to_end_of_line(); } // ^E
                                                    5 => { self.show_help()?; }          // ^H
                                                    _ => {}
                                                }
                                            }
                                        }
                                        MenuState::Menu4 => {
                                            if is_top_row {
                                                match col_idx {
                                                    0 => {                                        // M+S
                                                        self.soft_wrap = !self.soft_wrap;
                                                        self.save_config();
                                                        self.set_status(if self.soft_wrap { "Soft wrap enabled".into() } else { "Soft wrap disabled".into() });
                                                    }
                                                    1 => { self.cycle_theme(); }          // M+T
                                                    2 => { self.open_new_buffer()?; }     // M+I
                                                    3 => { self.cycle_next_buffer() }     // M+>
                                                    4 => {                                        // M+V
                                                        if self.update_version.is_some() {
                                                            let _ = webbrowser::open("https://github.com/mabognar/xnano/releases/latest");
                                                            self.set_status(String::from("Opened browser to download update."));
                                                        } else {
                                                            self.set_status(String::from("xnano is the latest version"));
                                                        }
                                                    }
                                                    5 => { self.menu_state = MenuState::Menu1; }  // M+O
                                                    _ => {}
                                                }
                                            } else {
                                                match col_idx {
                                                    0 => {                                        // M+L
                                                        self.show_line_numbers = !self.show_line_numbers;
                                                        self.save_config();
                                                        self.set_status(if self.show_line_numbers { "Line numbers enabled".into() } else { "Line numbers disabled".into() });
                                                    }
                                                    1 => { self.toggle_mouse(); }         // M+E
                                                    3 => { self.cycle_prev_buffer() }     // M+<
                                                    5 => { self.show_help()?; }           // ^H
                                                    _ => {}
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        else {
                            let visible_rows = rows.saturating_sub(3) as u16;

                            if mouse.row > 0 && mouse.row <= visible_rows {
                                if self.soft_wrap {
                                    self.set_status(String::from("Mouse jump disabled while Soft Wrap is active"));
                                } else {
                                    let max_line_num_len = self.buffer.len_lines().to_string().len();
                                    let gutter_width = if self.show_line_numbers {
                                        max_line_num_len + 2
                                    } else if !self.line_marks.is_empty() {
                                        2
                                    } else {
                                        0
                                    } as u16;

                                    let clicked_y = self.row_offset + (mouse.row.saturating_sub(1) as usize);
                                    let target_y = clicked_y.min(self.buffer.len_lines().saturating_sub(1));

                                    // click inside the gutter?
                                    if mouse.column < gutter_width {
                                        // Only toggle on the initial click to prevent drag-spamming
                                        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                                            if self.line_marks.contains(&target_y) {
                                                self.line_marks.remove(&target_y);
                                                self.set_status(format!("Mark removed from line {}", target_y + 1));
                                            } else {
                                                self.line_marks.insert(target_y);
                                                self.set_status(format!("Mark set on line {}", target_y + 1));
                                            }
                                        }
                                    }
                                    // otherwise, clicked the text area
                                    else {
                                        let target_visual_x = self.col_offset + mouse.column.saturating_sub(gutter_width) as usize;

                                        let line = self.buffer.line(target_y);
                                        let mut current_visual_x = 0;
                                        let mut char_x = 0;

                                        for ch in line.chars() {
                                            if ch == '\n' || ch == '\r' { break; }
                                            if current_visual_x >= target_visual_x { break; }

                                            if ch == '\t' {
                                                current_visual_x += 4 - (current_visual_x % 4);
                                            } else {
                                                current_visual_x += 1;
                                            }
                                            char_x += 1;

                                            if mouse.kind == event::MouseEventKind::Down(event::MouseButton::Left) {
                                                // a standard click clears any existing selection
                                                self.mark = None;
                                            } else if mouse.kind == event::MouseEventKind::Drag(event::MouseButton::Left) {
                                                // dragging starts a selection from the cursor's current position
                                                // (where the click started) if we don't already have one active
                                                if self.mark.is_none() {
                                                    self.mark = Some(self.get_cursor_char_idx());
                                                }
                                            }
                                        }

                                        self.cursor_y = target_y;
                                        self.cursor_x = char_x;
                                        self.desired_cursor_x = self.cursor_x;
                                        self.highlight_match = None;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        self.scroll()?;
        Ok(())
    }
    
    pub(crate) fn toggle_mouse(&mut self) {
        self.mouse_enabled = !self.mouse_enabled;
        let mut stdout = io::stdout();

        if self.mouse_enabled {
            let _ = execute!(stdout, EnableMouseCapture);
            self.set_status(String::from("Mouse capture enabled"));
        } else {
            let _ = execute!(stdout, DisableMouseCapture);
            self.set_status(String::from("Mouse capture disabled. Native OS copy/paste enabled."));
        }

        self.save_config();
    }

}

