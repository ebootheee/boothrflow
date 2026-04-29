#!/usr/bin/env bash
set -euo pipefail

# Downloads a Whisper model into the per-user models directory.
#
# Usage:
#   scripts/download-model.sh
#   scripts/download-model.sh tiny
#   scripts/download-model.sh small
#   scripts/download-model.sh large-v3-turbo

arg="${1:-tiny}"

case "${arg}" in
  tiny) model="ggml-tiny.en.bin" ;;
  base) model="ggml-base.en.bin" ;;
  small) model="ggml-small.en.bin" ;;
  medium) model="ggml-medium.en.bin" ;;
  large-v3-turbo) model="ggml-large-v3-turbo.bin" ;;
  large) model="ggml-large-v3.bin" ;;
  *) model="${arg}" ;;
esac

case "$(uname -s)" in
  Darwin)
    dest_dir="${HOME}/Library/Application Support/boothrflow/models"
    ;;
  Linux)
    dest_dir="${XDG_DATA_HOME:-${HOME}/.local/share}/boothrflow/models"
    ;;
  *)
    echo "error: unsupported OS for this script. Use scripts\\download-model.bat on Windows." >&2
    exit 1
    ;;
esac

dest="${dest_dir}/${model}"
url="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${model}"

if [[ -f "${dest}" ]]; then
  echo "Model already exists at ${dest}"
  echo "Delete it first if you want to re-download."
  exit 0
fi

mkdir -p "${dest_dir}"

echo "Downloading ${model} to ${dest} ..."
curl -L --fail -o "${dest}" "${url}"

echo "done."
if [[ "${model}" != "ggml-tiny.en.bin" ]]; then
  echo
  echo "To use this model on next launch, set:"
  echo "  export BOOTHRFLOW_WHISPER_MODEL_FILE=${model}"
fi
