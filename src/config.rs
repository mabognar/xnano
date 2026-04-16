use crate::editor::Editor;
use crossterm::style::Color;
use syntect::highlighting::Theme;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use include_dir::{include_dir, Dir};

static BUNDLED_THEMES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/themes");

pub trait ConfigExt {
    fn get_base_dir() -> Option<PathBuf>;
    fn initialize_themes() -> std::io::Result<()>;
    fn get_config_path() -> Option<PathBuf>;
    fn get_theme_dir() -> Option<PathBuf>;
    fn load_config() -> (String, bool, bool);
    fn save_config(&self);
    fn is_dark_theme(theme: &Theme) -> bool;
    fn derive_ui_color(bg: syntect::highlighting::Color, is_dark: bool) -> Color;
    fn cycle_theme(&mut self);
}

impl ConfigExt for Editor {
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

    fn initialize_themes() -> std::io::Result<()> {
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

    fn is_dark_theme(theme: &Theme) -> bool {
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

    fn cycle_theme(&mut self) {
        let mut themes: Vec<String> = self.theme_set.themes.keys().cloned().collect();
        themes.sort();

        if let Some(current_idx) = themes.iter().position(|t| t == &self.current_theme) {
            let next_idx = (current_idx + 1) % themes.len();
            self.current_theme = themes[next_idx].clone();

            self.save_config();
            self.clear_cache();

            // Note: set_status is in ui.rs, but Editor implements it, so we can call it here
            // if we import UiExt, or we just rely on the Editor struct knowing about it.
            // Wait, we need to import UiExt in config.rs to call set_status.
            // Let's just update the status message manually to avoid cyclic traits.
            self.status_message = format!("Theme changed to: {}", self.current_theme);
            self.status_time = Some(std::time::Instant::now());
        }
    }
}
