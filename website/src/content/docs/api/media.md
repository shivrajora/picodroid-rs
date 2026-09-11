---
title: "Audio"
description: "ToneGenerator: Android's tone table on a board's piezo buzzer."
---

`picodroid.media.*` — Android-compatible `ToneGenerator` for boards that declare an [`[audio]` section](/reference/porting-guide/#boardtoml-reference) in `board.toml`. See [Java API overview](/api/) for the full API index.

## What the hardware can do

The only sound output picodroid supports is a **piezo buzzer on a PWM pad**. That is one square wave at a time, with no DAC, no I2S and no amplifier anywhere in the path.

Tones and melodies work well. Sampled audio does not work at all, which is why `MediaPlayer`, `AudioTrack` and `SoundPool` have no picodroid counterpart rather than being stubbed out. If you need to play a recording, this is not the platform for it.

Two consequences are worth knowing before you start:

- **Tones are monophonic.** Most of Android's tone table is a sum of two or three sine components. Each tone here plays the *lowest* of them. Tones stay recognisable but are not spectrally correct.
- **DTMF will not decode.** Dropping the column frequency collapses the sixteen digits onto their four row frequencies, so `TONE_DTMF_1`, `TONE_DTMF_2`, `TONE_DTMF_3` and `TONE_DTMF_A` all sound the same. The constants exist so that ported code compiles and beeps, not as a signalling facility.

## Quick start

```java
import picodroid.media.AudioManager;
import picodroid.media.ToneGenerator;

ToneGenerator tones = new ToneGenerator(AudioManager.STREAM_SYSTEM, 80);

tones.startTone(ToneGenerator.TONE_PROP_BEEP);            // 400 Hz, 35 ms
tones.startTone(ToneGenerator.TONE_SUP_RINGTONE);         // repeats until stopped
tones.startTone(ToneGenerator.TONE_SUP_DIAL, 500);        // held tone, cut at 500 ms
tones.stopTone();

tones.release();
```

## `picodroid.media.ToneGenerator`

| Member | Description |
|--------|-------------|
| `ToneGenerator(int streamType, int volume)` | `streamType` is accepted for source compatibility and ignored -- there is no mixer behind it. `volume` is 0-100. |
| `boolean startTone(int toneType)` | Plays until the tone completes its cadence, or until `stopTone()` for a held or repeating one. `false` if the tone is not defined. |
| `boolean startTone(int toneType, int durationMs)` | As above, but stopped after `durationMs` whatever the cadence would do. `0` means no limit. |
| `boolean startToneSequence(int[] freqHz, int[] durationMs)` | **No `android.media` counterpart** -- see below. |
| `void stopTone()` | Silences whatever is playing. |
| `void release()` | Stops and leaves the pin idle. |
| `MAX_SEQUENCE_LENGTH` = 32 | Longest sequence `startToneSequence` accepts. |

Volume maps onto duty cycle. A square wave is loudest at 50%, so `100` is a 50% duty cycle and `0` is silence.

### Tone constants

Names and values match `android.media.ToneGenerator`. Cadences follow Android's CEPT variant.

| Constant | Value | What plays |
|----------|-------|------------|
| `TONE_DTMF_0` .. `TONE_DTMF_9`, `_S`, `_P`, `_A` .. `_D` | 0-15 | The digit's row frequency (697, 770, 852 or 941 Hz), held until stopped |
| `TONE_SUP_DIAL` | 16 | 425 Hz, continuous |
| `TONE_SUP_BUSY` | 17 | 425 Hz, 500 ms on / 500 ms off, repeating |
| `TONE_SUP_CONGESTION` | 18 | 425 Hz, 200 ms on / 200 ms off, repeating |
| `TONE_SUP_RADIO_ACK` | 19 | 425 Hz, 200 ms |
| `TONE_SUP_RADIO_NOTAVAIL` | 20 | 425 Hz, 200 ms on / 200 ms off, 3 bursts |
| `TONE_SUP_ERROR` | 21 | 950 Hz, 330 ms on / 1 s off, repeating |
| `TONE_SUP_CALL_WAITING` | 22 | 425 Hz, 200 on / 600 off / 200 on / 3 s off, repeating |
| `TONE_SUP_RINGTONE` | 23 | 425 Hz, 1 s on / 4 s off, repeating |
| `TONE_PROP_BEEP` | 24 | 400 Hz, 35 ms |
| `TONE_PROP_ACK` | 25 | 1200 Hz, 100 ms on / 100 ms off, 2 bursts |
| `TONE_PROP_NACK` | 26 | 300 Hz, 400 ms |
| `TONE_PROP_PROMPT` | 27 | 400 Hz, 200 ms |
| `TONE_PROP_BEEP2` | 28 | 400 Hz, 35 ms on / 200 ms off, 2 bursts |
| `TONE_SUP_CONFIRM` | 32 | 350 Hz, 100 ms on / 100 ms off, 3 bursts |
| `TONE_SUP_PIP` | 33 | 480 Hz, 100 ms on / 100 ms off, 4 bursts |

The CDMA tone range and the intercept tones are not implemented. `startTone` returns `false` for them, which is what Android reports for a tone its platform lacks.

## Melodies: `startToneSequence`

:::caution[Not an Android API]
`startToneSequence` has no `android.media` counterpart. Android has no API for an app-supplied note sequence, because on a phone a melody is an audio file played through `MediaPlayer` or `SoundPool` -- and neither can work on a buzzer. Code using this method will not compile against Android.
:::

The two arrays are read in step: entry *i* plays `freqHz[i]` for `durationMs[i]` milliseconds. A frequency of 0 is a rest. The sequence plays once.

```java
int[] hz = { 262, 330, 392, 523 };   // C E G C
int[] ms = { 120, 120, 120, 320 };
tones.startToneSequence(hz, ms);
```

The note data is copied out before the call returns, so the arrays can be reused immediately. `false` means the arrays were null, differed in length, or the sequence was empty or longer than `MAX_SEQUENCE_LENGTH`.

## `picodroid.media.AudioManager`

Stream-type constants only -- `STREAM_VOICE_CALL`, `STREAM_SYSTEM`, `STREAM_RING`, `STREAM_MUSIC`, `STREAM_ALARM`, `STREAM_NOTIFICATION`, `STREAM_DTMF`, `STREAM_ACCESSIBILITY`, with Android's values. There is no mixer, no per-stream volume and no audio policy, so every stream behaves identically. The constants exist so a `ToneGenerator` is constructed the way it is on Android rather than with a bare integer.

## Things to know

**Tones advance on the UI frame tick.** A listener should start a tone and return, as the [`tonedemo`](/examples/) example does. Blocking the UI thread to sleep between notes stalls playback exactly as it stalls animation.

**One buzzer, one tone.** Starting a tone replaces whatever was playing, across the whole system, and the volume in effect is the one from the most recently constructed generator. Android would mix per stream; a single piezo leaves no honest alternative.

**On a board with no `[audio]` section** the classes still exist and every method is safe to call: `startTone` and `startToneSequence` return `false` and the rest do nothing. An app that checks the return value needs no board-specific code.

**Idle display sleep silences the buzzer.** The tick that advances a tone stops when the panel blanks, so anything still sounding is stopped on the way down rather than left droning.
