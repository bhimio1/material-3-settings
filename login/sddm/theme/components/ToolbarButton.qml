import QtQuick
import QtQuick.Layouts

// A round (or pill, with text) toolbar button. "toggled" fills it with the
// primary colour, like the shell's lock-screen confirm button.
Item {
    id: btn
    property string icon: ""
    property string text: ""
    property bool toggled: false
    property bool filled: false
    property bool busy: false
    property string tooltip: ""
    property color colToggled: M3.primary
    property color colOnToggled: M3.colOnPrimary
    signal clicked()

    readonly property bool hovered: mouse.containsMouse
    Layout.fillHeight: true
    Layout.alignment: Qt.AlignVCenter
    implicitHeight: M3.toolbarHeight - 16 * M3.s
    implicitWidth: label.text.length > 0 ? row.implicitWidth + 28 * M3.s : implicitHeight
    opacity: enabled ? 1 : 0.5
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: tooltip || text || icon
    Keys.onReturnPressed: btn.clicked()
    Keys.onEnterPressed: btn.clicked()
    Keys.onSpacePressed: btn.clicked()

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: height / 2
        color: btn.toggled ? (mouse.pressed ? Qt.darker(btn.colToggled, 1.15) : btn.hovered ? Qt.lighter(btn.colToggled, 1.08) : btn.colToggled)
                           : (mouse.pressed ? M3.pressed : btn.hovered || btn.activeFocus ? M3.hover : "transparent")
        Behavior on color { ColorAnimation { duration: 150 } }
        border.width: btn.activeFocus ? 2 * M3.s : 0
        border.color: M3.primary
    }

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 6 * M3.s
        Icon {
            id: glyph
            anchors.verticalCenter: parent.verticalCenter
            visible: btn.icon.length > 0
            name: btn.busy ? "progress_activity" : btn.icon
            filled: btn.filled || btn.toggled
            color: btn.toggled ? btn.colOnToggled : M3.colOnSurfaceVariant
            size: 22 * M3.s
            RotationAnimation on rotation {
                running: btn.busy
                from: 0; to: 360; duration: 900
                loops: Animation.Infinite
                onStopped: glyph.rotation = 0
            }
        }
        Text {
            id: label
            anchors.verticalCenter: parent.verticalCenter
            visible: text.length > 0
            text: btn.text
            color: btn.toggled ? btn.colOnToggled : M3.colOnSurfaceVariant
            font.family: M3.fontFamily
            font.pixelSize: M3.fontSmall
            font.weight: Font.Medium
            elide: Text.ElideRight
            width: Math.min(implicitWidth, 220 * M3.s)
        }
    }

    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: btn.clicked()
    }
}
