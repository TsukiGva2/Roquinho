#!/usr/bin/env python3
"""Gravador roteirizado das demos do Foquinho.

Roda a interface QML real com um Backend falso e dados de demonstração,
simula mouse/teclado, captura os quadros e gera GIFs/PNGs em docs/media/.

Uso:  python3 tools/demo/record.py <cena> [<cena> ...]
Cenas: banner hero panel hide tip pets stills
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
    def __init__(self, app, qml, state, canvas_h, canvas_w=960, fps=30, scale=1.0, props=None, show=True):
        super().__init__()
        self.app = app
        self.canvas_w, self.canvas_h, self.fps, self.scale = canvas_w, canvas_h, fps, scale
        self.backend = FakeBackend(state)
        self.engine = QQmlApplicationEngine()
        self.engine.rootContext().setContextProperty("Backend", self.backend)
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
          "stills": scene_stills, "pets": scene_pets}

if __name__ == "__main__":
    names = sys.argv[1:] or list(SCENES)
    if "pets" in names or "banner" in names:
        fmt = QSurfaceFormat.defaultFormat()
        fmt.setSamples(8)
        QSurfaceFormat.setDefaultFormat(fmt)
    app = QGuiApplication(sys.argv[:1])
    for n in names:
        SCENES[n](app)
