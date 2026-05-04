#!/usr/bin/env python3
"""Propagate sherpa-onnx ASR metadata from encoder.onnx onto decoder.onnx
and joiner.onnx so sherpa-onnx 1.10+'s InitDecoder check is satisfied.

Why this exists
---------------

The published sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8 bundle
(currently the only Parakeet ONNX bundle on the asr-models GitHub
release tag) was built before sherpa-onnx 1.10 added a runtime check
for `vocab_size` (and friends) on the *decoder* metadata_props.
encoder.onnx ships with the keys; decoder.onnx and joiner.onnx ship
empty. Loading the bundle into sherpa-onnx 1.10+ triggers
`offline-transducer-model.cc:InitDecoder:201 'vocab_size' does not
exist in the metadata` and the C++ side calls `exit(-1)`.

This script reads the metadata_props from encoder.onnx and copies
the same key/value pairs onto decoder.onnx and joiner.onnx, in
place. After running, sherpa-onnx 1.10+ accepts the bundle.

Once a sherpa-onnx-prepared Parakeet bundle ships with the metadata
on all three files, this propagation step becomes a no-op and can
be removed from the download flow.

Run via the download script (`pnpm download:model:mac parakeet`)
or directly:
  python3 scripts/parakeet-propagate-metadata.py \
    "${HOME}/Library/Application Support/boothrflow/models/parakeet-tdt-0.6b-v3"

Requires the `onnx` Python package:
  pip3 install --user --break-system-packages onnx
"""
from __future__ import annotations

import sys
from pathlib import Path

try:
    import onnx
except ImportError:
    print(
        "error: the `onnx` Python package isn't installed.\n"
        "Install it with:\n"
        "  pip3 install --user --break-system-packages onnx",
        file=sys.stderr,
    )
    sys.exit(2)


def propagate(model_dir: Path) -> int:
    encoder = model_dir / "encoder.onnx"
    decoder = model_dir / "decoder.onnx"
    joiner = model_dir / "joiner.onnx"

    for required in (encoder, decoder, joiner):
        if not required.exists():
            print(f"error: missing {required}", file=sys.stderr)
            return 3

    enc = onnx.load(str(encoder), load_external_data=False)
    metadata = {p.key: p.value for p in enc.metadata_props}
    if "vocab_size" not in metadata:
        print(
            "error: encoder.onnx itself doesn't have `vocab_size` metadata. "
            "This bundle is not the expected NeMo Parakeet TDT 0.6B v2 export.",
            file=sys.stderr,
        )
        return 4

    # NeMo Parakeet TDT 0.6B v2 known constants — sherpa-onnx 1.10+
    # checks for these on the decoder, but the upstream NeMo export
    # script doesn't write them onto the encoder either. Inject the
    # standard values if absent. Confirmed from
    # github.com/k2-fsa/sherpa-onnx/blob/master/scripts/nemo/parakeet/add-meta-data.py
    # for the parakeet-tdt-0.6b-v2 architecture.
    derived = {
        "context_size": "2",  # NeMo TDT default predictor context
    }
    for k, v in derived.items():
        if k not in metadata:
            metadata[k] = v
            print(f"  (derived) {k} = {v!r}")

    print(f"propagating {len(metadata)} keys:")
    for k, v in metadata.items():
        print(f"  {k} = {v!r}")

    for path in (decoder, joiner):
        m = onnx.load(str(path), load_external_data=False)
        existing = {p.key for p in m.metadata_props}
        added = 0
        for k, v in metadata.items():
            if k in existing:
                continue
            entry = m.metadata_props.add()
            entry.key = k
            entry.value = v
            added += 1
        if added == 0:
            print(f"{path.name}: already populated, no change")
            continue
        # Re-serialize. Use save() with the original name so we
        # overwrite atomically. ONNX wants strings, not Path.
        onnx.save(m, str(path))
        print(f"{path.name}: wrote {added} new metadata key(s)")

    return 0


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        print(f"usage: {sys.argv[0]} <model_dir>", file=sys.stderr)
        return 1
    return propagate(Path(sys.argv[1]))


if __name__ == "__main__":
    sys.exit(main())
