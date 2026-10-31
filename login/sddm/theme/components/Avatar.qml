import QtQuick
import QtQuick.Effects

// A round user picture. Tries each URL in "sources" in turn and falls back
// to the user's initial on a primary-container disc, like the shell does.
Item {
    id: avatar
    property var sources: []
    property string initial: "?"
    property real ringWidth: 0
    property color ringColor: M3.primary

    readonly property bool effectsAvailable: GraphicsInfo.api !== GraphicsInfo.Software && GraphicsInfo.api !== GraphicsInfo.Unknown
    property int attempt: 0
    onSourcesChanged: attempt = 0
    readonly property string currentSource: attempt < sources.length ? String(sources[attempt]) : ""
    readonly property bool showingImage: img.status === Image.Ready

    implicitWidth: 120 * M3.s
    implicitHeight: implicitWidth

    Rectangle {
        anchors.fill: parent
        anchors.margins: -avatar.ringWidth
        radius: width / 2
        color: avatar.ringColor
        visible: avatar.ringWidth > 0
    }

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: M3.primaryContainer
        Text {
            anchors.centerIn: parent
            visible: !avatar.showingImage
            text: avatar.initial
            color: M3.colOnPrimaryContainer
            font.family: M3.fontFamily
            font.pixelSize: parent.height * 0.42
            font.weight: Font.Medium
        }
    }

    Image {
        id: img
        anchors.fill: parent
        source: avatar.currentSource
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        smooth: true
        mipmap: true
        sourceSize.width: Math.ceil(width * 2)
        sourceSize.height: Math.ceil(height * 2)
        // Without shader effects the picture is shown unmasked (square).
        visible: !avatar.effectsAvailable && status === Image.Ready
        onStatusChanged: if (status === Image.Error) avatar.attempt++
    }

    Item {
        id: mask
        anchors.fill: parent
        visible: false
        layer.enabled: true
        layer.smooth: true
        Rectangle {
            anchors.fill: parent
            radius: width / 2
            color: "black"
        }
    }

    MultiEffect {
        anchors.fill: img
        source: img
        visible: avatar.effectsAvailable && img.status === Image.Ready
        maskEnabled: true
        maskSource: mask
        maskThresholdMin: 0.5
        maskSpreadAtMin: 1.0
    }
}
