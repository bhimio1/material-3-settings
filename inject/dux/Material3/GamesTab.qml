// Dash tab: the game library of Material 3 Games, read from the library file
// the app keeps (~/.cache/m3-games/library.json). Added by Material 3 Settings.

import QtQuick
import Quickshell
import Quickshell.Io
import qs.Common
import qs.DCommon.Widgets
import qs.Modules.Plugins
import qs.Modules.DDash
import "M3Games.js" as M3Games

DashTabComponent {
    id: root

    property var games: []
    property string query: ""
    readonly property var filtered: {
        const q = query.trim().toLowerCase();
        const list = games.slice().sort((a, b) => (b.last_played ?? 0) - (a.last_played ?? 0));
        return q === "" ? list : list.filter(g => (g.name ?? "").toLowerCase().includes(q));
    }

    implicitHeight: Math.max(DashMetrics.tabMinHeight, 520)
    menuActions: [
        {
            label: "Open Material 3 Games",
            iconName: "open_in_new",
            enabled: true,
            action: () => Quickshell.execDetached(M3Games.command([]))
        },
        {
            label: "Game Mode",
            iconName: "sports_esports",
            enabled: true,
            action: () => Quickshell.execDetached(M3Games.command(["game-mode"]))
        }
    ]

    FileView {
        path: (Quickshell.env("XDG_CACHE_HOME") || Quickshell.env("HOME") + "/.cache") + "/m3-games/library.json"
        watchChanges: true
        onFileChanged: reload()
        onLoaded: root.games = M3Games.parseLibrary(text())
        onLoadFailed: root.games = []
    }

    StyledText {
        anchors.centerIn: parent
        visible: root.games.length === 0
        width: parent.width * 0.7
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        text: "No games yet. Open Material 3 Games once to build your library, or install it from Material 3 Settings."
        color: Theme.surfaceVariantText
        font.pixelSize: Theme.fontSizeMedium
    }

    GridView {
        id: grid
        anchors.fill: parent
        anchors.margins: Theme.spacingL
        visible: root.games.length > 0
        clip: true
        cellWidth: Math.floor(width / Math.max(1, Math.floor(width / 150)))
        cellHeight: cellWidth * 1.5 + 28
        model: root.filtered

        delegate: Item {
            id: tile
            required property var modelData
            width: grid.cellWidth
            height: grid.cellHeight

            Rectangle {
                id: cover
                anchors.top: parent.top
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width - Theme.spacingS * 2
                height: width * 1.5
                radius: Theme.cornerRadius
                color: Theme.surfaceContainerHigh
                clip: true

                Image {
                    anchors.fill: parent
                    source: tile.modelData.cover ? "file://" + tile.modelData.cover : ""
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Quickshell.execDetached(M3Games.command(["launch", tile.modelData.id]))
                }
            }

            StyledText {
                anchors.top: cover.bottom
                anchors.topMargin: Theme.spacingXS
                anchors.horizontalCenter: parent.horizontalCenter
                width: cover.width
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignHCenter
                text: tile.modelData.name ?? ""
                color: Theme.surfaceText
                font.pixelSize: Theme.fontSizeSmall
            }
        }
    }
}
