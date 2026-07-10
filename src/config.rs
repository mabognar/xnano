use crate::editor::Editor;
use crossterm::style::Color;
use syntect::highlighting::Theme;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use include_dir::{include_dir, Dir};

static BUNDLED_THEMES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/themes");

#[derive(Clone, Copy)]
pub struct UiColors {
    pub bg: Color,
    pub fg: Color,
    pub menu_bg: Color,
    pub selected_bg: Color,
    pub accent: Color,
    pub is_dark: bool,
}

pub trait ConfigExt {
    fn get_base_dir() -> Option<PathBuf>;
    fn initialize_themes() -> std::io::Result<()>;
    fn get_config_path() -> Option<PathBuf>;
    fn get_theme_dir() -> Option<PathBuf>;
    fn load_config() -> (String, bool, bool);
    fn save_config(&self);
    fn is_dark_theme(theme: &Theme) -> bool;
    // fn derive_ui_color(bg: syntect::highlighting::Color, is_dark: bool) -> Color;
    fn cycle_theme(&mut self);
    fn update_cursor_color(&self);
    fn derive_ui_colors(theme: &Theme) -> UiColors;
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
            if let Ok(content) = fs::read_to_string(path) {
                for line in content.lines() {
                    let parts: Vec<&str> = line.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        match parts[0] {
                            "theme" => theme = parts[1].to_string(),
                            "line_numbers" => line_numbers = parts[1] == "true",
                            "soft_wrap" => soft_wrap = parts[1] == "true",
                            _ => {}
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

    fn derive_ui_colors(theme: &Theme) -> UiColors {
        let raw_bg = theme.settings.background.unwrap_or(syntect::highlighting::Color { r: 0, g: 0, b: 0, a: 255 });
        let raw_fg = theme.settings.foreground.unwrap_or(syntect::highlighting::Color { r: 255, g: 255, b: 255, a: 255 });

        let bg = Color::Rgb { r: raw_bg.r, g: raw_bg.g, b: raw_bg.b };
        let fg = Color::Rgb { r: raw_fg.r, g: raw_fg.g, b: raw_fg.b };

        // Exact logic from xpine to determine luminance
        let is_dark = (raw_bg.r as u32 + raw_bg.g as u32 + raw_bg.b as u32) < 384;

        let ui_bg = if is_dark {
            Color::Rgb { r: raw_bg.r.saturating_add(20), g: raw_bg.g.saturating_add(20), b: raw_bg.b.saturating_add(20) }
        } else {
            Color::Rgb { r: raw_bg.r.saturating_sub(20), g: raw_bg.g.saturating_sub(20), b: raw_bg.b.saturating_sub(20) }
        };

        let selected_bg = if raw_bg.r < 128 {
            Color::Rgb { r: raw_bg.r.saturating_add(40), g: raw_bg.g.saturating_add(40), b: raw_bg.b.saturating_add(40) }
        } else {
            Color::Rgb { r: raw_bg.r.saturating_sub(40), g: raw_bg.g.saturating_sub(40), b: raw_bg.b.saturating_sub(40) }
        };

        let get_theme_color = |keys: &[&str]| -> Option<Color> {
            for item in &theme.scopes {
                let scope_str = format!("{:?}", item.scope).to_lowercase();
                for key in keys {
                    if scope_str.contains(key) {
                        if let Some(c) = item.style.foreground {
                            return Some(Color::Rgb { r: c.r, g: c.g, b: c.b });
                        }
                    }
                }
            }
            None
        };

        // Extracts the dynamic accent color from the active .tmTheme
        let mut accent = get_theme_color(&["entity.name.function", "variable"])
            .unwrap_or(if is_dark { Color::Rgb { r: 100, g: 200, b: 255 } } else { Color::Rgb { r: 20, g: 100, b: 180 } });

        // Ensure high contrast for menu hot-keys in specific algorithm-based themes
        if let Some(name) = &theme.name {
            let lower_name = name.to_lowercase();

            if lower_name.contains("catppuccin") {
                accent = if is_dark {
                    // Catppuccin Yellow for dark variants (Mocha, Macchiato, Frappe)
                    Color::Rgb { r: 249, g: 226, b: 175 }
                } else {
                    // Catppuccin Yellow for the light variant (Latte)
                    Color::Rgb { r: 223, g: 142, b: 29 }
                };
            } else if lower_name.contains("base16") {
                accent = if is_dark {
                    // Bright Golden-Yellow for dark base16 themes
                    Color::Rgb { r: 250, g: 188, b: 45 }
                } else {
                    // Bold Rust-Red for light base16 themes
                    Color::Rgb { r: 200, g: 60, b: 20 }
                };
            }
        }

        UiColors { bg, fg, menu_bg: ui_bg, selected_bg, accent, is_dark }

    }

    fn cycle_theme(&mut self) {
        let mut themes: Vec<String> = self.theme_set.themes.keys().cloned().collect();
        themes.sort();

        if let Some(current_idx) = themes.iter().position(|t| t == &self.current_theme) {
            let next_idx = (current_idx + 1) % themes.len();
            self.current_theme = themes[next_idx].clone();

            self.save_config();
            self.clear_cache();

            self.status_message = format!("Theme changed to: {}", self.current_theme);
            self.status_time = Some(std::time::Instant::now());

            // Trigger the cursor color update whenever the theme changes
            self.update_cursor_color();
        }
    }

    fn update_cursor_color(&self) {
        print!("\x1b]12;#888888\x07");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}
