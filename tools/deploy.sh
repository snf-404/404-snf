#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# tools/deploy.sh — put a built 404-snf on a board and start it.
#
#   tools/deploy.sh root@board.local
#   SNF_TARGET=root@board.local tools/deploy.sh
#   just deploy root@board.local
#
# The board's address lives nowhere in this repository on purpose: pass it, or
# export SNF_TARGET from a shell profile or an untracked file.
#
# The order below is the one that matters, and it is why this is a script rather
# than a handful of scp lines:
#
#   1. the model must exist before anything is uploaded, because exporting it
#      can fail and a half-deployed board is worse than an untouched one;
#   2. the application must be stopped before its binary is overwritten (a
#      running ELF is busy) and before the coprocessor under it disappears;
#   3. the CM33 reload is stop → remove → upload → point → start. This mirrors
#      the board's own `~/rfirm.sh` with its "press ENTER once you have scp'd the
#      firmware" pause replaced by the upload itself, and with the M33 found by
#      name each run, since its remoteproc index changes from boot to boot;
#   4. the application starts last, once there is a coprocessor for its IPC
#      bring-up handshake to find.
#
# Everything is idempotent: re-running it re-deploys and restarts.

set -euo pipefail

readonly UNIT_NAME="snf-app.service"
readonly REMOTE_APP_DIR="/opt/snf/app"
readonly REMOTE_LOG="/var/log/snf/snf-app.log"
readonly REMOTE_FIRMWARE_DIR="/lib/firmware"
readonly FIRMWARE_NAME="cm33.elf"
# The unit csti writes into dist/. It runs the same binary off the same UIO
# devices and the same BLE adapter, so the two must never both be enabled.
readonly SUPERSEDED_UNIT="consortium-app.service"

usage() {
    sed -n '3,26p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m warn\033[0m %s\n' "$*" >&2; }
die() {
    printf '\033[1;31mfail\033[0m %s\n' "$*" >&2
    exit 1
}

# First `key = "value"` in a TOML file. Adequate because every key this reads is
# unique across the file; it is not a TOML parser.
toml_string() {
    sed -n "s/^[[:space:]]*$1[[:space:]]*=[[:space:]]*\"\([^\"]*\)\".*/\1/p" "$2" | head -n 1
}

target=""
config="crates/app/Repose.toml"
do_firmware=1
do_model=1
do_enable=1

while [ $# -gt 0 ]; do
    case "$1" in
        -h | --help)
            usage
            exit 0
            ;;
        --config)
            config="${2:-}"
            [ -n "$config" ] || die "--config needs a path"
            shift 2
            ;;
        --skip-firmware)
            do_firmware=0
            shift
            ;;
        --skip-model)
            do_model=0
            shift
            ;;
        --no-enable)
            do_enable=0
            shift
            ;;
        -*) die "unknown option $1 (see --help)" ;;
        *)
            target="$1"
            shift
            ;;
    esac
done

target="${target:-${SNF_TARGET:-}}"
[ -n "$target" ] || die "no board: pass root@host, or export SNF_TARGET"

cd "$(dirname "${BASH_SOURCE[0]}")/.."

local_app="dist/app/snf-app"
local_firmware="dist/firmware/${FIRMWARE_NAME}"
local_model="ml/out/fatigue.onnx"
local_artifact="ml/out/fatigue-linear.json"

# ── 1. The model ─────────────────────────────────────────────────────────────
# `fatigue-export` loads the trained PyTorch artifact and re-emits the ONNX graph
# the board runs, checking the two agree to 1e-4 before it writes. Training needs
# a dataset this script has no business inventing, so a missing artifact stops
# here with the command that produces one.
if [ "$do_model" -eq 1 ]; then
    if [ ! -f "$local_model" ]; then
        [ -f "$local_artifact" ] ||
            die "neither $local_model nor $local_artifact exists; train first: just ml-train"
        command -v uv >/dev/null 2>&1 || die "uv not on PATH; needed to export $local_model"
        say "no $local_model; exporting it from $local_artifact"
        (cd ml && uv run fatigue-export)
        [ -f "$local_model" ] || die "export finished but $local_model is still missing"
    elif [ "$local_artifact" -nt "$local_model" ]; then
        warn "$local_artifact is newer than $local_model; re-export with 'just ml-export' if that is not intended"
    fi
    [ -f "$local_model" ] || die "$local_model is missing"
fi

