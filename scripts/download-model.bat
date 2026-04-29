@echo off
:: Downloads a Whisper model into the per-user models directory.
::
:: Default: ggml-tiny.en.bin (~75MB, fastest, English-only, lower accuracy)
::
:: Usage:
::   scripts\download-model.bat                       (tiny, default)
::   scripts\download-model.bat tiny                  (~75MB)
::   scripts\download-model.bat base                  (~150MB, better)
::   scripts\download-model.bat small                 (~466MB, much better)  <-- recommended
::   scripts\download-model.bat medium                (~1.5GB)
::   scripts\download-model.bat large-v3-turbo        (~1.6GB, best on GPU)
::   scripts\download-model.bat ggml-tiny.en.bin      (full filename also works)
::
:: After downloading a non-default size, point the app at it via env var:
::   set BOOTHRFLOW_WHISPER_MODEL_FILE=ggml-small.en.bin
::   pnpm dev:msvc

setlocal

set "ARG=%~1"
if "%ARG%"=="" set "ARG=tiny"

:: Map shortcuts -> filenames
if /I "%ARG%"=="tiny"   set "MODEL=ggml-tiny.en.bin"
if /I "%ARG%"=="base"   set "MODEL=ggml-base.en.bin"
if /I "%ARG%"=="small"  set "MODEL=ggml-small.en.bin"
if /I "%ARG%"=="medium" set "MODEL=ggml-medium.en.bin"
if /I "%ARG%"=="large-v3-turbo" set "MODEL=ggml-large-v3-turbo.bin"
if /I "%ARG%"=="large"  set "MODEL=ggml-large-v3.bin"

:: If no shortcut matched, treat ARG as the literal filename.
if not defined MODEL set "MODEL=%ARG%"

set "DEST_DIR=%APPDATA%\boothrflow\models"
set "DEST=%DEST_DIR%\%MODEL%"
set "URL=https://huggingface.co/ggerganov/whisper.cpp/resolve/main/%MODEL%"

if exist "%DEST%" (
  echo Model already exists at %DEST%
  echo Delete it first if you want to re-download.
  exit /b 0
)

if not exist "%DEST_DIR%" mkdir "%DEST_DIR%"

echo Downloading %MODEL% to %DEST% ...
curl -L --fail -o "%DEST%" "%URL%"
if errorlevel 1 (
  echo error: download failed.
  exit /b 1
)

echo done.
echo.
if /I not "%MODEL%"=="ggml-tiny.en.bin" (
  echo To use this model on next launch, set:
  echo   set BOOTHRFLOW_WHISPER_MODEL_FILE=%MODEL%
)
exit /b 0
