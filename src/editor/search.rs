use crate::editor::Editor;
use crate::editor::{EditExt, MovementExt};
use crate::ui::{HelpExt, PromptExt};
use std::io;

pub trait SearchExt {
    fn replace(&mut self) -> io::Result<()>;
    fn where_is(&mut self) -> io::Result<()>;
}

impl SearchExt for Editor {

    fn replace(&mut self) -> io::Result<()> {
        self.snapshot();
        let prompt_text = if let Some(ref last) = self.last_search {
            format!("Search to replace [{}]: ", last)
        } else {
            String::from("Search to replace: ")
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

            let re = match regex::Regex::new(&query) {
                Ok(re) => re,
                Err(_) => {
                    self.set_status(String::from("Invalid RegEx pattern"));
                    return Ok(());
                }
            };

            let replace_prompt = format!("Replace \"{}\" with: ", query);
            if let Some(replacement) = self.prompt(&replace_prompt, false)? {
                let mut current_idx = self.get_cursor_char_idx();
                let mut changes_made = 0;
                let mut replace_all = false;
                let mut wrapped = false;

                loop {
                    let text = self.buffer.to_string();
                    let current_byte = self.buffer.char_to_byte(current_idx);

                    if let Some(mat) = re.find_at(&text, current_byte) {
                        let start_idx = self.buffer.byte_to_char(mat.start());
                        let end_idx = self.buffer.byte_to_char(mat.end());

                        self.cursor_y = self.buffer.char_to_line(start_idx);
                        self.cursor_x = start_idx - self.buffer.line_to_char(self.cursor_y);
                        self.desired_cursor_x = self.cursor_x;
                        self.scroll()?;

                        // execute the Regex replace on this specific match chunk
                        let matched_text = mat.as_str();
                        let expanded_replacement = re.replace(matched_text, replacement.as_str()).into_owned();

                        if replace_all {
                            self.snapshot();
                            self.buffer.remove(start_idx..end_idx);
                            self.buffer.insert(start_idx, &expanded_replacement);
                            current_idx = start_idx + expanded_replacement.chars().count();
                            changes_made += 1;
                            self.mark_modified();
                            continue;
                        }

                        let match_len = end_idx - start_idx;
                        self.highlight_match = Some((self.cursor_y, self.cursor_x, self.cursor_x + match_len));

                        let prompt_result = self.prompt_replace("Replace this instance?");
                        self.highlight_match = None;

                        if let Some(action) = prompt_result? {
                            match action {
                                'y' => {
                                    self.snapshot();
                                    self.buffer.remove(start_idx..end_idx);
                                    self.buffer.insert(start_idx, &expanded_replacement);
                                    current_idx = start_idx + expanded_replacement.chars().count();
                                    changes_made += 1;
                                    self.mark_modified();
                                }
                                'n' => {
                                    current_idx = end_idx; // Move past this match
                                }
                                'a' => {
                                    self.snapshot();
                                    replace_all = true;
                                    self.buffer.remove(start_idx..end_idx);
                                    self.buffer.insert(start_idx, &expanded_replacement);
                                    current_idx = start_idx + expanded_replacement.chars().count();
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

            // compile the regex
            let re = match regex::Regex::new(&query) {
                Ok(re) => re,
                Err(_) => {
                    self.set_status(String::from("Invalid RegEx pattern"));
                    return Ok(());
                }
            };

            let mut current_idx = self.get_cursor_char_idx();
            let mut matches_found = 0;
            let mut wrapped = false;

            // if we are currently standing exactly on a match from a previous search,
            // bump our search forward by 1 character so we can continue cycling
            let initial_text = self.buffer.to_string();
            let initial_byte = self.buffer.char_to_byte(current_idx);

            if let Some(mat) = re.find_at(&initial_text, initial_byte) {
                if mat.start() == initial_byte {
                    if current_idx < self.buffer.len_chars() {
                        current_idx += 1;
                    }
                }
            }

            loop {
                let text = self.buffer.to_string();
                let current_byte = self.buffer.char_to_byte(current_idx);

                if let Some(mat) = re.find_at(&text, current_byte) {
                    let start_idx = self.buffer.byte_to_char(mat.start());
                    let end_idx = self.buffer.byte_to_char(mat.end());

                    // safely translate the found byte indices back to rope character coordinates
                    self.cursor_y = self.buffer.char_to_line(start_idx);
                    self.cursor_x = start_idx - self.buffer.line_to_char(self.cursor_y);
                    self.desired_cursor_x = self.cursor_x;
                    self.scroll()?;

                    let match_len = end_idx - start_idx;
                    self.highlight_match = Some((self.cursor_y, self.cursor_x, self.cursor_x + match_len));

                    // prompt the user to keep searching or stop
                    let prompt_str = format!("Find next instance \"{}\"? ", query);
                    let prompt_result = self.prompt_yn(&prompt_str);
                    self.highlight_match = None;
                    matches_found += 1;

                    if let Some(action) = prompt_result? {
                        if action {
                            // yes: advance past this match and loop again
                            current_idx = end_idx;
                            self.clear_status();
                        } else {
                            // no: stop at the current cursor location
                            self.clear_status();
                            break;
                        }
                    } else {
                        // cancel: stop at the current cursor location
                        self.set_status(String::from("Search cancelled"));
                        break;
                    }
                } else {
                    // no matches found from the cursor position downwards
                    if current_idx > 0 && !wrapped {
                        // wrap to the top of the document and search again
                        current_idx = 0;
                        wrapped = true;
                        self.set_status(String::from("Search wrapped to top"));
                    } else {
                        if matches_found == 0 {
                            self.set_status(format!("Pattern \"{}\" not found", query));
                        } else {
                            self.set_status(String::from("No more matches found"));
                        }
                        break;
                    }
                }
            }
        }
        Ok(())
    }

}