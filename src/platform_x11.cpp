// Plataforma X11 (Openbox, Fluxbox, i3, XFCE…): xcb puro, sem depender de KDE.
//
//  janela      _NET_WM_WINDOW_TYPE_DOCK + ABOVE/STICKY/SKIP_*: sem borda, fora do Alt+Tab,
//              em todas as áreas de trabalho e acima das janelas comuns
//  cliques     XShape (região de entrada): só o notch recebe clique, o resto passa
//  sem picom   sem compositor não existe transparência; aí a própria forma da janela
//              (XShape bounding) acompanha o contorno do notch, quadro a quadro
//  atalhos     XGrabKey na janela raiz; dá pra trocar em ~/.config/foco-maxrc [shortcuts]
//  foco        _NET_ACTIVE_WINDOW como "pedido do usuário" e devolve o foco ao sair
#include "platform.h"

#include <QAbstractNativeEventFilter>
#include <QDir>
#include <QFile>
#include <QGuiApplication>
#include <QHash>
#include <QPointer>
#include <QRegularExpression>
#include <QQuickWindow>
#include <QSaveFile>
#include <QScreen>
#include <QSet>
#include <QSettings>
#include <QStandardPaths>
#include <QTimer>
#include <QtGui/qguiapplication_platform.h>

#include <xcb/shape.h>
#include <xcb/xcb.h>

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <memory>

