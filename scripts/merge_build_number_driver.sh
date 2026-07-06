#!/bin/bash

# This is referenced in git attributes as a merge driver to keep the build number
# increasing, without a manual merge intervention

# Git passes three arguments to the merge driver:
# %O - ancestor's version
# %A - current version (the one that will be overwritten with the result)
# %B - other branch's version

ANCESTOR=$1
CURRENT=$2
OTHER=$3

# Read values, defaulting to 0 if file is missing or empty
VAL_A=$(cat "$CURRENT" 2>/dev/null || echo 0)
VAL_B=$(cat "$OTHER" 2>/dev/null || echo 0)

# Compare and keep the maximum value
if [ "$VAL_B" -gt "$VAL_A" ]; then
    echo "$VAL_B" > "$CURRENT"
else
    echo "$VAL_A" > "$CURRENT"
fi

# Exit with 0 to indicate successful resolution
exit 0
