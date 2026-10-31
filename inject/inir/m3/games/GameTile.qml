import qs
import qs.services
import qs.m3
import qs.modules.common
import qs.modules.common.widgets
import QtQuick
import QtQuick.Layouts
import Quickshell.Widgets

/* A game's cover art (2:3) with its name; activating it launches the game. */
Item {
    id: root

    required property var game
    property bool highlighted: false
    signal activated()

    Rectangle {
        id: card
        anchors.fill: parent
        anchors.margins: 6
        radius: Appearance.rounding.normal
        color: (mouse.containsMouse || root.highlighted) ? Appearance.colors.colLayer2Hover : Appearance.colors.colLayer2
        scale: mouse.pressed ? 0.97 : ((mouse.containsMouse || root.highlighted) ? 1.03 : 1)
        Behavior on scale { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
        Behavior on color { ColorAnimation { duration: 120 } }

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 8
            spacing: 6

            ClippingRectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                radius: Appearance.rounding.small
                color: Appearance.colors.colLayer1

                StyledImage {
                    anchors.fill: parent
                    source: M3Games.coverUrl(root.game)
                    fillMode: Image.PreserveAspectCrop
                    sourceSize.width: width
                    sourceSize.height: height
                }

                MaterialSymbol {
                    anchors.centerIn: parent
                    visible: !root.game.cover
                    text: "stadia_controller"
                    iconSize: 48
                    color: Appearance.colors.colOnLayer1
                }

                // Play badge on hover / keyboard focus.
                Rectangle {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    anchors.margins: 8
                    width: 44
                    height: 44
                    radius: width / 2
                    color: Appearance.colors.colPrimary
                    visible: mouse.containsMouse || root.highlighted
                    MaterialSymbol {
                        anchors.centerIn: parent
                        text: "play_arrow"
                        fill: 1
                        iconSize: 26
                        color: Appearance.colors.colOnPrimary
                    }
                }
            }

            StyledText {
                Layout.fillWidth: true
                text: root.game.name
                elide: Text.ElideRight
                font.pixelSize: Appearance.font.pixelSize.small
                color: Appearance.colors.colOnLayer1
            }
        }

        MouseArea {
            id: mouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.activated()
        }
    }
}
