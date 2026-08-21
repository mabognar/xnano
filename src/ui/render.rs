use crate::editor::{Editor, MenuState, MovementExt};
use crate::config::ConfigExt;
use crossterm::{
    cursor,
    queue,
    style::{Color, Print, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::Style;
use std::io::{self, stdout, Write};
use std::env;
use std::fs;

pub trait RenderExt {
    fn draw_menu_line(writer: &mut io::Stdout, row: u16, cols: u16, col_width: usize,
                      items: &[(&str, &str)], ui_bg: Color, key_fg: Color, text_fg: Color) -> io::Result<()>;
    fn draw_screen(&mut self) -> io::Result<()>;
    fn get_soft_wrap_metrics(line_chars: &[char], target_visual_x: Option<usize>, available_width: usize) -> (usize, usize, usize);
}

impl RenderExt for Editor {

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
        let visible_rows = rows.saturating_sub(3) as usize;

        let theme = &self.theme_set.themes[&self.current_theme];
        let colors = Self::get_theme_colors(theme);

        let ui_bg = colors.menu_bg;
        // let title_fg = if colors.is_dark { Color::Reset } else { Color::Black };
        let menu_key_fg = colors.accent;
        let menu_text_fg = colors.fg;

        let dollar_bg = if colors.is_dark { Color::Rgb { r: 180, g: 180, b: 180 } } else { Color::Rgb { r: 80, g: 80, b: 80 } };
        let dollar_fg = if colors.is_dark { Color::Black } else { Color::White };

        let version = env!("CARGO_PKG_VERSION");
        let title = format!("xnano ({})   ", version);

        queue!(stdout, cursor::MoveTo(0, 0), SetBackgroundColor(ui_bg))?;

        let file_display_string = match self.filename.as_deref() {
            Some(name) => {
                let path = std::path::Path::new(name);
                if path.is_absolute() {
                    name.to_string()
                } else if let Ok(cwd) = env::current_dir() {
                    let full_path = cwd.join(path);
                    fs::canonicalize(&full_path)
                        .unwrap_or(full_path)
                        .to_string_lossy()
                        .into_owned()
                } else {
                    name.to_string()
                }
            }
            None => String::from("New Buffer"),
        };

        let file_section = format!("{}", file_display_string);
        let mod_star = if self.is_modified { "*" } else { "" };

        let total_bufs = self.inactive_buffers.len() + 1;
        let buf_indicator = if total_bufs > 1 {
            format!("Buf:{}/{} ", self.active_buffer_index + 1, total_bufs)
        } else {
            String::new()
        };

        let cursor_str = format!("   {}L:{} C:{}", buf_indicator, self.cursor_y + 1, self.cursor_x + 1);

        let indicators_len = cursor_str.chars().count() + 5;

        let max_allowable_len = (cols as usize).saturating_sub(indicators_len);
        let full_len = title.chars().count() + file_section.chars().count() + mod_star.chars().count();

        // truncate only if the file path overflows, making room for the asterisk
        let mut final_file_section = file_section.clone();
        if full_len > max_allowable_len {
            let allowed_file_len = max_allowable_len.saturating_sub(title.chars().count() + mod_star.chars().count());
            if allowed_file_len > 1 {
                // Calculate how many characters from the END of the path we can keep
                let keep_len = allowed_file_len.saturating_sub(1);
                let skip_len = file_section.chars().count().saturating_sub(keep_len);
                // Skip the beginning characters and collect the tail
                let tail: String = file_section.chars().skip(skip_len).collect();
                // Prepend the single ellipsis character
                final_file_section = format!("…{}", tail);
            } else {
                final_file_section = String::new();
            }
        }

        let printed_left_len = title.chars().count() + final_file_section.chars().count() + mod_star.chars().count();
        let pad2_len = (cols as usize).saturating_sub(printed_left_len + indicators_len);
        let pad2 = " ".repeat(pad2_len);

        // let inactive_color = Color::Rgb { r: 128, g: 128, b: 128 }; // Muted grey
        let inactive_color = if colors.is_dark {
            Color::Rgb { r: 100, g: 100, b: 100 } // grey for dark themes
        } else {
            Color::Rgb { r: 180, g: 180, b: 180 } // grey for light themes
        };
        let s_color = if self.soft_wrap { colors.indicator } else { inactive_color };
        let l_color = if self.show_line_numbers { colors.indicator } else { inactive_color };
        let m_color = if !self.line_marks.is_empty() { colors.indicator } else { inactive_color };
        let e_color = if self.mouse_enabled { colors.indicator } else { inactive_color };

        queue!(
            stdout,
            SetForegroundColor(menu_key_fg),
            Print(&title),
            SetForegroundColor(colors.indicator),
            Print(&final_file_section),
            Print(mod_star),
            Print(pad2),
            SetForegroundColor(menu_key_fg),
            Print(&cursor_str),
            Print(" "), // Single space separating the cursor position from the indicators
            SetForegroundColor(s_color),
            Print("S"),
            SetForegroundColor(l_color),
            Print("L"),
            SetForegroundColor(m_color),
            Print("M"),
            SetForegroundColor(e_color),
            Print("E"),
            // Reset colors for the rest of the screen
            SetForegroundColor(Color::Reset),
            SetBackgroundColor(Color::Reset)
        )?;

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

        let default_cross_bg = colors.bg;

        let max_line_num_len = self.buffer.len_lines().to_string().len();
        let gutter_width = if self.show_line_numbers {
            max_line_num_len + 2
        } else if !self.line_marks.is_empty() {
            2
        } else {
            0
        };
        // let gutter_width = if self.show_line_numbers { max_line_num_len + 2 } else { 0 };
        let available_width = (cols as usize).saturating_sub(gutter_width).saturating_sub(1);

        let cursor_absolute = self.get_cursor_char_idx();
        let mark_range = self.mark.map(|m| {
            if m < cursor_absolute { (m, cursor_absolute) } else { (cursor_absolute, m) }
        });

        let mut last_fg: Option<Color> = None;
        let mut last_bg: Option<Color> = None;
        let mut fallback_highlighter = None;

        let mut terminal_y = 0;
        let mut file_y = self.row_offset;

        while terminal_y < visible_rows {
            if file_y < self.buffer.len_lines() {
                // instantiating highlightLines once per frame and reuse
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
                let line_chars: Vec<char> = self.buffer.line(file_y).chars().filter(|c| *c != '\n' && *c != '\r').collect();

                let mut visual_x = 0;
                let mut line_char_idx = 0;
                let line_has_search_highlight = self.highlight_match.map_or(false, |(h_y, _, _)| h_y == file_y);

                queue!(stdout, cursor::MoveTo(0, (terminal_y + 1) as u16))?;

                if gutter_width > 0 {
                    // check for the mark
                    let mark_indicator = if self.line_marks.contains(&file_y) { "*" } else { " " };

                    // format the gutter string depending on the line-number setting
                    let gutter_str = if self.show_line_numbers {
                        format!("{:>width$}{} ", file_y + 1, mark_indicator, width = max_line_num_len)
                    } else {
                        format!("{} ", mark_indicator)
                    };

                    // print the gutter
                    if last_bg != Some(default_cross_bg) { queue!(stdout, SetBackgroundColor(default_cross_bg))?; last_bg = Some(default_cross_bg); }
                    if last_fg != Some(menu_key_fg) { queue!(stdout, SetForegroundColor(menu_key_fg))?; last_fg = Some(menu_key_fg); }
                    queue!(stdout, Print(gutter_str))?;
                }

                let mut printed_on_current_line = 0;

                'char_loop: for (style, text) in ranges {
                    let syn_color = style.foreground;
                    let cross_color = Color::Rgb { r: syn_color.r, g: syn_color.g, b: syn_color.b };
                    let syn_bg = style.background;
                    let cross_bg = Color::Rgb { r: syn_bg.r, g: syn_bg.g, b: syn_bg.b };

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

                        let is_wrap_space = self.soft_wrap
                            && (printed_on_current_line == 0 || printed_on_current_line >= available_width)
                            && line_char_idx > 0
                            && ch.is_whitespace();

                        if is_wrap_space {
                            line_char_idx += 1;
                            continue;
                        }

                        if self.soft_wrap && printed_on_current_line > 0 && printed_on_current_line < available_width {
                            let is_start_of_word = line_char_idx > 0
                                && line_chars.get(line_char_idx - 1).map_or(false, |c| c.is_whitespace())
                                && !ch.is_whitespace();

                            if is_start_of_word {
                                let mut word_width = 0;
                                let mut peek_idx = line_char_idx;
                                while peek_idx < line_chars.len() && !line_chars[peek_idx].is_whitespace() {
                                    word_width += 1;
                                    peek_idx += 1;
                                }

                                if printed_on_current_line + word_width > available_width {
                                    printed_on_current_line = available_width;
                                }
                            }
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
                                    // re-apply styles after clearing line
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
                        queue!(stdout, cursor::MoveTo(cols - 1, (terminal_y + 1) as u16), Print('$'))?;
                    }
                }

                // reset before moving to the next line
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

        if !self.status_message.is_empty() {
            queue!(stdout, cursor::MoveTo(0, rows - 3))?;
            queue!(
                stdout,
                SetBackgroundColor(ui_bg),
                SetForegroundColor(menu_key_fg)
            )?;

            let mut printed_len = 0;

            let status_text = format!("{}", self.status_message);
            queue!(stdout, SetForegroundColor(menu_key_fg), Print(&status_text))?;
            printed_len += status_text.len();

            let status_fill = " ".repeat((cols as usize).saturating_sub(printed_len));
            queue!(
                stdout,
                Print(status_fill),
                SetBackgroundColor(Color::Reset),
                SetForegroundColor(Color::Reset)
            )?;
        }

        let col_width = (cols as usize) / 6;

        match self.menu_state {
            MenuState::Menu1 => {

                let cut_label = if self.mark.is_some() {
                    " Cut Sel"
                } else {
                    " Cut Line"
                };

                let copy_label = if self.mark.is_some() {
                    " Copy Sel"
                } else {
                    " Copy Line"
                };

                let mark_label = if self.mark.is_some() {
                    " Unmark"
                } else {
                    " Mark Beg"
                };

                let menu1 = [
                    ("^O", " Write Out"), ("^R", " Read File"), ("^J", " Justify"),
                    ("^K", cut_label), ("M+A", mark_label),
                    ("M+O", " Other 1/4")
                ];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;

                let menu2 = [
                    ("^X", " Exit"), ("^W", " Where Is"), ("^\\", " Replace"), ("^C", copy_label), (" ^V", " Paste"), (" ^H", " Help")
                ];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
            MenuState::Menu2 => {
                let menu1 = [
                    ("M+U", " Undo"), ("M+C", " Cur Pos"), ("^T", " To Spell"), ("M+M", " Line Mrk"), ("M+,", " Prev Mrk"), ("M+O", " Other 2/4")
                ];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;
                let menu2 = [
                    ("M+R", " Redo"), (" ^L", " To Line"), ("^D", " Delete"), ("", ""), ("M+.", " Next Mrk"), (" ^H", " Help")
                ];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
            MenuState::Menu3 => {
                let menu1 = [
                    ("^P", " Prv Lne"), ("  ^Y", " Prv Pg"), ("^B", " Back Chr"), ("M+B", " Back Wrd"), ("^A", " Beg Line"), ("M+O", " Other 3/4")
                ];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;
                let menu2 = [
                    ("^N", " Nxt Line"), ("^Spc", " Nxt Pg"), ("^F", " Frwd Chr"), ("M+F", " Frwd Wrd"), ("^E", " End Line"), (" ^H", " Help")
                ];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
            MenuState::Menu4 => {
                let menu1 = [
                    ("M+S", " Soft Wrp"), ("M+T", " Theme"), ("M+I", " New Buf"), ("M+>", " Next Buf"), ("M+V", " Update"), ("M+O", " Other 4/4")
                ];
                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1, ui_bg, menu_key_fg, menu_text_fg)?;
                let menu2 = [
                    ("M+L", " Line Num"), ("M+E", " Mouse"), ("", ""), ("M+<", " Prev Buf"), ("", ""), (" ^H", " Help")
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
                let col_width = (cols as usize) / 5;

                // Dynamically build the suggestions for Menu 1
                let keys = ["1", "2", "3", "4", "5"];
                let formatted_sugs: Vec<String> = self.current_suggestions
                    .iter()
                    .map(|s| format!(" {}", s))
                    .collect();

                let mut menu1_items: Vec<(&str, &str)> = Vec::new();

                if formatted_sugs.is_empty() {
                    menu1_items.push(("", "No suggestions"));
                } else {
                    for (i, sug) in formatted_sugs.iter().enumerate() {
                        let key = keys.get(i).unwrap_or(&"");
                        menu1_items.push((key, sug.as_str()));
                    }
                }

                Self::draw_menu_line(&mut stdout, rows - 2, cols, col_width, &menu1_items, ui_bg, menu_key_fg, menu_text_fg)?;

                // Define the fixed commands for Menu 2
                let menu2 = [("^C", " Cancel"), ("I", " Ignore"), ("A", " Add to Dict")];
                Self::draw_menu_line(&mut stdout, rows - 1, cols, col_width, &menu2, ui_bg, menu_key_fg, menu_text_fg)?;
            }
        }

        let cursor_screen_y;
        let cursor_screen_x;

        if self.soft_wrap {
            let mut temp_screen_y = 0;

            for i in self.row_offset..self.cursor_y {
                let line_str = self.buffer.line(i).to_string();
                let chars: Vec<char> = line_str.chars().filter(|c| *c != '\n' && *c != '\r').collect();
                let (lines, _, _) = Self::get_soft_wrap_metrics(&chars, None, available_width);
                temp_screen_y += lines;
            }

            let cursor_line_str = self.buffer.line(self.cursor_y).to_string();
            let cursor_chars: Vec<char> = cursor_line_str.chars().filter(|c| *c != '\n' && *c != '\r').collect();
            let cursor_visual = self.get_visual_cursor_x();

            let (_, target_y, target_x) = Self::get_soft_wrap_metrics(&cursor_chars, Some(cursor_visual), available_width);

            temp_screen_y += target_y;
            cursor_screen_x = gutter_width + target_x;
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

    
    fn get_soft_wrap_metrics(line_chars: &[char], target_visual_x: Option<usize>, available_width: usize) -> (usize, usize, usize) {
        let mut current_y = 0;
        let mut current_x = 0;
        let mut target_y = 0;
        let mut target_x = 0;
        let mut visual_x = 0;

        for (i, &ch) in line_chars.iter().enumerate() {
            let is_start_of_word = i > 0 && line_chars[i - 1].is_whitespace() && !ch.is_whitespace();
            if is_start_of_word && current_x > 0 && current_x < available_width {
                let mut word_width = 0;
                let mut peek_idx = i;
                while peek_idx < line_chars.len() && !line_chars[peek_idx].is_whitespace() {
                    word_width += 1;
                    peek_idx += 1;
                }
                if current_x + word_width > available_width {
                    current_x = available_width; // Force wrap
                }
            }

            // skip spaces that would appear at the start of a wrapped line
            let is_wrap_space = (current_x == 0 || current_x >= available_width) && i > 0 && ch.is_whitespace();
            let display_chars = if ch == '\t' { 4 - (visual_x % 4) } else { 1 };

            if is_wrap_space {
                // cursor tracking
                for _ in 0..display_chars {
                    if Some(visual_x) == target_visual_x {
                        target_y = if current_x >= available_width { current_y + 1 } else { current_y };
                        target_x = 0;
                    }
                    visual_x += 1;
                }
                continue;
            }

            for _ in 0..display_chars {
                if Some(visual_x) == target_visual_x {
                    if current_x >= available_width {
                        target_y = current_y + 1;
                        target_x = 0;
                    } else {
                        target_y = current_y;
                        target_x = current_x;
                    }
                }

                if current_x >= available_width {
                    current_y += 1;
                    current_x = 0;
                }

                current_x += 1;
                visual_x += 1;
            }
        }

        if Some(visual_x) == target_visual_x {
            if current_x >= available_width {
                target_y = current_y + 1;
                target_x = 0;
            } else {
                target_y = current_y;
                target_x = current_x;
            }
        }

        (current_y + 1, target_y, target_x)
    }

}