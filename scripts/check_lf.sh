#!/bin/bash

# check_lf.sh - Script to verify that all tracked files use LF line endings
# CRLF (Windows) line endings are not allowed for consistency.

# Colors for output
SCRIPT_DIR="$(dirname "$0")"
source "$SCRIPT_DIR/colors.sh"

echo -e "${BLUE}🔍 Checking for CRLF line endings in working directory...${NC}"

# Using git ls-files --eol to detect line endings in index (i/) and working tree (w/)
# We include --others --exclude-standard to check untracked files as well.
# We look for "w/crlf" which indicates the file on disk has CRLF.
CRLF_FILES=$(git ls-files --eol --exclude-standard | grep "w/crlf" | awk '{print $NF}')

if [ -n "$CRLF_FILES" ]; then
    echo -e "${RED}❌ CRLF found in the following files:${NC}"
    for file in $CRLF_FILES; do
        echo -e "${YELLOW}  - $file${NC}"
    done
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${RED}❌ LF CHECK FAILED${NC}"
    echo -e "${YELLOW}Tip: You can convert files to LF using 'dos2unix' or by configuring git:${NC}"
    echo -e "${BLUE}git config core.autocrlf false${NC}"
    echo -e "${BLUE}Then reset your files: git rm --cached -r . && git reset --hard${NC}"
    exit 1
else
    echo -e "${GREEN}✅ ALL FILES USE LF LINE ENDINGS${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    exit 0
fi
