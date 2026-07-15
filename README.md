# xnano
xnano is a fast, efficient text editor written from the ground-up in Rust. Includes syntax highlighting, themes, update checker, soft line-wrap, 
line numbers, etc. 

<img width="852" height="700" alt="xnano1" src="https://github.com/user-attachments/assets/b53f50da-3f7c-472e-a2dc-2df6562d2d41" />
<img width="852" height="700" alt="xnano2" src="https://github.com/user-attachments/assets/c75fd1be-8d2f-4f0b-baf5-9a551e9ed2bb" />
<img width="852" height="700" alt="xnano3" src="https://github.com/user-attachments/assets/3fc045ec-3942-4077-9712-7e799f862e3a" />

[xnano-mov.webm](https://github.com/user-attachments/assets/fb776553-e205-4d66-94da-0e12319a00bf)

## Download
For macOS and Linux (.deb) binaries, go to
````
https://github.com/mabognar/xnano/releases/latest
````

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
                     Does NOT work on Windows
    Ctrl+L           Go to line number
    Meta+T           Cycle Syntax Theme
    Meta+L           Toggle Line Numbers
    Meta+S           Toggle Soft Wrap
