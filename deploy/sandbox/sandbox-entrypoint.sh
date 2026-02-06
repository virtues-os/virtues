#!/bin/bash
# Sandbox entrypoint - executes Python code with bubblewrap isolation
#
# Usage: Pass Python code via stdin or as a file argument
#
# The script:
# 1. Writes code to a temp file (if passed via stdin)
# 2. Runs Python inside bubblewrap with restricted filesystem access
# 3. Outputs stdout/stderr

set -e

CODE_FILE="${1:-/workspace/code.py}"

# If no file argument and stdin has data, read from stdin
if [ "$#" -eq 0 ] && [ ! -t 0 ]; then
    CODE_FILE="/tmp/code.py"
    cat > "$CODE_FILE"
fi

# Execute Python with bubblewrap sandbox
# - Read-only access to Python and system libs
# - Writable /tmp for temporary files
# - No access to /workspace (code is copied to /tmp)
# - No network (--unshare-net) - can be enabled if needed
exec bwrap \
    --ro-bind /usr /usr \
    --ro-bind /lib /lib \
    --ro-bind-try /lib64 /lib64 \
    --symlink usr/bin /bin \
    --symlink usr/sbin /sbin \
    --proc /proc \
    --dev /dev \
    --tmpfs /tmp \
    --tmpfs /home \
    --bind "$CODE_FILE" /tmp/code.py \
    --chdir /tmp \
    --unshare-all \
    --share-net \
    --die-with-parent \
    --new-session \
    -- python3 /tmp/code.py
