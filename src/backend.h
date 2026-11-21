#pragma once

#include <QColor>
#include <QFileSystemWatcher>
#include <QObject>
#include <QPointer>
#include <QTimer>

class QQuickWindow;
class QLocalServer;
class QLocalSocket;

// Ponte entre o QML e o sistema: layer-shell, persistência, atalhos globais,
// cor de destaque do KDE, autostart e IPC de instância única.
class Backend : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QColor accent READ accent NOTIFY accentChanged)
    Q_PROPERTY(bool autostart READ autostart WRITE setAutostart NOTIFY autostartChanged)

public:
    explicit Backend(QObject *parent = nullptr);

    static QString serverName();

    void attachWindow(QQuickWindow *window);
    bool startServer();

    QColor accent() const { return m_accent; }
    bool autostart() const;
    void setAutostart(bool enabled);

    Q_INVOKABLE QString loadState() const;
    Q_INVOKABLE void saveState(const QString &json) const;
    Q_INVOKABLE void setInputRect(qreal x, qreal y, qreal w, qreal h);
    Q_INVOKABLE void setInputRegion(const QVariantList &rects);
    Q_INVOKABLE void grabKeyboard(bool exclusive);
    Q_INVOKABLE void quit();

signals:
    void accentChanged();
    void autostartChanged();
    // Comando vindo de atalho global ou de outra instância (ex.: "add Comprar pão").
    void command(const QString &name, const QString &arg);

private:
    void reloadAccent();
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
};