namespace {

constexpr uint16_t kModMask = XCB_MOD_MASK_SHIFT | XCB_MOD_MASK_CONTROL | XCB_MOD_MASK_1 | XCB_MOD_MASK_4;
// CapsLock e NumLock ligados não podem impedir o atalho de funcionar
constexpr uint16_t kLockCombos[] = {0, XCB_MOD_MASK_LOCK, XCB_MOD_MASK_2, XCB_MOD_MASK_LOCK | XCB_MOD_MASK_2};
const QString kAutostartMarker = QStringLiteral("# foquinho");

template<typename T>
using XcbReply = std::unique_ptr<T, decltype(&std::free)>;

QString configDir()
{
    return QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
}

// Qt::Key → keysym do X11 (Latin-1, teclas de função, navegação e mídia).
xcb_keysym_t keysymFor(int key)
{
    if (key >= Qt::Key_A && key <= Qt::Key_Z)
        return xcb_keysym_t(key + 0x20); // keysym das letras é a minúscula
    if (key >= 0xc0 && key <= 0xde && key != 0xd7)
        return xcb_keysym_t(key + 0x20); // À…Þ → à…þ
    if ((key >= Qt::Key_Space && key <= Qt::Key_AsciiTilde) || (key >= 0xa0 && key <= 0xff))
        return xcb_keysym_t(key);
    if (key >= Qt::Key_F1 && key <= Qt::Key_F35)
        return xcb_keysym_t(0xffbe + (key - Qt::Key_F1));
    switch (key) {
    case Qt::Key_Escape: return 0xff1b;
    case Qt::Key_Tab: return 0xff09;
    case Qt::Key_Backspace: return 0xff08;
    case Qt::Key_Return: return 0xff0d;
    case Qt::Key_Enter: return 0xff8d;
    case Qt::Key_Insert: return 0xff63;
    case Qt::Key_Delete: return 0xffff;
    case Qt::Key_Pause: return 0xff13;
    case Qt::Key_Print: return 0xff61;
    case Qt::Key_Home: return 0xff50;
    case Qt::Key_End: return 0xff57;
    case Qt::Key_Left: return 0xff51;
    case Qt::Key_Up: return 0xff52;
    case Qt::Key_Right: return 0xff53;
    case Qt::Key_Down: return 0xff54;
    case Qt::Key_PageUp: return 0xff55;
    case Qt::Key_PageDown: return 0xff56;
    case Qt::Key_MediaPlay:
    case Qt::Key_MediaTogglePlayPause: return 0x1008ff14;
    case Qt::Key_MediaStop: return 0x1008ff15;
    case Qt::Key_MediaPrevious: return 0x1008ff16;
    case Qt::Key_MediaNext: return 0x1008ff17;
    case Qt::Key_MediaPause: return 0x1008ff31;
    default: return 0;
    }
}

uint16_t xmodsFor(Qt::KeyboardModifiers mods)
{
    uint16_t out = 0;
    if (mods & Qt::ShiftModifier)
        out |= XCB_MOD_MASK_SHIFT;
    if (mods & Qt::ControlModifier)
        out |= XCB_MOD_MASK_CONTROL;
    if (mods & Qt::AltModifier)
        out |= XCB_MOD_MASK_1;
    if (mods & Qt::MetaModifier)
        out |= XCB_MOD_MASK_4; // Super / tecla Windows
    return out;
}

// Aceita "Meta+Alt+N", "Super+Alt+N" e o "W-A-n" do rc.xml do Openbox.
QKeySequence parseSequence(QString text)
{
    text = text.trimmed();
    if (text.isEmpty())
        return {};
    static const QRegularExpression openboxStyle(QStringLiteral("^(?:[WACSM]-)+\\S+$"));
    if (openboxStyle.match(text).hasMatch()) {
        QStringList parts = text.split(QLatin1Char('-'));
        QString key = parts.takeLast();
        QStringList mods;
        for (const QString &m : parts)
            mods << (m == QLatin1String("W") ? QStringLiteral("Meta")
                     : m == QLatin1String("A") ? QStringLiteral("Alt")
                     : m == QLatin1String("C") ? QStringLiteral("Ctrl")
                     : m == QLatin1String("S") ? QStringLiteral("Shift")
                                               : QStringLiteral("Alt"));
        if (key == QLatin1String("space"))
            key = QStringLiteral("Space");
        else if (key == QLatin1String("period"))
            key = QStringLiteral(".");
        else if (key == QLatin1String("comma"))
            key = QStringLiteral(",");
        text = (mods + QStringList{key}).join(QLatin1Char('+'));
    }
    text.replace(QStringLiteral("Super+"), QStringLiteral("Meta+"), Qt::CaseInsensitive);
    text.replace(QStringLiteral("Win+"), QStringLiteral("Meta+"), Qt::CaseInsensitive);
    return QKeySequence::fromString(text, QKeySequence::PortableText);
}

class X11Platform : public Platform, public QAbstractNativeEventFilter
{
public:
    explicit X11Platform(QObject *parent)
        : Platform(parent)
    {
        auto *x11 = qApp->nativeInterface<QNativeInterface::QX11Application>();
        if (!x11 || !x11->connection()) {
            std::fprintf(stderr, "foco-max: esta versão é para X11 (Openbox). Rode com QT_QPA_PLATFORM=xcb.\n");
            return;
        }
        m_conn = x11->connection();

        int screenNumber = 0;
        char *host = nullptr;
        int display = 0;
        if (xcb_parse_display(nullptr, &host, &display, &screenNumber))
            std::free(host);
        xcb_screen_iterator_t it = xcb_setup_roots_iterator(xcb_get_setup(m_conn));
        for (int i = 0; it.rem; ++i, xcb_screen_next(&it)) {
            if (i == screenNumber || !m_root)
                m_root = it.data->root;
            if (i == screenNumber)
                break;
        }
        m_cmAtom = atom(QByteArray("_NET_WM_CM_S") + QByteArray::number(screenNumber));

        const xcb_query_extension_reply_t *shape = xcb_get_extension_data(m_conn, &xcb_shape_id);
        m_hasShapeExt = shape && shape->present;

        qApp->installNativeEventFilter(this);

        // O compositor (picom, xcompmgr…) pode ser ligado ou desligado com o app aberto.
        m_composited = queryComposited();
        m_compositorPoll.setInterval(2000);
        connect(&m_compositorPoll, &QTimer::timeout, this, [this] {
            const bool now = queryComposited();
            if (now == m_composited)
                return;
            m_composited = now;
            applyShapes();
            emit needsShapeChanged();
        });
        m_compositorPoll.start();

        // Troca de layout do teclado muda os keycodes: refaz as capturas.
        m_regrab.setSingleShot(true);
        m_regrab.setInterval(300);
        connect(&m_regrab, &QTimer::timeout, this, [this] { grabKeys(); });
    }

    ~X11Platform() override
    {
        if (m_conn) {
            qApp->removeNativeEventFilter(this);
            ungrabKeys();
        }
    }

    QString name() const override { return QStringLiteral("x11"); }

    // ---------- janela ----------

