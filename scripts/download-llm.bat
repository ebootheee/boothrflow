@echo off
:: Downloads the default LLM cleanup model (Qwen 2.5 1.5B Instruct Q4_K_M,
:: ~1GB) into the per-user models directory. The session daemon reads it at
:: startup; if missing, the pipeline falls back to raw transcripts.
::
:: Usage:
::   scripts\download-llm.bat                       (default 1.5B)
::   scripts\download-llm.bat 0.5b                  (faster / lower quality)
::   scripts\download-llm.bat 3b                    (better quality / slower)

setlocal

set "TIER=%~1"
if "%TIER%"=="" set "TIER=1.5b"

if /I "%TIER%"=="0.5b" (
  set "FILE=qwen2.5-0.5b-instruct-q4_k_m.gguf"
  set "REPO=Qwen/Qwen2.5-0.5B-Instruct-GGUF"
) else if /I "%TIER%"=="1.5b" (
  set "FILE=qwen2.5-1.5b-instruct-q4_k_m.gguf"
  set "REPO=Qwen/Qwen2.5-1.5B-Instruct-GGUF"
) else if /I "%TIER%"=="3b" (
  set "FILE=qwen2.5-3b-instruct-q4_k_m.gguf"
  set "REPO=Qwen/Qwen2.5-3B-Instruct-GGUF"
) else (
  echo error: unknown tier "%TIER%". Use 0.5b / 1.5b / 3b.
  exit /b 1
)

set "DEST_DIR=%APPDATA%\boothrflow\models"
set "URL=https://huggingface.co/%REPO%/resolve/main/%FILE%"

:: The engine looks for the 1.5B file by name. If you pick a different
:: tier, copy/rename to qwen2.5-1.5b-instruct-q4_k_m.gguf or update
:: stt::DEFAULT_MODEL_FILE in src-tauri/src/llm/llama.rs.
set "ENGINE_EXPECTED=qwen2.5-1.5b-instruct-q4_k_m.gguf"
set "DEST=%DEST_DIR%\%ENGINE_EXPECTED%"

if exist "%DEST%" (
  echo Model already exists at %DEST%
  echo Delete it first if you want to re-download.
  exit /b 0
)

if not exist "%DEST_DIR%" mkdir "%DEST_DIR%"

echo Downloading %TIER% Qwen Instruct (~1GB for 1.5b) to %DEST% ...
curl -L --fail -o "%DEST%" "%URL%"
if errorlevel 1 (
  echo error: download failed.
  exit /b 1
)

echo done. The app will pick up the model on next launch.
exit /b 0
