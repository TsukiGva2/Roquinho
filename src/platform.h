#pragma once

#include <QKeySequence>
#include <QList>
#include <QObject>
#include <QRegion>
#include <QString>

class QQuickWindow;
class QScreen;

// Tudo que depende do ambiente gráfico fica atrás desta interface.
// Há duas implementações, escolhidas na compilação (make PLATFORM=...):
//   platform_kde.cpp  KDE Plasma 6 / Wayland: layer-shell + KGlobalAccel
//   platform_x11.cpp  Openbox e outros gerenciadores X11: xcb puro (EWMH, XShape, XGrabKey)
class Platform : public QObject
{
    Q_OBJECT
public:
    struct Shortcut {
        QString id; // identificador estável (também é a chave em [shortcuts] no foco-maxrc)
        QString text; // descrição para o usuário
        QKeySequence seq; // combinação; a plataforma pode trocar pela configurada
        QString cmd; // comando disparado (o mesmo da linha de comando)
    };

    using QObject::QObject;

    // Chamado antes de criar o QGuiApplication (variáveis de ambiente, formato de superfície…).
    static void preInit();
    static Platform *create(QObject *parent);

    // "kde" ou "x11"
    virtual QString name() const = 0;

    // Configura a janela do notch antes do primeiro show().
    virtual void setupWindow(QQuickWindow *window) = 0;
    // Coloca a janela centralizada no topo do monitor. remap = trocando de monitor.
    virtual void placeWindow(QQuickWindow *window, QScreen *screen, bool remap) = 0;
    // Só esta região recebe cliques; o resto passa para o que estiver embaixo.
    virtual void setInputRegion(QQuickWindow *window, const QRegion &region) = 0;
    // true = não há transparência de verdade (X11 sem compositor): o QML passa a forma exata
    // do notch, quadro a quadro, para recortarmos a própria janela.
    virtual bool needsShape() const { return false; }
    virtual void setShape(QQuickWindow *, const QRegion &) {}
    // exclusive = captura rápida (atalho de nova demanda): pega o teclado na hora.
    virtual void grabKeyboard(QQuickWindow *window, bool exclusive) = 0;

    // Registra os atalhos globais; cada um dispara shortcutActivated(cmd).
    // Pode ajustar shortcut.seq para a combinação realmente em uso (vazia = desligado).
    virtual void registerShortcuts(QList<Shortcut> &shortcuts) = 0;
    // Onde o usuário troca os atalhos (aparece nos ajustes).
    virtual QString shortcutsHint() const = 0;

    // Linha Exec do autostart e ganchos extras do ambiente (ex.: ~/.config/openbox/autostart).
    virtual QString autostartExec(const QString &binary) const { return binary; }
    virtual void autostartChanged(bool /*enabled*/, const QString & /*binary*/) {}

signals:
    void shortcutActivated(const QString &cmd);
    void needsShapeChanged();
};
