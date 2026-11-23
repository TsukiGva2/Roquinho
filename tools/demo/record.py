#!/usr/bin/env python3
"""Gravador roteirizado das demos do Foquinho.

Roda a interface QML real com um Backend falso e dados de demonstração,
simula mouse/teclado, captura os quadros e gera GIFs/PNGs em docs/media/.

Uso:  python3 tools/demo/record.py <cena> [<cena> ...]
Cenas: banner hero panel hide tip pets stills music music-panel
(stills e banner ficam melhores com QT_SCALE_FACTOR=2)
Requer: PySide6, ffmpeg.
"""
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile
import time

from PySide6.QtCore import (Property, QElapsedTimer, QEvent, QObject, QPoint, QPointF, QRectF, Qt,
                            QTimer, QUrl, Signal, Slot)
from PySide6.QtGui import (QBrush, QColor, QKeyEvent, QFont, QGuiApplication, QImage, QLinearGradient,
                           QPainter, QPainterPath, QPen, QRadialGradient, QSurfaceFormat)
from PySide6.QtQml import QQmlApplicationEngine
from PySide6.QtQuick import QQuickWindow
import shiboken6
from PySide6.QtTest import QTest

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.join(ROOT, "docs", "media")
WIN_W = 760
ACCENT = QColor(146, 110, 228)


# --------------------------------------------------------------------------- backend falso
class FakeBackend(QObject):
    accentChanged = Signal()
    autostartChanged = Signal()
    command = Signal(str, str)

    def __init__(self, state):
        super().__init__()
        self._state = state
        self._autostart = True

    def _get_accent(self):
        return ACCENT

    accent = Property(QColor, _get_accent, notify=accentChanged)

    def _get_autostart(self):
        return self._autostart

    def _set_autostart(self, v):
        self._autostart = v
        self.autostartChanged.emit()

    autostart = Property(bool, _get_autostart, _set_autostart, notify=autostartChanged)

    @Slot(result=str)
    def loadState(self):
        return self._state

    @Slot(str)
    def saveState(self, _json):
        pass

    @Slot(float, float, float, float)
    def setInputRect(self, *_):
        pass

    @Slot("QVariantList")
    def setInputRegion(self, _rects):
        pass

    @Slot(bool)
    def grabKeyboard(self, _b):
        pass

    @Slot()
    def quit(self):
        pass

    # monitores (um só na demo)
    screensChanged = Signal()
    screenNameChanged = Signal()
    currentScreenChanged = Signal()

    def _get_screens(self):
        return [{"name": "eDP-1", "label": "Notebook", "primary": True}]

    screens = Property("QVariantList", _get_screens, notify=screensChanged)

    def _get_screen_name(self):
        return ""

    def _set_screen_name(self, _v):
        pass

    screenName = Property(str, _get_screen_name, _set_screen_name, notify=screenNameChanged)

    def _get_current_screen(self):
        return "eDP-1"

    currentScreen = Property(str, _get_current_screen, notify=currentScreenChanged)

    @Slot()
    def nextScreen(self):
        pass


# --------------------------------------------------------------------------- player falso
def paint_cover(path, c1, c2, c3):
    """Capa fictícia: céu em degradê, sol listrado e montanhas."""
    n = 512
    img = QImage(n, n, QImage.Format_ARGB32_Premultiplied)
    p = QPainter(img)
    p.setRenderHint(QPainter.Antialiasing)
    g = QLinearGradient(0, 0, 0, n)
    g.setColorAt(0, QColor(c1))
    g.setColorAt(1, QColor(c2))
    p.fillRect(0, 0, n, n, g)
    p.setPen(Qt.NoPen)
    p.setBrush(QColor(c3))
    p.drawEllipse(QPointF(n / 2, n * 0.52), n * 0.26, n * 0.26)
    p.setBrush(QColor(c2))
    for i in range(5):
        p.drawRect(QRectF(0, n * 0.5 + i * n * 0.045, n, n * 0.012 + i * 3))
    path_m = QPainterPath()
    path_m.moveTo(0, n)
    path_m.lineTo(0, n * 0.72)
    path_m.lineTo(n * 0.3, n * 0.58)
    path_m.lineTo(n * 0.55, n * 0.74)
    path_m.lineTo(n * 0.8, n * 0.6)
    path_m.lineTo(n, n * 0.7)
    path_m.lineTo(n, n)
    p.setBrush(QColor(20, 14, 40, 235))
    p.drawPath(path_m)
    p.end()
    img.save(path)


