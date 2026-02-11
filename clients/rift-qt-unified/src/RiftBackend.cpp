#include "RiftBackend.h"
#include <QDateTime>
#include <QDebug>

RiftBackend::RiftBackend(QObject* parent)
    : QObject(parent)
{
    m_status = "disconnected";
    connect(&m_pollTimer, &QTimer::timeout, this, &RiftBackend::pollEvents);
}

RiftBackend::~RiftBackend()
{
    m_pollTimer.stop();
    if (m_handle) {
        rift_free(m_handle);
        m_handle = nullptr;
    }
}

QString RiftBackend::sdkVersion() const
{
    return QString::fromUtf8(rift_sdk_version());
}

bool RiftBackend::init(const QString& configPath)
{
    if (m_handle) {
        return true;
    }

    RiftErrorCode error = RiftErrorCode_Ok;
    const char* path = configPath.isEmpty() ? nullptr : configPath.toUtf8().constData();
    m_handle = rift_init(path, &error);

    if (!m_handle || error != RiftErrorCode_Ok) {
        setStatus("init failed");
        return false;
    }

    m_pollTimer.start(50); // Poll at 20Hz
    setStatus("initialized");
    return true;
}

bool RiftBackend::joinChannel(const QString& name, const QString& password, bool internet)
{
    if (!m_handle) {
        return false;
    }

    const char* pwd = password.isEmpty() ? nullptr : password.toUtf8().constData();
    int result = rift_join_channel(m_handle, name.toUtf8().constData(), pwd, internet ? 1 : 0);

    if (result == 0) {
        setConnected(true);
        setStatus("connected to " + name);
        return true;
    }

    setStatus("join failed");
    return false;
}

bool RiftBackend::leaveChannel(const QString& name)
{
    if (!m_handle) {
        return false;
    }

    int result = rift_leave_channel(m_handle, name.toUtf8().constData());
    setConnected(false);
    m_peers.clear();
    emit peersChanged();
    setStatus("disconnected");
    return result == 0;
}

bool RiftBackend::sendChat(const QString& text)
{
    if (!m_handle || text.isEmpty()) {
        return false;
    }

    int result = rift_send_chat(m_handle, text.toUtf8().constData());
    if (result == 0) {
        QVariantMap msg;
        msg["time"] = QDateTime::currentDateTime().toString("HH:mm:ss");
        msg["from"] = "me";
        msg["text"] = text;
        m_messages.append(msg);
        emit messagesChanged();
        return true;
    }
    return false;
}

void RiftBackend::startPtt()
{
    if (!m_handle) return;
    rift_start_ptt(m_handle);
    m_pttActive = true;
    emit pttActiveChanged();
}

void RiftBackend::stopPtt()
{
    if (!m_handle) return;
    rift_stop_ptt(m_handle);
    m_pttActive = false;
    emit pttActiveChanged();
}

void RiftBackend::setMuted(bool muted)
{
    if (!m_handle || m_muted == muted) return;
    rift_set_mute(m_handle, muted ? 1 : 0);
    m_muted = muted;
    emit mutedChanged();
}

QString RiftBackend::startCall(const QString& peerHex)
{
    if (!m_handle) return QString();

    PeerIdC peer = hexToPeerId(peerHex);
    SessionIdC session = rift_start_call(m_handle, &peer);

    // Check if session is all zeros (failure)
    bool allZero = true;
    for (int i = 0; i < 32; ++i) {
        if (session.bytes[i] != 0) {
            allZero = false;
            break;
        }
    }

    if (allZero) {
        return QString();
    }

    return sessionIdToHex(session);
}

bool RiftBackend::acceptCall(const QString& sessionHex)
{
    if (!m_handle) return false;
    SessionIdC session = hexToSessionId(sessionHex);
    return rift_accept_call(m_handle, session) == 0;
}

bool RiftBackend::declineCall(const QString& sessionHex, const QString& reason)
{
    if (!m_handle) return false;
    SessionIdC session = hexToSessionId(sessionHex);
    const char* r = reason.isEmpty() ? nullptr : reason.toUtf8().constData();
    return rift_decline_call(m_handle, session, r) == 0;
}

bool RiftBackend::endCall(const QString& sessionHex)
{
    if (!m_handle) return false;
    SessionIdC session = hexToSessionId(sessionHex);
    return rift_end_call(m_handle, session) == 0;
}

void RiftBackend::pollEvents()
{
    if (!m_handle) return;

    RiftEventC event;
    while (rift_next_event(m_handle, &event) == 1) {
        switch (event.tag) {
        case RiftEventTag_IncomingChat: {
            QVariantMap msg;
            msg["time"] = QDateTime::currentDateTime().toString("HH:mm:ss");
            msg["from"] = peerIdToHex(event.peer).left(12);
            msg["text"] = QString::fromUtf8(event.text);
            m_messages.append(msg);
            emit messagesChanged();
            break;
        }
        case RiftEventTag_IncomingCall: {
            QString sessionId = sessionIdToHex(event.session);
            QString fromPeer = peerIdToHex(event.peer);
            emit incomingCall(sessionId, fromPeer);
            break;
        }
        case RiftEventTag_CallStateChanged: {
            QString sessionId = sessionIdToHex(event.session);
            emit callStateChanged(sessionId, "changed");
            break;
        }
        case RiftEventTag_PeerJoined: {
            QString peerId = peerIdToHex(event.peer);
            bool found = false;
            for (const auto& p : m_peers) {
                if (p.toMap()["id"].toString() == peerId) {
                    found = true;
                    break;
                }
            }
            if (!found) {
                QVariantMap peer;
                peer["id"] = peerId;
                m_peers.append(peer);
                emit peersChanged();
            }
            break;
        }
        case RiftEventTag_PeerLeft: {
            QString peerId = peerIdToHex(event.peer);
            for (int i = 0; i < m_peers.size(); ++i) {
                if (m_peers[i].toMap()["id"].toString() == peerId) {
                    m_peers.removeAt(i);
                    emit peersChanged();
                    break;
                }
            }
            break;
        }
        case RiftEventTag_AudioLevel: {
            QString peerId = peerIdToHex(event.peer);
            emit audioLevel(peerId, event.level);
            break;
        }
        default:
            break;
        }

        rift_event_free(&event);
    }
}

void RiftBackend::setStatus(const QString& status)
{
    if (m_status != status) {
        m_status = status;
        emit statusChanged();
    }
}

void RiftBackend::setConnected(bool connected)
{
    if (m_connected != connected) {
        m_connected = connected;
        emit connectedChanged();
    }
}

QString RiftBackend::peerIdToHex(const PeerIdC& peer)
{
    QString hex;
    for (int i = 0; i < 32; ++i) {
        hex += QString::asprintf("%02x", peer.bytes[i]);
    }
    return hex;
}

QString RiftBackend::sessionIdToHex(const SessionIdC& session)
{
    QString hex;
    for (int i = 0; i < 32; ++i) {
        hex += QString::asprintf("%02x", session.bytes[i]);
    }
    return hex;
}

PeerIdC RiftBackend::hexToPeerId(const QString& hex)
{
    PeerIdC peer = {};
    QByteArray bytes = QByteArray::fromHex(hex.toUtf8());
    if (bytes.size() >= 32) {
        memcpy(peer.bytes, bytes.constData(), 32);
    }
    return peer;
}

SessionIdC RiftBackend::hexToSessionId(const QString& hex)
{
    SessionIdC session = {};
    QByteArray bytes = QByteArray::fromHex(hex.toUtf8());
    if (bytes.size() >= 32) {
        memcpy(session.bytes, bytes.constData(), 32);
    }
    return session;
}
