# xnano
xnano is a terminal text editor - efficient, easy-to-use, with modern features 

<img width="852" height="700" alt="xnano1" src="https://github.com/user-attachments/assets/479e8c56-5a40-4312-abf8-1fed286b20b7" />
<img width="852" height="700" alt="xnano2" src="https://github.com/user-attachments/assets/1c4ea614-51ed-467b-b7bf-d16f84ae9b25" />
<img width="852" height="700" alt="xnano3" src="https://github.com/user-attachments/assets/e994620c-7c5a-49dc-b460-2459c0133f48" />



## Download
For macOS and Linux (.deb) binaries, go to
````
https://github.com/mabognar/xnano/releases/latest
````

## Build
If you have Rust installed, in suitable directory run
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
    - On MacOS, make sure you have 'Use Option as Meta' selected 
      in your terminal settings
    - Mouse is supported for some actions
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
    Meta+M           Mark line
    Meta+,           Move to previous mark
    Meta.,           Move to next mark
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
    Meta+U           Undo
    Meta+R           Redo
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
                     Does NOT work on Windows
    Ctrl+L           Go to line number
    Meta+T           Cycle Syntax Theme
    Meta+L           Toggle Line Numbers
    Meta+S           Toggle Soft Wrap
