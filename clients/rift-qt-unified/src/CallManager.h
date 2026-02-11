#pragma once

#include <QObject>
#include <QString>

class RiftBackend;

class CallManager : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool inCall READ inCall NOTIFY inCallChanged)
    Q_PROPERTY(QString currentSessionId READ currentSessionId NOTIFY currentSessionIdChanged)
    Q_PROPERTY(QString currentPeerId READ currentPeerId NOTIFY currentPeerIdChanged)
    Q_PROPERTY(QString callState READ callState NOTIFY callStateChanged)
    Q_PROPERTY(bool hasIncomingCall READ hasIncomingCall NOTIFY hasIncomingCallChanged)
    Q_PROPERTY(QString incomingSessionId READ incomingSessionId NOTIFY incomingSessionIdChanged)
    Q_PROPERTY(QString incomingPeerId READ incomingPeerId NOTIFY incomingPeerIdChanged)

public:
    explicit CallManager(RiftBackend* backend, QObject* parent = nullptr);

    bool inCall() const { return m_inCall; }
    QString currentSessionId() const { return m_currentSessionId; }
    QString currentPeerId() const { return m_currentPeerId; }
    QString callState() const { return m_callState; }
    bool hasIncomingCall() const { return m_hasIncomingCall; }
    QString incomingSessionId() const { return m_incomingSessionId; }
    QString incomingPeerId() const { return m_incomingPeerId; }

    Q_INVOKABLE bool startCall(const QString& peerId);
    Q_INVOKABLE bool acceptCall();
    Q_INVOKABLE bool declineCall(const QString& reason = QString());
    Q_INVOKABLE bool endCall();

signals:
    void inCallChanged();
    void currentSessionIdChanged();
    void currentPeerIdChanged();
    void callStateChanged();
    void hasIncomingCallChanged();
    void incomingSessionIdChanged();
    void incomingPeerIdChanged();
    void callEnded();

private slots:
    void onIncomingCall(const QString& sessionId, const QString& fromPeer);
    void onCallStateChanged(const QString& sessionId, const QString& state);

private:
    RiftBackend* m_backend;
    bool m_inCall = false;
    QString m_currentSessionId;
    QString m_currentPeerId;
    QString m_callState;
    bool m_hasIncomingCall = false;
    QString m_incomingSessionId;
    QString m_incomingPeerId;
};