    void setupWindow(QQuickWindow *window) override
    {
        m_window = window;
        window->setFlags(Qt::FramelessWindowHint | Qt::WindowStaysOnTopHint);
        window->installEventFilter(this);
        if (!m_conn)
            return;
        window->create();
        const xcb_window_t id = xcb_window_t(window->winId());

        // Dock: o Openbox não desenha borda, não posiciona por conta própria,
        // deixa fora do Alt+Tab e da barra de tarefas e mantém na camada de cima.
        const xcb_atom_t type = atom("_NET_WM_WINDOW_TYPE_DOCK");
        xcb_change_property(m_conn, XCB_PROP_MODE_REPLACE, id, atom("_NET_WM_WINDOW_TYPE"), XCB_ATOM_ATOM, 32, 1, &type);
        const xcb_atom_t states[] = {atom("_NET_WM_STATE_ABOVE"), atom("_NET_WM_STATE_STICKY"),
                                     atom("_NET_WM_STATE_SKIP_TASKBAR"), atom("_NET_WM_STATE_SKIP_PAGER")};
        xcb_change_property(m_conn, XCB_PROP_MODE_REPLACE, id, atom("_NET_WM_STATE"), XCB_ATOM_ATOM, 32, 4, states);
        const uint32_t allDesktops = 0xFFFFFFFF;
        xcb_change_property(m_conn, XCB_PROP_MODE_REPLACE, id, atom("_NET_WM_DESKTOP"), XCB_ATOM_CARDINAL, 32, 1, &allDesktops);
        // Começa sem forma nenhuma: sem compositor, a janela inteira piscaria preta até o QML mandar o contorno.
        applyShapes();
    }

    void placeWindow(QQuickWindow *window, QScreen *screen, bool /*remap*/) override
    {
        if (screen != m_placedScreen) {
            if (m_placedScreen)
                disconnect(m_placedScreen, nullptr, this, nullptr);
            m_placedScreen = screen;
            // resolução mudou ou o monitor foi rearranjado (xrandr/arandr): recentraliza
            connect(screen, &QScreen::geometryChanged, this, [this, screen] {
                if (m_window)
                    placeWindow(m_window, screen, false);
            });
        }
        window->setScreen(screen);
        const QRect g = screen->geometry();
        window->setPosition(g.x() + (g.width() - window->width()) / 2, g.y());
    }

    void setInputRegion(QQuickWindow *window, const QRegion &region) override
    {
        m_window = window;
        m_input = region;
        applyShapes();
    }

    bool needsShape() const override { return m_conn && m_hasShapeExt && !m_composited; }

    void setShape(QQuickWindow *window, const QRegion &region) override
    {
        m_window = window;
        m_shape = region;
        m_haveShape = true;
        if (needsShape())
            applyShapes();
    }

    // ---------- teclado e foco ----------

    void grabKeyboard(QQuickWindow *window, bool exclusive) override
    {
        if (exclusive) {
            activate(window);
        } else if (window->isActive() && m_previousFocus) {
            // Saiu do painel por Esc/Enter: devolve o foco pra janela em que a pessoa estava.
            requestActive(m_previousFocus);
            m_previousFocus = 0;
        }
    }

    bool eventFilter(QObject *watched, QEvent *event) override
    {
        // No Openbox, clicar num dock não dá foco a ele; pedimos nós mesmos
        // (equivale ao "foco sob demanda" do layer-shell no KDE).
        if (event->type() == QEvent::MouseButtonPress && watched == m_window && !m_window->isActive())
            activate(m_window);
        return false;
    }

    // ---------- atalhos globais ----------

    void registerShortcuts(QList<Shortcut> &shortcuts) override
    {
        QSettings cfg(configDir() + QStringLiteral("/foco-maxrc"), QSettings::IniFormat);
        cfg.beginGroup(QStringLiteral("shortcuts"));
        for (Shortcut &s : shortcuts) {
            if (cfg.contains(s.id)) {
                const QVariant v = cfg.value(s.id);
                // "Meta+Alt+," sem aspas vira lista no QSettings
                const QString text = v.typeId() == QMetaType::QStringList ? v.toStringList().join(QLatin1Char(','))
                                                                          : v.toString();
                s.seq = parseSequence(text);
            } else {
                // deixa o padrão escrito no arquivo, pra pessoa saber o que pode trocar
                cfg.setValue(s.id, s.seq.toString(QKeySequence::PortableText));
            }
        }
        cfg.endGroup();
        m_shortcuts = shortcuts;
        grabKeys();
    }

