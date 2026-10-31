// Material 3 integration for this shell, added by Material 3 Settings
// (m3-settings). Everything of ours lives in this folder; the only edits to
// the shell's own files are between `m3:begin` / `m3:end` markers. Remove it
// all from Settings → Shell, or with `m3-settings inject remove`.

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
}
