#include <QGuiApplication>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQmlContext>

#if __has_include(<KLocalizedContext>)
#include <KLocalizedContext>
#define FLUFFINSTALL_HAS_KI18N 1
#else
#define FLUFFINSTALL_HAS_KI18N 0
#endif

extern "C" void set_fluffinstall_window_icon()
{
    // Set the icon before QML creates the first application window.
    QGuiApplication::setWindowIcon(
        QIcon(QStringLiteral("/usr/share/pixmaps/flufflinux-logo.svg")));
}

extern "C" void configure_fluffinstall_qml_context(void *enginePointer)
{
#if FLUFFINSTALL_HAS_KI18N
    auto *engine = static_cast<QQmlApplicationEngine *>(enginePointer);
    if (engine != nullptr) {
        engine->rootContext()->setContextObject(new KLocalizedContext(engine));
    }
#else
    Q_UNUSED(enginePointer);
#endif
}
