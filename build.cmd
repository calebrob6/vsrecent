@echo off
setlocal
cd /d "%~dp0"

REM Detect host architecture so a double-click "just works" on both x64 and ARM64.
set "RID=win-x64"
if /i "%PROCESSOR_ARCHITECTURE%"=="ARM64" set "RID=win-arm64"
if /i "%PROCESSOR_ARCHITEW6432%"=="ARM64" set "RID=win-arm64"

echo Publishing %RID% self-contained single-file build...
dotnet publish vsrecent.csproj -c Release -r %RID% --self-contained=true -o "publish\%RID%"
if errorlevel 1 (
    echo Build FAILED.
    exit /b 1
)

echo.
echo Build OK: %CD%\publish\%RID%\vsrecent.exe
endlocal
