#pragma once

#include <QColor>
#include <QFileSystemWatcher>
#include <QObject>
#include <QPointer>
#include <QTimer>
#include <QVariant>

class QQuickWindow;
class QScreen;
class QLocalServer;
class QLocalSocket;

// Ponte entre o QML e o sistema: layer-shell, persistência, atalhos globais,
// cor de destaque do KDE, autostart e IPC de instância única.
class Backend : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QColor accent READ accent NOTIFY accentChanged)
    Q_PROPERTY(bool autostart READ autostart WRITE setAutostart NOTIFY autostartChanged)
    // Monitores conectados, da esquerda para a direita: [{name, label, primary}].
    Q_PROPERTY(QVariantList screens READ screens NOTIFY screensChanged)
    // Monitor escolhido (nome do conector, ex.: "HDMI-A-1"); vazio = sempre o principal.
    Q_PROPERTY(QString screenName READ screenName WRITE setScreenName NOTIFY screenNameChanged)
    // Monitor em que o notch está de fato agora.
    Q_PROPERTY(QString currentScreen READ currentScreen NOTIFY currentScreenChanged)

public:
    explicit Backend(QObject *parent = nullptr);

    static QString serverName();

    void attachWindow(QQuickWindow *window);
    bool startServer();

    QColor accent() const { return m_accent; }
    bool autostart() const;
    void setAutostart(bool enabled);

    QVariantList screens() const;
    QString screenName() const { return m_screenName; }
    void setScreenName(const QString &name);
    QString currentScreen() const { return m_currentScreen; }
    Q_INVOKABLE void nextScreen();

    // Comandos de atalho/IPC: os de monitor são tratados aqui, o resto vai pro QML.
    void runCommand(const QString &name, const QString &arg);

    Q_INVOKABLE QString loadState() const;
    Q_INVOKABLE void saveState(const QString &json) const;
    Q_INVOKABLE void setInputRect(qreal x, qreal y, qreal w, qreal h);
    Q_INVOKABLE void setInputRegion(const QVariantList &rects);
    Q_INVOKABLE void grabKeyboard(bool exclusive);
    Q_INVOKABLE void quit();

signals:
    void accentChanged();
    void autostartChanged();
    void screensChanged();
    void screenNameChanged();
    void currentScreenChanged();
    // Comando vindo de atalho global ou de outra instância (ex.: "add Comprar pão").
    void command(const QString &name, const QString &arg);

private:
    void reloadAccent();
    QScreen *targetScreen() const;
    void placeWindow(bool remap);
    QString settingsPath() const;
    void setupShortcuts();
    void handleSocket(QLocalSocket *socket);
    void dispatch(const QString &line);
    QString statePath() const;
    QString autostartPath() const;

    QColor m_accent;
    QPointer<QQuickWindow> m_window;
    QFileSystemWatcher m_watcher;
    QTimer m_accentDebounce;
    QLocalServer *m_server = nullptr;
    QString m_screenName;
    QString m_currentScreen;
    QTimer m_screenDebounce;
};
