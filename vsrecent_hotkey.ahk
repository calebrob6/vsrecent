; AutoHotkey v2 script: bind a global hotkey to launch VS Recent.
; Requires AutoHotkey v2 (https://www.autohotkey.com/).
;
; Default: Win+Shift+R. Edit the line below to change.
; Modifier symbols: # = Win, ^ = Ctrl, ! = Alt, + = Shift.
;
; To use: double-click this file (with AHK installed). To start at logon,
; put a shortcut to this script in shell:startup.

#Requires AutoHotkey v2.0

ExePath := A_ScriptDir "\vsrecent.exe"

#+r:: {
    if FileExist(ExePath)
        Run '"' ExePath '"', A_ScriptDir
    else
        MsgBox "vsrecent.exe not found at:`n" ExePath, "VS Recent hotkey", 16
}
