@echo off
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust is required. Install it with: winget install Rustlang.Rustup
    exit /b 1
)

cargo build --release --locked
if errorlevel 1 exit /b %errorlevel%

if not exist publish mkdir publish
copy /y target\release\vsrecent.exe publish\vsrecent.exe >nul
echo Built publish\vsrecent.exe
