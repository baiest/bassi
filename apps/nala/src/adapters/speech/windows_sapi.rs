use std::{
    io::Write,
    process::{Command, Stdio},
};

use crate::ports::speech::{Speech, SpeechError};

/// Speaks text via Windows' built-in SAPI TTS engine, driven through
/// PowerShell's `System.Speech` assembly rather than a native SAPI COM
/// binding — no extra crate/build-script dependency, and PowerShell ships
/// with every Windows install this targets.
const SCRIPT: &str = "Add-Type -AssemblyName System.Speech; \
      $t = [Console]::In.ReadToEnd(); \
      $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
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
    fn say(&self, text: &str) -> Result<(), crate::ports::speech::SpeechError> {
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