class FakeMedia(QObject):
    enabledChanged = Signal()
    trackChanged = Signal()
    playingChanged = Signal()
    artChanged = Signal()
    positionChanged = Signal()
    capsChanged = Signal()
    visualizeChanged = Signal()
    levelsChanged = Signal()
    newTrack = Signal()

    TRACKS = [("Neon na Avenida", "Lua Azul", "#ff9a6b", "#b048d8", 214000, ("#2b1a6b", "#ff6f91", "#ffd36b")),
              ("Maré Alta", "Os Cometas", "#5fe3c8", "#3d7cf0", 187000, ("#0b3d5c", "#2ec4b6", "#f6f7c4"))]

    def __init__(self, active=False):
        super().__init__()
        self._active = active
        self._playing = active
        self._i = 0
        self._dir = tempfile.mkdtemp(prefix="foquinho-capas-")
        self._arts = []
        for k, t in enumerate(self.TRACKS):
            fn = os.path.join(self._dir, "capa%d.png" % k)
            paint_cover(fn, *t[5])
            self._arts.append(QUrl.fromLocalFile(fn).toString())
        self._pos = 71000.0
        self._stamp = time.time() * 1000
        self._levels = [0.0] * 24
        self._level = 0.0
        self._beat = 0.0
        self._clock = QElapsedTimer()
        self._clock.start()
        self._timer = QTimer(self)
        self._timer.setInterval(33)
        self._timer.timeout.connect(self._tick)
        self._timer.start()

    def _tick(self):
        # espectro sintético a 118 bpm, com graves batendo e agudos cintilando
        t = self._clock.elapsed() / 1000.0
        period = 60.0 / 118
        ph = (t % period) / period
        kick = math.exp(-ph * 9)
        on = 1.0 if self._playing and self._active else 0.0
        out = []
        for b in range(24):
            wob = 0.5 + 0.5 * math.sin(t * (3.1 + b * 0.37) + b * 1.7) * math.sin(t * 1.3 + b)
            v = (0.18 + 0.42 * wob) * (1 - b / 40) + (kick * (0.75 - b * 0.05) if b < 8 else 0)
            v = max(0.0, min(1.0, v)) * on
            prev = self._levels[b]
            out.append(prev + (v - prev) * 0.6 if v > prev else prev * 0.8 + v * 0.2)
        self._levels = out
        self._level = sum(out) / 24
        self._beat = kick * on if kick > 0.2 else self._beat * 0.86
        self.levelsChanged.emit()

    def _track(self):
        return self.TRACKS[self._i]

    def _get_enabled(self): return True
    def _set_enabled(self, _v): pass
    enabled = Property(bool, _get_enabled, _set_enabled, notify=enabledChanged)
    active = Property(bool, lambda self: self._active, notify=trackChanged)
    playing = Property(bool, lambda self: self._playing and self._active, notify=playingChanged)
    title = Property(str, lambda self: self._track()[0], notify=trackChanged)
    artist = Property(str, lambda self: self._track()[1], notify=trackChanged)
    album = Property(str, lambda self: "", notify=trackChanged)
    player = Property(str, lambda self: "Spotify", notify=trackChanged)
    art = Property(str, lambda self: self._arts[self._i], notify=artChanged)
    color = Property(QColor, lambda self: QColor(self._track()[2]), notify=artChanged)
    color2 = Property(QColor, lambda self: QColor(self._track()[3]), notify=artChanged)
    length = Property(float, lambda self: float(self._track()[4]), notify=trackChanged)
    position = Property(float, lambda self: self._pos, notify=positionChanged)
    positionStamp = Property(float, lambda self: self._stamp, notify=positionChanged)
    canNext = Property(bool, lambda self: True, notify=capsChanged)
    canPrevious = Property(bool, lambda self: True, notify=capsChanged)
    canSeek = Property(bool, lambda self: True, notify=capsChanged)
    def _get_vis(self): return True
    def _set_vis(self, _v): pass
    visualize = Property(bool, _get_vis, _set_vis, notify=visualizeChanged)
    levels = Property("QVariantList", lambda self: self._levels, notify=levelsChanged)
    level = Property(float, lambda self: self._level, notify=levelsChanged)
    beat = Property(float, lambda self: self._beat, notify=levelsChanged)

    @Slot()
    def playPause(self):
        self._playing = not self._playing
        self.playingChanged.emit()

    @Slot()
    def next(self):
        self._i = (self._i + 1) % len(self.TRACKS)
        self._pos = 0.0
        self._stamp = time.time() * 1000
        self.trackChanged.emit()
        self.artChanged.emit()
        self.positionChanged.emit()
        self.newTrack.emit()

    @Slot()
    def previous(self):
        self.next()

    @Slot(float)
    def seek(self, ms):
        self._pos = ms
        self._stamp = time.time() * 1000
        self.positionChanged.emit()

    @Slot(name="raise")
    def raise_(self):
        pass


