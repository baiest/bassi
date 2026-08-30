# stt-setup.ps1
#
# One-time, idempotent setup for the STT crate (crates/stt, whisper-rs).
# Two independent things, each skipped if already present:
#
#   1. LLVM/libclang - whisper-rs-sys's build script uses bindgen, which
#      needs libclang.dll to generate whisper.cpp's C bindings. This is a
#      build-time dependency of the crate itself, not something Nala reads
#      at runtime.
#   2. A Whisper GGML model into data/whisper/ (ggerganov/whisper.cpp on
#      Hugging Face). Override the size with -ModelSize (tiny/base/small/
#      medium/large), or the destination with NALA_WHISPER_MODEL - the same
#      variable crates/stt reads.
#
# Re-running this script is safe.

param(
    [string]$ModelSize = "base"
)

$ErrorActionPreference = "Stop"

# --- 1. LLVM / libclang ---------------------------------------------------

$libclangDir = "C:\Program Files\LLVM\bin"
$installedAtDefault = Test-Path (Join-Path $libclangDir "libclang.dll")

if ($env:LIBCLANG_PATH -and (Test-Path (Join-Path $env:LIBCLANG_PATH "libclang.dll"))) {
    Write-Host "libclang already configured via LIBCLANG_PATH=$env:LIBCLANG_PATH"
} elseif ($installedAtDefault) {
    Write-Host "LLVM already installed at $libclangDir"
    [Environment]::SetEnvironmentVariable("LIBCLANG_PATH", $libclangDir, "User")
    $env:LIBCLANG_PATH = $libclangDir
    Write-Host "Set LIBCLANG_PATH=$libclangDir for this session and future ones."
} else {
    Write-Host "Installing LLVM (provides libclang.dll)..."
    winget install --id LLVM.LLVM -e --accept-package-agreements --accept-source-agreements

    if (-not (Test-Path (Join-Path $libclangDir "libclang.dll"))) {
        Write-Error "LLVM install finished but libclang.dll wasn't found at $libclangDir. Find where it installed and set LIBCLANG_PATH manually."
        exit 1
    }

    [Environment]::SetEnvironmentVariable("LIBCLANG_PATH", $libclangDir, "User")
    $env:LIBCLANG_PATH = $libclangDir
    Write-Host "LLVM installed. Set LIBCLANG_PATH=$libclangDir for this session and future ones."
    Write-Host "(Restart your terminal so other tools pick up the User env var too.)"
}

# --- 2. Whisper model -------------------------------------------------------

$validSizes = @("tiny", "base", "small", "medium", "large")
if ($validSizes -notcontains $ModelSize) {
    Write-Error "Unknown -ModelSize '$ModelSize'. Valid: $($validSizes -join ', ')"
    exit 1
}

$modelPath = if ($env:NALA_WHISPER_MODEL) { $env:NALA_WHISPER_MODEL } else { "$PSScriptRoot\..\data\whisper\ggml-$ModelSize.bin" }
$modelDir = Split-Path -Parent $modelPath

if (Test-Path $modelPath) {
    Write-Host "Whisper model already present at $modelPath"
} else {
    Write-Host "Downloading Whisper '$ModelSize' model..."
    New-Item -ItemType Directory -Force -Path $modelDir | Out-Null

    $modelUrl = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-$ModelSize.bin"
    Invoke-WebRequest -Uri $modelUrl -OutFile $modelPath

    Write-Host "Whisper model installed at $modelPath"
}

Write-Host ""
Write-Host "STT setup complete."
Write-Host "  LIBCLANG_PATH     = $env:LIBCLANG_PATH"
Write-Host "  NALA_WHISPER_MODEL = $modelPath"
Write-Host "Run: cargo build -p stt"
