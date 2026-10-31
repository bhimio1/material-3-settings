import QtQuick
import QtQuick.Layouts

// The shell's Material 3 expressive toolbar: a pill-shaped surface container
// with a soft shadow, holding a row of controls.
Item {
    id: root
    property real padding: 8 * M3.s
    property alias spacing: row.spacing
    property alias color: bg.color
    default property alias content: row.data

    implicitWidth: row.implicitWidth + padding * 2
    implicitHeight: M3.toolbarHeight

    // Shadow drawn as stacked translucent pills: no shader needed, so it also
    // works when the greeter falls back to software rendering.
    Repeater {
        model: 3
        Rectangle {
            required property int index
            anchors.fill: bg
            anchors.margins: -(index + 1) * 1.5 * M3.s
            anchors.topMargin: -(index + 1) * 1.5 * M3.s + 2 * M3.s
            anchors.bottomMargin: -(index + 1) * 1.5 * M3.s - 2 * M3.s
            radius: height / 2
            color: M3.alpha(M3.shadow, 0.10 - index * 0.03)
        }
    }

    Rectangle {
        id: bg
        anchors.fill: parent
        radius: height / 2
        color: M3.toolbar
    }

    RowLayout {
        id: row
        spacing: 4 * M3.s
        anchors.fill: parent
        anchors.margins: root.padding
    }
}
