#include "RiftBackend.h"

#include <QStandardPaths>
#include <QDir>
#include <QFile>
#include <QTextStream>
#include <QThread>

RiftBackend::RiftBackend(QObject* parent)
    : QObject(parent),
      m_settings("rift", "rift-qt") {
    m_status = "disconnected";
    m_speakingTimer.setInterval(250);
    m_speakingTimer.setSingleShot(false);
    connect(&m_speakingTimer, &QTimer::timeout, this, [this]() {
        const auto now = QDateTime::currentDateTimeUtc();
        const auto peers = m_peers.peers();
        for (const auto& peer : peers) {
            const auto last = m_lastSpoke.value(peer.id);
            if (last.isValid() && last.msecsTo(now) > 800) {
                m_peers.setSpeaking(peer.id, false);
            }
        }
    });
    m_speakingTimer.start();
}

RiftBackend::~RiftBackend() {
    stopEventLoop();
    shutdownHandle();
}

RiftEventsModel* RiftBackend::eventsModel() {
    return &m_events;
}

RiftPeersModel* RiftBackend::peersModel() {
    return &m_peers;
}

QString RiftBackend::status() const {
    return m_status;
}

bool RiftBackend::init() {
    ensureHandle();
    return m_handle != nullptr;
}

void RiftBackend::ensureHandle() {
    if (m_handle) {
        return;
    }
    RiftErrorCode err = RIFT_OK;
    QString configPath = configPathFor(false, QString());
    QByteArray pathBytes = configPath.toUtf8();
    m_handle = rift_init(pathBytes.constData(), &err);
    if (!m_handle) {
        m_status = "init failed";
        emit statusChanged();
        emit errorOccurred(QString("init failed: %1").arg(err));
    }
}

void RiftBackend::shutdownHandle() {
    if (m_handle) {
        rift_free(m_handle);
        m_handle = nullptr;
    }
}

bool RiftBackend::joinChannel(const QString& name, const QString& password, bool internet, bool dht, const QString& bootstrapNodes) {
    if (name.trimmed().isEmpty()) {
        emit errorOccurred("channel is required");
        return false;
    }
    m_settings.setValue("network/bootstrap", bootstrapNodes);
    const QString configPath = configPathFor(dht, bootstrapNodes);
    writeConfig(dht, bootstrapNodes, configPath);
    shutdownHandle();
    ensureHandle();
    if (!m_handle) {
        return false;
    }

    QByteArray nameBytes = name.toUtf8();
    QByteArray passBytes = password.toUtf8();
    const char* passPtr = password.isEmpty() ? nullptr : passBytes.constData();
    int result = rift_join_channel(m_handle, nameBytes.constData(), passPtr, internet ? 1 : 0);
    if (result != 0) {
        m_status = "connect failed";
        emit statusChanged();
        emit errorOccurred("connect failed");
        return false;
    }
    m_status = "connected";
    emit statusChanged();
    startEventLoop();
    return true;
}

bool RiftBackend::leaveChannel(const QString& name) {
    if (!m_handle) {
        return false;
    }
    QByteArray nameBytes = name.toUtf8();
    rift_leave_channel(m_handle, nameBytes.constData());
    stopEventLoop();
    m_status = "disconnected";
    emit statusChanged();
    return true;
}

bool RiftBackend::sendChat(const QString& text) {
    if (!m_handle || text.trimmed().isEmpty()) {
        return false;
    }
    QByteArray bytes = text.toUtf8();
    int result = rift_send_chat(m_handle, bytes.constData());
    if (result != 0) {
        emit errorOccurred("send failed");
        return false;
    }
    m_events.addMessage("me", text, QDateTime::currentDateTimeUtc(), "chat");
    return true;
}

bool RiftBackend::startPtt() {
    if (!m_handle) {
        return false;
    }
    return rift_start_ptt(m_handle) == 0;
}

