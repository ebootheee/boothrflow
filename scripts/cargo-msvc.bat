@echo off
:: Cargo wrapper that pre-loads the MSVC + Windows SDK + LLVM environment.
::
:: bindgen-using crates (whisper-rs, sherpa-rs, llama-cpp-2) need libclang to
:: parse C headers, and libclang needs INCLUDE/LIB env vars pointing at the
:: Windows SDK + MSVC headers. Plain `cargo build` from a non-VS-dev shell
:: doesn't get these.
::
:: Prereqs (one-time):
::   winget install Microsoft.VisualStudio.2022.BuildTools
::   winget install Rustlang.Rustup ^&^& rustup toolchain install stable
::   winget install LLVM.LLVM
::
:: Usage:
::   scripts\cargo-msvc.bat build --features real-engines
::   scripts\cargo-msvc.bat nextest run --features real-engines

setlocal

:: --- locate libclang -------------------------------------------------------
if defined BOOTHRFLOW_LLVM_PATH goto :llvm_done
if exist "C:\Program Files\LLVM\bin\libclang.dll" (
  set "BOOTHRFLOW_LLVM_PATH=C:\Program Files\LLVM\bin"
  goto :llvm_done
)
if exist "C:\Program Files (x86)\LLVM\bin\libclang.dll" (
  set "BOOTHRFLOW_LLVM_PATH=C:\Program Files (x86)\LLVM\bin"
  goto :llvm_done
)
echo error: libclang.dll not found. Install LLVM: winget install LLVM.LLVM
exit /b 1
:llvm_done
set "LIBCLANG_PATH=%BOOTHRFLOW_LLVM_PATH%"

:: --- locate vcvars64.bat ---------------------------------------------------
set "VCVARS="
for %%P in (
  "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
  "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat"
  "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
  "C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvars64.bat"
) do (
  if exist %%P set "VCVARS=%%~P"
)

if not defined VCVARS (
  echo error: vcvars64.bat not found. Install Visual Studio 2022 BuildTools.
  exit /b 1
)

call "%VCVARS%" >nul 2>&1
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

cargo %*
exit /b %ERRORLEVEL%
