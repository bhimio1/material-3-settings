// Control Center tile: Game Mode through Material 3 Games (gamescope, other
// apps closed). Added by Material 3 Settings.

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Common
import qs.Services
import qs.Modules.Plugins
import "M3Games.js" as M3Games

PluginComponent {
    id: root

    property bool available: false
    property bool active: false

    ccWidgetIcon: "sports_esports"
    ccWidgetPrimaryText: "Game Mode"
    ccWidgetSecondaryText: !available ? "Install Material 3 Games" : active ? "On" : "Off"
    ccWidgetIsActive: active

    onCcWidgetToggled: {
        if (!available) {
            Quickshell.execDetached(["bash", "-c", 'PATH="$HOME/.local/bin:$PATH"; exec m3-settings material-apps m3-games']);
            return;
        }
        Quickshell.execDetached(M3Games.command(["game-mode"]));
        statusTimer.restart();
    }

    Process {
        running: true
        command: ["bash", "-c", 'PATH="$HOME/.local/bin:$PATH"; command -v m3-games']
        onExited: (exitCode, exitStatus) => root.available = exitCode === 0
    }

    Process {
        id: statusProc
        command: M3Games.command(["status", "--json"])
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    root.active = !!JSON.parse(text).game_mode;
                } catch (e) {}
            }
        }
    }

    Timer {
        id: statusTimer
        interval: 5000
        repeat: true
        running: root.available
        triggeredOnStart: true
        onTriggered: if (!statusProc.running) statusProc.running = true
    }
}
