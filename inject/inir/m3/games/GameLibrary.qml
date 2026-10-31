import qs
import qs.services
import qs.m3
import qs.modules.common
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

/*
 * The game library overlay: recently played games, every game Material 3
 * Games knows about, and the way into Game Mode. Toggle it with
 * `inir ipc call games toggle` (bind it in Niri's config).
 */
Scope {
    id: root

    Loader {
        active: M3.gamesOpen

        sourceComponent: PanelWindow {
            id: panelWindow

            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.namespace: "quickshell:games"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
            color: "transparent"

            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }

            Component.onCompleted: M3Games.rescan()

            // Dim the desktop; clicking outside the card closes the panel.
            Rectangle {
                anchors.fill: parent
                color: Appearance.m3colors.m3scrim
                opacity: 0.45
                MouseArea {
                    anchors.fill: parent
                    onClicked: M3.gamesOpen = false
                }
            }

            GameLibraryContent {
                anchors.centerIn: parent
                width: Math.min(parent.width - 80, 1180)
                height: Math.min(parent.height - 80, 760)
            }
        }
    }

    IpcHandler {
        target: "games"

        function toggle(): void { M3.gamesOpen = !M3.gamesOpen; }
        function open(): void { M3.gamesOpen = true; }
        function close(): void { M3.gamesOpen = false; }
        function gameMode(): void { M3Games.enterGameMode(); }
    }
}
