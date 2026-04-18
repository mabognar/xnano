# xnano
xnano is a fast text editor, inspired by nano, written in Rust. Includes syntax highlighting, themes, soft line-wrap, 
line numbers, etc. Efficient and easy to use. 30 included themes.

<img width="852" height="700" alt="Screenshot 2026-04-17 at 7 25 04 PM" src="https://github.com/user-attachments/assets/38a472e6-2c7c-46d5-89bb-7d33bb3de7b4" />
<img width="852" height="700" alt="Screenshot 2026-04-17 at 7 26 56 PM" src="https://github.com/user-attachments/assets/16676e57-a76e-45e1-8a32-99d4699827c6" />
<img width="852" height="700" alt="Screenshot 2026-04-17 at 7 31 42 PM" src="https://github.com/user-attachments/assets/12d0ed0a-0da2-4df8-81f8-9380e274d9a1" />

## Build
In suitable directory, run
```
git clone https://github.com/mabognar/xnano
cd xnano
cargo install --path .
xnano
```
Alternatively,
```
cargo install xnano
```

## Help
### Themes & Configuration: 
    - To cycle through the included themes, type Meta+T (ALT+T,
      Option+T) when in editor
    - On MacOS, make sure you have 'Use Option as Meta' selected 
      in your terminal settings
    - Theme, line numbers, soft wrap are persistent
    - Settings are stored in ~/.xnano/xnanorc
    - Themes are stored in ~/.xnano/themes
    - Additional .tmTheme themes can be added to ~/.xnano/themes
### Movement:
    Ctrl+P, Up       Move up one line
    Ctrl+N, Down     Move down one line
    Ctrl+B, Left     Move left one character
    Ctrl+F, Right    Move right one character
    Ctrl+A           Move to start of line
    Ctrl+E           Move to end of line
    Ctrl+Y, F7, PgUp Move up one page
    Ctrl+V, F8, PgDn Move down one page
### Editing:
    Ctrl+K, F9       Cut current line into clipboard
    Ctrl+U, F10      Paste contents of clipboard
    Ctrl+D, Del      Delete character under cursor
    Backspace        Delete character before cursor
    Ctrl+J, F4       Justify current paragraph
    Ctrl+I, Tab      Insert tab
    Ctrl+^, Meta+A   Mark beginning of selected text.
                     This key also unselects text.
                     Note: Ctrl+^ = Ctrl+Shift+6
### Search & Replace:
    Ctrl+W, F6       Where is (Search)
    Ctrl+\           Search and Replace
### File & System:
    Ctrl+O, F3       Write Out (Save)
    Ctrl+R, F5       Read File (Insert)
    Ctrl+G, F1       Get Help (this screen)
    Ctrl+X, F2       Exit xnano
### Tools:
    Ctrl+C, F11      Current Position
    Ctrl+T, F12      To Spell (Spell check)
    Ctrl+L           Go to line number
    Meta+T           Cycle Syntax Theme
    Meta+L           Toggle Line Numbers
    Meta+S           Toggle Soft Wrap
