#include "backend.h"

#include <QAction>
#include <algorithm>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QKeySequence>
#include <QLocalServer>
#include <QLocalSocket>
#include <QQuickWindow>
#include <QRegion>
#include <QSaveFile>
#include <QScreen>
#include <QSettings>
#include <QStandardPaths>
#include <QTextStream>

#include <KGlobalAccel>
#include <LayerShellQt/Window>

namespace {

QString configDir()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
}

QString kdeglobalsPath()
{
    return configDir() + QStringLiteral("/kdeglobals");
}

QColor parseColor(const QString &value)
{
    const QStringList parts = value.split(QLatin1Char(','));
    if (parts.size() >= 3)
        return QColor(parts[0].trimmed().toInt(), parts[1].trimmed().toInt(), parts[2].trimmed().toInt());
    return QColor(value.trimmed());
}

// Prefere o binário instalado (make install) ao da pasta de build.
QString installedBinary()
{
    const QString local = QDir::homePath() + QStringLiteral("/.local/bin/foco-max");
    if (QFile::exists(local))
        return local;
    const QString inPath = QStandardPaths::findExecutable(QStringLiteral("foco-max"));
    return inPath.isEmpty() ? QCoreApplication::applicationFilePath() : inPath;
}

} // namespace

Backend::Backend(QObject *parent)
    : QObject(parent)
{
    reloadAccent();

    // O KDE regrava o kdeglobals de forma atômica, então observamos também o diretório.
    m_accentDebounce.setSingleShot(true);
    m_accentDebounce.setInterval(300);
    connect(&m_accentDebounce, &QTimer::timeout, this, [this] {
        reloadAccent();
        if (QFile::exists(kdeglobalsPath()) && !m_watcher.files().contains(kdeglobalsPath()))
            m_watcher.addPath(kdeglobalsPath());
    });
    if (QFile::exists(kdeglobalsPath()))
        m_watcher.addPath(kdeglobalsPath());
    m_watcher.addPath(configDir());
    connect(&m_watcher, &QFileSystemWatcher::fileChanged, &m_accentDebounce, qOverload<>(&QTimer::start));
    connect(&m_watcher, &QFileSystemWatcher::directoryChanged, &m_accentDebounce, qOverload<>(&QTimer::start));

    setupShortcuts();

    m_screenName = QSettings(settingsPath(), QSettings::IniFormat).value(QStringLiteral("screen")).toString();

    // Monitor conectado/desconectado ou principal trocado: reposiciona o notch.
    // (Quando o monitor some, o compositor descarta a superfície; sem isso o notch desaparecia.)
    m_screenDebounce.setSingleShot(true);
    m_screenDebounce.setInterval(400);
    connect(&m_screenDebounce, &QTimer::timeout, this, [this] { placeWindow(true); });
    auto screensChangedNow = [this] {
        emit screensChanged();
        m_screenDebounce.start();
    };
    connect(qApp, &QGuiApplication::screenAdded, this, screensChangedNow);
    connect(qApp, &QGuiApplication::screenRemoved, this, screensChangedNow);
    connect(qApp, &QGuiApplication::primaryScreenChanged, this, screensChangedNow);
}

QString Backend::serverName()
{
    return QStringLiteral("foco-max-") + qEnvironmentVariable("USER", QStringLiteral("user"));
}

void Backend::attachWindow(QQuickWindow *window)
{
    m_window = window;
    if (auto *layer = LayerShellQt::Window::get(window))
        layer->setCloseOnDismissed(false); // nós mesmos remapeamos em outro monitor
    placeWindow(false);
}

QString Backend::settingsPath() const
{
    return configDir() + QStringLiteral("/foco-maxrc");
}

namespace {

QList<QScreen *> sortedScreens()
{
    QList<QScreen *> list = QGuiApplication::screens();
    std::sort(list.begin(), list.end(), [](QScreen *a, QScreen *b) {
        const QPoint pa = a->geometry().topLeft(), pb = b->geometry().topLeft();
        return pa.x() != pb.x() ? pa.x() < pb.x() : pa.y() < pb.y();
    });
    return list;
}

QString screenLabel(QScreen *s)
{
    if (s->name().startsWith(QLatin1String("eDP")) || s->name().startsWith(QLatin1String("LVDS")))
        return QStringLiteral("Notebook");
    const QString model = s->model().trimmed();
    return model.isEmpty() ? s->name() : model;
}

} // namespace

QVariantList Backend::screens() const
{
    QVariantList out;
    QScreen *primary = QGuiApplication::primaryScreen();
    for (QScreen *s : sortedScreens()) {
        out.append(QVariantMap{
            {QStringLiteral("name"), s->name()},
            {QStringLiteral("label"), screenLabel(s)},
            {QStringLiteral("primary"), s == primary},
        });
    }
    return out;
}