    QString shortcutsHint() const override
    {
        return QStringLiteral("Dá para trocar em ~/.config/foco-maxrc, na seção [shortcuts] "
                              "(deixe vazio para desligar e usar o rc.xml do Openbox).");
    }

    bool nativeEventFilter(const QByteArray &eventType, void *message, qintptr *) override
    {
        if (eventType != "xcb_generic_event_t")
            return false;
        auto *ev = static_cast<xcb_generic_event_t *>(message);
        switch (ev->response_type & 0x7f) {
        case XCB_KEY_PRESS: {
            auto *k = reinterpret_cast<xcb_key_press_event_t *>(ev);
            if (k->event != m_root)
                return false;
            m_lastTime = k->time;
            const uint16_t mods = k->state & kModMask;
            for (const Grab &g : std::as_const(m_grabs)) {
                if (g.code != k->detail || g.mods != mods)
                    continue;
                if (!m_down.contains(k->detail)) { // segurar a tecla não repete o comando
                    m_down.insert(k->detail);
                    const QString cmd = g.cmd;
                    QMetaObject::invokeMethod(this, [this, cmd] { emit shortcutActivated(cmd); }, Qt::QueuedConnection);
                }
                return true;
            }
            return false;
        }
        case XCB_KEY_RELEASE: {
            auto *k = reinterpret_cast<xcb_key_release_event_t *>(ev);
            if (k->event == m_root)
                m_down.remove(k->detail);
            return false;
        }
        case XCB_MAPPING_NOTIFY:
            m_regrab.start();
            return false;
        default:
            return false;
        }
    }

    // ---------- autostart ----------

    QString autostartExec(const QString &binary) const override
    {
        // "start" não abre o painel se já houver uma instância (o autostart pode rodar duas vezes)
        const QString bin = binary.contains(QLatin1Char(' ')) ? QLatin1Char('"') + binary + QLatin1Char('"') : binary;
        return bin + QStringLiteral(" start");
    }

    void autostartChanged(bool enabled, const QString &binary) override
    {
        // O Openbox só lê ~/.config/autostart se o python-xdg estiver instalado;
        // por garantia, também colocamos uma linha no autostart dele.
        const QString dir = configDir() + QStringLiteral("/openbox");
        if (!QDir(dir).exists())
            return;
        const QString path = dir + QStringLiteral("/autostart");
        QStringList lines;
        QFile in(path);
        const bool existed = in.exists();
        const QFile::Permissions perms = existed ? in.permissions() : QFile::Permissions();
        if (in.open(QIODevice::ReadOnly | QIODevice::Text)) {
            lines = QString::fromUtf8(in.readAll()).split(QLatin1Char('\n'));
            in.close();
        }
        while (!lines.isEmpty() && lines.last().isEmpty())
            lines.removeLast();
        lines.erase(std::remove_if(lines.begin(), lines.end(),
                                   [](const QString &l) { return l.contains(kAutostartMarker); }),
                    lines.end());
        if (enabled) {
            // espera um pouco: se o autostart do XDG também rodar, ele chega primeiro e este vira no-op
            lines << QStringLiteral("(sleep 3; \"%1\" start) &  %2").arg(binary, kAutostartMarker);
        }
        QSaveFile out(path);
        if (!out.open(QIODevice::WriteOnly | QIODevice::Text))
            return;
        out.write((lines.join(QLatin1Char('\n')) + QLatin1Char('\n')).toUtf8());
        if (out.commit() && existed)
            QFile::setPermissions(path, perms);
    }

private:
    struct Grab {
        xcb_keycode_t code;
        uint16_t mods;
        QString cmd;
    };

    xcb_atom_t atom(const QByteArray &name)
    {
        auto it = m_atoms.constFind(name);
        if (it != m_atoms.constEnd())
            return *it;
        XcbReply<xcb_intern_atom_reply_t> reply(
            xcb_intern_atom_reply(m_conn, xcb_intern_atom(m_conn, 0, name.size(), name.constData()), nullptr), &std::free);
        const xcb_atom_t a = reply ? reply->atom : xcb_atom_t(XCB_ATOM_NONE);
        m_atoms.insert(name, a);
        return a;
    }

