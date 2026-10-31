#!/usr/bin/env bash
# Installs the end4-material SDDM login theme (Material 3 Settings) and selects it.
#
# Run it as yourself; it asks for your password once (pkexec or sudo) for the
# system part, then syncs your wallpaper and colours. Running it with sudo
# works too.
#
#   theme       /usr/share/sddm/themes/end4-material      (root, read-only)
#   sync folder /var/lib/end4-material-sddm               (yours, 0755)
#   selection   /etc/sddm.conf.d/zz-end4-material.conf    ([Theme] Current=)
#
# A "Current=" line in /etc/sddm.conf (which SDDM reads last) is commented out
# after a backup to /etc/sddm.conf.end4-material.bak; --uninstall puts it back.
#
# Usage: install.sh [--uninstall] [--no-select] [--user NAME]
#   --uninstall   remove the theme and sync folder, restore the old theme
#   --no-select   install but leave the current SDDM theme selected
#   --user NAME   who owns the sync folder (default: you, or $SUDO_USER)
#
# ROOT=/some/dir (or DESTDIR) installs into a fake root instead, without
# asking for privileges and without checking that SDDM is installed.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
THEME=end4-material
ROOT="${ROOT:-${DESTDIR:-}}"
ROOT="${ROOT%/}"
mode=install select=1 user=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --uninstall) mode=uninstall ;;
        --no-select) select=0 ;;
        --user) user="$2"; shift ;;
        -h | --help) sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

if [[ -z "$user" ]]; then
    if [[ $EUID -eq 0 && -n "${SUDO_USER:-}" ]]; then
        user="$SUDO_USER"
    elif [[ $EUID -eq 0 && -n "${PKEXEC_UID:-}" ]]; then
        user="$(id -un "$PKEXEC_UID")"
    else
        user="$(id -un)"
    fi
fi
if [[ "$mode" == install && "$user" == root && -z "$ROOT" ]]; then
    echo "Run this as your own user (or pass --user NAME); the sync folder belongs to the user whose wallpaper it shows." >&2
    exit 1
fi
id "$user" >/dev/null 2>&1 || { echo "No such user: $user" >&2; exit 1; }

if [[ "$mode" == install && -z "$ROOT" ]] && ! { command -v sddm >/dev/null || [[ -x /usr/bin/sddm ]]; }; then
    echo "SDDM isn't installed. Install it with: sudo pacman -S sddm" >&2
    exit 1
fi

# --- Stage the theme as the user (no root needed) ---------------------------
stage=""
if [[ "$mode" == install ]]; then
    stage="$(mktemp -d)"
    trap 'rm -rf "$stage"' EXIT
    cp -r "$HERE/theme/." "$stage/"
    if [[ ! -f "$stage/fonts/MaterialSymbolsLite.ttf" ]]; then
        echo "Note: theme/fonts/MaterialSymbolsLite.ttf is missing; the login screen will use plain symbols for its icons." >&2
    fi
    chmod -R a+rX,go-w "$stage"
fi

# --- The privileged part ------------------------------------------------------
# Kept in one script so it runs under a single pkexec/sudo prompt. Arguments:
#   $1 mode  $2 root prefix  $3 theme name  $4 stage dir  $5 user  $6 select
# shellcheck disable=SC2016 # expanded by the root shell, not here
root_script='set -eu
mode="$1"; R="$2"; theme="$3"; stage="$4"; user="$5"; select="$6"
dest="$R/usr/share/sddm/themes/$theme"
sync="$R/var/lib/end4-material-sddm"
dropin_dir="$R/etc/sddm.conf.d"
dropin="$dropin_dir/zz-end4-material.conf"
main_conf="$R/etc/sddm.conf"
backup="$R/etc/sddm.conf.end4-material.bak"
marker="#end4-material-disabled# "
chown_ok=1; [ "$(id -u)" -eq 0 ] || chown_ok=0

if [ "$mode" = uninstall ]; then
    rm -f "$dropin"
    if [ -f "$main_conf" ] && grep -q "^$marker" "$main_conf"; then
        tmp="$(mktemp "$main_conf.XXXXXX")"
        sed "s/^$marker//" "$main_conf" > "$tmp"
        chmod --reference="$main_conf" "$tmp" 2>/dev/null || chmod 0644 "$tmp"
        mv -f "$tmp" "$main_conf"
        echo "Restored the theme line in $main_conf."
    fi
    rm -f "$backup"
    rm -rf "$dest"
    rm -rf "$sync"
    echo "Removed $dest and $sync."
    exit 0
fi