def demo_state(tips=False, running_min=23, with_focus=True):
    now = int(time.time() * 1000)
    day = time.strftime("%Y-") + str(int(time.strftime("%m"))) + "-" + str(int(time.strftime("%d")))

    def task(i, title, acc=0, running=False, done=False, spent=0):
        return {"tid": "demo%d" % i, "title": title, "accMs": acc, "startedAt": now if running else 0,
                "running": running, "createdAt": now - 3600000, "doneAt": (now - i * 600000) if done else 0,
                "spentMs": spent}

    focus = [task(1, "Revisar PR do checkout", running_min * 60000 + 14000, running=True)] if with_focus else []
    return json.dumps({
        "version": 1, "lastSeen": now,
        "settings": {"petKind": "cat", "petName": "Foquinho", "autoAdvance": True, "hidden": False,
                     "tipsEnabled": tips, "lastTipDay": "" if tips else day},
        "totalDone": 13,
        "focus": focus,
        "queue": [task(2, "Responder e-mails do cliente"), task(3, "Escrever proposta comercial"),
                  task(4, "Ajustar layout da landing")],
        "done": [task(5, "Daily com o time", done=True, spent=17 * 60000),
                 task(6, "Corrigir bug do login", done=True, spent=48 * 60000),
                 task(7, "Atualizar dependências", done=True, spent=26 * 60000)],
    })


