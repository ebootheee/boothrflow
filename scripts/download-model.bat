@echo off
:: Downloads the default Whisper model (ggml-tiny.en.bin, ~75MB) into the
:: per-user models directory the app reads at startup.
::
:: Usage:
::   scripts\download-model.bat
::
:: Override which model with an arg:
::   scripts\download-model.bat ggml-base.en.bin
::   scripts\download-model.bat ggml-large-v3-turbo.bin

setlocal

set "MODEL=%~1"
if "%MODEL%"=="" set "MODEL=ggml-tiny.en.bin"

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
echo The app will pick up the model on next launch.
exit /b 0
