#include "CallManager.h"
#include "RiftBackend.h"

CallManager::CallManager(RiftBackend* backend, QObject* parent)
    : QObject(parent)
    , m_backend(backend)
{
    connect(m_backend, &RiftBackend::incomingCall,
            this, &CallManager::onIncomingCall);
    connect(m_backend, &RiftBackend::callStateChanged,
            this, &CallManager::onCallStateChanged);
}

bool CallManager::startCall(const QString& peerId)
{
    if (m_inCall) {
        return false;
    }

    QString sessionId = m_backend->startCall(peerId);
    if (sessionId.isEmpty()) {
        return false;
    }

    m_currentSessionId = sessionId;
    m_currentPeerId = peerId;
    m_callState = "connecting";
    m_inCall = true;

    emit currentSessionIdChanged();
    emit currentPeerIdChanged();
    emit callStateChanged();
    emit inCallChanged();

    return true;
}

bool CallManager::acceptCall()
{
    if (!m_hasIncomingCall) {
        return false;
    }

    if (!m_backend->acceptCall(m_incomingSessionId)) {
        return false;
    }

    m_currentSessionId = m_incomingSessionId;
    m_currentPeerId = m_incomingPeerId;
    m_callState = "active";
    m_inCall = true;

    m_hasIncomingCall = false;
    m_incomingSessionId.clear();
    m_incomingPeerId.clear();

    emit currentSessionIdChanged();
    emit currentPeerIdChanged();
    emit callStateChanged();
    emit inCallChanged();
    emit hasIncomingCallChanged();
    emit incomingSessionIdChanged();
    emit incomingPeerIdChanged();

    return true;
}

bool CallManager::declineCall(const QString& reason)
{
    if (!m_hasIncomingCall) {
        return false;
    }

    m_backend->declineCall(m_incomingSessionId, reason);

    m_hasIncomingCall = false;
    m_incomingSessionId.clear();
    m_incomingPeerId.clear();

    emit hasIncomingCallChanged();
    emit incomingSessionIdChanged();
    emit incomingPeerIdChanged();

    return true;
}

bool CallManager::endCall()
{
    if (!m_inCall) {
        return false;
    }

    m_backend->endCall(m_currentSessionId);

    m_inCall = false;
    m_currentSessionId.clear();
    m_currentPeerId.clear();
    m_callState.clear();

    emit inCallChanged();
    emit currentSessionIdChanged();
    emit currentPeerIdChanged();
    emit callStateChanged();
    emit callEnded();

    return true;
}

void CallManager::onIncomingCall(const QString& sessionId, const QString& fromPeer)
{
    if (m_inCall) {
        // Auto-decline if already in a call
        m_backend->declineCall(sessionId, "busy");
        return;
    }

    m_hasIncomingCall = true;
    m_incomingSessionId = sessionId;
    m_incomingPeerId = fromPeer;

    emit hasIncomingCallChanged();
    emit incomingSessionIdChanged();
    emit incomingPeerIdChanged();
}

void CallManager::onCallStateChanged(const QString& sessionId, const QString& state)
{
    if (m_currentSessionId != sessionId) {
        return;
    }

    if (state == "ended" || state == "terminated" || state == "failed") {
        m_inCall = false;
        m_currentSessionId.clear();
        m_currentPeerId.clear();
        m_callState.clear();

        emit inCallChanged();
        emit currentSessionIdChanged();
        emit currentPeerIdChanged();
        emit callStateChanged();
        emit callEnded();
    } else {
        m_callState = state;
        emit callStateChanged();
    }
}