# --------------------------------------------------------------------------- cenário
def paint_wallpaper(w, h):
    img = QImage(w, h, QImage.Format_ARGB32_Premultiplied)
    p = QPainter(img)
    p.setRenderHint(QPainter.Antialiasing)
    g = QLinearGradient(0, 0, w, h)
    g.setColorAt(0.0, QColor("#1b1840"))
    g.setColorAt(0.55, QColor("#2a1f5c"))
    g.setColorAt(1.0, QColor("#3b1d5e"))
    p.fillRect(0, 0, w, h, g)
    for cx, cy, r, col in [(0.15, 0.9, 0.55, QColor(146, 110, 228, 120)),
                           (0.85, 0.75, 0.5, QColor(255, 120, 170, 70)),
                           (0.6, 0.15, 0.45, QColor(90, 160, 255, 60))]:
        rg = QRadialGradient(QPointF(cx * w, cy * h), r * max(w, h))
        rg.setColorAt(0, col)
        rg.setColorAt(1, QColor(col.red(), col.green(), col.blue(), 0))
        p.fillRect(0, 0, w, h, rg)
    # barra superior estilo WhiteSur-dark
    p.fillRect(0, 0, w, 44, QColor(38, 38, 40, 235))
    p.setPen(QColor(255, 255, 255, 18))
    p.drawLine(0, 44, w, 44)
    f = QFont()
    f.setPixelSize(13)
    f.setWeight(QFont.DemiBold)
    p.setFont(f)
    p.setPen(QColor("#f5f5f7"))
    p.drawText(QRectF(18, 0, 120, 44), Qt.AlignVCenter, "Foquinho")
    f.setWeight(QFont.Normal)
    p.setFont(f)
    p.setPen(QColor(245, 245, 247, 190))
    x = 100
    for item in ["Arquivo", "Editar", "Ver"]:
        p.drawText(QRectF(x, 0, 80, 44), Qt.AlignVCenter, item)
        x += 64
    # bandeja
    p.setPen(QPen(QColor(245, 245, 247, 200), 1.6, Qt.SolidLine, Qt.RoundCap))
    p.setBrush(Qt.NoBrush)
    bx = w - 250
    for i in range(3):  # wifi
        r = 3 + i * 3.2
        p.drawArc(QRectF(bx - r, 26 - r, 2 * r, 2 * r), 45 * 16, 90 * 16)
    p.drawRoundedRect(QRectF(bx + 22, 16, 20, 11), 3, 3)  # bateria
    p.fillRect(QRectF(bx + 24.5, 18.5, 12, 6), QColor(245, 245, 247, 200))
    p.drawLine(QPointF(bx + 44, 19.5), QPointF(bx + 44, 23.5))
    path = QPainterPath()  # volume
    path.moveTo(bx + 56, 19)
    path.lineTo(bx + 59, 19)
    path.lineTo(bx + 63, 15.5)
    path.lineTo(bx + 63, 28.5)
    path.lineTo(bx + 59, 25)
    path.lineTo(bx + 56, 25)
    path.closeSubpath()
    p.drawPath(path)
    p.drawArc(QRectF(bx + 62, 17, 8, 10), -50 * 16, 100 * 16)
    p.setPen(QColor("#f5f5f7"))
    p.drawText(QRectF(w - 162, 0, 150, 44), Qt.AlignVCenter | Qt.AlignRight, "sáb 21 nov  19:08")
    p.end()
    return img


def draw_cursor(p, pos, pressed):
    path = QPainterPath()
    pts = [(0, 0), (0, 17), (4.2, 13.2), (7.2, 20), (10, 18.8), (7.1, 12.2), (12.6, 12.2)]
    path.moveTo(*pts[0])
    for pt in pts[1:]:
        path.lineTo(*pt)
    path.closeSubpath()
    p.save()
    p.translate(pos)
    s = 0.88 if pressed else 1.0
    p.scale(s, s)
    p.setRenderHint(QPainter.Antialiasing)
    p.setPen(QPen(QColor("white"), 1.4))
    p.setBrush(QColor("#111"))
    p.drawPath(path)
    p.restore()