QScreen *Backend::targetScreen() const
{
    if (!m_screenName.isEmpty()) {
        for (QScreen *s : QGuiApplication::screens())
            if (s->name() == m_screenName)
                return s;
    }
    return QGuiApplication::primaryScreen();
}

void Backend::setScreenName(const QString &name)
{
    if (name == m_screenName)
        return;
    m_screenName = name;
    QSettings settings(settingsPath(), QSettings::IniFormat);
    settings.setValue(QStringLiteral("screen"), name);
    emit screenNameChanged();
    placeWindow(true);
}

void Backend::nextScreen()
{
    const QList<QScreen *> list = sortedScreens();
    if (list.size() < 2)
        return;
    const int i = list.indexOf(targetScreen());
    setScreenName(list.at((i + 1) % list.size())->name());
}

void Backend::placeWindow(bool remap)
{
    QScreen *screen = targetScreen();
    if (!m_window || !screen)
        return;
    auto *layer = LayerShellQt::Window::get(m_window);
    // A superfície layer-shell fica presa ao monitor em que foi criada:
    // para trocar, desfaz o mapeamento e mapeia de novo no destino.
    const bool wasVisible = m_window->isVisible();
    if (remap && wasVisible)
        m_window->hide();
    m_window->setScreen(screen);
    if (layer)
        layer->setScreen(screen);
    if (remap && wasVisible)
        m_window->show();
    if (m_currentScreen != screen->name()) {
        m_currentScreen = screen->name();
        emit currentScreenChanged();
    }
}

void Backend::runCommand(const QString &name, const QString &arg)
{
    if (name == QLatin1String("screen")) {
        if (arg.isEmpty() || arg == QLatin1String("next"))
            nextScreen();
        else if (arg == QLatin1String("primary") || arg == QLatin1String("principal"))
            setScreenName(QString());
        else
            setScreenName(arg);
        return;
    }
    emit command(name, arg);
}

bool Backend::startServer()
{
    QLocalServer::removeServer(serverName());
    m_server = new QLocalServer(this);
    m_server->setSocketOptions(QLocalServer::UserAccessOption);
    if (!m_server->listen(serverName()))
        return false;
    connect(m_server, &QLocalServer::newConnection, this, [this] {
        while (QLocalSocket *socket = m_server->nextPendingConnection())
            handleSocket(socket);
    });
    return true;
}

void Backend::handleSocket(QLocalSocket *socket)
{
    auto drain = [this, socket] {
        while (socket->canReadLine())
            dispatch(QString::fromUtf8(socket->readLine()).trimmed());
    };
    connect(socket, &QLocalSocket::readyRead, this, drain);
    connect(socket, &QLocalSocket::disconnected, this, [socket, drain] {
        drain();
        socket->deleteLater();
    });
    drain();
}

void Backend::dispatch(const QString &line)
{
    if (line.isEmpty())
        return;
    const int space = line.indexOf(QLatin1Char(' '));
    const QString name = space < 0 ? line : line.left(space);
    const QString arg = space < 0 ? QString() : line.mid(space + 1).trimmed();
    runCommand(name, arg);
}

void Backend::reloadAccent()
{
    QColor accent;
    QColor selection;
    QFile file(kdeglobalsPath());
    if (file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        QTextStream stream(&file);
        QString group;
        while (!stream.atEnd()) {
            const QString line = stream.readLine().trimmed();
            if (line.startsWith(QLatin1Char('['))) {
                group = line;
                continue;
            }
            if (group == QLatin1String("[General]") && line.startsWith(QLatin1String("AccentColor=")))
                accent = parseColor(line.mid(12));
            else if (group == QLatin1String("[Colors:Selection]") && line.startsWith(QLatin1String("BackgroundNormal=")))
                selection = parseColor(line.mid(17));
        }
    }
    if (!accent.isValid())
        accent = selection;
    if (!accent.isValid())
        accent = QColor(49, 91, 239); // azul padrão do WhiteSur
    if (accent != m_accent) {
        m_accent = accent;
        emit accentChanged();
    }
}

