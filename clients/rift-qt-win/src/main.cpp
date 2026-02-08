#include <QApplication>

#include "MainWindow.h"

int main(int argc, char* argv[]) {
    QApplication app(argc, argv);
    app.setApplicationName("Rift");
    app.setOrganizationName("rift");

    MainWindow window;
    window.show();

    return app.exec();
}
