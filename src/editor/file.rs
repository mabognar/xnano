use crate::editor::{EditExt, Editor, MovementExt};
use crate::ui::{PromptExt, HelpExt};
use std::env;
use std::fs::{self, File};
use std::io::{self, BufWriter};
use std::path::Path;

pub trait FileExt {
    fn expand_tilde(path: &str) -> String;
    fn is_file_open(&self, expanded_path: &str) -> bool;
    fn read_file(&mut self) -> io::Result<()>;
    fn save_file(&mut self) -> io::Result<()>;
}

impl FileExt for Editor {

    fn expand_tilde(path: &str) -> String {
        if path.starts_with("~/") || path.starts_with("~\\") || path == "~" {
            let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).unwrap_or_default();
            if !home.is_empty() {
                return path.replacen('~', &home, 1);
            }
        }
        path.to_string()
    }


    fn is_file_open(&self, expanded_path: &str) -> bool {
        let target = Path::new(expanded_path);
        let target_canon = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());

        // check the active buffer
        if let Some(ref current) = self.filename {
            let current_canon = fs::canonicalize(current).unwrap_or_else(|_| Path::new(current).to_path_buf());
            if current_canon == target_canon {
                return true;
            }
        }

        // check background buffers
        for state in &self.inactive_buffers {
            if let Some(ref fname) = state.filename {
                let fname_canon = fs::canonicalize(fname).unwrap_or_else(|_| Path::new(fname).to_path_buf());
                if fname_canon == target_canon {
                    return true;
                }
            }
        }

        false
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
                    self.snapshot();
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

            // --- NEW: Prevent saving over a file that is open in another buffer ---
            let target_canon = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let mut is_in_background = false;

            for state in &self.inactive_buffers {
                if let Some(ref fname) = state.filename {
                    let fname_canon = fs::canonicalize(fname).unwrap_or_else(|_| Path::new(fname).to_path_buf());
                    if fname_canon == target_canon {
                        is_in_background = true;
                        break;
                    }
                }
            }

            if is_in_background {
                let short_name = path.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| new_name.clone());

                self.set_status(format!("Save cancelled: '{}' is already open in another buffer", short_name));
                return Ok(());
            }

            if path.exists() && Some(&new_name) != self.filename.as_ref() {
                let warning = format!("File \"{}\" exists. Overwrite? ", new_name);
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

}