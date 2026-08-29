# chatterbox-server.ps1
#
# Starts a local Chatterbox TTS server compatible with the
# travisvn/chatterbox-tts-api contract (`POST /v1/audio/speech/stream`,
# `GET /health`) that nala's ChatterboxSupervisor expects. Nala streams
# audio from `/v1/audio/speech/stream` rather than waiting for the full
# response from `/v1/audio/speech`, so playback starts as soon as the first
# chunk is generated instead of after the whole answer. This is the command
# NALA_CHATTERBOX_CMD points to by default - Nala spawns it automatically on
# startup unless a server is already answering `/health`, or
# NALA_CHATTERBOX_AUTOSTART=0.
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

$repo = if ($env:NALA_CHATTERBOX_REPO) { $env:NALA_CHATTERBOX_REPO } else { "$PSScriptRoot\..\chatterbox-tts-api" }
$port = if ($env:NALA_CHATTERBOX_PORT) { $env:NALA_CHATTERBOX_PORT } else { "4123" }
$voiceSample = if ($env:NALA_CHATTERBOX_REFERENCE) { $env:NALA_CHATTERBOX_REFERENCE } else { "$PSScriptRoot\..\data\voices\nala\reference.wav" }

$python = Join-Path $repo ".venv\Scripts\python.exe"
if (-not (Test-Path $python)) {
    Write-Error "Chatterbox server checkout/venv not found at '$repo'. Clone https://github.com/travisvn/chatterbox-tts-api there and create its venv (see the header of this script), or set NALA_CHATTERBOX_REPO."
    exit 1
}

# The server reads its voice reference from this env var at startup (used
# only as a fallback when a request sends no `voice` name - the real "nala"
# voice comes from the voice library registered below).
$env:VOICE_SAMPLE_PATH = $voiceSample
$env:PORT = $port
# Without these the server (a) loads the English-only model, so a "nala"
# voice registered with language=es gets rejected, and (b) crashes right
# after loading while printing a checkmark, because Windows' console
# codepage can't encode it.
$env:USE_MULTILINGUAL_MODEL = "true"
$env:PYTHONIOENCODING = "utf-8"
$env:PYTHONUTF8 = "1"

$healthUrl = "http://127.0.0.1:$port/health"
$voicesUrl = "http://127.0.0.1:$port/voices"

Push-Location $repo
try {
    $proc = Start-Process -FilePath $python -ArgumentList "-m", "uvicorn", "app.main:app", "--host", "127.0.0.1", "--port", $port -NoNewWindow -PassThru

    # Wait for the model to finish loading, then register "nala" in the
    # server's voice library so requests for voice="nala" resolve to our
    # reference.wav (and its language) instead of silently falling back to
    # the built-in English default.
    $ready = $false
    for ($i = 0; $i -lt 60 -and -not $proc.HasExited; $i++) {
        Start-Sleep -Seconds 3
        try {
            $health = Invoke-RestMethod -Uri $healthUrl -TimeoutSec 3
            if ($health.initialization_state -eq "ready") { $ready = $true; break }
            if ($health.initialization_state -eq "error") {
                Write-Warning "Chatterbox model failed to initialize: $($health.initialization_error)"
                break
            }
        } catch {}
    }

    if ($ready) {
        # Windows PowerShell 5.1 has no -Form parameter on Invoke-RestMethod
        # (added in PS6+), so shell out to curl.exe for the multipart POST.
        $curlOutput = & curl.exe -s -o NUL -w "%{http_code}" -X POST $voicesUrl `
            -F "voice_name=nala" `
            -F "language=es" `
            -F "voice_file=@$voiceSample"
        if ($curlOutput -eq "200" -or $curlOutput -eq "201") {
            Write-Host "Registered 'nala' voice from $voiceSample"
        } elseif ($curlOutput -eq "409") {
            # already registered from a previous run - that's fine.
        } else {
            Write-Warning "Failed to register 'nala' voice: HTTP $curlOutput"
        }
    }

    Wait-Process -Id $proc.Id
} finally {
    Pop-Location
}
