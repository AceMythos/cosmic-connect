#!/usr/bin/env bash
# Companion script: send N files of varying sizes to a KDE Connect device in
# parallel, so the stress-test monitor can validate concurrent transfers.
#
# Usage:
#   ./kdeconnect-stress-send.sh <device_id> [files_per_size]
#
# Sends files_per_size (default: 3) files each of size SMALL, MEDIUM, LARGE
# (see env vars below), launching all sends as background jobs.

set -euo pipefail

DEVICE_ID="${1:?Usage: $0 <device_id> [files_per_size]}"
N="${2:-3}"

# Size overrides (in bytes); adjust via env, e.g. SMALL=$((2*1024*1024))
SMALL="${SMALL:-$((   1 * 1024 * 1024 ))}"   #   1 MB
MEDIUM="${MEDIUM:-$(( 50 * 1024 * 1024 ))}"   #  50 MB
LARGE="${LARGE:-$(( 500 * 1024 * 1024 ))}"    # 500 MB

# How many sends to run concurrently before inserting a short breather.
# 0 = no limit (fire everything at once).
BATCH="${BATCH:-0}"

KDECONNECT_CLI="$(command -v kdeconnect-cli || true)"
QDBUS="$(command -v qdbus || true)"

if [ -z "$KDECONNECT_CLI" ] && [ -z "$QDBUS" ]; then
    echo "ERROR: neither kdeconnect-cli nor qdbus found in PATH" >&2
    exit 1
fi

SEND_CMD=""
if [ -n "$KDECONNECT_CLI" ]; then
    SEND_CMD=("$KDECONNECT_CLI" --share-file "" -d "$DEVICE_ID")
    echo "Using: kdeconnect-cli"
else
    QOBJ="/modules/kdeconnect/devices/${DEVICE_ID}/share"
    SEND_CMD=("$QDBUS" "org.kde.kdeconnect" "$QOBJ" "shareUrl" "")
    echo "Using: qdbus"
fi

# --- helpers ----------------------------------------------------------------
TMPDIR=$(mktemp -d /tmp/kdeconnect-stress-send-XXXXXX)
trap 'rm -rf "$TMPDIR"' EXIT

make_file() {
    local path="$1" size="$2"
    if command -v fallocate &>/dev/null; then
        fallocate -l "$size" "$path"
    else
        dd if=/dev/zero of="$path" bs=1048576 count=$(( size / 1048576 )) \
            status=none 2>/dev/null
        # handle remainder
        local rem=$(( size % 1048576 ))
        if [ "$rem" -gt 0 ]; then
            dd if=/dev/zero of="$path" bs=1 count="$rem" \
               seek="$(( size - rem ))" status=none 2>/dev/null
        fi
    fi
}

send_one() {
    local file="$1"
    if [ -n "$KDECONNECT_CLI" ]; then
        "$KDECONNECT_CLI" --share-file "$file" -d "$DEVICE_ID" &>/dev/null || true
    else
        "$QDBUS" "org.kde.kdeconnect" \
            "/modules/kdeconnect/devices/${DEVICE_ID}/share" \
            "shareUrl" "file://${file}" &>/dev/null || true
    fi
}

# --- create files -----------------------------------------------------------
echo "Creating test files in ${TMPDIR} ..."

for class in small medium large; do
    case "$class" in
        small)  SIZE=$SMALL;  LABEL="1MB";;
        medium) SIZE=$MEDIUM; LABEL="50MB";;
        large)  SIZE=$LARGE;  LABEL="500MB";;
    esac
    for i in $(seq 1 "$N"); do
        f="${TMPDIR}/${class}-${i}.dat"
        make_file "$f" "$SIZE"
        echo "  Created ${class}-${i}.dat (${LABEL})"
    done
done

TOTAL_FILES=$(( 3 * N ))
echo "Total files: ${TOTAL_FILES}"
echo "Size breakdown: ${SMALL}+${MEDIUM}+${LARGE} bytes each × ${N}"
echo

# --- send files -------------------------------------------------------------
echo "Sending files in parallel to device ${DEVICE_ID} ..."
echo

START=$(date +%s%N)
PID_LIST=()

launch_all() {
    for f in "$TMPDIR"/*.dat; do
        send_one "$f" &
        PID_LIST+=("$!")
        if [ "$BATCH" -gt 0 ]; then
            # limit concurrent jobs
            while jobs -r | wc -l | grep -q "^[${BATCH}-9]"; do
                sleep 0.1
            done
        fi
    done
}

launch_all

echo "Waiting for ${#PID_LIST[@]} background send jobs to complete ..."
wait

END=$(date +%s%N)
DURATION_MS=$(( (END - START) / 1000000 ))
echo
echo "All ${TOTAL_FILES} send commands completed in ${DURATION_MS}ms"
echo "Temporary files cleaned up automatically."
