// Plataforma KDE Plasma 6 / Wayland: layer-shell (camada overlay) + atalhos no KGlobalAccel.
#include "platform.h"

#include <QAction>
#include <QQuickWindow>
#include <QScreen>

#include <KGlobalAccel>
#include <LayerShellQt/Window>

namespace {

class KdePlatform : public Platform
{
public:
    using Platform::Platform;

    QString name() const override { return QStringLiteral("kde"); }

    void setupWindow(QQuickWindow *window) override
    {
        if (auto *layer = LayerShellQt::Window::get(window)) {
            layer->setScope(QStringLiteral("foco-max"));
            layer->setLayer(LayerShellQt::Window::LayerOverlay); // acima de tudo, até de tela cheia
            layer->setAnchors(LayerShellQt::Window::AnchorTop); // centralizado no topo
            layer->setExclusiveZone(-1); // sobrepõe a barra superior em vez de ficar abaixo dela
            layer->setKeyboardInteractivity(LayerShellQt::Window::KeyboardInteractivityOnDemand);
            layer->setActivateOnShow(false);
            layer->setCloseOnDismissed(false); // nós mesmos remapeamos em outro monitor
        }
    }

    void placeWindow(QQuickWindow *window, QScreen *screen, bool remap) override
    {
        auto *layer = LayerShellQt::Window::get(window);
        // A superfície layer-shell fica presa ao monitor em que foi criada:
        // para trocar, desfaz o mapeamento e mapeia de novo no destino.
        const bool wasVisible = window->isVisible();
        if (remap && wasVisible)
            window->hide();
        window->setScreen(screen);
        if (layer)
            layer->setScreen(screen);
        if (remap && wasVisible)
            window->show();
    }

    void setInputRegion(QQuickWindow *window, const QRegion &region) override
    {
        window->setMask(region);
    }

    void grabKeyboard(QQuickWindow *window, bool exclusive) override
    {
        if (auto *layer = LayerShellQt::Window::get(window)) {
            layer->setKeyboardInteractivity(exclusive ? LayerShellQt::Window::KeyboardInteractivityExclusive
                                                      : LayerShellQt::Window::KeyboardInteractivityOnDemand);
        }
        if (exclusive)
            window->requestActivate();
    }

    void registerShortcuts(QList<Shortcut> &shortcuts) override
    {
        for (const Shortcut &s : shortcuts) {
            auto *action = new QAction(s.text, this);
            action->setObjectName(s.id);
            action->setProperty("componentName", QStringLiteral("foco-max"));
            action->setProperty("componentDisplayName", QStringLiteral("Foquinho"));
            KGlobalAccel::self()->setDefaultShortcut(action, {s.seq});
            KGlobalAccel::self()->setShortcut(action, {s.seq});
            const QString cmd = s.cmd;
            connect(action, &QAction::triggered, this, [this, cmd] { emit shortcutActivated(cmd); });
        }
        // remove do KDE atalhos de versões antigas que não existem mais (ex.: music-play)
        KGlobalAccel::cleanComponent(QStringLiteral("foco-max"));
    }

    QString shortcutsHint() const override
    {
        return QStringLiteral("Dá para trocar em Configurações do Sistema › Teclado › Atalhos › Foquinho.");
    }
};

} // namespace

void Platform::preInit() {}

Platform *Platform::create(QObject *parent)
{
    return new KdePlatform(parent);
}
