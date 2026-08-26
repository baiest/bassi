#!/bin/bash

# check_commit.sh - Script to verify commit message format
# Format:
# [Action]: [title]
#
# What: [text]
# Why: [text]
#
# [Ticket ID]

# Configuration
MAX_TITLE_LENGTH=72
MAX_BODY_LINE_LENGTH=100
BASE_BRANCH="$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@')"
BASE_BRANCH="${BASE_BRANCH:-main}"

# Colors for output
SCRIPT_DIR="$(dirname "$0")"
source "$SCRIPT_DIR/colors.sh"

echo -e "${BLUE}🔍 Checking commit message formats...${NC}"

# Allowed actions exactly as requested
ALLOWED_ACTIONS_REGEX="^(Add|Fix|Cut|Optimise|Refactor|Delete|Docs|Style|Merge)"

validate_commit_message() {
    local msg="$1"
    local hash="$2"
    # 1. Check title line (first line)
    local title=$(echo "$msg" | head -n 1)

    # 0. Skip merge commits (automatic or manual pattern)
    if [[ "$title" =~ ^Merge[[:space:]].* ]]; then
        return 0
    fi

    local status=0

    
    if [[ ! "$title" =~ $ALLOWED_ACTIONS_REGEX ]]; then
        echo -e "${RED}❌ Invalid Action in commit ${YELLOW}$hash${NC}"
        echo -e "   Title: \"$title\""
        echo -e "   Allowed actions: Add, Fix, Cut, Optimise, Refactor, Delete, Docs, Style, Merge"
        status=1
    fi

    if [[ ! "$title" =~ :[[:space:]].+ ]]; then
        echo -e "${RED}❌ Missing title description or colon in commit ${YELLOW}$hash${NC}"
        echo -e "   Format: [Action]: [title]"
        status=1
    fi

    if [[ ${#title} -gt $MAX_TITLE_LENGTH ]]; then
        echo -e "${RED}❌ Title too long (${#title}/$MAX_TITLE_LENGTH) in commit ${YELLOW}$hash${NC}"
        status=1
    fi

    # 2. Check for empty line after title
    local second_line=$(echo "$msg" | sed -n '2p')
    if [[ -n "$second_line" ]]; then
        echo -e "${RED}❌ Body must be separated from title by an empty line in commit ${YELLOW}$hash${NC}"
        status=1
    fi

    # 3. Check What: and Why: sections
    if [[ ! "$msg" =~ "What:" ]]; then
        echo -e "${RED}❌ Missing 'What:' section in commit ${YELLOW}$hash${NC}"
        status=1
    fi

    if [[ ! "$msg" =~ "Why:" ]]; then
        echo -e "${RED}❌ Missing 'Why:' section in commit ${YELLOW}$hash${NC}"
        status=1
    fi

    # 4. Check lengths of What/Why lines
    while IFS= read -r line; do
        if [[ "$line" =~ ^(What:|Why:) ]]; then
            local content=${line#*: }
            if [[ ${#content} -gt $MAX_BODY_LINE_LENGTH ]]; then
                echo -e "${RED}❌ Body line too long (${#content}/$MAX_BODY_LINE_LENGTH) in commit ${YELLOW}$hash${NC}"
                echo -e "   Line: \"$line\""
                status=1
            fi
        fi
    done <<< "$msg"

    return $status
}

# Get commits in current branch not in BASE_BRANCH
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$CURRENT_BRANCH" == "$BASE_BRANCH" ]; then
    echo -e "${YELLOW}On main branch. Checking last 5 commits (no merges)...${NC}"
    COMMITS=$(git rev-list --no-merges --max-count=5 HEAD)
else
    # Try to find a valid base branch (local main or origin/main)
    actual_base=$BASE_BRANCH
    if ! git rev-parse --verify "$actual_base" >/dev/null 2>&1; then
        actual_base="origin/$BASE_BRANCH"
    fi

    if ! git rev-parse --verify "$actual_base" >/dev/null 2>&1; then
        echo -e "${RED}❌ Error: Could not find base branch '$BASE_BRANCH' or 'origin/$BASE_BRANCH'${NC}"
        exit 1
    fi

    echo -e "${BLUE}Checking commits in branch ${YELLOW}$CURRENT_BRANCH${BLUE} (comparing to ${YELLOW}$actual_base${BLUE})...${NC}"
    COMMITS=$(git rev-list --no-merges "$actual_base..HEAD" --)

    if [ -z "$COMMITS" ]; then
        echo -e "${YELLOW}No new commits found in this branch.${NC}"
        exit 0
    fi
fi

FAIL=0
for hash in $COMMITS; do
    MESSAGE=$(git log -1 --pretty=%B $hash)
    if ! validate_commit_message "$MESSAGE" "$hash"; then
        FAIL=1
    fi
done

echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

if [ $FAIL -ne 0 ]; then
    echo -e "${RED}❌ COMMIT FORMAT CHECK FAILED${NC}"
    exit 1
else
    echo -e "${GREEN}✅ ALL COMMITS PASS FORMAT CHECK${NC}"
    exit 0
fi
