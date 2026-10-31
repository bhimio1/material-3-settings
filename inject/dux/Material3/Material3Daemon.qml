// Material 3 apps integration for Dux Shell, added by Material 3 Settings.
// Exports Dux Shell's look for the Material 3 apps (StyleExporter), and
// shows Material 3 Settings' welcome the first time.

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Common
import qs.Services
import qs.Modules.Plugins

PluginComponent {
    id: root

    property bool checked: false

    onPluginServiceChanged: maybeWelcome()
    Component.onCompleted: maybeWelcome()

    function maybeWelcome() {
        if (checked || !pluginService)
            return;
        checked = true;
        if (pluginService.loadPluginState("material3", "welcomed", false))
            return;
        welcomeProc.running = true;
    }

    StyleExporter {}

    Process {
        id: welcomeProc
        command: ["bash", "-c", 'PATH="$HOME/.local/bin:$PATH"; command -v m3-settings >/dev/null || exit 3; exec m3-settings --welcome']
        // Exit 3: Settings isn't installed; ask again next session.
        onExited: (exitCode, exitStatus) => {
            if (exitCode !== 3)
                root.pluginService?.savePluginState("material3", "welcomed", true);
        }
    }
}
