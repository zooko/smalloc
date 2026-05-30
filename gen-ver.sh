#!/bin/bash

if [ -n "$(git status --porcelain)" ] ; then
    echo "Error: current working directory is git-status-dirty"
    echo "exiting"
    exit 1
fi

CURRENT_TAG=$(git describe --tags --abbrev=0)
if [[ ! "$CURRENT_TAG" =~ ^v ]]; then
    echo "Error: current tag must start with 'v'. Got: '$CURRENT_TAG'" >&2
    echo "exiting"
    exit 1
fi
CURRENT_TAG_WITHOUT_V="${CURRENT_TAG#v}"

NEW_VERSION=$1

assert_version_greater() {
    local v1="$1"
    local v2="$2"

    # Validate inputs are not empty
    if [ -z "$v1" ] || [ -z "$v2" ]; then
        echo "Error: Both version arguments must be provided." >&2
        echo "exiting"
        return 1
    fi

    # Use sort -V (version sort) to compare.
    # If v2 is greater than v1, then sorting them and taking the first (head -n 1)
    # will result in v1. If v1 is the first one, then v2 > v1 is TRUE.
    # If v2 is the first one (or equal), then v2 > v1 is FALSE.

    local lowest
    lowest=$(printf '%s\n' "$v1" "$v2" | sort -V | head -n 1)

    if [ "$lowest" != "$v1" ]; then
        echo "Error: Version '$v2' is not greater than '$v1'." >&2
        echo "exiting"
        return 1
    fi

    # Optional: Check for equality explicitly if sort -V behavior on your system is ambiguous for equals
    if [ "$v1" = "$v2" ]; then
        echo "Error: Version '$v2' is equal to '$v1', not greater." >&2
        echo "exiting"
        return 1
    fi

    echo "Success: $v2 is greater than $v1"
    return 0
}

assert_version_greater "$CURRENT_TAG_WITHOUT_V" "$NEW_VERSION" || exit 1

# Okay now write the new version into Cargo.toml
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/^version = .*/version = \"$NEW_VERSION\"/" Cargo.toml
else
    sed -i "s/^version = .*/version = \"$NEW_VERSION\"/" Cargo.toml
fi

# … and update Cargo.lock
cargo update -w --offline

# … and commit Cargo.toml and Cargo.lock
git add Cargo.toml Cargo.lock
git commit -m "Update the Cargo.toml version number and Cargo.lock files to reflect version $NEW_VERSION (automated commit by gen-ver.sh)"

# Now get the commit
CURRENT_COMMIT=$(git rev-parse HEAD)

# Append the commit to the version in the "build metadata" slot
NEW_FULL_VERSION="${NEW_VERSION}+${CURRENT_COMMIT}"

# Write the full new version into Cargo.toml
if [[ "$OSTYPE" == "darwin"* ]]; then
    sed -i '' "s/^version = .*/version = \"$NEW_FULL_VERSION\"/" Cargo.toml
else
    sed -i "s/^version = .*/version = \"$NEW_FULL_VERSION\"/" Cargo.toml
fi

# … and update Cargo.lock
cargo update -w --offline

# … and commit Cargo.toml and Cargo.lock
git add Cargo.toml Cargo.lock
git commit -m "Update the Cargo.toml version number and Cargo.lock files to reflect version $NEW_FULL_VERSION (automated commit by gen-ver.sh)"

# Now git tag it with $NEW_FULL_VERSION
git tag v${NEW_FULL_VERSION}
