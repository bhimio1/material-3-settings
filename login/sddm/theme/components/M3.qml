pragma Singleton
import QtQuick

// Palette, fonts and sizes shared by every part of the login screen.
//
// Colours come from theme.conf.user (written by sddm/sync.sh from the shell's
// generated colors.json). Anything missing or malformed falls back to the
// shell's own defaults from modules/common/Appearance.qml, so a broken or
// absent sync never leaves the greeter unreadable.
QtObject {
    id: m3

    // Reads a theme setting; never throws, never returns undefined.
    function cfg(key, fallback) {
        try {
            if (typeof config === "undefined" || !config || typeof config.stringValue !== "function")
                return fallback
            const v = config.stringValue(key)
            if (v === undefined || v === null)
                return fallback
            const str = String(v).trim()
            return str.length > 0 && str.length <= 512 ? str : fallback
        } catch (e) {
            return fallback
        }
    }
    function cfgBool(key, fallback) {
        const v = cfg(key, "").toLowerCase()
        if (v === "true") return true
        if (v === "false") return false
        return fallback
    }
    function cfgInt(key, fallback, min, max) {
        const n = parseInt(cfg(key, ""), 10)
        return isNaN(n) ? fallback : Math.max(min, Math.min(max, n))
    }
    function role(name, fallback) {
        const v = cfg("color_" + name, "")
        return /^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(v) ? v : fallback
    }
    function alpha(c, a) {
        return Qt.rgba(c.r, c.g, c.b, a)
    }
    function mix(c1, c2, t) { // t = share of c1, like the shell's ColorUtils.mix
        return Qt.rgba(c1.r * t + c2.r * (1 - t), c1.g * t + c2.g * (1 - t), c1.b * t + c2.b * (1 - t), 1)
    }

    // The folder sync.sh writes to. Only plain absolute paths are accepted.
    readonly property string syncDir: {
        const d = cfg("syncDir", "/var/lib/end4-material-sddm")
        return (/^\/[A-Za-z0-9._\/-]*$/.test(d) && d.indexOf("..") < 0) ? d.replace(/\/+$/, "") : "/var/lib/end4-material-sddm"
    }
    // URL of a file in the sync folder, or "" for anything but a bare name.
    function syncUrl(name) {
        return /^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$/.test(name || "") ? "file://" + syncDir + "/" + name : ""
    }

    // --- Material 3 roles (dark defaults from the shell) --------------------
    readonly property color background: role("background", "#141313")
    readonly property color colOnBackground: role("on_background", "#e6e1e1")
    readonly property color surface: role("surface", "#141313")
    readonly property color surfaceContainerLow: role("surface_container_low", "#1c1b1c")
    readonly property color surfaceContainer: role("surface_container", "#201f20")
    readonly property color surfaceContainerHigh: role("surface_container_high", "#2b2a2a")
    readonly property color surfaceContainerHighest: role("surface_container_highest", "#363435")
    readonly property color colOnSurface: role("on_surface", "#e6e1e1")
    readonly property color colOnSurfaceVariant: role("on_surface_variant", "#cbc5ca")
    readonly property color outline: role("outline", "#948f94")
    readonly property color outlineVariant: role("outline_variant", "#49464a")
    readonly property color primary: role("primary", "#cbc4cb")
    readonly property color colOnPrimary: role("on_primary", "#322f34")
    readonly property color primaryContainer: role("primary_container", "#2d2a2f")
    readonly property color colOnPrimaryContainer: role("on_primary_container", "#bcb6bc")
    readonly property color secondaryContainer: role("secondary_container", "#4d4b4d")
    readonly property color colOnSecondaryContainer: role("on_secondary_container", "#ece6e9")
    readonly property color tertiaryContainer: role("tertiary_container", "#31292b")
    readonly property color colOnTertiaryContainer: role("on_tertiary_container", "#c1b4b7")
    readonly property color error: role("error", "#ffb4ab")
    readonly property color errorContainer: role("error_container", "#93000a")
    readonly property color colOnErrorContainer: role("on_error_container", "#ffdad6")
    readonly property color shadow: role("shadow", "#000000")
    readonly property bool dark: background.hslLightness < 0.5

    // The shell's layer colours used by the lock screen.
    readonly property color layer1: surfaceContainerLow          // text field
    readonly property color toolbar: surfaceContainer             // toolbars
    readonly property color subtext: outline
    readonly property color hover: alpha(colOnSurface, 0.08)
    readonly property color pressed: alpha(colOnSurface, 0.14)

    // --- Fonts ----------------------------------------------------------------
    // Bumped by Main.qml whenever a FontLoader finishes, so the family
    // choice below is re-evaluated against the fonts that actually exist.
    property int fontsRevision: 0
    property string iconFamily: ""

    function pickFamily(wanted) {
        const _ = fontsRevision
        const available = Qt.fontFamilies()
        for (let i = 0; i < wanted.length; i++) {
            if (wanted[i] && available.indexOf(wanted[i]) >= 0)
                return wanted[i]
        }
        return "sans-serif"
    }
    readonly property string fontFamily: pickFamily([cfg("fontFamily", ""), "Google Sans Flex", "Google Sans", "Rubik", "Noto Sans", "Cantarell"])
    readonly property string clockFamily: pickFamily([cfg("clockFontFamily", ""), cfg("fontFamily", ""), "Google Sans Flex", "Google Sans", "Rubik", "Noto Sans"])
    readonly property int clockWeight: cfgInt("clockFontWeight", 350, 100, 1000)

    // --- Sizes ------------------------------------------------------------------
    // Main.qml sets this per screen: 1 at 1080p, larger on big screens
    // without HiDPI scaling, a little smaller on small ones.
    property real s: 1
    readonly property real fontSmall: 15 * s
    readonly property real fontNormal: 16 * s
    readonly property real fontLarge: 19 * s
    readonly property real fontTitle: 28 * s
    readonly property real toolbarHeight: 56 * s
    readonly property real rounding: 17 * s
    readonly property real roundingLarge: 23 * s

    // --- Motion (the shell's expressive curves) ------------------------------
    readonly property var curveSpatial: [0.38, 1.21, 0.22, 1.00, 1, 1]
    readonly property var curveFastSpatial: [0.42, 1.67, 0.21, 0.90, 1, 1]
    readonly property var curveEffects: [0.34, 0.80, 0.34, 1.00, 1, 1]
}