# --------------------------------------------------------------------------- gravação
class Recorder(QObject):
    def __init__(self, app, qml, state, canvas_h, canvas_w=960, fps=30, scale=1.0, props=None, show=True, music=False):
        super().__init__()
        self.app = app
        self.canvas_w, self.canvas_h, self.fps, self.scale = canvas_w, canvas_h, fps, scale
        self.backend = FakeBackend(state)
        self.engine = QQmlApplicationEngine()
        self.engine.rootContext().setContextProperty("Backend", self.backend)
        self.media = FakeMedia(active=music)
        self.engine.rootContext().setContextProperty("Media", self.media)
        self.engine.load(QUrl.fromLocalFile(qml))
        if not self.engine.rootObjects():
            sys.exit("falha ao carregar " + qml)
        root = self.engine.rootObjects()[0]
        self.win = shiboken6.wrapInstance(shiboken6.getCppPointer(root)[0], QQuickWindow)
        for k, v in (props or {}).items():
            self.win.setProperty(k, v)
        self.win.setFlags(self.win.flags() | Qt.FramelessWindowHint)
        if show:
            self.win.show()
        self.wall = paint_wallpaper(int(canvas_w * scale), int(canvas_h * scale))
        self.frames = []
        self.events = []
        self.cursor = None          # (x, y) em coordenadas da janela
        self.cursor_moves = []      # (t0, t1, (x0,y0), (x1,y1))
        self.clicks = []            # tempos de clique
        self.clock = QElapsedTimer()
        self.capturing = False

    # roteiro --------------------------------------------------------------
    def at(self, ms, fn):
        self.events.append((ms, fn))

    def move(self, t0, t1, a, b):
        self.cursor_moves.append((t0, t1, a, b))

    def click(self, ms, pos):
        def fn():
            QTest.mouseClick(self.win, Qt.LeftButton, Qt.NoModifier, QPoint(*pos))
        self.clicks.append(ms)
        self.at(ms, fn)

    def type_text(self, ms, text, step=55):
        for i, ch in enumerate(text):
            self.at(ms + i * step, lambda ch=ch: self.send_char(ch))
        return ms + len(text) * step

    def send_char(self, ch):
        key = Qt.Key(ord(ch.upper())) if ch.isascii() and ch.isalnum() else Qt.Key_Space
        for et in (QEvent.KeyPress, QEvent.KeyRelease):
            QGuiApplication.sendEvent(self.win, QKeyEvent(et, key, Qt.NoModifier, ch))

    def cmd(self, ms, name, arg=""):
        self.at(ms, lambda: self.backend.command.emit(name, arg))

    def cursor_at(self, t):
        pos = self.cursor
        for t0, t1, a, b in self.cursor_moves:
            if t >= t1:
                pos = b
            elif t >= t0:
                k = (t - t0) / max(1, t1 - t0)
                k = k * k * (3 - 2 * k)
                pos = (a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k)
        return pos

    # execução -------------------------------------------------------------
    def run(self, start_ms, end_ms):
        self.events.sort(key=lambda e: e[0])
        self.clock.start()
        pending = list(self.events)
        last_mouse = None
        frame_ms = 1000.0 / self.fps
        next_frame = start_ms
        while True:
            t = self.clock.elapsed()
            while pending and pending[0][0] <= t:
                pending.pop(0)[1]()
            pos = self.cursor_at(t)
            if pos is not None and pos != last_mouse:
                QTest.mouseMove(self.win, QPoint(int(pos[0]), int(pos[1])))
                last_mouse = pos
            if t >= next_frame and t <= end_ms:
                img = self.win.grabWindow()
                self.frames.append((t, img, pos, any(0 <= t - c < 220 for c in self.clicks)))
                next_frame += frame_ms
                if next_frame < t:
                    next_frame = t + frame_ms / 2
            if t > end_ms and not pending:
                break
            self.app.processEvents()
            time.sleep(0.002)

    def compose(self, img, pos, pressed):
        s = self.scale
        out = QImage(self.wall)
        p = QPainter(out)
        p.setRenderHint(QPainter.SmoothPixmapTransform)
        ox = (self.canvas_w - WIN_W) / 2
        src_h = min(img.height(), int(self.canvas_h * img.devicePixelRatio()))
        piece = img.copy(0, 0, img.width(), src_h)
        p.drawImage(QRectF(ox * s, 0, WIN_W * s, src_h / img.devicePixelRatio() * s), piece)
        if pos is not None:
            if pressed:
                p.setRenderHint(QPainter.Antialiasing)
                p.setPen(Qt.NoPen)
                p.setBrush(QColor(255, 255, 255, 60))
                p.drawEllipse(QPointF((ox + pos[0]) * s, pos[1] * s), 16 * s, 16 * s)
            p.scale(s, s)
            draw_cursor(p, QPointF(ox + pos[0], pos[1]), pressed)
        p.end()
        return out

    def save_gif(self, name, width=880):
        tmp = tempfile.mkdtemp(prefix="foquinho-")
        lines = []
        for i, (t, img, pos, pressed) in enumerate(self.frames):
            fn = os.path.join(tmp, "f%05d.png" % i)
            self.compose(img, pos, pressed).save(fn)
            nxt = self.frames[i + 1][0] if i + 1 < len(self.frames) else t + 1000 / self.fps
            lines.append("file '%s'\nduration %.4f\n" % (fn, (nxt - t) / 1000.0))
        lines.append("file '%s'\n" % os.path.join(tmp, "f%05d.png" % (len(self.frames) - 1)))
        lst = os.path.join(tmp, "list.txt")
        with open(lst, "w") as f:
            f.writelines(lines)
        os.makedirs(OUT, exist_ok=True)
        out = os.path.join(OUT, name)
        vf = ("fps=25,scale=%d:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=256:stats_mode=diff[p];"
              "[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle" % width)
        subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", lst,
                        "-vf", vf, "-loop", "0", out], check=True)
        shutil.rmtree(tmp)
        print("gif:", out, "%.1f MB" % (os.path.getsize(out) / 1e6), len(self.frames), "quadros")

    def save_png(self, name, img=None):
        os.makedirs(OUT, exist_ok=True)
        t, last, pos, pressed = self.frames[-1]
        out = os.path.join(OUT, name)
        self.compose(img or last, None, False).save(out)
        print("png:", out)


