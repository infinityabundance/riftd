import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    spacing: 4

    Label {
        text: "Peers"
        font.bold: true
    }

    ListView {
        id: peerList
        Layout.fillWidth: true
        Layout.fillHeight: true
        clip: true
        model: backend.peers

        Rectangle {
            anchors.fill: parent
            color: "#1e1e1e"
            radius: 4
            z: -1
        }

        delegate: ItemDelegate {
            width: peerList.width
            height: 44
            background: Rectangle {
                color: parent.hovered ? "#3d3d3d" : "transparent"
            }

            RowLayout {
                anchors.fill: parent
                anchors.margins: 8
                spacing: 8

                Label {
                    text: modelData.id.substring(0, 12) + "..."
                    Layout.fillWidth: true
                    elide: Text.ElideRight
                    color: "#e0e0e0"
                }

                Button {
                    text: "Call"
                    visible: root.enabled && !callManager.inCall
                    onClicked: callManager.startCall(modelData.id)
                }
            }
        }
    }

    Label {
        visible: backend.peers.length === 0
        text: "No peers connected"
        color: "#808080"
        Layout.alignment: Qt.AlignHCenter
    }
}