    bool queryComposited()
    {
        XcbReply<xcb_get_selection_owner_reply_t> reply(
            xcb_get_selection_owner_reply(m_conn, xcb_get_selection_owner(m_conn, m_cmAtom), nullptr), &std::free);
        return reply && reply->owner != XCB_NONE;
    }

    xcb_window_t activeWindow()
    {
        XcbReply<xcb_get_property_reply_t> reply(
            xcb_get_property_reply(m_conn,
                                   xcb_get_property(m_conn, 0, m_root, atom("_NET_ACTIVE_WINDOW"), XCB_ATOM_WINDOW, 0, 1),
                                   nullptr),
            &std::free);
        if (!reply || reply->type != XCB_ATOM_WINDOW || xcb_get_property_value_length(reply.get()) < 4)
            return 0;
        return *static_cast<xcb_window_t *>(xcb_get_property_value(reply.get()));
    }

    // Pede foco como se fosse um clique do usuário (origem 2 = "pager"):
    // a prevenção de roubo de foco do Openbox não barra esse tipo de pedido.
    void requestActive(xcb_window_t target)
    {
        xcb_client_message_event_t ev = {};
        ev.response_type = XCB_CLIENT_MESSAGE;
        ev.format = 32;
        ev.window = target;
        ev.type = atom("_NET_ACTIVE_WINDOW");
        ev.data.data32[0] = 2;
        ev.data.data32[1] = m_lastTime;
        xcb_send_event(m_conn, 0, m_root, XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT | XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY,
                       reinterpret_cast<const char *>(&ev));
        xcb_flush(m_conn);
    }

    void activate(QQuickWindow *window)
    {
        if (!window)
            return;
        if (!m_conn) {
            window->requestActivate();
            return;
        }
        const xcb_window_t id = xcb_window_t(window->winId());
        const xcb_window_t prev = activeWindow();
        if (prev && prev != id)
            m_previousFocus = prev;
        requestActive(id);
        // Gerenciador que recusa foco em dock (ou nenhum gerenciador): foco direto.
        QPointer<QQuickWindow> guard(window);
        QTimer::singleShot(150, this, [this, guard, id] {
            if (guard && guard->isVisible() && !guard->isActive()) {
                xcb_set_input_focus(m_conn, XCB_INPUT_FOCUS_PARENT, id, XCB_CURRENT_TIME);
                xcb_flush(m_conn);
            }
        });
    }

    void ungrabKeys()
    {
        for (const Grab &g : std::as_const(m_grabs))
            for (uint16_t lock : kLockCombos)
                xcb_ungrab_key(m_conn, g.code, m_root, g.mods | lock);
        m_grabs.clear();
        m_down.clear();
    }

    void grabKeys()
    {
        if (!m_conn)
            return;
        ungrabKeys();

        const xcb_setup_t *setup = xcb_get_setup(m_conn);
        const int count = setup->max_keycode - setup->min_keycode + 1;
        XcbReply<xcb_get_keyboard_mapping_reply_t> map(
            xcb_get_keyboard_mapping_reply(m_conn, xcb_get_keyboard_mapping(m_conn, setup->min_keycode, count), nullptr),
            &std::free);
        if (!map)
            return;
        const xcb_keysym_t *syms = xcb_get_keyboard_mapping_keysyms(map.get());
        const int per = map->keysyms_per_keycode;

        struct Pending {
            QString id;
            QList<xcb_void_cookie_t> cookies;
        };
        QList<Pending> pending;
        QStringList failed;
        for (const Shortcut &s : std::as_const(m_shortcuts)) {
            if (s.seq.isEmpty())
                continue;
            const QKeyCombination combo = s.seq[0];
            const xcb_keysym_t sym = keysymFor(combo.key());
            const uint16_t mods = xmodsFor(combo.keyboardModifiers());
            Pending p{s.id, {}};
            for (int i = 0; sym && i < count; ++i) {
                bool match = false;
                for (int j = 0; j < qMin(per, 2); ++j) // nível normal e com Shift
                    match = match || syms[i * per + j] == sym;
                if (!match)
                    continue;
                const xcb_keycode_t code = xcb_keycode_t(setup->min_keycode + i);
                for (uint16_t lock : kLockCombos)
                    p.cookies << xcb_grab_key_checked(m_conn, 0, m_root, mods | lock, code, XCB_GRAB_MODE_ASYNC,
                                                      XCB_GRAB_MODE_ASYNC);
                m_grabs.append({code, mods, s.cmd});
            }
            if (p.cookies.isEmpty())
                failed << s.id + QStringLiteral(" (tecla não existe neste layout)");
            else
                pending << p;
        }
        for (const Pending &p : std::as_const(pending)) {
            bool taken = false;
            for (const xcb_void_cookie_t &c : p.cookies) {
                if (xcb_generic_error_t *err = xcb_request_check(m_conn, c)) {
                    taken = true;
                    std::free(err);
                }
            }
            if (taken)
                failed << p.id + QStringLiteral(" (já usado por outro programa)");
        }
        xcb_flush(m_conn);
        if (!failed.isEmpty()) {
            std::fprintf(stderr, "foco-max: atalhos que não puderam ser registrados: %s\n"
                                 "          troque-os em ~/.config/foco-maxrc, seção [shortcuts].\n",
                         qPrintable(failed.join(QStringLiteral(", "))));
        }
    }

