#include <QApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QIcon>

#include "RiftBackend.h"
#include "CallManager.h"
#include "NotificationManager.h"

int main(int argc, char* argv[])
{
    QApplication app(argc, argv);
    app.setApplicationName("Rift");
    app.setApplicationVersion("0.1.4");
    app.setOrganizationName("Rift");
    app.setOrganizationDomain("rift.io");
    app.setWindowIcon(QIcon::fromTheme("audio-headphones"));

    // Create backend and managers
    RiftBackend backend;
    CallManager callManager(&backend);
    NotificationManager notificationManager;

    // Initialize the SDK
    backend.init();

    // Connect notifications
    QObject::connect(&callManager, &CallManager::hasIncomingCallChanged, [&]() {
        if (callManager.hasIncomingCall()) {
            notificationManager.showIncomingCall(callManager.incomingPeerId());
        } else {
            notificationManager.hideIncomingCall();
        }
    });

    QObject::connect(&backend, &RiftBackend::connectedChanged, [&]() {
        if (backend.connected()) {
            notificationManager.setTrayTooltip("Rift - Connected");
        } else {
            notificationManager.setTrayTooltip("Rift - Disconnected");
        }
    });

    // Setup QML engine
    QQmlApplicationEngine engine;

    // Expose objects to QML
    engine.rootContext()->setContextProperty("backend", &backend);
    engine.rootContext()->setContextProperty("callManager", &callManager);
    engine.rootContext()->setContextProperty("notifications", &notificationManager);

    // Load main QML
    const QUrl url(QStringLiteral("qrc:/Rift/qml/Main.qml"));
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreated,
                     &app, [url](QObject* obj, const QUrl& objUrl) {
        if (!obj && url == objUrl) {
            QCoreApplication::exit(-1);
        }
    }, Qt::QueuedConnection);

    engine.load(url);

    // Show window when tray is clicked
    QObject::connect(&notificationManager, &NotificationManager::trayActivated, [&engine]() {
        if (auto* root = engine.rootObjects().value(0)) {
            QMetaObject::invokeMethod(root, "showNormal");
            QMetaObject::invokeMethod(root, "raise");
            QMetaObject::invokeMethod(root, "requestActivate");
        }
    });

    return app.exec();
}
