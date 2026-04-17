use crate::editor::{Editor, MenuState};
use crate::ui::UiExt;
use crate::config::ConfigExt; // Added so we can find the ~/.xnano/ directory
use std::collections::HashSet;
use std::fs::{File, OpenOptions}; // Added OpenOptions for appending to files
use std::io::{self, BufRead, BufReader, Write}; // Added Write for writeln!

pub trait SpellExt {
    fn load_dictionary() -> HashSet<String>;
    fn find_next_misspelled(&self, start_idx: usize) -> Option<(String, usize, usize)>;
    fn spell_check(&mut self) -> io::Result<()>;
    fn get_suggestions(word: &str, dict: &HashSet<String>) -> Vec<String>;
}

impl SpellExt for Editor {
    // fn load_dictionary() -> HashSet<String> {
    //     let mut dict = HashSet::new();
    //
    //     // 1. Load standard system dictionary
    //     let dict_paths = ["/usr/share/dict/words", "/usr/dict/words"];
    //
    //     // ... inside fn load_dictionary() ...
    //     for path in dict_paths {
    //         if let Ok(file) = File::open(path) {
    //             let reader = BufReader::new(file);
    //             // Change map_while to flatten() to skip invalid bytes
    //             for line in reader.lines().flatten() {
    //                 dict.insert(line.trim().to_lowercase());
    //             }
    //             break;
    //         }
    //     }
    //
    //     // Do the same for the custom persistent dictionary check below it:
    //     if let Some(mut custom_path) = Self::get_base_dir() {
    //         custom_path.push("custom_dict.txt");
    //         if let Ok(file) = File::open(&custom_path) {
    //             let reader = BufReader::new(file);
    //             for line in reader.lines().flatten() {
    //                 dict.insert(line.trim().to_lowercase());
    //             }
    //         }
    //     }
    //
    //     // for path in dict_paths {
    //     //     if let Ok(file) = File::open(path) {
    //     //         let reader = BufReader::new(file);
    //     //         for line in reader.lines().map_while(Result::ok) {
    //     //             dict.insert(line.trim().to_lowercase());
    //     //         }
    //     //         break;
    //     //     }
    //     // }
    //     //
    //     // // 2. Load custom persistent dictionary (if it exists)
    //     // if let Some(mut custom_path) = Self::get_base_dir() {
    //     //     custom_path.push("custom_dict.txt");
    //     //     if let Ok(file) = File::open(&custom_path) {
    //     //         let reader = BufReader::new(file);
    //     //         for line in reader.lines().map_while(Result::ok) {
    //     //             dict.insert(line.trim().to_lowercase());
    //     //         }
    //     //     }
    //     // }
    //
    //     dict
    // }
    fn load_dictionary() -> HashSet<String> {
        let mut dict = HashSet::new();

        // 1. Load standard system dictionary safely bypassing UTF-8 decode errors
        let dict_paths = ["/usr/share/dict/words", "/usr/dict/words"];
        for path in dict_paths {
            if let Ok(bytes) = std::fs::read(path) {
                let content = String::from_utf8_lossy(&bytes);
                for line in content.lines() {
                    dict.insert(line.trim().to_lowercase());
                }
                break;
            }
        }

        // 2. Load custom persistent dictionary (if it exists)
        if let Some(mut custom_path) = Self::get_base_dir() {
            custom_path.push("custom_dict.txt");
            if let Ok(bytes) = std::fs::read(&custom_path) {
                let content = String::from_utf8_lossy(&bytes);
                for line in content.lines() {
                    dict.insert(line.trim().to_lowercase());
                }
            }
        }

        dict
    }

    // fn find_next_misspelled(&self, start_idx: usize) -> Option<(String, usize, usize)> {
    //     let dict = self.dictionary.as_ref().unwrap();
    //     let mut in_word = false;
    //     let mut word_start = 0;
    //     let mut word = String::new();
    //
    //     let chars = self.buffer.chars().skip(start_idx);
    //     for (i, c) in chars.enumerate() {
    //         let actual_idx = start_idx + i;
    //         if c.is_alphabetic() {
    //             if !in_word {
    //                 in_word = true;
    //                 word_start = actual_idx;
    //             }
    //             word.push(c);
    //         } else {
    //             if in_word {
    //                 if !dict.contains(&word.to_lowercase()) {
    //                     return Some((word, word_start, actual_idx));
    //                 }
    //                 in_word = false;
    //                 word.clear();
    //             }
    //         }
    //     }
    //     if in_word && !dict.contains(&word.to_lowercase()) {
    //         return Some((word, word_start, self.buffer.len_chars()));
    //     }
    //     None
    // }
    fn find_next_misspelled(&self, start_idx: usize) -> Option<(String, usize, usize)> {
        let dict = self.dictionary.as_ref().unwrap();
        let mut in_word = false;
        let mut word_start = 0;
        let mut word = String::new();

        let chars = self.buffer.chars().skip(start_idx);
        for (i, c) in chars.enumerate() {
            let actual_idx = start_idx + i;

            // Allow alphabetic characters, and apostrophes if they are inside a word
            if c.is_alphabetic() || (in_word && c == '\'') {
                if !in_word {
                    in_word = true;
                    word_start = actual_idx;
                }
                word.push(c);
            } else {
                if in_word {
                    // Strip trailing apostrophe if the word ends with one (e.g., "cats'")
                    let mut check_word = word.clone();
                    while check_word.ends_with('\'') {
                        check_word.pop();
                    }

                    if !check_word.is_empty() && !dict.contains(&check_word.to_lowercase()) {
                        // Return the actual end index of the isolated word
                        return Some((check_word.clone(), word_start, word_start + check_word.len()));
                    }
                    in_word = false;
                    word.clear();
                }
            }
        }

        if in_word {
            let mut check_word = word.clone();
            while check_word.ends_with('\'') {
                check_word.pop();
            }
            if !check_word.is_empty() && !dict.contains(&check_word.to_lowercase()) {
                return Some((check_word.clone(), word_start, word_start + check_word.len()));
            }
        }
        None
    }
    
