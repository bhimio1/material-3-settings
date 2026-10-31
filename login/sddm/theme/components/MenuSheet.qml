import QtQuick

// A Material 3 menu: a rounded surface with one row per entry, opened above
// (or below) an anchor. Entries are {icon, text, subtitle?, avatar?, selected}.
// Clicking outside, Escape, or picking an entry closes it.
Item {
    id: sheet
    property var entries: []
    property bool open: false
    property string title: ""
    signal picked(int index)

    anchors.fill: parent
    visible: open || card.opacity > 0
    z: 100

    function close() { open = false }
    onOpenChanged: if (open) { list.currentIndex = Math.max(0, entries.findIndex(e => e.selected)); list.forceActiveFocus() }

    // Where the card goes, in the coordinates of this item's parent.
    property real cardX: 0
    property real cardBottom: 0
    readonly property real rowHeight: 48 * M3.s

    MouseArea {
        anchors.fill: parent
        enabled: sheet.open
        onClicked: sheet.close()
        onWheel: wheel => wheel.accepted = true
    }

    Rectangle {
        id: card
        x: Math.max(16 * M3.s, Math.min(sheet.cardX, sheet.width - width - 16 * M3.s))
        y: Math.max(16 * M3.s, sheet.cardBottom - height)
        width: Math.min(sheet.width - 32 * M3.s, Math.max(240 * M3.s, list.contentMaxWidth + 32 * M3.s))
        height: Math.min(sheet.height * 0.6, (titleText.visible ? titleText.height + 4 * M3.s : 0) + list.contentHeight + 16 * M3.s)
        radius: M3.rounding
        color: M3.surfaceContainerHigh
        opacity: sheet.open ? 1 : 0
        scale: sheet.open ? 1 : 0.92
        transformOrigin: Item.Bottom
        Behavior on opacity { NumberAnimation { duration: 160 } }
        Behavior on scale { NumberAnimation { duration: 260; easing.type: Easing.BezierSpline; easing.bezierCurve: M3.curveFastSpatial } }

        MouseArea { anchors.fill: parent } // keep clicks inside from closing it

        Text {
            id: titleText
            visible: sheet.title.length > 0
            x: 20 * M3.s
            y: 12 * M3.s
            text: sheet.title
            color: M3.subtext
            font.family: M3.fontFamily
            font.pixelSize: M3.fontSmall * 0.9
            font.weight: Font.Medium
        }

        ListView {
            id: list
            anchors.fill: parent
            anchors.margins: 8 * M3.s
            anchors.topMargin: titleText.visible ? titleText.y + titleText.height + 4 * M3.s : 8 * M3.s
            clip: true
            model: sheet.entries
            boundsBehavior: Flickable.StopAtBounds
            keyNavigationWraps: true
            property real contentMaxWidth: 0
            Keys.onEscapePressed: sheet.close()
            Keys.onReturnPressed: sheet.picked(currentIndex)
            Keys.onEnterPressed: sheet.picked(currentIndex)

            delegate: Rectangle {
                id: row
                required property var modelData
                required property int index
                width: ListView.view.width
                height: sheet.rowHeight
                radius: M3.rounding * 0.7
                color: modelData.selected ? M3.secondaryContainer
                     : (rowMouse.containsMouse || ListView.isCurrentItem && list.activeFocus) ? M3.hover : "transparent"

                Row {
                    id: rowContent
                    anchors.verticalCenter: parent.verticalCenter
                    x: 12 * M3.s
                    spacing: 12 * M3.s
                    Component.onCompleted: list.contentMaxWidth = Math.max(list.contentMaxWidth, implicitWidth + 24 * M3.s)
                    Loader {
                        anchors.verticalCenter: parent.verticalCenter
                        active: row.modelData.avatar !== undefined
                        visible: active
                        sourceComponent: Avatar {
                            implicitWidth: 32 * M3.s
                            sources: row.modelData.avatar ?? []
                            initial: row.modelData.initial ?? "?"
                        }
                    }
                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: row.modelData.avatar === undefined
                        name: row.modelData.icon ?? ""
                        filled: !!row.modelData.selected
                        color: row.modelData.selected ? M3.colOnSecondaryContainer : M3.colOnSurfaceVariant
                        size: 22 * M3.s
                    }
                    Column {
                        anchors.verticalCenter: parent.verticalCenter
                        Text {
                            text: row.modelData.text ?? ""
                            color: row.modelData.selected ? M3.colOnSecondaryContainer : M3.colOnSurface
                            font.family: M3.fontFamily
                            font.pixelSize: M3.fontSmall
                            font.weight: row.modelData.selected ? Font.Medium : Font.Normal
                        }
                        Text {
                            visible: text.length > 0
                            text: row.modelData.subtitle ?? ""
                            color: M3.subtext
                            font.family: M3.fontFamily
                            font.pixelSize: M3.fontSmall * 0.8
                        }
                    }
                }
                MouseArea {
                    id: rowMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: sheet.picked(row.index)
                }
            }
        }
    }
}
