@echo off
setlocal
set "ROOT=%~dp0"
set "EXE=%ROOT%native-ui\target\debug\prompt-studio-native.exe"

if not exist "%EXE%" (
    echo Building Prompt Studio, please wait...
    set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
    pushd "%ROOT%"
    cargo build --manifest-path native-ui\Cargo.toml
    popd
)

if not exist "%EXE%" (
    echo.
    echo Build failed. Install Rust and check the errors above.
    pause
    exit /b 1
)

start "" "%EXE%"
exit /b 0
