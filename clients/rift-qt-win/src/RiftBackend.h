#pragma once

#include <QObject>
#include <QDateTime>
#include <QSettings>
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

public:
    explicit RiftBackend(QObject* parent = nullptr);
    ~RiftBackend() override;

    bool init(const QString& configPath);
    bool joinChannel(const QString& name, const QString& password, bool internet, bool dht);
    bool leaveChannel(const QString& name);
    bool sendChat(const QString& text);
    bool startPtt();
    bool stopPtt();
    bool setMute(bool muted);
    void setBootstrapNodes(const QString& nodes);

    RiftEventsModel* eventsModel();
    RiftPeersModel* peersModel();

    QString status() const;

signals:
    void statusChanged(const QString& status);
    void errorOccurred(const QString& message);

private:
    void startEventLoop();
    void stopEventLoop();
    QString configPathFor() const;
    void writeConfig(const QString& path, bool dht, const QString& bootstrapNodes) const;
    QString peerIdToHex(const PeerId& peer) const;

    RiftHandle* m_handle = nullptr;
    std::atomic<bool> m_running{false};
    std::thread m_eventThread;

    RiftEventsModel m_events;
    RiftPeersModel m_peers;
    QHash<QString, QDateTime> m_lastSpoke;
    QTimer m_speakingTimer;
    QSettings m_settings;
    QString m_status;
};
