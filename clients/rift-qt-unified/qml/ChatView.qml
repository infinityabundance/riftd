import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Page {
    id: root
    signal disconnected()
    signal openSettings()

    // Invite dialog
    Dialog {
        id: inviteDialog
        title: "Channel Invite"
        modal: true
        anchors.centerIn: parent
        width: Math.min(parent.width - 40, 600)

        property string inviteText: ""

        ColumnLayout {
            spacing: 12
            width: parent.width

            Label {
                text: "Share this command to invite others:"
                wrapMode: Text.Wrap
            }

            Rectangle {
                Layout.fillWidth: true
                height: inviteCommand.implicitHeight + 16
                color: "#2d2d2d"
                radius: 4

                TextEdit {
                    id: inviteCommand
                    anchors.fill: parent
                    anchors.margins: 8
                    text: "cargo run -p rift -- join --invite \"" + inviteDialog.inviteText + "\" --voice"
                    wrapMode: TextEdit.Wrap
                    readOnly: true
                    selectByMouse: true
                    color: "#e0e0e0"
                    font.family: "monospace"
                }
            }

            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 8

                Button {
                    text: "Copy"
                    onClicked: {
                        inviteCommand.selectAll()
                        inviteCommand.copy()
                        inviteCommand.deselect()
                    }
                }

                Button {
                    text: "Close"
                    onClicked: inviteDialog.close()
                }
            }
        }
    }

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: 8

            Label {
                text: backend.status
                font.bold: true
            }

            Item { Layout.fillWidth: true }

            Label {
                text: "Peers: " + backend.peers.length
            }

            ToolButton {
                text: "Invite"
                onClicked: {
                    var invite = backend.getInvite()
                    if (invite.length > 0) {
                        inviteDialog.inviteText = invite
                        inviteDialog.open()
                    }
                }
            }

            ToolButton {
                text: "Settings"
                onClicked: root.openSettings()
            }

            ToolButton {
                text: "Leave"
                onClicked: {
                    backend.leaveChannel("")
                    root.disconnected()
                }
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 8

        // Active call banner
        Rectangle {
            visible: callManager.inCall
            Layout.fillWidth: true
            height: 60
            color: "#1a3a5c"
            radius: 4

            RowLayout {
                anchors.fill: parent
                anchors.margins: 8
                spacing: 12

                ColumnLayout {
                    spacing: 2
                    Label {
                        text: "In call with: " + callManager.currentPeerId.substring(0, 12) + "..."
                        font.bold: true
                        color: "#e0e0e0"
                    }
                    Label {
                        text: "Status: " + callManager.callState
                        font.pixelSize: 12
                        color: "#a0a0a0"
                    }
                }

                Item { Layout.fillWidth: true }

                Button {
                    text: backend.muted ? "Unmute" : "Mute"
                    onClicked: backend.muted = !backend.muted
                }

                Button {
                    text: "End Call"
                    highlighted: true
                    onClicked: callManager.endCall()
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 12

            // Peer list
            PeerList {
                Layout.preferredWidth: 250
                Layout.fillHeight: true
                enabled: !callManager.inCall
            }

            // Chat area
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 8

                Label {
                    text: "Chat"
                    font.bold: true
                }

                ListView {
                    id: chatList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    model: backend.messages

                    Rectangle {
                        anchors.fill: parent
                        color: "#1e1e1e"
                        radius: 4
                        z: -1
                    }

                    delegate: ItemDelegate {
                        width: chatList.width
                        height: msgText.implicitHeight + 12
                        background: Rectangle {
                            color: "transparent"
                        }

                        Text {
                            id: msgText
                            anchors.fill: parent
                            anchors.margins: 6
                            text: "[" + modelData.time + "] " + modelData.from + ": " + modelData.text
                            wrapMode: Text.Wrap
                            color: "#e0e0e0"
                        }
                    }

                    onCountChanged: {
                        Qt.callLater(function() {
                            chatList.positionViewAtEnd()
                        })
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 8

                    TextField {
                        id: messageField
                        Layout.fillWidth: true
                        placeholderText: "Type a message..."
                        enabled: backend.connected

                        Keys.onReturnPressed: sendMessage()
                    }

                    Button {
                        text: "Send"
                        enabled: backend.connected && messageField.text.length > 0
                        onClicked: sendMessage()
                    }
                }
            }
        }

        // PTT Button
        CallView {
            Layout.fillWidth: true
        }
    }

    function sendMessage() {
        if (messageField.text.length > 0) {
            backend.sendChat(messageField.text)
            messageField.text = ""
        }
    }
}