# --------------------------------------------------------------------------- cenas
MAIN = os.path.join(ROOT, "qml", "Main.qml")
NOTCH = (WIN_W / 2, 19)


def scene_hero(app):
    r = Recorder(app, MAIN, demo_state(), canvas_h=170)
    r.cursor = (640, 150)
    r.move(1500, 2300, (640, 150), (455, 20))       # passa o mouse no notch
    r.cmd(3300, "done")                              # conclui (✓)
    r.clicks.append(3300)
    r.move(3500, 4100, (455, 20), (600, 150))
    r.run(0, 8200)
    r.save_gif("hero.gif")


def scene_panel(app):
    r = Recorder(app, MAIN, demo_state(), canvas_h=600)
    r.cursor = (560, 300)
    r.move(700, 1400, (560, 300), NOTCH)
    r.click(1550, NOTCH)                              # abre o painel
    r.move(1800, 2500, NOTCH, (300, 260))
    r.cmd(3400, "add")                                # Meta+Alt+N
    end = r.type_text(3700, "Ligar para o cliente")
    r.at(end + 350, lambda: QTest.keyClick(r.win, Qt.Key_Return, Qt.ControlModifier))
    r.move(end + 600, end + 1200, (300, 260), (620, 330))
    r.run(500, end + 4200)
    r.save_gif("panel.gif")


def scene_hide(app):
    r = Recorder(app, MAIN, demo_state(), canvas_h=130)
    r.cursor = (600, 110)
    r.cmd(1300, "hide")
    r.move(2200, 3000, (600, 110), (WIN_W / 2 + 2, 22))
    r.click(3500, (WIN_W / 2, 22))
    r.move(3800, 4400, (WIN_W / 2 + 2, 22), (600, 110))
    r.run(700, 6200)
    r.save_gif("hide.gif", width=760)


def scene_tip(app):
    r = Recorder(app, MAIN, demo_state(tips=True), canvas_h=300)
    r.run(2700, 7600)
    r.save_gif("tip.gif", width=760)


