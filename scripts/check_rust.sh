#!/bin/bash

# check_rust.sh - Script to verify Rust standards (fmt, clippy, check, test)
# This script is intended to be used in both local development and CI/CD pipelines.

set -e

# Colors for output
SCRIPT_DIR="$(dirname "$0")"
source "$SCRIPT_DIR/colors.sh"

echo -e "${BLUE}🔍 Starting Rust Code Quality Checks...${NC}"

# Ensure we're in the project root (where Cargo.toml is)
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT" || exit 1

# 1. Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}❌ Error: cargo is not installed. Please install Rust before running this script.${NC}"
    exit 1
fi

# 2. Run cargo fmt check
echo -e "${BLUE}1/4 Formatting Check (cargo fmt)...${NC}"
if cargo fmt --all -- --check; then
    echo -e "${GREEN}✅ Code is formatted correctly.${NC}"
else
    echo -e "${RED}❌ The following files are not formatted correctly.${NC}"
    echo -e "${BLUE}💡 Tip: Run 'cargo fmt --all' to fix these issues.${NC}"
    exit 1
fi

# 3. Run cargo clippy
echo -e "${BLUE}2/4 Running cargo clippy...${NC}"
if ! cargo clippy --version &> /dev/null; then
    echo -e "${BLUE}Installing clippy component...${NC}"
    rustup component add clippy
fi

if cargo clippy --workspace --all-targets --all-features -- -D warnings; then
    echo -e "${GREEN}✅ cargo clippy passed.${NC}"
else
    echo -e "${RED}❌ cargo clippy found issues.${NC}"
    exit 1
fi

# 4. Run cargo check
echo -e "${BLUE}3/4 Running cargo check...${NC}"
if cargo check --workspace --all-targets; then
    echo -e "${GREEN}✅ cargo check passed.${NC}"
else
    echo -e "${RED}❌ cargo check found issues.${NC}"
    exit 1
fi

# 5. Run cargo test
echo -e "${BLUE}4/4 Running cargo test...${NC}"
if cargo test --workspace --all-features; then
    echo -e "${GREEN}✅ cargo test passed.${NC}"
else
    echo -e "${RED}❌ cargo test found issues.${NC}"
    exit 1
fi

echo -e "${GREEN}🚀 All checks passed successfully! Your code is clean.${NC}"
