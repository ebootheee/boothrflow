@echo off
rem Generic wrapper that pre-loads the MSVC + Windows SDK + LLVM environment,
rem then runs whatever command was passed. Works with any tool that needs
rem bindgen / libclang access -- cargo, pnpm, npx, raw exes.
rem
rem bindgen-using crates (whisper-rs, sherpa-rs, llama-cpp-2) need libclang
rem to parse C headers, and libclang needs INCLUDE/LIB env vars pointing at
rem the Windows SDK + MSVC headers. Plain `cargo build` from a non-VS-dev
rem shell does not get these.
rem
rem Prereqs (one-time):
rem   winget install Microsoft.VisualStudio.2022.BuildTools
rem   winget install Rustlang.Rustup ^&^& rustup toolchain install stable
rem   winget install LLVM.LLVM
rem
rem Usage:
rem   scripts\cargo-msvc.bat cargo build --features real-engines
rem   scripts\cargo-msvc.bat cargo nextest run --features real-engines
rem   scripts\cargo-msvc.bat pnpm exec tauri dev
rem
rem IMPORTANT: keep this file CRLF-encoded. Some Windows cmd.exe versions
rem misparse LF-only batch files and silently drop comment lines or skip
rem `goto` jumps -- which manifests as cryptic "program not found" errors
rem deep in third-party build scripts. The repo .gitattributes pins .bat
rem files to CRLF so editors do not flatten them.

setlocal enableextensions

rem --- locate libclang -------------------------------------------------------
if defined BOOTHRFLOW_LLVM_PATH goto :llvm_done
set "BOOTHRFLOW_LLVM_PATH=C:\Program Files\LLVM\bin"
if exist "%BOOTHRFLOW_LLVM_PATH%\libclang.dll" goto :llvm_done
set "BOOTHRFLOW_LLVM_PATH=C:\Program Files (x86)\LLVM\bin"
if exist "%BOOTHRFLOW_LLVM_PATH%\libclang.dll" goto :llvm_done
echo error: libclang.dll not found. Install LLVM: winget install LLVM.LLVM
exit /b 1
:llvm_done
set "LIBCLANG_PATH=%BOOTHRFLOW_LLVM_PATH%"

rem --- locate vcvars64.bat ---------------------------------------------------
set "VCVARS="
set "_CAND=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if exist "%_CAND%" set "VCVARS=%_CAND%"
if not defined VCVARS set "_CAND=C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%_CAND%" set "VCVARS=%_CAND%"
if not defined VCVARS set "_CAND=C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%_CAND%" set "VCVARS=%_CAND%"
if not defined VCVARS set "_CAND=C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
if not defined VCVARS if exist "%_CAND%" set "VCVARS=%_CAND%"

if not defined VCVARS (
  echo error: vcvars64.bat not found. Install Visual Studio 2022 BuildTools.
  exit /b 1
)

rem vcvars64.bat shells out to vswhere.exe to discover the VS install. vswhere
rem lives in the shared "Visual Studio Installer" directory and is not on
rem PATH by default. Without it, vcvars64 prints "Environment initialized"
rem but actually leaves PATH unchanged -- cmake / cl.exe stay missing and
rem downstream cargo builds (whisper-rs-sys, sherpa-rs-sys) fail with cryptic
rem "program not found" errors. Prepend it explicitly so vcvars64 works.
set "VSWHERE_DIR=C:\Program Files (x86)\Microsoft Visual Studio\Installer"
if exist "%VSWHERE_DIR%\vswhere.exe" set "PATH=%VSWHERE_DIR%;%PATH%"

call "%VCVARS%" >nul 2>&1
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

rem cmake-rs (used by whisper-rs-sys, sherpa-rs-sys) reads `CMAKE` first,
rem only falling back to PATH lookup. pnpm / tauri-cli on Windows have
rem been observed to spawn child processes with a sanitized PATH that
rem drops the vcvars64-added MSVC bin dirs -- so even though `where cmake`
rem succeeds in this shell, cargo's build script subprocess fires from a
rem PATH that no longer contains it. Pinning CMAKE absolute makes the
rem build robust to that.
if not defined CMAKE set "CMAKE=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
if not exist "%CMAKE%" (
  rem Fall back to whatever cmake we can find on the (still-modified) PATH.
  for /f "delims=" %%C in ('where cmake 2^>nul') do set "CMAKE=%%C"& goto :cmake_done
  echo error: cmake.exe not found. Install VS 2022 BuildTools with the C++ workload.
  exit /b 1
)
:cmake_done

rem Forward whatever command was passed (cargo build, pnpm exec tauri dev, ...)
%*
exit /b %ERRORLEVEL%
