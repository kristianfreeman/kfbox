# Test fixtures

Two 16-second MP3s of the same made-up click track, 120 BPM, mono, 44.1 kHz: a
click on every beat and silence between. They're not music and not from any
library. `click-lame.mp3` has LAME's Info frame, which rekordbox plays as
1,152 samples of silence; `click-plain.mp3` doesn't. `tests/import.rs` checks
that kfbox's grid follows rekordbox's timeline on both.

They were made with ffmpeg from the WAV `kfbox_core::testing::wav` writes
(seed 0, 16 s rather than 30):

```bash
ffmpeg -i click.wav -c:a libmp3lame -b:a 64k -metadata title="Click Lame" -metadata artist=kfbox click-lame.mp3
```

```bash
ffmpeg -i click.wav -c:a libmp3lame -b:a 64k -write_xing 0 -metadata title="Click Plain" -metadata artist=kfbox click-plain.mp3
```
