import QtQuick 2.15
import QtQuick.Controls 2.15
import QtQuick.Layouts 1.15
import org.kde.kirigami 2.20 as Kirigami

Kirigami.ApplicationWindow {
    id: root
    width: 1100
    height: 700
    visible: true
    title: "Rift"
    flags: Qt.Window

    property string channelName: "gaming"
    property string password: ""
    property string bootstrapNodes: ""
    property bool useInternet: true
    property bool useDht: true

    header: Kirigami.InlineMessage {
        visible: false
        text: ""
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.largeSpacing
        padding: Kirigami.Units.largeSpacing

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            ColumnLayout {
                Layout.preferredWidth: 280
                spacing: Kirigami.Units.smallSpacing

                Kirigami.Heading { text: "Connection"; level: 3 }
                TextField {
                    placeholderText: "Channel"
                    text: root.channelName
                    onTextChanged: root.channelName = text
                }
                TextField {
                    placeholderText: "Password (optional)"
                    echoMode: TextInput.Password
                    text: root.password
                    onTextChanged: root.password = text
                }
                TextField {
                    placeholderText: "DHT bootstrap (ip:port, comma)"
                    text: root.bootstrapNodes
                    onTextChanged: root.bootstrapNodes = text
                }
                RowLayout {
                    CheckBox { text: "Internet"; checked: root.useInternet; onToggled: root.useInternet = checked }
                    CheckBox { text: "DHT"; checked: root.useDht; onToggled: root.useDht = checked }
                }
                RowLayout {
                    Button {
                        text: "Connect"
                        onClicked: riftBackend.joinChannel(root.channelName, root.password, root.useInternet, root.useDht, root.bootstrapNodes)
                    }
                    Button {
                        text: "Disconnect"
                        onClicked: riftBackend.leaveChannel(root.channelName)
                    }
                }
                Label {
                    text: "Status: " + riftBackend.status
                    color: Kirigami.Theme.textColor
                }
                Kirigami.Heading { text: "Peers"; level: 4 }
                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    model: peersModel
                    delegate: RowLayout {
                        spacing: Kirigami.Units.smallSpacing
                        Rectangle {
                            width: 8
                            height: 8
                            radius: 4
                            color: speaking ? "#4caf50" : "#8e8e8e"
                        }
                        Text { text: name; color: Kirigami.Theme.textColor }
                        Text { text: quality; color: Kirigami.Theme.disabledTextColor }
                    }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: Kirigami.Units.smallSpacing

                Kirigami.Heading { text: "Chat"; level: 3 }
                ListView {
                    id: chatView
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    model: eventsModel
                    delegate: Column {
                        width: chatView.width
                        Text {
                            text: "[" + timestamp.toLocaleTimeString() + "] " + from + ": " + text
                            color: Kirigami.Theme.textColor
                            wrapMode: Text.Wrap
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    TextField {
                        id: chatInput
                        Layout.fillWidth: true
                        placeholderText: "Type a message..."
                        onAccepted: {
                            if (text.length > 0) {
                                riftBackend.sendChat(text)
                                text = ""
                            }
                        }
                    }
                    Button {
                        text: "Send"
                        onClicked: {
                            if (chatInput.text.length > 0) {
                                riftBackend.sendChat(chatInput.text)
                                chatInput.text = ""
                            }
                        }
                    }
                }

                Button {
                    id: pttButton
                    text: "Hold to Talk"
                    Layout.fillWidth: true
                    height: 64
                    onPressed: riftBackend.startPtt()
                    onReleased: riftBackend.stopPtt()
                }
            }
        }
    }

    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_Space && !event.isAutoRepeat) {
            riftBackend.startPtt()
            event.accepted = true
        }
    }
    Keys.onReleased: (event) => {
        if (event.key === Qt.Key_Space && !event.isAutoRepeat) {
            riftBackend.stopPtt()
            event.accepted = true
        }
    }

    onClosing: (close) => {
        close.accepted = false
        root.visible = false
    }
}
