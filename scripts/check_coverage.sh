#!/bin/bash

# check_coverage.sh - Script to verify test coverage thresholds
# Requirement: Total project line coverage must be at least MIN_TOTAL %

# Configuration
MIN_TOTAL=80

# Colors for output
SCRIPT_DIR="$(dirname "$0")"
source "$SCRIPT_DIR/colors.sh"

echo -e "${BLUE}🔍 Starting Rust Coverage Checks...${NC}"
echo -e "${BLUE}Threshold: Total line coverage >= ${MIN_TOTAL}%${NC}"

# Ensure we're in the project root (where Cargo.toml is)
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT" || exit 1

# 1. Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}❌ Error: cargo is not installed. Please install Rust before running this script.${NC}"
    exit 1
fi

# 2. Check/Install cargo-llvm-cov
if ! cargo llvm-cov --version &> /dev/null; then
    echo -e "${BLUE}Installing cargo-llvm-cov...${NC}"
    cargo install cargo-llvm-cov
fi

# 3. Generate lcov report (for artifacts / external tooling)
# main.rs and bootstrap.rs (in both apps/nala and apps/voice) are excluded:
# they're just wiring (composition root), not testable logic.
# adapters/process/windows.rs and crates/mcp's child_process.rs are
# excluded: both are thin OS boundaries (Command::new) that can't be
# exercised portably in CI. The protocol logic they carry (StdioMcpClient)
# is covered separately against an in-memory Transport fake. cli/prompt.rs
# is excluded the same way: a thin composition of reedline's own
# (independently tested) editor, with no branching logic of our own left
# to exercise.
# crates/tts's chatterbox/supervisor.rs is excluded for the same reason:
# past its pure `decide` function (covered by chatterbox_supervisor.rs),
# it's Command::spawn + a real HTTP health check against a live Chatterbox
# server, which isn't available in CI.
# crates/tts's piper/speech.rs is excluded for the same reason as
# supervisor.rs: past its pure `build_args`/`normalize_text` helpers
# (covered by piper.rs), the rest is Command::spawn + reading a real
# Piper process's stdout/exit status, which needs a real Piper install
# that isn't available in CI. backend.rs is excluded like main.rs: it's
# TTS backend selection wiring, not logic with its own branches to test.
# crates/stt's capture.rs is excluded past its pure `resample_linear`
# helper (covered by its own unit tests): the rest needs a real
# microphone/input device, which CI doesn't have. transcribe.rs is
# excluded for the same reason as chatterbox/supervisor.rs and
# piper/speech.rs: it needs a real Whisper model file, which is
# gigabytes and gitignored, not something CI downloads. vad.rs is a thin
# wrapper over ONNX Runtime, which CI doesn't provide either; the logic
# built on top of it (session.rs, ring.rs, resample.rs, wake.rs) is pure
# or generic over a trait and stays fully measured. stream.rs needs a
# real microphone/input device, same reason as adapters/process/windows.rs.
# apps/nala-overlay's overlay.rs and playback.rs are excluded for the same
# reason as capture.rs/stream.rs above: past the pure steps they call into
# (amplitude.rs, clip.rs, color.rs, voice_client.rs — all still fully
# measured), the rest is a real audio input/output device and an on-screen
# eframe window, neither available in CI.
IGNORE_REGEX='(main|bootstrap)\.rs$|adapters[/\\]process[/\\]windows\.rs$|crates[/\\]mcp[/\\]src[/\\]child_process\.rs$|crates[/\\]tts[/\\]src[/\\](chatterbox[/\\]supervisor|piper[/\\]speech|backend)\.rs$|crates[/\\]stt[/\\]src[/\\](capture|transcribe|vad|stream)\.rs$|cli[/\\]prompt\.rs$|nala-overlay[/\\]src[/\\](overlay|playback)\.rs$'
echo -e "${BLUE}Running tests with coverage instrumentation...${NC}"
cargo llvm-cov --workspace --all-features --ignore-filename-regex "$IGNORE_REGEX" --lcov --output-path lcov.info

# 4. Enforce the total line coverage threshold
echo -e "${BLUE}Analyzing total coverage...${NC}"
if cargo llvm-cov report --workspace --ignore-filename-regex "$IGNORE_REGEX" --summary-only --fail-under-lines "$MIN_TOTAL"; then
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}✅ COVERAGE CHECK PASSED${NC}"
    exit 0
else
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${RED}❌ COVERAGE CHECK FAILED (Minimum ${MIN_TOTAL}%)${NC}"
    echo -e "${YELLOW}Tip: Add tests to reach ${MIN_TOTAL}% total line coverage.${NC}"
    exit 1
fi