# ── Everything else that has to exist before the board is touched ────────────
[ -f "$config" ] || die "$config not found"
[ -f "$local_app" ] || die "$local_app not found; build first: just container-build"
if [ "$do_firmware" -eq 1 ]; then
    [ -f "$local_firmware" ] || die "$local_firmware not found; build first: just container-build"
fi

if command -v file >/dev/null 2>&1; then
    case "$(file -b "$local_app")" in
        *aarch64* | *ARM*) ;;
        *) warn "$local_app does not look like an aarch64 binary; is dist/ from a host build?" ;;
    esac
fi

log_filter="$(toml_string log "$config")"
log_filter="${log_filter:-info}"
remote_model="$(toml_string model_path "$config")"
remote_model="${remote_model:-/opt/snf/fatigue.onnx}"

# ── A single multiplexed SSH connection for the whole run ────────────────────
ssh_dir="$(mktemp -d)"
cleanup() {
    ssh -o ControlPath="$ssh_dir/control" -O exit "$target" 2>/dev/null || true
    rm -rf "$ssh_dir"
}
trap cleanup EXIT

ssh_options=(-o ControlMaster=auto -o ControlPath="$ssh_dir/control"
    -o ControlPersist=120 -o ConnectTimeout=15)

remote() { ssh "${ssh_options[@]}" "$target" "$@"; }

# Upload beside the destination and rename, so a connection that drops mid-copy
# leaves the previous file in place rather than a truncated one.
push() {
    local source="$1" destination="$2" mode="${3:-644}"
    scp -q -o ControlPath="$ssh_dir/control" "$source" "$target:$destination.upload"
    remote "chmod $mode '$destination.upload' && mv -f '$destination.upload' '$destination'"
}

say "connecting to $target"
remote true || die "cannot reach $target over ssh"
remote 'command -v systemctl >/dev/null 2>&1' || die "$target has no systemctl"
if [ "$do_firmware" -eq 1 ]; then
    remote '[ -d /sys/class/remoteproc/remoteproc0 ]' ||
        die "no /sys/class/remoteproc/remoteproc0 on $target"
fi

# ── 2. Stop what is running ──────────────────────────────────────────────────
say "stopping $UNIT_NAME and $SUPERSEDED_UNIT"
remote "systemctl stop '$UNIT_NAME' >/dev/null 2>&1 || true
        systemctl disable --now '$SUPERSEDED_UNIT' >/dev/null 2>&1 || true"

# ── 3. Upload the payload ────────────────────────────────────────────────────
say "uploading application, configuration and model"
remote "mkdir -p '$REMOTE_APP_DIR' '$(dirname "$remote_model")' '$(dirname "$REMOTE_LOG")'"
push "$local_app" "$REMOTE_APP_DIR/snf-app" 755
push "$config" "$REMOTE_APP_DIR/Repose.toml" 644
if [ "$do_model" -eq 1 ]; then
    push "$local_model" "$remote_model" 644
fi

# ── 4. The CM33: stop, remove, upload, point, start ──────────────────────────
if [ "$do_firmware" -eq 1 ]; then
    say "reloading the CM33"
    # Which index the M33 lands on is decided at probe time and changes from one
    # boot to the next, so it is looked up by name on every run rather than
    # inferred from remoteproc0 the way the board's rfirm.sh does.
    #
    # Read into a variable first rather than inlining the here-document in the
    # `$(ssh …)` below: bash 3.2, which is what macOS ships, counts parentheses
    # inside a here-document that sits in a command substitution and ends the
    # substitution early on the first unbalanced one.
    IFS= read -r -d '' stop_script <<'REMOTE' || true
set -e
base=/sys/class/remoteproc
firmware_dir=$1
firmware_name=$2

rproc=
found=
for candidate in "$base"/remoteproc*; do
    [ -r "$candidate/name" ] || continue
    candidate_name=$(cat "$candidate/name")
    found="$found $(basename "$candidate")=$candidate_name"
    if [ "$candidate_name" = m33 ]; then
        rproc=$(basename "$candidate")
        break
    fi
done
if [ -z "$rproc" ]; then
    echo "no remoteproc instance is named m33; found:$found" >&2
    exit 1
fi

state=$(cat "$base/$rproc/state" 2>/dev/null || echo unknown)
echo "  m33 is $rproc, currently $state" >&2
# Writing `stop` to an already-stopped instance is an error, not a no-op.
if [ "$state" = running ]; then
    echo "  stopping $rproc" >&2
    echo stop > "$base/$rproc/state"
fi
if [ -f "$firmware_dir/$firmware_name" ]; then
    echo "  removing $firmware_dir/$firmware_name" >&2
    rm -f "$firmware_dir/$firmware_name"
