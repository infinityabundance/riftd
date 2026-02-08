#pragma once

#include <QMainWindow>
#include <QSplitter>
#include <QListView>
#include <QLineEdit>
#include <QPushButton>
#include <QCheckBox>
#include <QLabel>
#include <QSystemTrayIcon>
#include <QCloseEvent>

#include "RiftBackend.h"

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow() override;

protected:
    void closeEvent(QCloseEvent* event) override;
    bool eventFilter(QObject* obj, QEvent* event) override;

private:
    void setupUi();
    void setupTray();
    void setupTray();
    void connectSignals();
    void setStatus(const QString& text);

    RiftBackend m_backend;

    QLineEdit* m_channelEdit = nullptr;
    QLineEdit* m_passwordEdit = nullptr;
    QLineEdit* m_bootstrapEdit = nullptr;
    QCheckBox* m_internetCheck = nullptr;
    QCheckBox* m_dhtCheck = nullptr;
    QPushButton* m_connectButton = nullptr;
    QPushButton* m_disconnectButton = nullptr;
    QListView* m_peerList = nullptr;
    QListView* m_chatList = nullptr;
    QLineEdit* m_inputEdit = nullptr;
    QPushButton* m_sendButton = nullptr;
    QPushButton* m_pttButton = nullptr;
    QLabel* m_statusLabel = nullptr;

    QSystemTrayIcon* m_tray = nullptr;
    QAction* m_muteAction = nullptr;
    bool m_muted = false;
};
