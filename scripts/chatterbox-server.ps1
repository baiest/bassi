# chatterbox-server.ps1
#
# Starts a local Chatterbox TTS server compatible with the
# `chatterbox-tts-api` (FastAPI) `/v1/audio/speech` + `/health` contract
# that `nala`'s ChatterboxSupervisor expects. This is the command
# NALA_CHATTERBOX_CMD points to by default - Nala spawns it automatically
# on startup unless a server is already answering `/health`, or
# NALA_CHATTERBOX_AUTOSTART=0.
#
# Setup (one-time):
#   1. Clone/install chatterbox-tts-api into a Python venv, e.g.:
#        python -m venv .venv
#        .venv\Scripts\pip install chatterbox-tts-api
#   2. Point CHATTERBOX_VENV below at that venv, or set it via the
#      NALA_CHATTERBOX_VENV environment variable before running this script.
#
# You can also just run the server yourself in another terminal (e.g. for
# faster iteration) - Nala reuses whatever already answers `/health` at
# NALA_CHATTERBOX_URL and won't spawn a second one.

$ErrorActionPreference = "Stop"

$venv = if ($env:NALA_CHATTERBOX_VENV) { $env:NALA_CHATTERBOX_VENV } else { "$PSScriptRoot\..\.chatterbox-venv" }
$port = if ($env:NALA_CHATTERBOX_PORT) { $env:NALA_CHATTERBOX_PORT } else { "4123" }

$python = Join-Path $venv "Scripts\python.exe"
if (-not (Test-Path $python)) {
    Write-Error "Chatterbox venv not found at '$venv'. Create it first (see the header of this script) or set NALA_CHATTERBOX_VENV."
    exit 1
}

& $python -m chatterbox_tts_api.server --host 127.0.0.1 --port $port
