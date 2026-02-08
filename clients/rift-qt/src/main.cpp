#include <QApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QSystemTrayIcon>
#include <QMenu>

#include "RiftBackend.h"

int main(int argc, char* argv[]) {
    QApplication app(argc, argv);
    app.setApplicationName("Rift");
    app.setOrganizationName("rift");

    RiftBackend backend;
    backend.init();

    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("riftBackend", &backend);
    engine.rootContext()->setContextProperty("eventsModel", backend.eventsModel());
    engine.rootContext()->setContextProperty("peersModel", backend.peersModel());
    engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));
    if (engine.rootObjects().isEmpty()) {
        return 1;
    }

    QSystemTrayIcon tray;
    tray.setIcon(QIcon::fromTheme("audio-headset", QIcon::fromTheme("network-transmit-receive")));
    tray.setToolTip("Rift");

    QMenu menu;
    QAction* showAction = menu.addAction("Show");
    QAction* hideAction = menu.addAction("Hide");
    QAction* muteAction = menu.addAction("Mute mic");
    QAction* quitAction = menu.addAction("Quit");

    QObject::connect(showAction, &QAction::triggered, [&engine]() {
        if (!engine.rootObjects().isEmpty()) {
            engine.rootObjects().first()->setProperty("visible", true);
        }
    });
    QObject::connect(hideAction, &QAction::triggered, [&engine]() {
        if (!engine.rootObjects().isEmpty()) {
            engine.rootObjects().first()->setProperty("visible", false);
        }
    });
    QObject::connect(muteAction, &QAction::triggered, [&backend, muteAction]() {
        static bool muted = false;
        muted = !muted;
        backend.setMute(muted);
        muteAction->setText(muted ? "Unmute mic" : "Mute mic");
    });
    QObject::connect(quitAction, &QAction::triggered, &app, &QCoreApplication::quit);

    tray.setContextMenu(&menu);
    tray.show();

    return app.exec();
}
