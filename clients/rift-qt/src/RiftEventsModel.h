#pragma once

#include <QAbstractListModel>
#include <QDateTime>
#include <QString>
#include <QVector>

struct RiftChatEvent {
    QString from;
    QString text;
    QDateTime timestamp;
    QString type;
};

class RiftEventsModel : public QAbstractListModel {
    Q_OBJECT

public:
    enum Roles {
        FromRole = Qt::UserRole + 1,
        TextRole,
        TimestampRole,
        TypeRole
    };

    explicit RiftEventsModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    Q_INVOKABLE void addMessage(const QString& from, const QString& text, const QDateTime& timestamp, const QString& type = "chat");
    Q_INVOKABLE void clear();

private:
    QVector<RiftChatEvent> m_events;
};
