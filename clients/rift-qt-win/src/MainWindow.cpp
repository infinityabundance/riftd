#include "MainWindow.h"

#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QStatusBar>
#include <QSettings>
#include <QKeyEvent>
#include <QMenu>
#include <QStandardPaths>
#include <QApplication>

MainWindow::MainWindow(QWidget* parent)
    : QMainWindow(parent) {
    setupUi();
    setupTray();
    connectSignals();

    const QString configPath = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation) + "/rift-qt-win.toml";
    m_backend.init(configPath);
}

MainWindow::~MainWindow() {
    if (m_tray) {
        m_tray->hide();
    }
}

void MainWindow::setupUi() {
    setWindowTitle("Rift");
    resize(1100, 700);

    auto* central = new QWidget(this);
    auto* mainLayout = new QVBoxLayout(central);

    auto* topRow = new QHBoxLayout();
    m_channelEdit = new QLineEdit();
    m_channelEdit->setPlaceholderText("Channel");
    m_channelEdit->setText("gaming");

    m_passwordEdit = new QLineEdit();
    m_passwordEdit->setPlaceholderText("Password (optional)");
    m_passwordEdit->setEchoMode(QLineEdit::Password);

    m_bootstrapEdit = new QLineEdit();
    m_bootstrapEdit->setPlaceholderText("DHT bootstrap (ip:port, comma)");

    m_internetCheck = new QCheckBox("Internet");
    m_internetCheck->setChecked(true);
    m_dhtCheck = new QCheckBox("DHT");
    m_dhtCheck->setChecked(true);

    m_connectButton = new QPushButton("Connect");
    m_disconnectButton = new QPushButton("Disconnect");

    topRow->addWidget(m_channelEdit);
    topRow->addWidget(m_passwordEdit);
    topRow->addWidget(m_bootstrapEdit);
    topRow->addWidget(m_internetCheck);
    topRow->addWidget(m_dhtCheck);
    topRow->addWidget(m_connectButton);
    topRow->addWidget(m_disconnectButton);

    mainLayout->addLayout(topRow);

    auto* splitter = new QSplitter(Qt::Horizontal);
    m_peerList = new QListView();
    m_peerList->setModel(m_backend.peersModel());
    splitter->addWidget(m_peerList);

    auto* chatPane = new QWidget();
    auto* chatLayout = new QVBoxLayout(chatPane);
    m_chatList = new QListView();
    m_chatList->setModel(m_backend.eventsModel());
    chatLayout->addWidget(m_chatList);

    auto* inputRow = new QHBoxLayout();
    m_inputEdit = new QLineEdit();
    m_inputEdit->setPlaceholderText("Type a message...");
    m_sendButton = new QPushButton("Send");
    inputRow->addWidget(m_inputEdit);
    inputRow->addWidget(m_sendButton);
    chatLayout->addLayout(inputRow);

    m_pttButton = new QPushButton("Push to Talk");
    m_pttButton->setMinimumHeight(48);
    chatLayout->addWidget(m_pttButton);

    splitter->addWidget(chatPane);
    splitter->setStretchFactor(0, 1);
    splitter->setStretchFactor(1, 3);

    mainLayout->addWidget(splitter);

    setCentralWidget(central);
    m_statusLabel = new QLabel("Disconnected");
    statusBar()->addWidget(m_statusLabel);

    m_inputEdit->installEventFilter(this);
    installEventFilter(this);

    QSettings settings("rift", "rift-qt-win");
    m_channelEdit->setText(settings.value("network/lastChannel", "gaming").toString());
    m_bootstrapEdit->setText(settings.value("network/bootstrap", "").toString());
    m_internetCheck->setChecked(settings.value("network/useInternet", true).toBool());
    m_dhtCheck->setChecked(settings.value("network/useDht", true).toBool());
}

void MainWindow::setupTray() {
    m_tray = new QSystemTrayIcon(this);
    m_tray->setIcon(QIcon::fromTheme("audio-headset"));

    auto* menu = new QMenu(this);
    auto* showAction = menu->addAction("Show");
    auto* hideAction = menu->addAction("Hide");
    m_muteAction = menu->addAction("Mute mic");
    auto* quitAction = menu->addAction("Quit");

    connect(showAction, &QAction::triggered, this, [this]() {
        showNormal();
        raise();
        activateWindow();
    });
    connect(hideAction, &QAction::triggered, this, [this]() {
        hide();
    });
    connect(m_muteAction, &QAction::triggered, this, [this]() {
        m_muted = !m_muted;
        m_backend.setMute(m_muted);
        m_muteAction->setText(m_muted ? "Unmute mic" : "Mute mic");
    });
    connect(quitAction, &QAction::triggered, this, []() {
        qApp->quit();
    });

    m_tray->setContextMenu(menu);
    m_tray->show();
}

void MainWindow::connectSignals() {
    connect(m_connectButton, &QPushButton::clicked, this, [this]() {
        m_backend.setBootstrapNodes(m_bootstrapEdit->text());
        m_backend.joinChannel(
            m_channelEdit->text(),
            m_passwordEdit->text(),
            m_internetCheck->isChecked(),
            m_dhtCheck->isChecked()
        );
    });
    connect(m_disconnectButton, &QPushButton::clicked, this, [this]() {
        m_backend.leaveChannel(m_channelEdit->text());
    });
    connect(m_sendButton, &QPushButton::clicked, this, [this]() {
        const QString text = m_inputEdit->text();
        if (!text.isEmpty()) {
            m_backend.sendChat(text);
            m_inputEdit->clear();
        }
    });
    connect(m_inputEdit, &QLineEdit::returnPressed, this, [this]() {
        const QString text = m_inputEdit->text();
        if (!text.isEmpty()) {
            m_backend.sendChat(text);
            m_inputEdit->clear();
        }
    });
    connect(m_pttButton, &QPushButton::pressed, this, [this]() {
        m_backend.startPtt();
    });
    connect(m_pttButton, &QPushButton::released, this, [this]() {
        m_backend.stopPtt();
    });
    connect(&m_backend, &RiftBackend::statusChanged, this, [this](const QString& status) {
        setStatus(status);
    });
}

void MainWindow::setStatus(const QString& text) {
    m_statusLabel->setText(text);
}

void MainWindow::closeEvent(QCloseEvent* event) {
    if (m_tray && m_tray->isVisible()) {
        hide();
        event->ignore();
        return;
    }
    QMainWindow::closeEvent(event);
}

bool MainWindow::eventFilter(QObject* obj, QEvent* event) {
    if (event->type() == QEvent::KeyPress || event->type() == QEvent::KeyRelease) {
        auto* keyEvent = static_cast<QKeyEvent*>(event);
        const bool isPress = event->type() == QEvent::KeyPress;
        const bool isRepeat = keyEvent->isAutoRepeat();
        if (isRepeat) {
            return false;
        }
        if (keyEvent->key() == Qt::Key_Space && obj != m_inputEdit) {
            if (isPress) {
                m_backend.startPtt();
            } else {
                m_backend.stopPtt();
            }
            return true;
        }
        if ((keyEvent->modifiers() & Qt::ControlModifier) && keyEvent->key() == Qt::Key_Space) {
            if (isPress) {
                m_backend.startPtt();
            } else {
                m_backend.stopPtt();
            }
            return true;
        }
    }
    return QMainWindow::eventFilter(obj, event);
}
