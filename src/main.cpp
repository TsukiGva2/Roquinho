#include "backend.h"

#include <QGuiApplication>
#include <QLocalSocket>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QTimer>

#include <LayerShellQt/Window>

// Uso:
//   foco-max                 inicia (ou abre o painel da instância que já roda)
//   foco-max add [texto]     nova demanda (sem texto abre o campo de digitação)
//   foco-max focus <texto>   cria a demanda direto em foco
//   foco-max toggle|hide|show|done|quit
int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);
    app.setApplicationName(QStringLiteral("foco-max"));
    app.setApplicationDisplayName(QStringLiteral("Foquinho"));
    app.setDesktopFileName(QStringLiteral("foco-max"));
    app.setQuitOnLastWindowClosed(false);

    QStringList args = app.arguments().mid(1);
    const QString commandLine = args.join(QLatin1Char(' ')).trimmed();

    // Instância única: se já existe uma rodando, só repassa o comando.
    {
        QLocalSocket probe;
        probe.connectToServer(Backend::serverName());
        if (probe.waitForConnected(250)) {
            const QString line = commandLine.isEmpty() ? QStringLiteral("toggle") : commandLine;
            probe.write(line.toUtf8() + '\n');
            probe.waitForBytesWritten(500);
            probe.disconnectFromServer();
            return 0;
        }
    }

    Backend backend;
    backend.startServer();

    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty(QStringLiteral("Backend"), &backend);
    engine.load(QUrl(QStringLiteral("qrc:/qml/Main.qml")));

    auto *window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
    if (!window)
        return 1;

    window->setColor(Qt::transparent);
    if (auto *layer = LayerShellQt::Window::get(window)) {
        layer->setScope(QStringLiteral("foco-max"));
        layer->setLayer(LayerShellQt::Window::LayerOverlay); // acima de tudo, até de tela cheia
        layer->setAnchors(LayerShellQt::Window::AnchorTop); // centralizado no topo
        layer->setExclusiveZone(-1); // sobrepõe a barra superior em vez de ficar abaixo dela
        layer->setKeyboardInteractivity(LayerShellQt::Window::KeyboardInteractivityOnDemand);
        layer->setActivateOnShow(false);
    }
    backend.attachWindow(window);
    window->show();

    if (!commandLine.isEmpty()) {
        QTimer::singleShot(700, &backend, [&backend, commandLine] {
            const int space = commandLine.indexOf(QLatin1Char(' '));
            emit backend.command(space < 0 ? commandLine : commandLine.left(space),
                                 space < 0 ? QString() : commandLine.mid(space + 1));
        });
    }

    return app.exec();
}
