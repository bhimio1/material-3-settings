import qs
import qs.services
import qs.m3
import qs.modules.common
import qs.modules.common.widgets
import qs.modules.common.functions
import QtQuick
import QtQuick.Layouts

Rectangle {
    id: root

    property string query: ""
    readonly property var filtered: {
        const q = root.query.trim().toLowerCase();
        return q === "" ? M3Games.games : M3Games.games.filter(g => (g.name ?? "").toLowerCase().includes(q));
    }

    function play(game) {
        M3Games.launch(game.id);
        M3.gamesOpen = false;
    }

    radius: Appearance.rounding.windowRounding
    color: Appearance.colors.colLayer0
    border.width: 1
    border.color: Appearance.colors.colLayer0Border

    // Swallow clicks so they don't reach the scrim behind.
    MouseArea { anchors.fill: parent }

    Keys.onEscapePressed: M3.gamesOpen = false

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 16

        // Header
        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            MaterialSymbol {
                text: "stadia_controller"
                iconSize: 32
                fill: 1
                color: Appearance.colors.colPrimary
            }
            StyledText {
                text: Translation.tr("Games")
                font.pixelSize: Appearance.font.pixelSize.huge
                font.family: Appearance.font.family.title
                color: Appearance.colors.colOnLayer0
            }
            StyledText {
                visible: M3Games.runningGame !== null
                text: Translation.tr("Playing %1").arg(M3Games.runningGame?.name ?? "")
                color: Appearance.colors.colSubtext
            }

            Item { Layout.fillWidth: true }

            MaterialTextField {
                id: search
                Layout.preferredWidth: 280
                placeholderText: Translation.tr("Search games")
                focus: true
                onTextChanged: root.query = text
                Keys.onDownPressed: grid.forceActiveFocus()
                Keys.onReturnPressed: if (root.filtered.length > 0) root.play(root.filtered[0])
                Keys.onEscapePressed: M3.gamesOpen = false
            }

            RippleButtonWithIcon {
                visible: M3Games.available
                materialIcon: "sports_esports"
                mainText: Translation.tr("Game Mode")
                colBackground: Appearance.colors.colPrimaryContainer
                onClicked: {
                    M3.gamesOpen = false;
                    M3Games.enterGameMode();
                }
                StyledToolTip {
                    text: M3Games.sessionReady
                        ? Translation.tr("Closes your apps and switches from Hyprland to gamescope for the best performance")
                        : Translation.tr("Runs the big-picture library in gamescope. Pick \"Hyprland (with Game Mode)\" at login for the full switch")
                }
            }
            RippleButtonWithIcon {
                visible: M3Games.available
                materialIcon: "open_in_new"
                mainText: Translation.tr("Library")
                onClicked: {
                    M3.gamesOpen = false;
                    M3Games.openApp();
                }
            }
            RippleButtonWithIcon {
                materialIcon: "close"
                mainText: ""
                onClicked: M3.gamesOpen = false
            }
        }

        // Not installed yet
        ColumnLayout {
            visible: !M3Games.available
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 12
            Item { Layout.fillHeight: true }
            MaterialSymbol {
                Layout.alignment: Qt.AlignHCenter
                text: "stadia_controller"
                iconSize: 64
                color: Appearance.colors.colSubtext
            }
            StyledText {
                Layout.alignment: Qt.AlignHCenter
                text: Translation.tr("Material 3 Games isn't installed")
                font.pixelSize: Appearance.font.pixelSize.larger
                color: Appearance.colors.colOnLayer0
            }
            RippleButtonWithIcon {
                Layout.alignment: Qt.AlignHCenter
                materialIcon: "download"
                mainText: Translation.tr("Install in Settings")
                onClicked: {
                    M3.gamesOpen = false;
                    M3.manage("m3-games");
                }
            }
            Item { Layout.fillHeight: true }
        }

        // Recently played
        StyledText {
            visible: M3Games.available && root.query === "" && M3Games.recent.length > 0
            text: Translation.tr("Recently played")
            font.pixelSize: Appearance.font.pixelSize.larger
            color: Appearance.colors.colOnLayer0
        }
        ListView {
            visible: M3Games.available && root.query === "" && M3Games.recent.length > 0
            Layout.fillWidth: true
            Layout.preferredHeight: 250
            orientation: ListView.Horizontal
            clip: true
            spacing: 4
            model: M3Games.recent
            delegate: GameTile {
                required property var modelData
                game: modelData
                width: 168
                height: 250
                onActivated: root.play(modelData)
            }
        }

        // All games
        StyledText {
            visible: M3Games.available
            text: root.query === "" ? Translation.tr("All games (%1)").arg(M3Games.games.length) : Translation.tr("%1 results").arg(root.filtered.length)
            font.pixelSize: Appearance.font.pixelSize.larger
            color: Appearance.colors.colOnLayer0
        }
        GridView {
            id: grid
            visible: M3Games.available
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            cellWidth: Math.floor(width / Math.max(1, Math.floor(width / 168)))
            cellHeight: cellWidth * 1.5 + 30
            model: root.filtered
            keyNavigationEnabled: true
            currentIndex: -1
            delegate: GameTile {
                required property var modelData
                required property int index
                game: modelData
                width: grid.cellWidth
                height: grid.cellHeight
                highlighted: grid.activeFocus && grid.currentIndex === index
                onActivated: root.play(modelData)
            }
            onActiveFocusChanged: if (activeFocus && currentIndex < 0) currentIndex = 0
            Keys.onReturnPressed: if (currentIndex >= 0) root.play(root.filtered[currentIndex])
            Keys.onEscapePressed: M3.gamesOpen = false

            StyledText {
                anchors.centerIn: parent
                visible: M3Games.available && root.filtered.length === 0
                text: M3Games.games.length === 0
                    ? Translation.tr("No games found yet. Open the library to scan Steam, Lutris and Heroic.")
                    : Translation.tr("No games match \"%1\"").arg(root.query)
                color: Appearance.colors.colSubtext
            }
        }
    }
}
