# Nala's voice

`reference.wav` is the reference audio Chatterbox uses to clone Nala's
voice. It's not included in the repo by default — if it's missing, Nala
automatically falls back to Windows SAPI (see `NALA_TTS` in the main
README).

## Recording a reference

- 10-20 seconds of a single voice, no music or background noise.
- Mono, 24 kHz or higher.
- Natural, continuous speech (avoid long silences or filler words).
- Save the file as `data/voices/nala/reference.wav`.

## Using a different reference

Without moving any files, point `NALA_CHATTERBOX_REFERENCE` at the path
you want:

```powershell
$env:NALA_CHATTERBOX_REFERENCE = "C:\path\to\other_voice.wav"
```
