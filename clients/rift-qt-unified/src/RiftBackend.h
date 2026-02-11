#pragma once

#include <QObject>
#include <QString>
#include <QTimer>
#include <QVariantList>
#include <memory>

// C FFI types from rift-sdk
extern "C" {
    struct RiftHandleC;

    struct PeerIdC {
        uint8_t bytes[32];
    };

    struct SessionIdC {
        uint8_t bytes[32];
    };

    enum RiftEventTag {
        RiftEventTag_None = 0,
        RiftEventTag_IncomingChat = 1,
        RiftEventTag_IncomingCall = 2,
        RiftEventTag_CallStateChanged = 3,
        RiftEventTag_PeerJoined = 4,
        RiftEventTag_PeerLeft = 5,
        RiftEventTag_AudioLevel = 6,
    };

    struct RiftEventC {
        RiftEventTag tag;
        PeerIdC peer;
        SessionIdC session;
        float level;
        char* text;
    };

    enum RiftErrorCode {
        RiftErrorCode_Ok = 0,
        RiftErrorCode_InvalidConfig = 1,
        RiftErrorCode_InitFailed = 2,
        RiftErrorCode_NotJoined = 3,
        RiftErrorCode_Other = 255,
    };

    RiftHandleC* rift_init(const char* config_path, RiftErrorCode* out_error);
    const char* rift_sdk_version();
    int rift_sdk_abi_version();
    void rift_free(RiftHandleC* handle);
    int rift_join_channel(RiftHandleC* handle, const char* name, const char* password, int internet);
    int rift_leave_channel(RiftHandleC* handle, const char* name);
    int rift_send_chat(RiftHandleC* handle, const char* text);
    int rift_start_ptt(RiftHandleC* handle);
    int rift_stop_ptt(RiftHandleC* handle);
    int rift_set_mute(RiftHandleC* handle, int muted);
    SessionIdC rift_start_call(RiftHandleC* handle, const PeerIdC* peer);
    int rift_accept_call(RiftHandleC* handle, SessionIdC session);
    int rift_decline_call(RiftHandleC* handle, SessionIdC session, const char* reason);
    int rift_end_call(RiftHandleC* handle, SessionIdC session);
    int rift_next_event(RiftHandleC* handle, RiftEventC* out_event);
    void rift_event_free(RiftEventC* event);
}

class RiftBackend : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString status READ status NOTIFY statusChanged)
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    Q_PROPERTY(QVariantList peers READ peers NOTIFY peersChanged)
    Q_PROPERTY(QVariantList messages READ messages NOTIFY messagesChanged)
    Q_PROPERTY(bool muted READ muted WRITE setMuted NOTIFY mutedChanged)
    Q_PROPERTY(bool pttActive READ pttActive NOTIFY pttActiveChanged)
    Q_PROPERTY(QString sdkVersion READ sdkVersion CONSTANT)

public:
    explicit RiftBackend(QObject* parent = nullptr);
    ~RiftBackend() override;

    QString status() const { return m_status; }
    bool connected() const { return m_connected; }
    QVariantList peers() const { return m_peers; }
    QVariantList messages() const { return m_messages; }
    bool muted() const { return m_muted; }
    bool pttActive() const { return m_pttActive; }
    QString sdkVersion() const;

    void setMuted(bool muted);

    Q_INVOKABLE bool init(const QString& configPath = QString());
    Q_INVOKABLE bool joinChannel(const QString& name, const QString& password = QString(), bool internet = true);
    Q_INVOKABLE bool leaveChannel(const QString& name);
    Q_INVOKABLE bool sendChat(const QString& text);
    Q_INVOKABLE void startPtt();
    Q_INVOKABLE void stopPtt();
    Q_INVOKABLE QString startCall(const QString& peerHex);
    Q_INVOKABLE bool acceptCall(const QString& sessionHex);
    Q_INVOKABLE bool declineCall(const QString& sessionHex, const QString& reason = QString());
    Q_INVOKABLE bool endCall(const QString& sessionHex);

signals:
    void statusChanged();
    void connectedChanged();
    void peersChanged();
    void messagesChanged();
    void mutedChanged();
    void pttActiveChanged();
    void incomingCall(const QString& sessionId, const QString& fromPeer);
    void callStateChanged(const QString& sessionId, const QString& state);
    void audioLevel(const QString& peerId, float level);

private slots:
    void pollEvents();

private:
    void setStatus(const QString& status);
    void setConnected(bool connected);
    static QString peerIdToHex(const PeerIdC& peer);
    static QString sessionIdToHex(const SessionIdC& session);
    static PeerIdC hexToPeerId(const QString& hex);
    static SessionIdC hexToSessionId(const QString& hex);

    RiftHandleC* m_handle = nullptr;
    QString m_status;
    bool m_connected = false;
    QVariantList m_peers;
    QVariantList m_messages;
    bool m_muted = false;
    bool m_pttActive = false;
    QTimer m_pollTimer;
};
