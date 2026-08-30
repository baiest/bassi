use std::{
    io::Write,
    process::{Command, Stdio},
};

use crate::speech::{Speech, SpeechError};

/// Speaks text via Windows' built-in SAPI TTS engine, driven through
/// PowerShell's `System.Speech` assembly rather than a native SAPI COM
/// binding — no extra crate/build-script dependency, and PowerShell ships
/// with every Windows install this targets.
///
/// Two details that aren't optional:
/// - `[Console]::InputEncoding` must be set to UTF-8 *before* reading
///   stdin. `[Console]::In.ReadToEnd()` otherwise reads using the
///   console's legacy OEM codepage, which mangles any accented character
///   (á, é, í, ó, ú, ñ) since text is written to the child's stdin as
///   UTF-8 bytes.
/// - If a Spanish voice is installed, it's selected explicitly. The
///   default voice on an English Windows install mispronounces Spanish
///   even once the characters themselves are correct.
const SCRIPT: &str = "Add-Type -AssemblyName System.Speech; \
      [Console]::InputEncoding = [System.Text.Encoding]::UTF8; \
      $t = [Console]::In.ReadToEnd(); \
      $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
      $voice = $s.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.TwoLetterISOLanguageName -eq 'es' } | Select-Object -First 1; \
      if ($voice) { $s.SelectVoice($voice.VoiceInfo.Name) }; \
      $s.Speak($t)";

pub struct WindowsSapiSpeech;

impl WindowsSapiSpeech {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsSapiSpeech {
    fn default() -> Self {
        Self::new()
    }
}

impl Speech for WindowsSapiSpeech {
    fn say(&self, text: &str) -> Result<(), SpeechError> {
        // Hardcoded rather than resolved via PATH: `System.Speech` only
        // exists in Windows PowerShell 5.1 (.NET Framework), not PowerShell
        // 7/Core, and a `powershell` on PATH can resolve to either
        // depending on the shell.
        let mut child = Command::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| SpeechError::Backend(e.to_string()))?;

        child
            .stdin
            .take()
            .expect("stdin was piped")
            .write_all(text.as_bytes())
            .map_err(|e| SpeechError::Backend(e.to_string()))?;

        let output = child
            .wait_with_output()
            .map_err(|e| SpeechError::Backend(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SpeechError::Backend(stderr.trim().to_string()));
        }

        Ok(())
    }
}
