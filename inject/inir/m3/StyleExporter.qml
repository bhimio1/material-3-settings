// The style bridge: writes this shell's resolved look (colors, light/dark,
// fonts, rounding, transparency, wallpaper) to $XDG_STATE_HOME/m3/style.json whenever it
// changes, so the Material 3 apps follow the shell, presets and custom
// themes included. material-3-ui reads the file (see its shell_style.rs).

import qs.modules.common
import QtQuick
import Quickshell
import Quickshell.Io

Scope {
    id: root

    /** Which shell this is, for the file's `shell` field. */
    property string shellName: "ii"

    readonly property string stateDir: Quickshell.env("XDG_STATE_HOME") || (Quickshell.env("HOME") + "/.local/state")
    readonly property string path: `${stateDir}/m3/style.json`

    // Appearance.m3colors.m3<Role> → snake_case role names (matugen's keys).
    readonly property var roles: ["background", "onBackground", "surface", "surfaceDim", "surfaceBright",
        "surfaceContainerLowest", "surfaceContainerLow", "surfaceContainer", "surfaceContainerHigh",
        "surfaceContainerHighest", "onSurface", "surfaceVariant", "onSurfaceVariant", "inverseSurface",
        "inverseOnSurface", "outline", "outlineVariant", "shadow", "scrim", "surfaceTint", "primary",
        "onPrimary", "primaryContainer", "onPrimaryContainer", "inversePrimary", "secondary", "onSecondary",
        "secondaryContainer", "onSecondaryContainer", "tertiary", "onTertiary", "tertiaryContainer",
        "onTertiaryContainer", "error", "onError", "errorContainer", "onErrorContainer"]

    function snake(name) {
        return name.replace(/[A-Z]/g, c => "_" + c.toLowerCase());
    }

    function hex(c) {
        // Qt gives #rrggbb, or #aarrggbb when translucent; keep the color.
        const s = String(c);
        return s.length === 9 ? "#" + s.slice(3) : s;
    }

    readonly property string json: {
        const m = Appearance.m3colors;
        const colors = {};
        for (const role of roles) {
            const value = m["m3" + role];
            if (value !== undefined) colors[snake(role)] = hex(value);
        }
        return JSON.stringify({
            version: 1,
            shell: root.shellName,
            dark: m.darkmode ?? true,
            colors: colors,
            fonts: {
                main: Appearance.font.family.main ?? "",
                title: Appearance.font.family.title ?? "",
                monospace: Appearance.font.family.monospace ?? ""
            },
            radius: {
                small: Appearance.rounding.small,
                normal: Appearance.rounding.normal,
                large: Appearance.rounding.large
            },
            transparency: {
                background: Appearance.backgroundTransparency ?? 0,
                content: Appearance.contentTransparency ?? 0
            },
            // For the login screen and boot splash Settings themes.
            wallpaper: String(Config.options?.background?.wallpaperPath ?? "").replace(/^file:\/\//, ""),
            wallpaper_thumbnail: String(Config.options?.background?.thumbnailPath ?? "").replace(/^file:\/\//, "")
        }, null, 2) + "\n";
    }
    onJsonChanged: writeTimer.restart()

    // Colors change in bursts on a wallpaper switch; write once it settles.
    Timer {
        id: writeTimer
        interval: 400
        onTriggered: mkdir.running = true
    }

    Process {
        id: mkdir
        command: ["mkdir", "-p", `${root.stateDir}/m3`]
        onExited: file.setText(root.json)
    }

    FileView {
        id: file
        path: root.path
        atomicWrites: true
        blockLoading: false
    }

    Component.onCompleted: writeTimer.restart()
}
