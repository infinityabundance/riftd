#pragma once

#include <QAbstractListModel>
#include <QString>
#include <QVector>

struct RiftPeerInfo {
    QString id;
    QString name;
    bool speaking = false;
    QString route = "direct";
    QString quality = "unknown";
    float rttMs = 0.0f;
    float loss = 0.0f;
};

class RiftPeersModel : public QAbstractListModel {
    Q_OBJECT

public:
    enum Roles {
        IdRole = Qt::UserRole + 1,
        NameRole,
        SpeakingRole,
        RouteRole,
        QualityRole,
        RttRole,
        LossRole
    };

    explicit RiftPeersModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void upsertPeer(const RiftPeerInfo& peer);
    void removePeer(const QString& id);
    void setSpeaking(const QString& id, bool speaking);
    void setQuality(const QString& id, const QString& quality, float rttMs = 0.0f, float loss = 0.0f);

private:
    int indexOf(const QString& id) const;
    QVector<RiftPeerInfo> m_peers;
};