    fn spell_check(&mut self) -> io::Result<()> {
        if self.dictionary.is_none() {
            self.dictionary = Some(Self::load_dictionary());
        }

        let mut current_idx = 0;
        let mut corrections = 0;

        while let Some((word, start, end)) = self.find_next_misspelled(current_idx) {
            let lower_word = word.to_lowercase();

            if self.ignored_words.contains(&lower_word) {
                current_idx = end;
                continue;
            }

            self.cursor_y = self.buffer.char_to_line(start);
            self.cursor_x = start - self.buffer.line_to_char(self.cursor_y);
            self.desired_cursor_x = self.cursor_x;
            self.scroll()?;

            let word_len = word.chars().count();
            self.highlight_match = Some((self.cursor_y, self.cursor_x, self.cursor_x + word_len));
            self.draw_screen()?;

            let dict = self.dictionary.as_ref().unwrap();
            let suggestions = Self::get_suggestions(&lower_word, dict);

            self.current_suggestions = suggestions.into_iter().take(4).collect();
            self.menu_state = MenuState::SpellCheck;

            let choice_result = self.prompt("Replace with: ", false)?;

            self.menu_state = MenuState::Default;
            let current_suggs_copy = self.current_suggestions.clone();
            self.current_suggestions.clear();

            if let Some(choice) = choice_result {
                let choice_clean = choice.trim().to_lowercase();

                if choice_clean == "i" {
                    self.ignored_words.insert(lower_word);
                    current_idx = end;
                } else if choice_clean == "a" {
                    // Add to the active session dictionary
                    self.dictionary.as_mut().unwrap().insert(lower_word.clone());

                    // Add to the persistent file
                    if let Some(mut custom_path) = Self::get_base_dir() {
                        custom_path.push("custom_dict.txt");
                        // OpenOptions allows us to append to the file, or create it if it doesn't exist
                        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(custom_path) {
                            let _ = writeln!(file, "{}", lower_word);
                        }
                    }

                    current_idx = end;
                } else if let Ok(num) = choice_clean.parse::<usize>() {
                    if num > 0 && num <= current_suggs_copy.len() {
                        let replacement = &current_suggs_copy[num - 1];
                        self.buffer.remove(start..end);
                        self.buffer.insert(start, replacement);
                        current_idx = start + replacement.chars().count();
                        corrections += 1;
                        self.mark_modified();
                    } else {
                        current_idx = end;
                    }
                } else if !choice.is_empty() {
                    self.buffer.remove(start..end);
                    self.buffer.insert(start, &choice);
                    current_idx = start + choice.chars().count();
                    corrections += 1;
                    self.mark_modified();
                } else {
                    current_idx = end;
                }
            } else {
                self.highlight_match = None;
                self.set_status(String::from("Spell check cancelled"));
                return Ok(());
            }

            self.highlight_match = None;
        }

        self.set_status(format!("Spell check complete. {} corrections made.", corrections));
        Ok(())
    }

    fn get_suggestions(word: &str, dict: &HashSet<String>) -> Vec<String> {
        let mut scored: Vec<(&String, usize)> = dict.iter()
            .filter(|w| (w.len() as isize - word.len() as isize).abs() <= 2)
            .map(|w| (w, edit_distance(word, w)))
            .filter(|(_, dist)| *dist <= 3)
            .collect();

        scored.sort_by_key(|&(_, dist)| dist);
        scored.into_iter().take(6).map(|(w, _)| w.clone()).collect()
    }
}

pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut cache: Vec<usize> = (0..=b.len()).collect();
    let mut result = cache.clone();

    for (i, &a_char) in a.iter().enumerate() {
        result[0] = i + 1;
        for (j, &b_char) in b.iter().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            result[j + 1] = (result[j] + 1).min(cache[j + 1] + 1).min(cache[j] + cost);
        }
        cache.copy_from_slice(&result);
    }
    result[b.len()]
}
