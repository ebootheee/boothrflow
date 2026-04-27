# Audio test fixtures — attribution

Any WAV files in this directory are derived from **LibriSpeech**'s `dev-clean`
split (Vassil Panayotov et al., 2015), licensed under
[CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/). Original clips are
truncated to ≤5 seconds for fast test runtime.

Source: <https://www.openslr.org/12>

When adding a new fixture, drop a line below noting the source LibriSpeech
identifier, the trim range, and the speech content.

## Fixtures

_(none yet — Phase 1 will add `clean_short.wav`, `silence.wav`,
`noisy_short.wav`, and `partial_utterance.wav` once the cpal capture path is
wired.)_

## Generating fixtures programmatically

For pure-tone, white-noise, and silence fixtures, prefer programmatic
generation in `tests/` rather than vendoring a WAV. Keeps the repo small and
keeps generation parameters visible.