bool RiftBackend::stopPtt() {
    if (!m_handle) {
        return false;
    }
    return rift_stop_ptt(m_handle) == 0;
}

bool RiftBackend::setMute(bool muted) {
    if (!m_handle) {
        return false;
    }
    return rift_set_mute(m_handle, muted ? 1 : 0) == 0;
}

void RiftBackend::startEventLoop() {
    if (m_running.load()) {
        return;
    }
    m_running.store(true);
    m_eventThread = std::thread([this]() {
        RiftEvent evt{};
        while (m_running.load()) {
            const int rc = rift_next_event(m_handle, &evt);
            if (rc == 0 && evt.tag != RIFT_EVENT_NONE) {
                const QString peerId = peerIdToHex(evt.peer);
                if (evt.tag == RIFT_EVENT_INCOMING_CHAT) {
                    const QString text = evt.text ? QString::fromUtf8(evt.text) : QString();
                    const QDateTime ts = QDateTime::currentDateTimeUtc();
                    QMetaObject::invokeMethod(this, [this, peerId, text, ts]() {
                        m_events.addMessage(peerId, text, ts, "chat");
                    }, Qt::QueuedConnection);
                } else if (evt.tag == RIFT_EVENT_PEER_JOINED) {
                    QMetaObject::invokeMethod(this, [this, peerId]() {
                        RiftPeerInfo info;
                        info.id = peerId;
                        info.name = peerId.left(8);
                        m_peers.upsertPeer(info);
                    }, Qt::QueuedConnection);
                } else if (evt.tag == RIFT_EVENT_PEER_LEFT) {
                    QMetaObject::invokeMethod(this, [this, peerId]() {
                        m_peers.removePeer(peerId);
                    }, Qt::QueuedConnection);
                } else if (evt.tag == RIFT_EVENT_AUDIO_LEVEL) {
                    if (evt.level > 0.02f) {
                        QMetaObject::invokeMethod(this, [this, peerId]() {
                            m_lastSpoke[peerId] = QDateTime::currentDateTimeUtc();
                            m_peers.setSpeaking(peerId, true);
                        }, Qt::QueuedConnection);
                    }
                }
                rift_event_free(&evt);
            }
            QThread::msleep(30);
        }
    });
}

void RiftBackend::stopEventLoop() {
    if (!m_running.load()) {
        return;
    }
    m_running.store(false);
    if (m_eventThread.joinable()) {
        m_eventThread.join();
    }
}

QString RiftBackend::configPathFor(bool dht, const QString& bootstrapNodes) const {
    Q_UNUSED(dht);
    Q_UNUSED(bootstrapNodes);
    const auto baseDir = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    QDir().mkpath(baseDir);
    return baseDir + "/rift-qt.toml";
}

void RiftBackend::writeConfig(bool dht, const QString& bootstrapNodes, const QString& path) const {
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate | QIODevice::Text)) {
        return;
    }
    QTextStream out(&file);
    out << "listen_port = 0\n";
    out << "\n[dht]\n";
    out << "enabled = " << (dht ? "true" : "false") << "\n";
    out << "bootstrap_nodes = [";
    const auto parts = bootstrapNodes.split(',', Qt::SkipEmptyParts);
    for (int i = 0; i < parts.size(); ++i) {
        if (i > 0) {
            out << ", ";
        }
        out << "\"" << parts[i].trimmed() << "\"";
    }
    out << "]\n";
    out << "\n[audio]\n";
    out << "ptt = true\n";
    out << "vad = false\n";
    out << "allow_fail = true\n";
}

QString RiftBackend::peerIdToHex(const PeerId& peer) {
    static const char* hex = "0123456789abcdef";
    QString out;
    out.reserve(64);
    for (int i = 0; i < 32; ++i) {
        const uint8_t b = peer.bytes[i];
        out.append(hex[(b >> 4) & 0xF]);
        out.append(hex[b & 0xF]);
    }
    return out;
}
