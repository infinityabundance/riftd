#pragma once

#include <QObject>
#include <QString>
#include <QSystemTrayIcon>
#include <QMenu>

class NotificationManager : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool systemTrayAvailable READ systemTrayAvailable CONSTANT)

public:
    explicit NotificationManager(QObject* parent = nullptr);
    ~NotificationManager() override;

    bool systemTrayAvailable() const;

    Q_INVOKABLE void showNotification(const QString& title, const QString& message);
    Q_INVOKABLE void showIncomingCall(const QString& fromPeer);
    Q_INVOKABLE void hideIncomingCall();
    Q_INVOKABLE void setTrayTooltip(const QString& tooltip);

signals:
    void trayActivated();
    void acceptCallClicked();
    void declineCallClicked();

private:
    void setupTrayIcon();

    QSystemTrayIcon* m_trayIcon = nullptr;
    QMenu* m_trayMenu = nullptr;
};
