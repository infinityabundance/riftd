import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Page {
    id: root
    signal back()

    header: ToolBar {
        RowLayout {
            anchors.fill: parent
            anchors.margins: 8

            ToolButton {
                text: "< Back"
                onClicked: root.back()
            }

            Label {
                text: "Settings"
                font.pixelSize: 18
                font.bold: true
            }

            Item { Layout.fillWidth: true }
        }
    }

    ScrollView {
        anchors.fill: parent
        anchors.margins: 16

        ColumnLayout {
            width: Math.min(500, root.width - 32)
            spacing: 24

            GroupBox {
                title: "Audio"
                Layout.fillWidth: true

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 12

                    RowLayout {
                        Label { text: "Quality:" }
                        ComboBox {
                            id: qualityCombo
                            model: ["Low", "Medium", "High"]
                            currentIndex: 1
                            Layout.fillWidth: true
                        }
                    }

                    RowLayout {
                        Label { text: "Muted:" }
                        Switch {
                            checked: backend.muted
                            onToggled: backend.muted = checked
                        }
                    }
                }
            }

            GroupBox {
                title: "Network"
                Layout.fillWidth: true

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 12

                    TextField {
                        id: turnField
                        placeholderText: "TURN servers (turn:host:port)"
                        Layout.fillWidth: true
                    }

                    TextField {
                        id: bootstrapField
                        placeholderText: "DHT bootstrap nodes (ip:port)"
                        Layout.fillWidth: true
                    }
                }
            }

            GroupBox {
                title: "About"
                Layout.fillWidth: true

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 8

                    Label {
                        text: "Rift Desktop Client"
                        font.bold: true
                    }

                    Label {
                        text: "SDK Version: " + backend.sdkVersion
                    }

                    Label {
                        text: "Secure peer-to-peer voice communication"
                        opacity: 0.7
                    }
                }
            }

            Item { Layout.fillHeight: true }
        }
    }
}
