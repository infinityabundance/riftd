import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Page {
    id: root
    signal joined()

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: 8

            Label {
                text: "Rift"
                font.pixelSize: 20
                font.bold: true
            }

            Item { Layout.fillWidth: true }

            Label {
                text: "v" + backend.sdkVersion
                opacity: 0.6
            }
        }
    }

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(400, parent.width - 40)
        spacing: 16

        Label {
            text: "Status: " + backend.status
            Layout.alignment: Qt.AlignHCenter
        }

        TextField {
            id: channelField
            placeholderText: "Channel name"
            text: "gaming"
            Layout.fillWidth: true
        }

        TextField {
            id: passwordField
            placeholderText: "Password (optional)"
            echoMode: TextInput.Password
            Layout.fillWidth: true
        }

        TextField {
            id: inviteField
            placeholderText: "Invite link (optional)"
            Layout.fillWidth: true
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            CheckBox {
                id: internetCheck
                text: "Internet"
                checked: true
            }

            CheckBox {
                id: dhtCheck
                text: "DHT"
                checked: false
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 12

            Button {
                text: "Join Channel"
                highlighted: true
                Layout.fillWidth: true
                onClicked: {
                    if (channelField.text.length > 0) {
                        if (backend.joinChannel(channelField.text, passwordField.text, internetCheck.checked)) {
                            root.joined()
                        }
                    }
                }
            }

            Button {
                text: "Join with Invite"
                Layout.fillWidth: true
                enabled: inviteField.text.length > 0
                onClicked: {
                    // TODO: Parse invite and join
                    if (channelField.text.length > 0) {
                        if (backend.joinChannel(channelField.text, passwordField.text, internetCheck.checked)) {
                            root.joined()
                        }
                    }
                }
            }
        }
    }
}
