use crate::editor::Editor;
use crate::config::ConfigExt;
use crate::ui::RenderExt;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    queue,
    style::{Print, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::io::{self, stdout, Write};

pub trait HelpExt {
    fn clear_status(&mut self);
    fn set_status(&mut self, message: String);
    fn show_help(&mut self) -> io::Result<()>;
}

impl HelpExt for Editor {


    fn clear_status(&mut self) {
        self.status_message.clear();
        self.status_time = None;
    }


    fn set_status(&mut self, message: String) {
        self.status_message = message;
        self.status_time = Some(std::time::Instant::now());
    }


    fn show_help(&mut self) -> io::Result<()> {
        let help_lines = [
            "  Notation:",
            "    ^T   ---> Ctrl+T ",
            "    M+T  ---> Meta+T (Alt+T)",
            "",
            "  - On MacOS, make sure you have 'Use Option as Meta' selected ",
            "    in your terminal settings",
            "",
            "  - Settings are stored in ~/.xnano/xnanorc",
            "  - Themes are stored in ~/.xnano/themes",
            "  - Additional .tmTheme themes can be added to ~/.xnano/themes",
            "",
            "  Movement:",
            "    ^P, Up                 Move up one line",
            "    ^N, Down               Move down one line",
            "    ^Y, M+P, PgUp, F7      Move up one page",
            "    ^Space, M+N, PgDn, F8  Move down one page",
            "    ^B, Left               Move left/back one character",
            "    ^F, Right              Move right/forward one character",
            "    M+B, M+Left            Move left/back one word",
            "    M+F, M+Right           Move right/forward one word",
            "    ^A                     Move to start of line",
            "    ^E                     Move to end of line",
            "    M+M                    Mark current line",
            "    M+,                    Move to previous mark",
            "    M+.                    Move to next mark",
            "",
            "  Editing:",
            "    M+U          Undo last action",
            "    M+R          Redo last undone action",
            "    ^C           Copy marked text or current line",
            "    ^K, F9       Cut current line into clipboard",
            "    ^V, ^U, F10  Paste contents of clipboard",
            "    ^D, Del      Delete character under cursor",
            "    Backspace    Delete character before cursor",
            "    ^J, F4       Justify current paragraph",
            "    ^I, Tab      Insert tab",
            "    ^^, M+A      Mark beginning of selected text.",
            "                 This key also unselects text.",
            "                 Note: ^^ = Ctrl+^ = Ctrl+Shift+6",
            "",
            "  Search & Replace:",
            "    ^W, F6  Where is (Search)",
            "    ^\\      Search and Replace",
            "",
            "  File & System:",
            "    ^O, F3  Write Out (Save)",
            "    ^R, F5  Read File (Insert)",
            "    ^H, F1  Get Help (this screen)",
            "    ^X, F2  Exit xnano",
            "",
            "  Tools:",
            "    M+C, F11  Current Position",
            "    ^L        Go to line number",
            "    ^T, F12   To Spell (Spell check)",
            "              Does NOT work in Windows",
            "    M+T       Cycle Syntax Theme",
            "    M+L       Toggle Line Numbers",
            "    M+S       Toggle Soft Wrap",
            "    M+O       Toggle Menu Pages",
            "    M+E       Toggle Mouse Capture",
            " ",
            "  Buffers:",
            "    M+I     New buffer",
            "    M+>,    Next buffer",
            "    M+<,    Previous buffer",
            "",
            "  Written by: Matt Bognar, https://github.com/mabognar",
            " ",
        ];

        let mut scroll_offset = 0;

        let theme = &self.theme_set.themes[&self.current_theme];
        let colors = Self::get_theme_colors(theme);

        let theme_bg = colors.bg;
        let theme_fg = colors.fg;

        let ui_bg = colors.menu_bg;
        let menu_key_fg = colors.accent;
        let menu_text_fg = colors.fg;

        loop {
            let mut stdout = stdout();
            let (cols, rows) = terminal::size()?;
            let visible_rows = rows.saturating_sub(3) as usize;

            queue!(stdout, SetBackgroundColor(theme_bg), terminal::Clear(ClearType::All))?;

            queue!(stdout, cursor::MoveTo(0, 0),
                SetBackgroundColor(ui_bg), SetForegroundColor(menu_key_fg))?;

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

                    let mut split_idx = None;

                    // process lines that start with 4 spaces
                    if truncated.starts_with("    ") && !truncated.starts_with("     ") {
                        let mut found_double_space = false;
                        let mut prev_char_was_space = false;

                        for (idx, c) in truncated.char_indices().skip(4) {
                            if c == ' ' {
                                if prev_char_was_space {
                                    // double space indicates hot-key is finished
                                    found_double_space = true;
                                }
                                prev_char_was_space = true;
                            } else {
                                // description text
                                if found_double_space {
                                    split_idx = Some(idx);
                                    break;
                                }
                                prev_char_was_space = false;
                            }
                        }
                    }

                    if let Some(idx) = split_idx {
                        // split the line where the description starts
                        let (hotkey, desc) = truncated.split_at(idx);
                        queue!(
                            stdout,
                            SetForegroundColor(menu_key_fg),
                            Print(hotkey),
                            SetForegroundColor(theme_fg),
                            Print(desc)
                        )?;
                    } else {
                        // standard text
                        queue!(stdout, SetForegroundColor(theme_fg), Print(truncated))?;
                    }
                }

                queue!(stdout, terminal::Clear(ClearType::UntilNewLine))?;
            }

            let col_width = (cols as usize) / 6;

            let menu1 = [("",""), ("  ^Y", " Prev Pg")];
            Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

            let menu2 = [("^X", " Exit Help"), ("^Spc", " Next Pg")];
            Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;

            stdout.flush()?;

            let ev = event::read()?;
            match ev {
                Event::Key(key) => {
                    if key.kind != event::KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('h') | KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
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
                        KeyCode::Char(' ') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            let max_offset = help_lines.len().saturating_sub(visible_rows);
                            scroll_offset = (scroll_offset + visible_rows).min(max_offset);
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
                    match mouse.kind {
                        event::MouseEventKind::ScrollUp => {
                            scroll_offset = scroll_offset.saturating_sub(3);
                        }
                        event::MouseEventKind::ScrollDown => {
                            let max_offset = help_lines.len().saturating_sub(visible_rows);
                            scroll_offset = (scroll_offset + 3).min(max_offset);
                        }
                        event::MouseEventKind::Down(event::MouseButton::Left) => {
                            let (cols, rows) = terminal::size()?;

                            // Check if the user clicked the bottom two rows
                            if mouse.row >= rows - 2 {
                                let is_top = mouse.row == rows - 2;
                                let col_width = (cols as usize) / 6;
                                let col_idx = (mouse.column as usize) / col_width;

                                if is_top {
                                    if col_idx == 1 {
                                        // ^Y Prev Pg
                                        scroll_offset = scroll_offset.saturating_sub(visible_rows);
                                    }
                                } else {
                                    if col_idx == 0 {
                                        // ^X Exit Help
                                        break;
                                    } else if col_idx == 1 {
                                        // ^V Next Pg
                                        let max_offset = help_lines.len().saturating_sub(visible_rows);
                                        scroll_offset = (scroll_offset + visible_rows).min(max_offset);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        self.clear_status();
        self.draw_screen()?;
        Ok(())
    }

}