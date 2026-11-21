@echo off
REM ============================================================================
REM setup-dev-environment.bat - development environment setup for MikanLauncher
REM (Windows).
REM
REM Prepares the environment so you can build and run. It does NOT build. It:
REM   1. installs Git via winget,
REM   2. installs the MSVC C++ toolset that Rust's x86_64-pc-windows-msvc target
REM      links with, but only when no 2022+ Visual Studio / Build Tools VC toolset
REM      is found, and before rustup, whose silent install proceeds without one
REM      and leaves the first cargo build failing at link,
REM   3. installs rustup via winget, then fetches the toolchain the repo's
REM      rust-toolchain.toml pins, so the first cargo run is a build and not a
REM      download,
REM   4. installs cargo-deny through cargo (winget has no package for it), the
REM      dependency audit gate.
REM
REM Usage:  setup-dev-environment.bat
REM ============================================================================

setlocal EnableExtensions EnableDelayedExpansion
REM Directory this script lives in (tools\), with a trailing backslash.
set "TOOLS=%~dp0"

REM --- winget must be available -----------------------------------------------
where winget >nul 2>&1
if errorlevel 1 (
    echo(
    echo ERROR: winget ^(App Installer^) was not found.
    echo Install "App Installer" from the Microsoft Store, then re-run this script.
    exit /b 1
)

echo(
echo === Installing the toolchain via winget ===
call :winget_install Git.Git "Git"

REM --- MSVC C++ toolset: install only if no 2022+ VC toolset is already present -
REM Rust's MSVC target links with the toolset's linker and the Windows SDK. The
REM vswhere query is constrained to version 17.0 and newer, since without the
REM -version filter a stale 2019 install would satisfy the check and skip the
REM install.
REM The version range is written with escaped punctuation, [17.0^,^), rather
REM than quoted: a quoted range is a second quoted span in the backquoted
REM command, and cmd's /c quote rule then strips the outermost quotes, which
REM breaks the quoted vswhere path at its first space and finds nothing. A
REM 2022+ instance without the C++ toolset takes the installer's modify rather
REM than the fresh-install command line, which the bootstrapper refuses for an
REM instance that exists.
echo(
echo === Checking for the MSVC C++ toolset 2022+ (Rust's MSVC target links with it) ===
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VSSETUP=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\setup.exe"
set "HAVE_VCTOOLS="
set "HAVE_VS2022="
if exist "%VSWHERE%" (
    for /f "usebackq delims=" %%I in (`"%VSWHERE%" -latest -products * -version [17.0^,^) -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2^>nul`) do set "HAVE_VCTOOLS=%%I"
    for /f "usebackq delims=" %%I in (`"%VSWHERE%" -latest -products * -version [17.0^,^) -property installationPath 2^>nul`) do set "HAVE_VS2022=%%I"
)
if defined HAVE_VCTOOLS (
    echo Found a 2022+ MSVC C++ toolset at "!HAVE_VCTOOLS!". Skipping the Build Tools install.
) else if defined HAVE_VS2022 (
    echo Found a 2022+ Visual Studio at "!HAVE_VS2022!" without the C++ toolset. Adding it ^(accept the elevation prompt^)...
    "%VSSETUP%" modify --installPath "!HAVE_VS2022!" --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --quiet --wait --norestart
) else (
    echo No 2022+ MSVC C++ toolset found. Installing the standalone Build Tools ^(large download^)...
    winget install -e --id Microsoft.VisualStudio.2022.BuildTools --accept-package-agreements --accept-source-agreements --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
)

REM --- Rust: rustup, then the pinned toolchain --------------------------------
REM rustup-init installs rustup and puts %USERPROFILE%\.cargo\bin on the user
REM PATH. That directory joins PATH here for a shell that predates the install,
REM then rustup toolchain install, run from the repo root, reads
REM rust-toolchain.toml and fetches the pinned toolchain with its components.
echo(
echo === Installing rustup ===
set "PATH=%PATH%;%USERPROFILE%\.cargo\bin"
where rustup >nul 2>&1
if errorlevel 1 (
    call :winget_install Rustlang.Rustup "rustup"
) else (
    echo rustup is already installed.
)
echo(
echo === Fetching the pinned Rust toolchain ===
pushd "%TOOLS%.."
rustup toolchain install
if errorlevel 1 echo   ^(If rustup was just installed, open a NEW terminal and re-run from the repo root: rustup toolchain install^)
popd

REM --- cargo-deny: the dependency audit gate ----------------------------------
echo(
echo === Installing cargo-deny ===
where cargo-deny >nul 2>&1
if errorlevel 1 (
    cargo install cargo-deny --locked
    if errorlevel 1 echo   ^(If rustup was just installed, open a NEW terminal and re-run: cargo install cargo-deny --locked^)
) else (
    echo cargo-deny is already installed.
)

echo(
echo === Environment ready. ===
echo Open a NEW terminal (so the updated PATH applies), then from the repo root:
echo     cargo run -- --root .
echo(
echo If VS Code was open during this setup, restart it so rust-analyzer starts
echo against the installed toolchain, and accept its recommended extensions.
endlocal
exit /b 0

REM ----------------------------------------------------------------------------
:winget_install
REM %1 = winget package id, %2 = friendly name
echo(
echo --- %~2 [%~1] ---
winget install -e --id %~1 --accept-package-agreements --accept-source-agreements
if errorlevel 1 echo   ^(winget returned non-zero; if the package is already installed this is expected.^)
exit /b 0
