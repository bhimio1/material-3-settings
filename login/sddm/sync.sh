#!/usr/bin/env bash
# Copies what the end4-material login screen needs from your session into the
# folder the greeter can read (SDDM runs as the "sddm" user and can't see your
# home): the current wallpaper (a sharp and a pre-blurred copy), the Material
# You colours, your fonts if they live in your home, your avatar and the
# clock format.
#
# The look comes from $XDG_STATE_HOME/m3/style.json, which every shell
# Material 3 Settings integrates with (ii, end4-pC, iNiR, Dux Shell) keeps up
# to date; end-4's config.json and generated colors fill in what it lacks.
#
# install.sh creates that folder (/var/lib/end4-material-sddm, owned by you)
# and runs this once. Material 3 Settings runs it again (m3-settings login
# sync) whenever style.json changes, so nothing here needs root.
#
# Usage: sync.sh [--if-installed] [--force] [--sync-dir DIR]
#   --if-installed   exit quietly when the theme isn't installed (for hooks)
#   --force          redo the images even when the wallpaper didn't change
#   --sync-dir DIR   write here instead (tests); also $END4_SDDM_SYNC_DIR
set -uo pipefail

SYNC_DIR="${END4_SDDM_SYNC_DIR:-/var/lib/end4-material-sddm}"
if_installed=0 force=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --if-installed) if_installed=1 ;;
        --force) force=1 ;;
        --sync-dir) SYNC_DIR="$2"; shift ;;
        -h | --help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

if [[ ! -d "$SYNC_DIR" || ! -w "$SYNC_DIR" ]]; then
    [[ $if_installed -eq 1 ]] && exit 0
    echo "$SYNC_DIR doesn't exist or isn't writable by $(id -un). Run sddm/install.sh first." >&2
    exit 1
fi

XDG_CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
XDG_STATE_HOME="${XDG_STATE_HOME:-$HOME/.local/state}"
SHELL_CONFIG="$XDG_CONFIG_HOME/illogical-impulse/config.json"
GENERATED="$XDG_STATE_HOME/quickshell/user/generated"
STYLE="$XDG_STATE_HOME/m3/style.json"

# One sync at a time; a newer wallpaper switch waits for the previous sync.
if command -v flock >/dev/null; then
    exec 9>"$SYNC_DIR/.lock" || exit 1
    flock -w 60 9 || exit 1
fi

have() { command -v "$1" >/dev/null 2>&1; }

# Reads a key from the shell config; prints nothing when it's missing.
cfg() {
    [[ -f "$SHELL_CONFIG" ]] && have jq || return 0
    jq -r "($1) // empty | tostring" "$SHELL_CONFIG" 2>/dev/null | head -n1
}

# Values go into an INI file that SDDM reads with QSettings. Keep them on one
# line, drop quotes and backslashes, and quote them so commas stay literal.
ini_value() {
    local v="${1//[$'\n\r']/}"
    v="${v//\"/}"
    v="${v//\\/}"
    printf '"%s"' "${v:0:200}"
}

out="$(mktemp "$SYNC_DIR/.theme.conf.user.XXXXXX")" || exit 1
trap 'rm -f "$out" "$SYNC_DIR"/.img.* 2>/dev/null' EXIT
{
    echo "# Written by Material 3 Settings (login/sddm/sync.sh) on $(date -Is). Changes are overwritten."
    echo "[General]"
} >"$out"
put() { printf '%s=%s\n' "$1" "$(ini_value "$2")" >>"$out"; }

# Reads a key from style.json; prints nothing when it's missing.
style() {
    [[ -f "$STYLE" ]] && have jq || return 0
    jq -r "($1) // empty | tostring" "$STYLE" 2>/dev/null | head -n1
}
shell="$(style '.shell')"

# --- Which wallpaper and colours the lock screen uses -----------------------
# end-4's config.json only describes ii and end4-pC; another shell's
# style.json is the whole truth.
end4=1
[[ -n "$shell" && "$shell" != ii && "$shell" != end4-pC ]] && end4=0
wall="$(style '.wallpaper')" thumb="$(style '.wallpaper_thumbnail')" lock_wall=""
if [[ $end4 -eq 1 ]]; then
    lock_wall="$(cfg '.background.lockWall')"
    [[ -n "$wall" ]] || wall="$(cfg '.background.wallpaperPath')"
    [[ -n "$thumb" ]] || thumb="$(cfg '.background.thumbnailPath')"
