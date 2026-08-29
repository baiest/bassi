# piper-setup.ps1
#
# One-time, idempotent setup for the Piper TTS backend (NALA_TTS=piper,
# the default). Unlike Chatterbox, Piper has no server to run - it's a
# small CLI Nala spawns per utterance - so this script only needs to fetch
# the binary and a voice once.
#
# Downloads:
#   1. The Piper Windows x64 binary (rhasspy/piper GitHub releases) into
#      tools/piper/.
#   2. A Latin American Spanish voice (rhasspy/piper-voices on
#      Hugging Face) into data/voices/piper/.
#
# Both steps are skipped if their target already exists, so re-running
# this script is safe. Override where things go with NALA_PIPER_BIN /
# NALA_PIPER_MODEL - the same variables Nala itself reads.

$ErrorActionPreference = "Stop"

$piperVersion = "2023.11.14-2"
$voiceName = "es_MX-claude-high"

$binPath = if ($env:NALA_PIPER_BIN) { $env:NALA_PIPER_BIN } else { "$PSScriptRoot\..\tools\piper\piper.exe" }
$modelPath = if ($env:NALA_PIPER_MODEL) { $env:NALA_PIPER_MODEL } else { "$PSScriptRoot\..\data\voices\piper\$voiceName.onnx" }
$configPath = "$modelPath.json"

$binDir = Split-Path -Parent $binPath
$modelDir = Split-Path -Parent $modelPath

if (Test-Path $binPath) {
    Write-Host "Piper binary already present at $binPath"
} else {
    Write-Host "Downloading Piper $piperVersion..."
    New-Item -ItemType Directory -Force -Path $binDir | Out-Null

    $zipUrl = "https://github.com/rhasspy/piper/releases/download/$piperVersion/piper_windows_amd64.zip"
    $zipPath = Join-Path $binDir "piper.zip"
    Invoke-WebRequest -Uri $zipUrl -OutFile $zipPath

    # The release zip extracts into a top-level `piper/` folder; move its
    # contents up so `$binPath` (piper.exe) ends up directly in $binDir.
    $extractDir = Join-Path $binDir "_extract"
    Expand-Archive -Path $zipPath -DestinationPath $extractDir -Force
    Get-ChildItem (Join-Path $extractDir "piper") | Move-Item -Destination $binDir -Force
    Remove-Item $extractDir -Recurse -Force
    Remove-Item $zipPath -Force

    Write-Host "Piper binary installed at $binPath"
}

if ((Test-Path $modelPath) -and (Test-Path $configPath)) {
    Write-Host "Voice '$voiceName' already present at $modelPath"
} else {
    Write-Host "Downloading voice '$voiceName'..."
    New-Item -ItemType Directory -Force -Path $modelDir | Out-Null

    # rhasspy/piper-voices lays voices out as <lang>/<lang_region>/<name>/<quality>/.
    $voiceUrlBase = "https://huggingface.co/rhasspy/piper-voices/resolve/main/es/es_MX/claude/high"
    Invoke-WebRequest -Uri "$voiceUrlBase/$voiceName.onnx" -OutFile $modelPath
    Invoke-WebRequest -Uri "$voiceUrlBase/$voiceName.onnx.json" -OutFile $configPath

    Write-Host "Voice installed at $modelPath"
}

Write-Host ""
Write-Host "Piper setup complete."
Write-Host "  NALA_PIPER_BIN   = $binPath"
Write-Host "  NALA_PIPER_MODEL = $modelPath"
Write-Host "Run Nala with NALA_TTS=piper (the default) to use it."
