# chatterbox-server.ps1
#
# Starts a local Chatterbox TTS server compatible with the
# travisvn/chatterbox-tts-api contract (`POST /v1/audio/speech`,
# `GET /health`) that nala's ChatterboxSupervisor expects. This is the
# command NALA_CHATTERBOX_CMD points to by default - Nala spawns it
# automatically on startup unless a server is already answering `/health`,
# or NALA_CHATTERBOX_AUTOSTART=0.
#
# Setup (one-time - the published PyPI package is currently broken, so this
# clones and runs the server from source):
#   1. git clone https://github.com/travisvn/chatterbox-tts-api <repo>
#      (defaults to ..\chatterbox-tts-api next to this repo checkout)
#   2. cd <repo>; python -m venv .venv; .venv\Scripts\activate
#   3. pip install --force-reinstall typing_extensions
#   4. CPU only (no supported CUDA GPU): pip install torch torchvision
#      torchaudio --index-url https://download.pytorch.org/whl/cpu
#      (swap the index URL for a CUDA build if you do have one)
#   5. pip install -r requirements.txt
#
# You can also just run the server yourself in another terminal (e.g. for
# faster iteration) - Nala reuses whatever already answers `/health` at
# NALA_CHATTERBOX_URL and won't spawn a second one.

$ErrorActionPreference = "Stop"

$repo = if ($env:NALA_CHATTERBOX_REPO) { $env:NALA_CHATTERBOX_REPO } else { "$PSScriptRoot\..\..\chatterbox-tts-api" }
$port = if ($env:NALA_CHATTERBOX_PORT) { $env:NALA_CHATTERBOX_PORT } else { "4123" }
$voiceSample = if ($env:NALA_CHATTERBOX_REFERENCE) { $env:NALA_CHATTERBOX_REFERENCE } else { "$PSScriptRoot\..\data\voices\nala\reference.wav" }

$python = Join-Path $repo ".venv\Scripts\python.exe"
if (-not (Test-Path $python)) {
    Write-Error "Chatterbox server checkout/venv not found at '$repo'. Clone https://github.com/travisvn/chatterbox-tts-api there and create its venv (see the header of this script), or set NALA_CHATTERBOX_REPO."
    exit 1
}

# The server reads its voice reference from this env var at startup.
$env:VOICE_SAMPLE_PATH = $voiceSample
$env:PORT = $port

Push-Location $repo
try {
    & $python -m uvicorn app.main:app --host 127.0.0.1 --port $port
} finally {
    Pop-Location
}
