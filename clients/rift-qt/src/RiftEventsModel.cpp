#include "RiftEventsModel.h"

RiftEventsModel::RiftEventsModel(QObject* parent)
    : QAbstractListModel(parent) {}

int RiftEventsModel::rowCount(const QModelIndex& parent) const {
    if (parent.isValid()) {
        return 0;
    }
    return m_events.size();
}

QVariant RiftEventsModel::data(const QModelIndex& index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_events.size()) {
        return {};
    }
    const auto& evt = m_events.at(index.row());
    switch (role) {
    case FromRole:
        return evt.from;
    case TextRole:
        return evt.text;
    case TimestampRole:
        return evt.timestamp;
    case TypeRole:
        return evt.type;
    default:
        return {};
    }
}

QHash<int, QByteArray> RiftEventsModel::roleNames() const {
    return {
        {FromRole, "from"},
        {TextRole, "text"},
        {TimestampRole, "timestamp"},
        {TypeRole, "type"},
    };
}

void RiftEventsModel::addMessage(const QString& from, const QString& text, const QDateTime& timestamp, const QString& type) {
    const int row = m_events.size();
    beginInsertRows(QModelIndex(), row, row);
    m_events.push_back({from, text, timestamp, type});
    endInsertRows();
}

void RiftEventsModel::clear() {
    beginResetModel();
    m_events.clear();
    endResetModel();
}