fi

echo "$rproc"
REMOTE

    rproc="$(printf '%s' "$stop_script" |
        ssh "${ssh_options[@]}" "$target" sh -s -- "$REMOTE_FIRMWARE_DIR" "$FIRMWARE_NAME")"
    [ -n "$rproc" ] || die "could not work out which remoteproc instance is the M33"

    push "$local_firmware" "$REMOTE_FIRMWARE_DIR/$FIRMWARE_NAME" 644

    ssh "${ssh_options[@]}" "$target" sh -s -- "$rproc" "$FIRMWARE_NAME" <<'REMOTE'
set -e
base=/sys/class/remoteproc
rproc=$1
firmware_name=$2

echo "$firmware_name" > "$base/$rproc/firmware"
echo start > "$base/$rproc/state"

state=$(cat "$base/$rproc/state")
echo "  $rproc is $state"
[ "$state" = running ] || {
    echo "coprocessor did not start; check dmesg on the board" >&2
    exit 1
}
REMOTE
else
    say "skipping the CM33 (--skip-firmware)"
fi

# ── 5. The unit, and the log ─────────────────────────────────────────────────
# `log` in Repose.toml is the filter, so the unit deliberately sets no RUST_LOG:
# one source of truth, and `systemctl edit` is still there to override it for a
# single boot.
say "installing $UNIT_NAME (log filter '$log_filter' -> $REMOTE_LOG)"
ssh "${ssh_options[@]}" "$target" sh -s -- \
    "$UNIT_NAME" "$REMOTE_APP_DIR" "$REMOTE_LOG" <<'REMOTE'
set -e
unit=$1
app_dir=$2
log=$3
log_dir=$(dirname "$log")

mkdir -p "$log_dir"

# `append:` needs systemd 240. Older images fall back to the journal rather than
# failing to start with an unparsable unit.
version=$(systemctl --version | head -n1 | cut -d' ' -f2)
if [ "${version:-0}" -ge 240 ] 2>/dev/null; then
    output="append:$log"
else
    output="journal"
    echo "  systemd $version predates append:; logging to the journal instead" >&2
fi

# /var/log is a tmpfs on some images, so let systemd recreate the directory
# before every start rather than relying on the mkdir above surviving a reboot.
case "$log_dir" in
    /var/log/*) logs_directory="LogsDirectory=${log_dir#/var/log/}" ;;
    *) logs_directory="" ;;
esac

cat > "/etc/systemd/system/$unit" <<UNIT
[Unit]
Description=404-snf sensing application (radar, fatigue, BLE, pneumatics)
Documentation=file://$app_dir/Repose.toml
After=network.target
# A sensor that is unplugged, or a CLI port that answers nothing, stops the
# application by design. Retry a few times for the transient version of that,
# then stay in failed rather than restarting every few seconds forever: failed
# is a state you can see, a restart loop is one you have to notice.
StartLimitIntervalSec=60
StartLimitBurst=5

[Service]
Type=simple
ExecStart=$app_dir/snf-app
WorkingDirectory=$app_dir
Restart=on-failure
RestartSec=5
SyslogIdentifier=snf-app
$logs_directory
StandardOutput=$output
StandardError=$output

[Install]
WantedBy=multi-user.target
UNIT

if [ -d /etc/logrotate.d ]; then
    # copytruncate because the service holds the file open for the whole run and
    # only reopens it on restart.
    cat > /etc/logrotate.d/snf-app <<ROTATE
$log {
    weekly
    rotate 4
    missingok
    notifempty
    compress
    copytruncate
}
ROTATE
fi

systemctl daemon-reload
REMOTE

# ── 6. Start it ──────────────────────────────────────────────────────────────
if [ "$do_enable" -eq 1 ]; then
    say "enabling and starting $UNIT_NAME"
    remote "systemctl enable --now '$UNIT_NAME' >/dev/null"
    remote "systemctl restart '$UNIT_NAME'"
else
    say "starting $UNIT_NAME (not enabled at boot)"
    remote "systemctl restart '$UNIT_NAME'"
fi

sleep 2
if remote "systemctl is-active --quiet '$UNIT_NAME'"; then
    say "$UNIT_NAME is running"
else
    warn "$UNIT_NAME is not running; the last of its output follows"
fi

echo
remote "tail -n 25 '$REMOTE_LOG' 2>/dev/null || journalctl -u '$UNIT_NAME' -n 25 --no-pager"
echo
say "follow it with: ssh $target 'tail -f $REMOTE_LOG'"