    static QList<xcb_rectangle_t> toXcb(const QRegion &region, qreal dpr)
    {
        QList<xcb_rectangle_t> out;
        for (const QRect &r : region) {
            const QRect s = dpr == 1.0 ? r : QRectF(r.x() * dpr, r.y() * dpr, r.width() * dpr, r.height() * dpr).toAlignedRect();
            out.append({int16_t(s.x()), int16_t(s.y()), uint16_t(s.width()), uint16_t(s.height())});
        }
        return out;
    }

    void setShapeRects(xcb_shape_kind_t kind, const QRegion &region)
    {
        const xcb_window_t id = xcb_window_t(m_window->winId());
        const QList<xcb_rectangle_t> rects = toXcb(region, m_window->devicePixelRatio());
        xcb_shape_rectangles(m_conn, XCB_SHAPE_SO_SET, kind, XCB_CLIP_ORDERING_UNSORTED, id, 0, 0,
                             uint32_t(rects.size()), rects.constData());
    }

    void applyShapes()
    {
        if (!m_window)
            return;
        if (!m_conn || !m_hasShapeExt) {
            m_window->setMask(m_input);
            return;
        }
        const xcb_window_t id = xcb_window_t(m_window->winId());
        if (m_composited) {
            // Com compositor: janela inteira desenhada (transparente), só o notch recebe clique.
            xcb_shape_mask(m_conn, XCB_SHAPE_SO_SET, XCB_SHAPE_SK_BOUNDING, id, 0, 0, XCB_NONE);
        } else {
            // Sem compositor: o que estiver fora da forma nem existe na tela.
            setShapeRects(XCB_SHAPE_SK_BOUNDING, m_haveShape ? m_shape : m_input);
        }
        setShapeRects(XCB_SHAPE_SK_INPUT, m_input);
        xcb_flush(m_conn);
    }

    xcb_connection_t *m_conn = nullptr;
    xcb_window_t m_root = 0;
    xcb_atom_t m_cmAtom = XCB_ATOM_NONE;
    bool m_hasShapeExt = false;
    bool m_composited = false;
    QHash<QByteArray, xcb_atom_t> m_atoms;
    QTimer m_compositorPoll;
    QTimer m_regrab;

    QPointer<QQuickWindow> m_window;
    QPointer<QScreen> m_placedScreen;
    QRegion m_input;
    QRegion m_shape;
    bool m_haveShape = false;

    QList<Shortcut> m_shortcuts;
    QList<Grab> m_grabs;
    QSet<xcb_keycode_t> m_down;
    xcb_timestamp_t m_lastTime = XCB_CURRENT_TIME;
    xcb_window_t m_previousFocus = 0;
};

} // namespace

void Platform::preInit()
{
    // Numa sessão Wayland esta versão roda pelo Xwayland; no Openbox já é X11 de qualquer jeito.
    if (qEnvironmentVariableIsEmpty("QT_QPA_PLATFORM"))
        qputenv("QT_QPA_PLATFORM", "xcb");
    // No X11 a janela só é transparente se pedir um visual com canal alfa.
    QQuickWindow::setDefaultAlphaBuffer(true);
}

Platform *Platform::create(QObject *parent)
{
    return new X11Platform(parent);
}