# Theme: replace the whole folder so files from older versions go away.
install -d -m 0755 "$R/usr/share/sddm/themes"
rm -rf "$dest.new"
cp -r "$stage" "$dest.new"
[ $chown_ok -eq 1 ] && chown -R root:root "$dest.new"
chmod -R u+rwX,go+rX,go-w "$dest.new"
# The synced settings are read through this link; SDDM loads theme.conf.user
# next to theme.conf and ignores it when the target is missing.
ln -sfn /var/lib/end4-material-sddm/theme.conf.user "$dest.new/theme.conf.user"
rm -rf "$dest"
mv "$dest.new" "$dest"

# Sync folder: readable by everyone (the greeter runs as "sddm"), writable
# only by its owner. Not recursive, so nothing in it is followed as root.
if [ -L "$sync" ]; then rm -f "$sync"; fi
install -d -m 0755 "$sync"
if [ $chown_ok -eq 1 ]; then chown "$user":root "$sync"; fi
chmod 0755 "$sync"

if [ "$select" = 1 ]; then
    install -d -m 0755 "$dropin_dir"
    # SDDM reads /etc/sddm.conf after the drop-ins, so a theme chosen there
    # would win. Comment it out (with a backup) so ours applies.
    if [ -f "$main_conf" ] && awk "
            /^[[:space:]]*\\[/ { sec = \$0; gsub(/[[:space:]]/, \"\", sec) }
            sec == \"[Theme]\" && /^[[:space:]]*Current[[:space:]]*=/ { found = 1 }
            END { exit !found }" "$main_conf"; then
        [ -f "$backup" ] || cp -p "$main_conf" "$backup"
        tmp="$(mktemp "$main_conf.XXXXXX")"
        awk -v m="$marker" "
            /^[[:space:]]*\\[/ { sec = \$0; gsub(/[[:space:]]/, \"\", sec) }
            sec == \"[Theme]\" && /^[[:space:]]*Current[[:space:]]*=/ { print m \$0; next }
            { print }" "$main_conf" > "$tmp"
        chmod --reference="$main_conf" "$tmp" 2>/dev/null || chmod 0644 "$tmp"
        mv -f "$tmp" "$main_conf"
        echo "Commented out the theme in $main_conf (backup: $backup)."
    fi
    printf "%s\n" "# Written by Material 3 Settings (login/sddm/install.sh); remove with install.sh --uninstall" "[Theme]" "Current=$theme" > "$dropin"
    chmod 0644 "$dropin"
fi
echo "Installed $dest."'

run_root() {
    if [[ -n "$ROOT" ]]; then
        bash -c "$root_script" _ "$@"
    elif [[ $EUID -eq 0 ]]; then
        bash -c "$root_script" _ "$@"
    elif command -v pkexec >/dev/null && [[ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
        pkexec bash -c "$root_script" _ "$@"
    else
        sudo bash -c "$root_script" _ "$@"
    fi
}

run_root "$mode" "$ROOT" "$THEME" "$stage" "$user" "$select"

if [[ "$mode" == uninstall ]]; then
    echo "SDDM uses its previous theme from the next login screen on."
    exit 0
fi

# --- First sync, as the owning user ---------------------------------------------
sync_dir="$ROOT/var/lib/end4-material-sddm"
if [[ "$(id -un)" == "$user" ]]; then
    END4_SDDM_SYNC_DIR="$sync_dir" "$HERE/sync.sh" --force || echo "The first sync failed; the login screen uses its default look until the next wallpaper change." >&2
elif [[ $EUID -eq 0 ]] && command -v runuser >/dev/null; then
    home="$(getent passwd "$user" | cut -d: -f6)"
    runuser -u "$user" -- env -u XDG_CONFIG_HOME -u XDG_STATE_HOME HOME="$home" \
        END4_SDDM_SYNC_DIR="$sync_dir" bash "$HERE/sync.sh" --force ||
        echo "The first sync failed; run m3-settings login sync as $user." >&2
else
    echo "Run m3-settings login sync as $user to copy the wallpaper and colours." >&2
fi

# Warn when another file still overrides the selection.
if [[ $select -eq 1 ]]; then
    effective=""
    for f in "$ROOT"/usr/lib/sddm/sddm.conf.d/*.conf "$ROOT"/etc/sddm.conf.d/*.conf "$ROOT/etc/sddm.conf"; do
        [[ -f "$f" ]] || continue
        v="$(awk '/^[[:space:]]*\[/ { sec = $0; gsub(/[[:space:]]/, "", sec) }
                  sec == "[Theme]" && /^[[:space:]]*Current[[:space:]]*=/ { sub(/^[^=]*=[[:space:]]*/, ""); v = $0 }
                  END { print v }' "$f")"
        [[ -n "$v" ]] && effective="$v"
    done
    if [[ "$effective" != "$THEME" ]]; then
        echo "Warning: SDDM's settings still select \"$effective\". Check /etc/sddm.conf.d/ for a later Current= line." >&2
    else
        echo "Selected $THEME. You'll see it at the next login screen."
    fi
fi
