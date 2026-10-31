// Material 3 integration for this shell, added by Material 3 Settings
// (m3-settings). Everything of ours lives in this folder; the only edits to
// the shell's own files are between `m3:begin` / `m3:end` markers. Remove it
// all from Settings → Shell integration, or with `m3-settings inject remove`.

import qs.modules.common
import qs.m3
import qs.m3.games
import QtQuick
import Quickshell
import Quickshell.Io

Scope {
    id: root

    // Our own options, written by Material 3 Settings. They stay out of the
    // shell's config.json so the shell's own Config.qml needs no edit.
    property var options: ({})

    FileView {
        path: Quickshell.env("HOME") + "/.config/m3-settings/shell.json"
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                root.options = JSON.parse(text())
            } catch (e) {
                console.warn("[m3] shell.json:", e)
            }
        }
        onLoadFailed: root.options = ({})
    }

    // Bluetooth and network buttons open Material 3 Settings instead of
    // KDE's modules, but only while they still hold the shell's defaults.
    readonly property var kdeDefaults: ({
        "bluetooth": ["kcmshell6 kcm_bluetooth", "m3-settings bluetooth"],
        "network": ["kcmshell6 kcm_networkmanagement", "m3-settings network"],
        "networkEthernet": ["kcmshell6 kcm_networkmanagement", "m3-settings network"]
    })
    function useSettingsApp() {
        if (!Config.ready || !M3.settingsAvailable || !Config.options.apps) return;
        for (const key in root.kdeDefaults) {
            const [kde, ours] = root.kdeDefaults[key];
            if (Config.options.apps[key] === kde)
                Config.options.apps[key] = `${M3.pathPrelude}; command -v m3-settings >/dev/null && exec ${ours} || exec ${kde}`;
        }
    }
    Connections {
        target: Config
        function onReadyChanged() { root.useSettingsApp() }
    }
    Connections {
        target: M3
        function onSettingsAvailableChanged() { root.useSettingsApp() }
    }

    GameLibrary {}

    StyleExporter {
        shellName: "inir"
    }
}
