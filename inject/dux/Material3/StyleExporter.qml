// The style bridge for Dux Shell: writes its resolved look to
// $XDG_STATE_HOME/m3/style.json whenever it changes, so the Material 3 apps
// follow Dux Shell's theme, custom themes included. Same format as the
// exporter Material 3 Settings adds to the other shells.

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Common

Item {
    id: root

    readonly property string stateDir: Quickshell.env("XDG_STATE_HOME") || (Quickshell.env("HOME") + "/.local/state")

    function hex(c) {
        const s = String(c);
        return s.length === 9 ? "#" + s.slice(3) : s;
    }

    // Material 3 role (matugen key) → Theme property.
    readonly property var roles: ({
        "primary": "primary", "on_primary": "onPrimary",
        "primary_container": "primaryContainer", "on_primary_container": "onPrimaryContainer",
        "secondary": "secondary", "secondary_container": "secondaryContainer",
        "on_secondary_container": "onSecondaryContainer",
        "tertiary": "tertiary", "tertiary_container": "tertiaryContainer",
        "on_tertiary_container": "onTertiaryContainer",
        "error": "error", "on_error": "onError", "error_container": "errorContainer",
        "on_error_container": "onErrorContainer",
        "background": "background", "on_background": "backgroundText",
        "surface": "surface", "on_surface": "onSurface",
        "surface_variant": "surfaceVariant", "on_surface_variant": "onSurfaceVariant",
        "surface_dim": "surfaceDim", "surface_bright": "surfaceBright", "surface_tint": "surfaceTint",
        "surface_container_lowest": "surfaceContainerLowest", "surface_container_low": "surfaceContainerLow",
        "surface_container": "surfaceContainer", "surface_container_high": "surfaceContainerHigh",
        "surface_container_highest": "surfaceContainerHighest",
        "outline": "outline", "outline_variant": "outlineVariant",
        "inverse_surface": "inverseSurface", "inverse_on_surface": "inverseOnSurface",
        "inverse_primary": "inversePrimary", "scrim": "scrimColor"
    })

    readonly property string json: {
        const colors = {};
        for (const key in roles) {
            const value = Theme[roles[key]];
            if (value !== undefined && value !== null) colors[key] = hex(value);
        }
        return JSON.stringify({
            version: 1,
            shell: "dux",
            dark: !Theme.isLightMode,
            colors: colors,
            fonts: {
                main: Theme.fontFamily ?? "",
                title: Theme.fontFamily ?? "",
                monospace: Theme.monoFontFamily ?? ""
            },
            radius: {
                small: Theme.cornerRadiusS,
                normal: Theme.cornerRadiusM,
                large: Theme.cornerRadiusL
            },
            transparency: {
                background: 1 - (Theme.popupTransparency ?? 1),
                content: 0
            },
            // A solid color is stored in the same field; only paths count.
            wallpaper: String(SessionData.wallpaperPath ?? "").startsWith("/") ? SessionData.wallpaperPath : ""
        }, null, 2) + "\n";
    }
    onJsonChanged: writeTimer.restart()

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
        path: `${root.stateDir}/m3/style.json`
        atomicWrites: true
    }

    Component.onCompleted: writeTimer.restart()
}
