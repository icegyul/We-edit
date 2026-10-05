# Silence, filler words and repeated takes

**Updated:** 2026-10-04 (version 0.9.0)

Clip > Remove Silence (also in the timeline's clip menu) finds the parts of a recording nobody
needs and cuts, speeds up or marks them. Detection runs on this computer, in Rust, in the
`op-silence` crate; nothing is uploaded.

## How it works

1. **Choose what to look at**: the selected clips (the sound of a selected video clip counts), the
   whole sequence, or the range between In and Out. Only that audio is mixed, as it plays (with
   its gain and effects).
2. **Measure once**: the mix is reduced to 16 kHz mono and measured every 10 ms (loudness,
   voice-band energy, a neural voice detector). Changing any setting afterwards only repeats the
   cheap decision step, so the preview answers at once.
3. **Review**: the dialog draws the waveform with every stretch to cut shaded over it (colors by
   kind) and lists them; click a stretch or untick it to keep it.
4. **Apply**: one undo step. The picture and every track stay in sync (the cuts are ripple
   deletes across all tracks), cuts land on frame boundaries, and a short constant-power
   crossfade joins the audio so cuts do not click.

## Ways to find silence

| Method | Decides that a moment is silence when... | Good for |
|---|---|---|
| **Loudness** | it is quieter than a fixed level (dBFS) | clean recordings; the classic threshold |
| **Relative to Loudest** | it is N dB under the recording's loud parts | recordings of any level |
| **Adaptive Noise Floor** (default) | it is within N dB of the recording's own background noise (the floor is never taken below 55 dB under the loud parts, so noise gates and noise suppression do not fool it) | most speech, no level to guess |
| **Speech Band** | the 300 to 3400 Hz energy is not above its own noise floor, or the spectrum is flat like noise | quiet speech, steady hiss and hum |
| **Voice Detector (AI)** | a small neural network ([Earshot](https://github.com/pykeio/earshot), pure Rust, 40 KB) does not hear a voice | music, room noise, keyboard sounds |

Any method can be restricted with **Voices Only** (music and noise count as silence) and refined
with:

- **Shortest Silence**: shorter pauses stay (speech is full of them between syllables).
- **Shortest Sound**: shorter sounds (clicks, breaths) count as silence.
- **Keep Before / After Cut**: air left after a word and before the next, so speech is not clipped.

## Ways to find words (needs a transcription)

The audio is transcribed with Whisper (tiny, base or small; downloaded once) and the words are
searched for:

- **Filler words** in English, Spanish, Portuguese, French, German and Italian (um, uh, eh, euh,
  ...), plus optionally **crutch phrases** ("you know", "like", "o sea", "tipo", "du coup"...) and
  **your own list**.
- **Repeated words and phrases** ("I I think", "to the to the store"): the earlier copies go.
- **Repeated takes**: a sentence said again, nearly the same (word-sequence similarity), is cut and
  the last take stays; a false start that is the beginning of the next sentence goes too.
- **Pauses between words** longer than a chosen length, with the dead air removed even when
  there is background noise (the sound methods would count it as sound).

Word times from Whisper are approximate, so word cuts are moved to the quietest moment within
120 ms (**Cut at the Quietest Moment**). Whisper often leaves out "um" and writes a repeated
word once; the filler and repeat searches find what the transcript kept (Whisper Small keeps more).

In the dialog these have their own section, **Fillers, Repeats and Retakes**, apart from the
**Silences** section (the sound methods), so each kind of cut is switched on and tuned by itself;
each kind has its own color over the waveform and its own count in the list.

## Batch: many files at once

Clip > Remove Silence from Files... cuts the silences (and, if switched on, the filler words,
repeats and retakes) of any number of video or audio files with the same settings and writes each
result to an output folder, with a suffix added to the name (the originals are never touched).
The dialog shows the state of every file (waiting, working, cuts and seconds removed, or why it
failed) and a Stop button; it works in the background, so the project stays usable. The same
settings panel as the single dialog is used, including presets. From the command line,
`OpenPremier --remove-silence a.mp4 b.mp4 --out-dir cut --suffix _cut` does the same.

## What happens to each stretch

Remove and close the gaps (ripple), remove and leave gaps, **speed up** (silences play at 110 to
2000 %, like unsilence), or only add markers.

## Presets

| Preset | For |
|---|---|
| **Gentle** | long pauses only, a natural rhythm: interviews and lectures |
| **Podcast** | tight pauses, fillers and repeated words out |
| **Tutorial** | short pauses, repeated words and a repeated sentence keeps its last take |
| **Jump Cuts** | fast cuts for social video, crutch phrases out |
| **Noisy Room** | voice detector: ignores music and noise |

## Without the window

- **Assistants**: `detect_silence` previews and `remove_silence` applies (see
  [assistants.md](assistants.md)); every setting above is a parameter.
- **Command line**: `OpenPremier --remove-silence input.mp4 output.mp4 --preset podcast`
  cuts the silences and writes the file; `--remove-silence --help` lists the options.

## What it builds on

The design follows what editors and open source tools do, and adds what they lack:

- [auto-editor](https://github.com/wyattblue/auto-editor): loudness threshold with margins and
  combining several detectors (here: any method plus Voices Only).
- [unsilence](https://github.com/lagmoellertim/unsilence): speeding silences up instead of cutting.
- [tightcut](https://github.com/AndreaGiulianini/tightcut): Whisper word gaps and filler lists by
  language, with padding.
- [auto-video-editor](https://github.com/benpiper/auto-video-editor): threshold plus minimum
  length, and merging every kind of cut into one list before editing.
- Recut and TimeBolt (commercial): threshold and padding sliders with a visual preview; TimeBolt
  layers waveform, transcript and custom phrases for fillers.
- Drift (see [drift-comparison.md](drift-comparison.md)): voice-activity detection instead of
  energy thresholds, cutting at the quietest point, a short de-click at every cut, cutting words
  from the transcript.
- OpenCutAgent (a Premiere panel): dead air, filler, false starts and duplicate takes in one pass.

Not done yet: a larger neural detector (Silero) and word alignment to the audio (Drift uses a CTC
aligner), which would make word cuts exact and catch repeats Whisper writes once; see
[roadmap.md](roadmap.md).
