#!/usr/bin/env bash
# Installs the boot animation as a Plymouth theme (part of Material 3 Settings).
#
# The frames are rendered with your shell's current colors (the style.json
# Material 3 Settings' shell integration keeps up to date, else end-4's
# generated colors.json), so the boot splash matches your desktop. Run it
# again after changing wallpaper to update them (Settings → Login & boot). Needs root for the theme
# folder and the initramfs rebuild (asks through pkexec or sudo).
#
# Usage: install.sh [--colors FILE] [--default-colors] [--no-rebuild]
#                   [--stage-only DIR]
#   --colors FILE      use this style.json or colors.json instead
#   --default-colors   use the logo's own teal
#   --no-rebuild       install and select the theme, but don't rebuild the
#                      initramfs (it applies on the next rebuild)
#   --stage-only DIR   just build the theme into DIR (no root, no install)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THEME=end4-pc
DEST="/usr/share/plymouth/themes/$THEME"
STATE="${XDG_STATE_HOME:-$HOME/.local/state}"
COLORS="$STATE/m3/style.json"
[[ -f "$COLORS" ]] || COLORS="$STATE/quickshell/user/generated/colors.json"
rebuild=1 stage_only=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --colors) COLORS="$2"; shift ;;
        --default-colors) COLORS="" ;;
        --no-rebuild) rebuild=0 ;;
        --stage-only) stage_only="$2"; shift ;;
        -h | --help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

[[ -n "$COLORS" && ! -f "$COLORS" ]] && COLORS=""

stage="${stage_only:-$(mktemp -d)}"
[[ -z "$stage_only" ]] && trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage"

echo "Rendering frames${COLORS:+ with colors from $COLORS}…"
python3 "$HERE/render.py" "$stage" ${COLORS:+--colors "$COLORS"}

# Fill the script template from palette.txt.
declare -A pal
while IFS='=' read -r k v; do pal[$k]=$v; done < "$stage/palette.txt"
rgb() { # "#rrggbb" -> "r g b" as decimal
    local h=${1#\#}
    echo "$((16#${h:0:2})) $((16#${h:2:2})) $((16#${h:4:2}))"
}
read -r bg_r bg_g bg_b <<<"$(rgb "${pal[background]}")"
read -r tx_r tx_g tx_b <<<"$(rgb "${pal[text]}")"
sed -e "s/@FPS@/${pal[fps]}/" -e "s/@INTRO@/${pal[intro_frames]}/" -e "s/@LOOP@/${pal[loop_frames]}/" \
    -e "s/@BG_R@/$bg_r/g" -e "s/@BG_G@/$bg_g/g" -e "s/@BG_B@/$bg_b/g" \
    -e "s/@TEXT_R@/$tx_r/g" -e "s/@TEXT_G@/$tx_g/g" -e "s/@TEXT_B@/$tx_b/g" \
    "$HERE/theme/$THEME.script.in" > "$stage/$THEME.script"
cp "$HERE/theme/$THEME.plymouth" "$stage/"
rm -f "$stage/palette.txt"

if [[ -n "$stage_only" ]]; then
    echo "Theme staged in $stage_only"
    exit 0
fi

if ! command -v plymouth-set-default-theme >/dev/null; then
    echo "Plymouth isn't installed. Install it with: sudo pacman -S plymouth" >&2
    exit 1
fi

# One privileged step: copy the theme, select it, rebuild the initramfs.
root_script='set -e
dest="$1"; stage="$2"; theme="$3"; rebuild="$4"
rm -rf "$dest"
install -d "$dest"
cp -r "$stage"/. "$dest"/
chmod -R a+rX "$dest"
if [ "$rebuild" = 1 ]; then
    plymouth-set-default-theme -R "$theme"
else
    plymouth-set-default-theme "$theme"
fi'
if [[ $EUID -eq 0 ]]; then
    bash -c "$root_script" _ "$DEST" "$stage" "$THEME" "$rebuild"
elif command -v pkexec >/dev/null && [[ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
    pkexec bash -c "$root_script" _ "$DEST" "$stage" "$THEME" "$rebuild"
else
    sudo bash -c "$root_script" _ "$DEST" "$stage" "$THEME" "$rebuild"
fi

echo "Installed the $THEME Plymouth theme."

# Plymouth only shows when the initramfs has it and the kernel asks for it.
warn=0
if [[ -f /etc/mkinitcpio.conf ]] && ! grep -Eq '^[[:space:]]*HOOKS=.*\bplymouth\b' /etc/mkinitcpio.conf /etc/mkinitcpio.conf.d/*.conf 2>/dev/null; then
    echo
    echo "Note: add \"plymouth\" to HOOKS in /etc/mkinitcpio.conf (after \"systemd\" or \"udev\", before \"encrypt\"/\"sd-encrypt\"), then run: sudo mkinitcpio -P"
    warn=1
fi
if ! grep -qw splash /proc/cmdline 2>/dev/null; then
    echo
    echo "Note: add \"quiet splash\" to your kernel command line (GRUB: GRUB_CMDLINE_LINUX_DEFAULT in /etc/default/grub; systemd-boot: the options line of your entry; Limine: KERNEL_CMDLINE in /etc/default/limine)."
    warn=1
fi
[[ $warn -eq 0 ]] && echo "It will show on your next boot."
exit 0
