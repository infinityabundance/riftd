import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    visible: true
    width: 900
    height: 600
    minimumWidth: 700
    minimumHeight: 400
    title: "Rift"

    // Incoming call dialog
    Dialog {
        id: incomingCallDialog
        title: "Incoming Call"
        modal: true
        anchors.centerIn: parent
        visible: callManager.hasIncomingCall
        closePolicy: Dialog.NoAutoClose

        ColumnLayout {
            spacing: 16

            Label {
                text: "Call from: " + callManager.incomingPeerId.substring(0, 16) + "..."
                font.pixelSize: 16
            }

            RowLayout {
                spacing: 12
                Layout.alignment: Qt.AlignHCenter

                Button {
                    text: "Accept"
                    highlighted: true
                    onClicked: callManager.acceptCall()
                }

                Button {
                    text: "Decline"
                    onClicked: callManager.declineCall()
                }
            }
        }
    }

    StackView {
        id: stackView
        anchors.fill: parent
        initialItem: channelView
    }

    Component {
        id: channelView
        ChannelView {
            onJoined: stackView.push(chatView)
        }
    }

    Component {
        id: chatView
        ChatView {
            onDisconnected: stackView.pop()
            onOpenSettings: stackView.push(settingsView)
        }
    }

    Component {
        id: settingsView
        SettingsView {
            onBack: stackView.pop()
        }
    }
}
