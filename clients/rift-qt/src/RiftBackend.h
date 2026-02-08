#pragma once

#include <QObject>
#include <QDateTime>
#include <QSettings>
#include <QThread>
#include <QTimer>
#include <atomic>
#include <thread>

#include "RiftEventsModel.h"
#include "RiftPeersModel.h"

extern "C" {
#include "rift.h"
}

class RiftBackend : public QObject {
    Q_OBJECT
    Q_PROPERTY(RiftEventsModel* eventsModel READ eventsModel CONSTANT)
    Q_PROPERTY(RiftPeersModel* peersModel READ peersModel CONSTANT)
    Q_PROPERTY(QString status READ status NOTIFY statusChanged)

public:
    explicit RiftBackend(QObject* parent = nullptr);
    ~RiftBackend() override;

    Q_INVOKABLE bool init();
    Q_INVOKABLE bool joinChannel(const QString& name, const QString& password, bool internet, bool dht, const QString& bootstrapNodes);
    Q_INVOKABLE bool leaveChannel(const QString& name);
    Q_INVOKABLE bool sendChat(const QString& text);
    Q_INVOKABLE bool startPtt();
    Q_INVOKABLE bool stopPtt();
    Q_INVOKABLE bool setMute(bool muted);

    RiftEventsModel* eventsModel();
    RiftPeersModel* peersModel();
    QString status() const;

signals:
    void chatMessageReceived(const QString& from, const QString& text, const QDateTime& timestamp);
    void connectionStatusChanged(const QString& status);
    void errorOccurred(const QString& message);
    void statusChanged();

private:
    void ensureHandle();
    void shutdownHandle();
    void startEventLoop();
    void stopEventLoop();
    QString configPathFor(bool dht, const QString& bootstrapNodes) const;
    void writeConfig(bool dht, const QString& bootstrapNodes, const QString& path) const;
    static QString peerIdToHex(const PeerId& peer);

    RiftHandle* m_handle = nullptr;
    std::atomic<bool> m_running{false};
    std::thread m_eventThread;
    QString m_status;

    RiftEventsModel m_events;
    RiftPeersModel m_peers;
    QTimer m_speakingTimer;
    QHash<QString, QDateTime> m_lastSpoke;
    QSettings m_settings;
};