fi
# The colour roles: style.json's "colors", else matugen's flat colors.json.
colors="$GENERATED/colors.json" colors_filter='.'
if [[ -f "$STYLE" ]] && have jq && jq -e '.colors.primary' "$STYLE" >/dev/null 2>&1; then
    colors="$STYLE" colors_filter='.colors'
fi
if [[ -n "$lock_wall" && -f "$lock_wall" ]]; then
    wall="$lock_wall"
    [[ -f "$GENERATED/colors-lock.json" ]] && colors="$GENERATED/colors-lock.json" colors_filter='.'
fi
case "${wall,,}" in
    *.mp4 | *.webm | *.mkv | *.avi | *.mov)
        if [[ -n "$thumb" && -f "$thumb" ]]; then wall="$thumb"; else wall=""; fi ;;
esac
wall="${wall#file://}"

# --- Colours ----------------------------------------------------------------
if [[ -f "$colors" ]] && have jq && jq -e "$colors_filter | type == \"object\"" "$colors" >/dev/null 2>&1; then
    while IFS=$'\t' read -r key value; do
        [[ "$key" =~ ^[a-z_]{1,40}$ && "$value" =~ ^#[0-9A-Fa-f]{6}([0-9A-Fa-f]{2})?$ ]] || continue
        put "color_$key" "$value"
    done < <(jq -r "$colors_filter"' | to_entries[] | select(.value | type == "string") | "\(.key)\t\(.value)"' "$colors" 2>/dev/null)
fi

# --- Fonts and clock --------------------------------------------------------
font_main="$(style '.fonts.main')"
[[ -n "$font_main" || $end4 -eq 0 ]] || font_main="$(cfg '.appearance.fonts.main')"
font_clock=""
[[ $end4 -eq 1 ]] && font_clock="$(cfg '.background.widgets.clock.digital.font.family')"
put fontFamily "${font_main:-Google Sans Flex}"
put clockFontFamily "${font_clock:-${font_main:-Google Sans Flex}}"
w="$(cfg '.background.widgets.clock.digital.font.weight')"
[[ "$w" =~ ^[0-9]{3}(\.[0-9]+)?$ ]] && put clockFontWeight "${w%%.*}"
put timeFormat "$(cfg '.time.format')"
put dateFormat "$(cfg '.time.dateFormat')"
blur="$(cfg '.lock.blur.enable')"
[[ "$blur" == "true" || "$blur" == "false" ]] && put blur "$blur"
put syncUser "$(id -un)"

# Fonts installed in your home (~/.local/share/fonts) are invisible to the
# greeter, so copy their files. System fonts are left where they are.
font_files=()
if have fc-list; then
    declare -A seen=()
    for family in "$font_main" "$font_clock"; do
        [[ -n "$family" && -z "${seen[$family]:-}" ]] || continue
        seen[$family]=1
        while IFS= read -r f; do
            [[ -f "$f" && -r "$f" && "$f" == "$HOME"/* ]] || continue
            [[ ${#font_files[@]} -lt 6 ]] || break
            # Skip anything over 30 MB; the greeter has to load it at startup.
            [[ $(stat -c %s "$f" 2>/dev/null || echo 0) -le 31457280 ]] || continue
            ext="${f##*.}"; ext="${ext,,}"
            [[ "$ext" =~ ^(ttf|otf)$ ]] || continue
            name="font-${#font_files[@]}.$ext"
            if cmp -s "$f" "$SYNC_DIR/$name" || { cp -f "$f" "$SYNC_DIR/.$name" && chmod 0644 "$SYNC_DIR/.$name" && mv -f "$SYNC_DIR/.$name" "$SYNC_DIR/$name"; }; then
                font_files+=("$name")
            fi
        done < <(fc-list --format '%{file}\n' "$family" 2>/dev/null | sort -u)
    done
fi
for f in "$SYNC_DIR"/font-*; do
    [[ -e "$f" && " ${font_files[*]} " != *" ${f##*/} "* ]] && rm -f "$f"
done
put fontFiles "$(IFS=';'; echo "${font_files[*]}")"

# --- Avatar -----------------------------------------------------------------
avatar_src=""
for f in "$HOME/.face.icon" "$HOME/.face" "/var/lib/AccountsService/icons/$(id -un)"; do
    [[ -f "$f" && -r "$f" ]] && { avatar_src="$f"; break; }
done
if [[ -n "$avatar_src" ]]; then
    tmp="$(mktemp "$SYNC_DIR/.img.XXXXXX")"
    if { have magick && magick "${avatar_src}[0]" -resize '256x256^' -gravity center -extent 256x256 "png:$tmp" 2>/dev/null; } ||
       { have convert && convert "${avatar_src}[0]" -resize '256x256^' -gravity center -extent 256x256 "png:$tmp" 2>/dev/null; } ||
       cp -f "$avatar_src" "$tmp"; then
        chmod 0644 "$tmp" && mv -f "$tmp" "$SYNC_DIR/avatar.png" && put avatar avatar.png
    fi
else
    rm -f "$SYNC_DIR/avatar.png"
fi

# --- Wallpaper --------------------------------------------------------------
im=""
have magick && im=magick
[[ -z "$im" ]] && have convert && im=convert
sig=""
[[ -n "$wall" && -f "$wall" ]] && sig="$(stat -c '%n|%s|%Y' "$wall" 2>/dev/null)|$im"
old_sig="$(cat "$SYNC_DIR/.wallpaper.sig" 2>/dev/null)"

if [[ -z "$sig" ]]; then
    rm -f "$SYNC_DIR/wallpaper.jpg" "$SYNC_DIR/wallpaper-blur.jpg" "$SYNC_DIR/.wallpaper.sig"
elif [[ $force -eq 1 || "$sig" != "$old_sig" || ! -f "$SYNC_DIR/wallpaper.jpg" ]]; then
    ok=0
    tmp="$(mktemp "$SYNC_DIR/.img.XXXXXX")"
    tmpb="$(mktemp "$SYNC_DIR/.img.XXXXXX")"
    if [[ -n "$im" ]]; then
        # A sharp copy no wider than 2560 px, and a pre-blurred one so the
        # greeter doesn't need a GPU blur (it may run on software rendering).
        if nice -n 10 "$im" "${wall}[0]" -auto-orient -resize '2560x1600>' -quality 90 "jpg:$tmp" 2>/dev/null &&
           nice -n 10 "$im" "$tmp" -resize '960x960>' -blur 0x16 -quality 92 "jpg:$tmpb" 2>/dev/null; then
            ok=1
        fi
    else
        case "${wall,,}" in
            *.jpg | *.jpeg | *.png) cp -f "$wall" "$tmp" && ok=1 && rm -f "$tmpb" ;;
        esac
    fi
    if [[ $ok -eq 1 ]]; then
        chmod 0644 "$tmp" && mv -f "$tmp" "$SYNC_DIR/wallpaper.jpg"
        if [[ -s "$tmpb" ]]; then
            chmod 0644 "$tmpb" && mv -f "$tmpb" "$SYNC_DIR/wallpaper-blur.jpg"
        else
            rm -f "$SYNC_DIR/wallpaper-blur.jpg"
        fi
        echo "$sig" >"$SYNC_DIR/.wallpaper.sig"
    else
        echo "Couldn't convert $wall (install imagemagick for webp/avif/gif wallpapers)" >&2
        rm -f "$SYNC_DIR/wallpaper.jpg" "$SYNC_DIR/wallpaper-blur.jpg" "$SYNC_DIR/.wallpaper.sig"
    fi
fi
[[ -f "$SYNC_DIR/wallpaper.jpg" ]] && put wallpaper wallpaper.jpg
[[ -f "$SYNC_DIR/wallpaper-blur.jpg" ]] && put wallpaperBlurred wallpaper-blur.jpg

chmod 0644 "$out" && mv -f "$out" "$SYNC_DIR/theme.conf.user"
exit 0
