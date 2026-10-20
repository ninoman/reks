# Beat This! model files

ONNX models for the beat tracker (`src/beats.rs`, docs/decisions.md D10).
The app bundles them as Tauri resources under `models/`
(`src-tauri/tauri.conf.json`); tests load them from here.

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `beat_this_small.onnx` | 10,555,592 | `a5f8d39d989f31859454ba27afe61c5317ca95e4d9373e6853e5361b8937172f` |
| `mel_spectrogram.onnx` | 270,742 | `fdd59e65c515331308e4c8841edf99972deca646bdf6197744c2a5b7755e3de9` |

- Source: `models/` of [danigb/beat-this-rs](https://github.com/danigb/beat-this-rs)
  at tag `v1.1.0` (matches the pinned `beat-this = "=1.1.0"`), downloaded
  2026-10-20. `beat_this_small.onnx` is the `small1` checkpoint of
  [CPJKU/beat_this](https://github.com/CPJKU/beat_this) converted to ONNX by
  that repo's `scripts/ckpt2onnx.py`.
- Licence: MIT, code and published weights (CPJKU README "License", checked
  2026-10-20; port's notice in [`LICENSE-beat-this.txt`](LICENSE-beat-this.txt),
  shipped with the models). CPJKU note that some training audio is copyrighted
  or under limited CC licences; the weights themselves are released under MIT.
- The full model (`beat_this.onnx`, ~83 MB, beat-this-rs release asset) is
  not committed; it becomes an optional download (TODO in `src/beats.rs`).

Update these files only together with the `beat-this` pin, and rerun
`cargo test -p reks-analysis --test beats`.
