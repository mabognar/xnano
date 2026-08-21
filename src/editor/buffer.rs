use crate::editor::{BufferState, Editor};
use crate::editor::FileExt;
use crate::ui::{HelpExt, PromptExt};
use ropey::Rope;
use std::io;
use std::path::Path;

pub trait BufferManagerExt {
    fn cycle_next_buffer(&mut self);
    fn cycle_prev_buffer(&mut self);
    fn exit_editor(&mut self) -> io::Result<()>;
    fn open_new_buffer(&mut self) -> io::Result<()>;
    fn pack_buffer(&mut self) -> BufferState;
    fn unpack_buffer(&mut self, state: BufferState);
}

impl BufferManagerExt for Editor {

    fn cycle_next_buffer(&mut self) {
        if self.inactive_buffers.is_empty() { self.set_status(String::from("No other buffers")); return; }

        let current = self.pack_buffer();
        self.inactive_buffers.push_back(current);
        let next = self.inactive_buffers.pop_front().unwrap();
        self.unpack_buffer(next);

        let total = self.inactive_buffers.len() + 1;
        self.active_buffer_index = (self.active_buffer_index + 1) % total;

        // --- ONLY GRAB THE FILE NAME ---
        let fname = self.filename.as_deref()
            .and_then(|p| Path::new(p).file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| String::from("New Buffer"));

        self.set_status(format!("Switched to: {}", fname));
    }


    fn cycle_prev_buffer(&mut self) {
        if self.inactive_buffers.is_empty() { self.set_status(String::from("No other buffers")); return; }

        let current = self.pack_buffer();
        self.inactive_buffers.push_front(current);
        let prev = self.inactive_buffers.pop_back().unwrap();
        self.unpack_buffer(prev);

        let total = self.inactive_buffers.len() + 1;
        self.active_buffer_index = (self.active_buffer_index + total - 1) % total;

        let fname = self.filename.as_deref()
            .and_then(|p| Path::new(p).file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| String::from("New Buffer"));

        self.set_status(format!("Switched to: {}", fname));
    }


    fn exit_editor(&mut self) -> io::Result<()> {
        if self.is_modified {
            match self.prompt_yn("Save modified buffer (ANSWERING \"No\" WILL DESTROY CHANGES) ?")? {
                Some(true) => { self.save_file()?; if self.is_modified { return Ok(()); } }
                Some(false) => {} None => { return Ok(()); }
            }
        }

        if self.inactive_buffers.is_empty() {
            self.should_quit = true;
        } else {
            let next = self.inactive_buffers.pop_front().unwrap();
            self.unpack_buffer(next);

            let total = self.inactive_buffers.len() + 1;
            if self.active_buffer_index >= total {
                self.active_buffer_index = 0;
            }

            let fname = self.filename.as_deref()
                .and_then(|p| Path::new(p).file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| String::from("New Buffer"));

            self.set_status(format!("Closed buffer - switching to {}", fname));
        }
        Ok(())
    }


    fn open_new_buffer(&mut self) -> io::Result<()> {
        if let Some(filepath) = self.prompt("File to open (leave blank for new): ", true)? {
            if filepath.trim().is_empty() {
                let current = self.pack_buffer();
                self.inactive_buffers.push_back(current);
                self.unpack_buffer(BufferState::new(None));
                self.active_buffer_index = self.inactive_buffers.len();
                self.set_status(String::from("Opened new empty buffer"));
            } else {
                let expanded = Self::expand_tilde(&filepath);

                let short_name = Path::new(&expanded)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| filepath.clone());

                if self.is_file_open(&expanded) {
                    self.set_status(format!("File is already open: {}", short_name));
                    return Ok(());
                }

                let current = self.pack_buffer();
                self.inactive_buffers.push_back(current);
                self.unpack_buffer(BufferState::new(Some(expanded.clone())));
                self.active_buffer_index = self.inactive_buffers.len();
                self.set_status(format!("Opened buffer: {}", short_name));
            }
        }
        Ok(())
    }


    fn pack_buffer(&mut self) -> BufferState {
        BufferState {
            buffer: std::mem::replace(&mut self.buffer, Rope::new()),
            cursor_x: self.cursor_x, cursor_y: self.cursor_y, desired_cursor_x: self.desired_cursor_x,
            undo_stack: std::mem::take(&mut self.undo_stack), redo_stack: std::mem::take(&mut self.redo_stack),
            mark: self.mark.take(), row_offset: self.row_offset, col_offset: self.col_offset,
            filename: self.filename.take(), is_modified: self.is_modified,
            highlight_cache: std::mem::take(&mut self.highlight_cache), line_marks: std::mem::take(&mut self.line_marks),
        }
    }

    
    fn unpack_buffer(&mut self, mut state: BufferState) {
        self.buffer = std::mem::replace(&mut state.buffer, Rope::new());
        self.cursor_x = state.cursor_x; self.cursor_y = state.cursor_y; self.desired_cursor_x = state.desired_cursor_x;
        self.undo_stack = std::mem::take(&mut state.undo_stack); self.redo_stack = std::mem::take(&mut state.redo_stack);
        self.mark = state.mark.take(); self.row_offset = state.row_offset; self.col_offset = state.col_offset;
        self.filename = state.filename.take(); self.is_modified = state.is_modified;
        self.highlight_cache = std::mem::take(&mut state.highlight_cache); self.line_marks = std::mem::take(&mut state.line_marks);
    }

}