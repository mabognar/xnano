mod config;
mod editor;
mod ui;

use crate::config::ConfigExt;
use crate::editor::Editor;
use std::env;
use std::io;

fn main() -> io::Result<()> {
    let _ = Editor::initialize_themes();
    let args: Vec<String> = env::args().collect();
    // snatch all arguments after the executable name
    let filenames: Vec<String> = args.into_iter().skip(1).collect();

    // pass the vector
    let mut editor = Editor::new(filenames);
    editor.run()
}
