import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    spacing: 8

    Rectangle {
        id: pttButton
        Layout.fillWidth: true
        height: 60
        radius: 4
        color: backend.pttActive ? "#2196F3" : "#607D8B"

        MouseArea {
            anchors.fill: parent
            enabled: backend.connected

            onPressed: {
                backend.startPtt()
            }

            onReleased: {
                backend.stopPtt()
            }

            onCanceled: {
                backend.stopPtt()
            }
        }

        Label {
            anchors.centerIn: parent
            text: {
                if (!backend.connected) {
                    return "Connect to Talk"
                } else if (backend.pttActive) {
                    return "Talking..."
                } else {
                    return "Hold to Talk"
                }
            }
            color: "white"
            font.pixelSize: 16
            font.bold: true
        }
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 16

        Label {
            text: backend.muted ? "Muted" : "Unmuted"
            opacity: 0.7
        }

        Switch {
            checked: !backend.muted
            onToggled: backend.muted = !checked
        }

        Item { Layout.fillWidth: true }

        Label {
            visible: callManager.inCall
            text: "Call: " + callManager.callState
            font.bold: true
        }
    }
}
