# NOTICE

**We edit** is a modified version of a GPL-3.0-or-later video editor, version 0.9.0 (commit `cda185d`).
The copyright notices of the original work are kept as the license requires: the original work is
copyright (C) the OpenPremier contributors. Our modifications are listed in CHANGES.md and in this
repository's commit history. Licensed under the GNU General Public License, version 3 or (at your
option) any later version. The full license text is in [LICENSE](LICENSE). The history of the
original project is kept in this repository.

We edit adds and changes files as listed in [CHANGES.md](CHANGES.md); every changed file is
marked by the commit history. This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by the Free Software Foundation,
either version 3 of the License, or (at your option) any later version. It is distributed in the
hope that it will be useful, but WITHOUT ANY WARRANTY.

Adobe and Premiere Pro are trademarks of Adobe Inc. We edit is an independent project and is not
affiliated with Adobe.

## Other software and data used

| Component | License | Use |
|---|---|---|
| FFmpeg (GPL build, BtbN FFmpeg-Builds) | GPL-3.0 | decoding, encoding |
| OpenAI Whisper models, faster-whisper (CTranslate2) | MIT | speech recognition (downloaded by the user) |
| ComfyUI and its workflow templates | GPL-3.0 | optional image/video generation (separate program) |
| Workflow files adapted from joenorton/comfyui-mcp-server | Apache-2.0 | Z-Image / MiniMax H3 workflows |
| Krea 2 Turbo (Comfy-Org/Krea-2) workflow wiring | see the model card | Krea 2 workflow (models downloaded by the user) |
| Korean caption fonts from google/fonts: Black Han Sans, Do Hyeon, Jua, Nanum Gothic, Nanum Myeongjo, IBM Plex Sans KR, Gothic A1, Bagel Fat One, Gugi, Gaegu, Sunflower, Dongle | SIL OFL 1.1 | caption designs; each font file is downloaded on first use of a design (commit-pinned URL), not shipped |
| Sound effects of Kenney (kenney.nl), found through the index Mcamento8/open-game-sfx-index | CC0 1.0 | sound library, downloaded on demand (commit-pinned) |
| RezaParsian/free-sfx-bgm-pack (sounds synthesized from code) | CC0 1.0 | sound library, downloaded on demand |
| AERIS MUSIC tracks (farmmer938-lgtm/aeris-cc0-music) | CC0 1.0 | sound library, downloaded on demand |
| PyMusicLooper by arkrow (algorithm for finding the places where music repeats: candidate pairs from beats, chroma and loudness, similarity score; ported to Rust in `crates/op-remix`, no Python code is included) | MIT | Remix (기본 사운드 패널) |
| librosa (definitions of the chroma, loudness, onset, tempo and beat-tracking features; reimplemented in Rust) | ISC | Remix |
| keyring (Rust crate) | MIT OR Apache-2.0 | API keys kept in the Windows Credential Manager |

Related work that was looked at but is not used: reamix.me (a web service that remixes music, closed source).

Model files are never part of this repository; they are downloaded by the user from their
original sources and keep their own licenses.
