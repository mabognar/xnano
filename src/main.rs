mod config;
mod editor;
mod spell;
mod ui;

use crate::config::ConfigExt;
use crate::editor::Editor;
use std::env;
use std::io;

fn main() -> io::Result<()> {
    let _ = Editor::initialize_themes();

    let args: Vec<String> = env::args().collect();
    let filename = args.get(1).cloned();

    let mut editor = Editor::new(filename);
    editor.run()
}
