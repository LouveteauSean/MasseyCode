@echo off
rem ===========================================================================
rem  MasseyCode -- start the debugger.
rem
rem  Double click this file, or drag a .mc file onto it to open that program.
rem  Build it first with:  cargo build -p massey-gui
rem ===========================================================================
setlocal
set "EXE=%~dp0target\debug\masseycode.exe"

if not exist "%EXE%" (
    echo.
    echo   MasseyCode has not been built yet.
    echo.
    echo   Build it with:
    echo       cargo build -p massey-gui
    echo.
    pause
    exit /b 1
)

start "" "%EXE%" %*
