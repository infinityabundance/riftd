#include "RiftPeersModel.h"

RiftPeersModel::RiftPeersModel(QObject* parent)
    : QAbstractListModel(parent) {}

int RiftPeersModel::rowCount(const QModelIndex& parent) const {
    if (parent.isValid()) {
        return 0;
    }
    return m_peers.size();
}

QVariant RiftPeersModel::data(const QModelIndex& index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_peers.size()) {
        return {};
    }
    const auto& peer = m_peers.at(index.row());
    switch (role) {
    case Qt::DisplayRole:
        return QString("%1  [%2]").arg(peer.name, peer.quality);
    case IdRole:
        return peer.id;
    case NameRole:
        return peer.name;
    case SpeakingRole:
        return peer.speaking;
    case RouteRole:
        return peer.route;
    case QualityRole:
        return peer.quality;
    case RttRole:
        return peer.rttMs;
    case LossRole:
        return peer.loss;
    default:
        return {};
    }
}

QHash<int, QByteArray> RiftPeersModel::roleNames() const {
    return {
        {IdRole, "peerId"},
        {NameRole, "name"},
        {SpeakingRole, "speaking"},
        {RouteRole, "route"},
        {QualityRole, "quality"},
        {RttRole, "rttMs"},
        {LossRole, "loss"},
    };
}

int RiftPeersModel::indexOf(const QString& id) const {
    for (int i = 0; i < m_peers.size(); ++i) {
        if (m_peers[i].id == id) {
            return i;
        }
    }
    return -1;
}

void RiftPeersModel::upsertPeer(const RiftPeerInfo& peer) {
    int idx = indexOf(peer.id);
    if (idx == -1) {
        beginInsertRows(QModelIndex(), m_peers.size(), m_peers.size());
        m_peers.push_back(peer);
        endInsertRows();
    } else {
        m_peers[idx] = peer;
        const QModelIndex modelIndex = index(idx);
        emit dataChanged(modelIndex, modelIndex);
    }
}

void RiftPeersModel::removePeer(const QString& id) {
    int idx = indexOf(id);
    if (idx == -1) {
        return;
    }
    beginRemoveRows(QModelIndex(), idx, idx);
    m_peers.removeAt(idx);
    endRemoveRows();
}

void RiftPeersModel::setSpeaking(const QString& id, bool speaking) {
    int idx = indexOf(id);
    if (idx == -1) {
        return;
    }
    if (m_peers[idx].speaking == speaking) {
        return;
    }
    m_peers[idx].speaking = speaking;
    const QModelIndex modelIndex = index(idx);
    emit dataChanged(modelIndex, modelIndex, {SpeakingRole});
}

void RiftPeersModel::setQuality(const QString& id, const QString& quality, float rttMs, float loss) {
    int idx = indexOf(id);
    if (idx == -1) {
        return;
    }
    m_peers[idx].quality = quality;
    m_peers[idx].rttMs = rttMs;
    m_peers[idx].loss = loss;
    const QModelIndex modelIndex = index(idx);
    emit dataChanged(modelIndex, modelIndex, {QualityRole, RttRole, LossRole});
}