def scene_stills(app):
    # notch fechado com hover
    r = Recorder(app, MAIN, demo_state(), canvas_h=120, scale=2.0)
    r.cursor = (455, 20)
    r.run(0, 2500)
    r.save_png("notch.png")
    # painel aberto
    r2 = Recorder(app, MAIN, demo_state(), canvas_h=600, scale=2.0)
    r2.cmd(600, "open")
    r2.run(0, 2800)
    r2.save_png("panel.png")
    # ajustes
    r3 = Recorder(app, MAIN, demo_state(), canvas_h=720, scale=2.0)
    r3.cmd(600, "open")
    r3.at(900, lambda: r3.win.setProperty("settingsOpen", True))
    r3.run(0, 3000)
    r3.save_png("settings.png")


def scene_music(app):
    # notch com as ondinhas → troca de faixa (aviso com o DVD) → bolinha dançando
    r = Recorder(app, MAIN, demo_state(), canvas_h=120, music=True)
    r.cursor = (640, 105)
    r.at(2600, r.media.next)
    r.cmd(6800, "hide")
    r.run(1000, 10800)
    r.save_gif("music.gif", width=760)


def scene_music_panel(app):
    # painel aberto: DVD girando, equalizador e troca de música
    r = Recorder(app, MAIN, demo_state(), canvas_h=330, music=True)
    r.cmd(900, "open")
    r.at(5200, r.media.next)
    r.run(2000, 9400)
    r.save_gif("music-panel.gif")


def scene_pets(app):
    qml = os.path.join(ROOT, "tools", "demo", "Pets.qml")
    r = Recorder(app, qml, demo_state(), canvas_h=0)
    r.run(0, 9600)
    # GIF dos três mascotes reagindo (sem wallpaper/barra)
    tmp = tempfile.mkdtemp(prefix="foquinho-")
    lines = []
    for i, (t, img, _p, _c) in enumerate(r.frames):
        fn = os.path.join(tmp, "f%05d.png" % i)
        img.save(fn)
        nxt = r.frames[i + 1][0] if i + 1 < len(r.frames) else t + 33
        lines.append("file '%s'\nduration %.4f\n" % (fn, (nxt - t) / 1000.0))
    lines.append("file '%s'\n" % os.path.join(tmp, "f%05d.png" % (len(r.frames) - 1)))
    with open(os.path.join(tmp, "list.txt"), "w") as f:
        f.writelines(lines)
    out = os.path.join(OUT, "pets.gif")
    vf = ("fps=25,scale=760:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=256:stats_mode=diff[p];"
          "[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle")
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i",
                    os.path.join(tmp, "list.txt"), "-vf", vf, "-loop", "0", out], check=True)
    shutil.rmtree(tmp)
    print("gif:", out, "%.1f MB" % (os.path.getsize(out) / 1e6))


def scene_banner(app):
    r = Recorder(app, os.path.join(ROOT, "tools", "demo", "Banner.qml"), demo_state(), canvas_h=0)
    r.run(0, 1250)
    os.makedirs(OUT, exist_ok=True)
    r.frames[-1][1].save(os.path.join(OUT, "banner.png"))
    print("png:", os.path.join(OUT, "banner.png"))


SCENES = {"banner": scene_banner, "hero": scene_hero, "panel": scene_panel, "hide": scene_hide, "tip": scene_tip,
          "stills": scene_stills, "pets": scene_pets, "music": scene_music, "music-panel": scene_music_panel}

if __name__ == "__main__":
    names = sys.argv[1:] or list(SCENES)
    if "pets" in names or "banner" in names:
        fmt = QSurfaceFormat.defaultFormat()
        fmt.setSamples(8)
        QSurfaceFormat.setDefaultFormat(fmt)
    app = QGuiApplication(sys.argv[:1])
    for n in names:
        SCENES[n](app)
