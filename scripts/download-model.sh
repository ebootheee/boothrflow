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
# and select "NVIDIA Parakeet TDT 0.6B (preview)" in Settings →
# Voice → Recognition. Note: the bundle is sherpa-onnx's v2 ONNX
# export of NeMo Parakeet TDT 0.6B (English only). The "v3"
# multilingual variant is queued as a Future Idea once sherpa-onnx
# ships its ONNX export of v3 — see ROADMAP.md.

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

  # Patch the bundle's decoder + joiner ONNX files to carry the
  # ASR metadata sherpa-onnx 1.10+ checks for. The published v2-int8
  # bundle puts `vocab_size`/`pred_rnn_layers`/etc. only on encoder.onnx,
  # but newer sherpa-onnx wants them on the decoder too — without
  # this the C++ side throws during decode. The Python script reads
  # encoder.onnx, copies its metadata onto the other two, and
  # injects a `context_size=2` constant (NeMo TDT default).
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  patcher="${script_dir}/parakeet-propagate-metadata.py"
  if ! python3 -c "import onnx" 2>/dev/null; then
    echo
    echo "Installing the `onnx` Python package (one-time, ~80MB) so we"
    echo "can patch the model bundle's metadata for sherpa-onnx 1.10+:"
    python3 -m pip install --user --break-system-packages --quiet onnx \
      || {
        echo "error: pip install onnx failed; install it manually then re-run" >&2
        exit 1
      }
  fi
  python3 "${patcher}" "${bundle_dir}" \
    || {
      echo "error: metadata propagation failed — see traceback above" >&2
      exit 1
    }

  echo
  echo "done. Parakeet model installed at ${bundle_dir}"
  echo
  echo "Next steps:"
  echo "  1. Run with the parakeet-engine feature:"
  echo "       pnpm dev:parakeet"
  echo "  2. In Settings → Voice → Recognition, pick"
  echo "     'NVIDIA Parakeet TDT 0.6B — final transcript only (preview)'."
  echo
  echo "Note: the bundle is sherpa-onnx's v2 ONNX export of NeMo"
  echo "Parakeet TDT 0.6B (English only). The v3 multilingual variant"
  echo "is queued as a future ROADMAP item."
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
