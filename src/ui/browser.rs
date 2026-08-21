use crate::editor::Editor;
use crate::config::ConfigExt;
use crate::ui::{RenderExt, PromptExt, HelpExt};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    queue,
    style::{Color, Print, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::env;
use std::fs;
use std::io::{self, stdout, Write};
use std::path::PathBuf;


pub trait BrowserExt {
    fn run_file_browser(&mut self, is_save_context: bool) -> io::Result<Option<String>>;
}


impl BrowserExt for Editor {

    fn run_file_browser(&mut self, is_save_context: bool) -> io::Result<Option<String>> {
        // start with the current working directory as the default fallback
        let mut current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // if a file is currently open, resolve its absolute path and target its parent directory
        if let Some(ref fname) = self.filename {
            let path = std::path::Path::new(fname);

            let full_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                current_dir.join(path)
            };

            if let Some(parent) = full_path.parent() {
                current_dir = parent.to_path_buf();
            }
        }

        // canonicalize the directory to resolve any ".." or symlinks
        if let Ok(canon) = current_dir.canonicalize() {
            current_dir = canon;
        }

        let mut selected = 0;
        let mut scroll = 0;
        loop {
            let mut entries: Vec<(String, bool)> = Vec::new();

            // --- NEW: Only show '.' if we are in a save context ---
            if is_save_context {
                entries.push((String::from("."), true));
            }

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
                let visible_rows = rows.saturating_sub(3) as usize;

                if selected < scroll { scroll = selected; }
                if selected >= scroll + visible_rows { scroll = selected - visible_rows + 1; }

                let theme = &self.theme_set.themes[&self.current_theme];
                let colors = Self::get_theme_colors(theme);

                let default_cross_bg = colors.bg;
                let default_cross_fg = colors.fg;

                let ui_bg = colors.menu_bg;
                let title_fg = if colors.is_dark { Color::Reset } else { Color::Rgb { r: 0, g: 50, b: 150 } };
                let menu_key_fg = colors.accent;

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
                            queue!(stdout, SetBackgroundColor(colors.selected_bg), SetForegroundColor(colors.fg))?;
                        } else {
                            queue!(stdout, SetBackgroundColor(default_cross_bg), SetForegroundColor(default_cross_fg))?;
                        }

                        queue!(stdout, Print(format!("{}{}", truncated, padding)))?;
                    } else {
                        queue!(stdout, SetBackgroundColor(default_cross_bg), terminal::Clear(ClearType::UntilNewLine))?;
                    }
                }

                let menu_text_fg = colors.fg;
                let col_width = (cols as usize) / 6;

                let menu1 = [("Enter", " Select"), ("  ^Y", " Prev Pg")];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

                let menu2 = [("   ^C", " Cancel"), ("^Spc", " Next Pg"), ("", "")];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;

                stdout.flush()?;

                let ev = event::read()?;
                let mut trigger_enter = false;

                match ev {
                    Event::Key(key) => {
                        if key.kind != event::KeyEventKind::Press {
                            continue;
                        }
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
                            KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                let max_offset = entries.len().saturating_sub(1);
                                selected = (selected + visible_rows).min(max_offset);
                            }
                            KeyCode::Enter => {
                                trigger_enter = true;
                            }
                            _ => {}
                        }
                    }
                    Event::Mouse(mouse) => {
                        match mouse.kind {
                            event::MouseEventKind::ScrollUp => {
                                selected = selected.saturating_sub(1);
                            }
                            event::MouseEventKind::ScrollDown => {
                                if selected + 1 < entries.len() {
                                    selected += 1;
                                }
                            }
                            event::MouseEventKind::Down(event::MouseButton::Left) => {
                                let (cols, rows) = terminal::size()?;
                                let visible_rows = rows.saturating_sub(3) as usize;

                                // 1. Menu Clicks
                                if mouse.row >= rows - 2 {
                                    let is_top = mouse.row == rows - 2;
                                    let col_width = (cols as usize) / 6;
                                    let col_idx = (mouse.column as usize) / col_width;

                                    if col_idx < 6 {
                                        if is_top && col_idx == 1 {
                                            // ^Y Prev Pg
                                            selected = selected.saturating_sub(visible_rows);
                                        } else if !is_top {
                                            match col_idx {
                                                0 => return Ok(None), // ^C Cancel
                                                1 => { // ^V Next Pg
                                                    let max_offset = entries.len().saturating_sub(1);
                                                    selected = (selected + visible_rows).min(max_offset);
                                                }
                                                2 => trigger_enter = true, // Enter Select
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                                // 2. File List Clicks
                                else if mouse.row > 0 && (mouse.row as usize) <= visible_rows {
                                    let clicked_idx = scroll + (mouse.row as usize) - 1;
                                    if clicked_idx < entries.len() {
                                        if selected == clicked_idx {
                                            // act as a double-click if it's already selected
                                            trigger_enter = true;
                                        } else {
                                            selected = clicked_idx;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }

                if trigger_enter {
                    let (name, is_dir) = &entries[selected];
                    if *is_dir {
                        if name == "." {
                            let mut prefill = current_dir.to_string_lossy().into_owned();
                            if !prefill.ends_with(std::path::MAIN_SEPARATOR) {
                                prefill.push(std::path::MAIN_SEPARATOR);
                            }
                            // pass the prefill string to inline_prompt
                            if let Some(input) = self.inline_prompt("File name to write: ", &prefill)? {
                                let target = std::path::PathBuf::from(&input);
                                // validate that the edited path is valid
                                if let Some(parent) = target.parent() {
                                    if !parent.as_os_str().is_empty() && !parent.exists() {
                                        self.set_status(String::from("File not saved. Directory does not exist."));
                                        return Ok(None);
                                    }
                                }
                                return Ok(Some(input));
                            }
                            continue; // if escape or canceled, go back to the browser
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
            }
        }
    }

}