#include <QGuiApplication>
#include <QIcon>

extern "C" void set_fluffinstall_window_icon()
{
    // Set the icon before QML creates the first application window.
    QGuiApplication::setWindowIcon(
        QIcon(QStringLiteral("/usr/share/pixmaps/flufflinux-logo.svg")));
}