void Backend::setupShortcuts()
{
    struct Def {
        const char *id;
        const char *text;
        QKeySequence seq;
        const char *cmd;
    };
    const Def defs[] = {
        {"quick-add", "Nova demanda", QKeySequence(Qt::META | Qt::ALT | Qt::Key_N), "add"},
        {"toggle-panel", "Abrir/fechar painel", QKeySequence(Qt::META | Qt::ALT | Qt::Key_F), "toggle"},
        {"toggle-hide", "Esconder/mostrar (bolinha)", QKeySequence(Qt::META | Qt::ALT | Qt::Key_H), "hide"},
        {"complete", "Concluir demanda em foco", QKeySequence(Qt::META | Qt::ALT | Qt::Key_D), "done"},
        {"next-screen", "Mover para o próximo monitor", QKeySequence(Qt::META | Qt::ALT | Qt::Key_M), "screen"},
        // Meta+Alt+P é do Plasma (focar painéis); o id mudou para não herdar o atalho antigo salvo
        {"music-toggle", "Música: tocar / pausar", QKeySequence(Qt::META | Qt::ALT | Qt::Key_Space), "play"},
        {"music-next", "Música: próxima", QKeySequence(Qt::META | Qt::ALT | Qt::Key_Period), "next"},
        {"music-prev", "Música: anterior", QKeySequence(Qt::META | Qt::ALT | Qt::Key_Comma), "prev"},
    };
    for (const Def &d : defs) {
        auto *action = new QAction(QString::fromUtf8(d.text), this);
        action->setObjectName(QString::fromLatin1(d.id));
        action->setProperty("componentName", QStringLiteral("foco-max"));
        action->setProperty("componentDisplayName", QStringLiteral("Foquinho"));
        KGlobalAccel::self()->setDefaultShortcut(action, {d.seq});
        KGlobalAccel::self()->setShortcut(action, {d.seq});
        const QString cmd = QString::fromLatin1(d.cmd);
        connect(action, &QAction::triggered, this, [this, cmd] { runCommand(cmd, QString()); });
    }
    // remove do KDE atalhos de versões antigas que não existem mais (ex.: music-play)
    KGlobalAccel::cleanComponent(QStringLiteral("foco-max"));
}

QString Backend::statePath() const
{
    const QString dir = QStandardPaths::writableLocation(QStandardPaths::AppDataLocation);
    QDir().mkpath(dir);
    return dir + QStringLiteral("/state.json");
}

QString Backend::loadState() const
{
    QFile file(statePath());
    if (!file.open(QIODevice::ReadOnly))
        return QString();
    return QString::fromUtf8(file.readAll());
}

void Backend::saveState(const QString &json) const
{
    QSaveFile file(statePath());
    if (!file.open(QIODevice::WriteOnly))
        return;
    file.write(json.toUtf8());
    file.commit();
}

void Backend::setInputRect(qreal x, qreal y, qreal w, qreal h)
{
    if (!m_window)
        return;
    const QRect rect(qRound(x), qRound(y), qMax(1, qRound(w)), qMax(1, qRound(h)));
    m_window->setMask(QRegion(rect));
}

void Backend::setInputRegion(const QVariantList &rects)
{
    if (!m_window)
        return;
    QRegion region;
    for (const QVariant &v : rects) {
        const QRectF r = v.toRectF();
        if (r.width() >= 1 && r.height() >= 1)
            region += r.toAlignedRect();
    }
    if (region.isEmpty())
        region = QRegion(0, 0, 1, 1); // máscara vazia = janela inteira clicável; evita isso
    m_window->setMask(region);
}

void Backend::grabKeyboard(bool exclusive)
{
    if (!m_window)
        return;
    if (auto *layer = LayerShellQt::Window::get(m_window)) {
        layer->setKeyboardInteractivity(exclusive ? LayerShellQt::Window::KeyboardInteractivityExclusive
                                                  : LayerShellQt::Window::KeyboardInteractivityOnDemand);
    }
    if (exclusive)
        m_window->requestActivate();
}

QString Backend::autostartPath() const
{
    return configDir() + QStringLiteral("/autostart/foco-max.desktop");
}

bool Backend::autostart() const
{
    return QFile::exists(autostartPath());
}

void Backend::setAutostart(bool enabled)
{
    if (enabled == autostart())
        return;
    if (enabled) {
        QDir().mkpath(configDir() + QStringLiteral("/autostart"));
        QSaveFile file(autostartPath());
        if (file.open(QIODevice::WriteOnly)) {
            const QString content = QStringLiteral(
                                        "[Desktop Entry]\n"
                                        "Type=Application\n"
                                        "Name=Foquinho\n"
                                        "Comment=Notch de foco para o KDE\n"
                                        "Exec=%1\n"
                                        "Icon=foco-max\n"
                                        "X-KDE-autostart-phase=2\n")
                                        .arg(installedBinary());
            file.write(content.toUtf8());
            file.commit();
        }
    } else {
        QFile::remove(autostartPath());
    }
    emit autostartChanged();
}

void Backend::quit()
{
    QCoreApplication::quit();
}
