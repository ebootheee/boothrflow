#!/usr/bin/env bash
set -euo pipefail

# Downloads a model into the per-user models directory.
#
# Whisper models (single .bin files):
#   scripts/download-model.sh
#   scripts/download-model.sh tiny
#   scripts/download-model.sh small
#   scripts/download-model.sh large-v3-turbo
#
# Parakeet (multi-file ONNX bundle from sherpa-onnx):
#   scripts/download-model.sh parakeet
#
# After downloading parakeet, rebuild with:
#   cargo build --features "real-engines parakeet-engine"
# and select "NVIDIA Parakeet TDT 0.6B v3 (preview)" in Settings →
# Voice → Whisper model.

arg="${1:-tiny}"

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

mkdir -p "${dest_dir}"

# Parakeet branches off here — multi-file ONNX bundle, different
# host (sherpa-onnx GitHub releases). Single tarball that we extract
# into a per-engine subdirectory.
if [[ "${arg}" == "parakeet" ]]; then
  bundle_dir="${dest_dir}/parakeet-tdt-0.6b-v3"
  if [[ -f "${bundle_dir}/encoder.onnx" ]]; then
    echo "Parakeet model already present at ${bundle_dir}"
    echo "Delete the directory first if you want to re-download."
    exit 0
  fi

  # sherpa-onnx publishes ONNX-converted Parakeet bundles on GitHub
  # releases. The v2 int8 variant is the smallest + most actively
  # maintained; we alias it as "v3" in the picker until NVIDIA's v3
  # release has an official ONNX export.
  bundle_name="sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8"
  url="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/${bundle_name}.tar.bz2"
  archive="${dest_dir}/${bundle_name}.tar.bz2"

  echo "Downloading Parakeet bundle to ${archive} ..."
  curl -L --fail -o "${archive}" "${url}"

  echo "Extracting bundle ..."
  tar -xjf "${archive}" -C "${dest_dir}"
  rm "${archive}"

  # Normalize file names to the layout ParakeetSttEngine expects:
  # encoder.onnx / decoder.onnx / joiner.onnx / tokens.txt.
  src_dir="${dest_dir}/${bundle_name}"
  if [[ ! -d "${src_dir}" ]]; then
    echo "error: extracted directory ${src_dir} not found" >&2
    exit 1
  fi
  mkdir -p "${bundle_dir}"
  cp "${src_dir}"/encoder*.onnx "${bundle_dir}/encoder.onnx"
  cp "${src_dir}"/decoder*.onnx "${bundle_dir}/decoder.onnx"
  cp "${src_dir}"/joiner*.onnx  "${bundle_dir}/joiner.onnx"
  cp "${src_dir}"/tokens.txt    "${bundle_dir}/tokens.txt"
  rm -rf "${src_dir}"

  echo "done. Parakeet model installed at ${bundle_dir}"
  echo
  echo "Next steps:"
  echo "  1. Rebuild with the parakeet-engine feature:"
  echo "       cargo build --manifest-path src-tauri/Cargo.toml \\"
  echo "         --features \"real-engines parakeet-engine\""
  echo "     or with pnpm tauri:"
  echo "       pnpm tauri build -- --features \"parakeet-engine\""
  echo "  2. Launch boothrflow and pick Parakeet TDT 0.6B v3 (preview)"
  echo "     in Settings → Voice → Whisper model."
  exit 0
fi

# ── Whisper download path ────────────────────────────────────────
case "${arg}" in
  tiny) model="ggml-tiny.en.bin" ;;
  base) model="ggml-base.en.bin" ;;
  small) model="ggml-small.en.bin" ;;
  medium) model="ggml-medium.en.bin" ;;
  large-v3-turbo) model="ggml-large-v3-turbo.bin" ;;
  large) model="ggml-large-v3.bin" ;;
  *) model="${arg}" ;;
esac

dest="${dest_dir}/${model}"
url="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${model}"

if [[ -f "${dest}" ]]; then
  echo "Model already exists at ${dest}"
  echo "Delete it first if you want to re-download."
  exit 0
fi

echo "Downloading ${model} to ${dest} ..."
curl -L --fail -o "${dest}" "${url}"

echo "done."
if [[ "${model}" != "ggml-tiny.en.bin" ]]; then
  echo
  echo "To use this model on next launch, set:"
  echo "  export BOOTHRFLOW_WHISPER_MODEL_FILE=${model}"
fi
