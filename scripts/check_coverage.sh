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
# main.rs is excluded: it's just wiring (composition root), not testable logic.
# adapters/process/windows.rs, adapters/mcp/child_process.rs, and
# adapters/mcp/job_object.rs are excluded: all are thin OS boundaries
# (Command::new / Win32 Job Objects) that can't be exercised portably in CI.
# The protocol logic they carry (StdioMcpClient) is covered separately
# against an in-memory Transport fake. cli/prompt.rs is excluded the same
# way: a thin composition of reedline's own (independently tested) editor,
# with no branching logic of our own left to exercise.
# adapters/speech/chatterbox/supervisor.rs is excluded for the same reason:
# past its pure `decide` function (covered by chatterbox_supervisor.rs),
# it's Command::spawn + a real HTTP health check against a live Chatterbox
# server, which isn't available in CI.
IGNORE_REGEX='main\.rs$|adapters[/\\](process[/\\]windows|mcp[/\\](child_process|job_object)|speech[/\\]chatterbox[/\\]supervisor)\.rs$|cli[/\\]prompt\.rs$'
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
