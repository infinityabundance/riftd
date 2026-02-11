#include "NotificationManager.h"
#include <QApplication>
#include <QIcon>

NotificationManager::NotificationManager(QObject* parent)
    : QObject(parent)
{
    setupTrayIcon();
}

NotificationManager::~NotificationManager()
{
    delete m_trayMenu;
    delete m_trayIcon;
}

bool NotificationManager::systemTrayAvailable() const
{
    return QSystemTrayIcon::isSystemTrayAvailable();
}

void NotificationManager::setupTrayIcon()
{
    if (!QSystemTrayIcon::isSystemTrayAvailable()) {
        return;
    }

    m_trayIcon = new QSystemTrayIcon(this);
    m_trayIcon->setIcon(QIcon::fromTheme("audio-headphones", QIcon(":/icons/rift.png")));
    m_trayIcon->setToolTip("Rift");

    m_trayMenu = new QMenu();
    m_trayMenu->addAction("Show", this, &NotificationManager::trayActivated);
    m_trayMenu->addSeparator();
    m_trayMenu->addAction("Quit", qApp, &QApplication::quit);

    m_trayIcon->setContextMenu(m_trayMenu);
    m_trayIcon->show();

    connect(m_trayIcon, &QSystemTrayIcon::activated,
            this, [this](QSystemTrayIcon::ActivationReason reason) {
        if (reason == QSystemTrayIcon::Trigger ||
            reason == QSystemTrayIcon::DoubleClick) {
            emit trayActivated();
        }
    });
}

void NotificationManager::showNotification(const QString& title, const QString& message)
{
    if (m_trayIcon && m_trayIcon->supportsMessages()) {
        m_trayIcon->showMessage(title, message, QSystemTrayIcon::Information, 5000);
    }
}

void NotificationManager::showIncomingCall(const QString& fromPeer)
{
    if (m_trayIcon && m_trayIcon->supportsMessages()) {
        QString displayPeer = fromPeer.left(12) + "...";
        m_trayIcon->showMessage(
            "Incoming Call",
            QString("Call from %1").arg(displayPeer),
            QSystemTrayIcon::Information,
            30000
        );
    }
}

void NotificationManager::hideIncomingCall()
{
    // System tray notifications auto-hide, no action needed
}

void NotificationManager::setTrayTooltip(const QString& tooltip)
{
    if (m_trayIcon) {
        m_trayIcon->setToolTip(tooltip);
    }
}
