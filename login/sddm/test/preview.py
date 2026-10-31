#!/usr/bin/env python3
"""Loads the login theme outside SDDM with mock context objects and takes
screenshots, for checking changes without logging out.

Needs PySide6 (``pip install pyside6-essentials`` in a venv) and a display
(``xvfb-run -s '-screen 0 1920x1080x24' python3 preview.py ...`` works).

The mocks follow SDDM's greeter API (src/greeter in sddm's sources): the
``sddm`` proxy, ``userModel``, ``sessionModel``, ``screenModel``,
``keyboard`` and ``config`` (theme.conf overlaid with theme.conf.user, empty
values skipped, exactly like ThemeConfig::setTo).

Usage:
  preview.py THEME_DIR [--out DIR] [--scenario NAME ...] [--size WxH]
             [--set key=value ...] [--user-conf FILE] [--secondary]

Scenarios: default typing failed caps sessions users manual success power
           info. Each writes OUT/<scenario>.png and checks what the theme
           sent to the mock; the exit status is non-zero if a check fails.
"""
import argparse
import os
import sys

from PySide6.QtCore import (Property, QAbstractListModel, QByteArray, QModelIndex, QObject, QRect,
                            QEvent, QSettings, Qt, QTimer, QUrl, Signal, Slot)
from PySide6.QtGui import QGuiApplication, QKeyEvent
from PySide6.QtQuick import QQuickView
from PySide6.QtTest import QTest


class Proxy(QObject):
    loginFailed = Signal()
    loginSucceeded = Signal()
    informationMessage = Signal(str)
    hostNameChanged = Signal(str)
    canChanged = Signal(bool)

    def __init__(self):
        super().__init__()
        self.calls = []
        self.good_password = "hunter2"

    hostName = Property(str, lambda self: "archbox", notify=hostNameChanged)
    canPowerOff = Property(bool, lambda self: True, notify=canChanged)
    canReboot = Property(bool, lambda self: True, notify=canChanged)
    canSuspend = Property(bool, lambda self: True, notify=canChanged)
    canHibernate = Property(bool, lambda self: False, notify=canChanged)
    canHybridSleep = Property(bool, lambda self: False, notify=canChanged)

    @Slot(str, str, int)
    def login(self, user, password, session):
        self.calls.append(("login", user, password, session))
        if password == self.good_password:
            QTimer.singleShot(300, self.loginSucceeded.emit)
        else:
            QTimer.singleShot(300, self.loginFailed.emit)

    @Slot()
    def powerOff(self): self.calls.append(("powerOff",))

    @Slot()
    def reboot(self): self.calls.append(("reboot",))

    @Slot()
    def suspend(self): self.calls.append(("suspend",))

    @Slot()
    def hibernate(self): self.calls.append(("hibernate",))

    @Slot()
    def hybridSleep(self): self.calls.append(("hybridSleep",))


class ListModel(QAbstractListModel):
    """A read-only list model with SDDM's role names, in SDDM's role order."""
    countChanged = Signal()

    def __init__(self, roles, rows, last_index=0, last_user=""):
        super().__init__()
        self._roles = roles
        self._rows = rows
        self._last_index = last_index
        self._last_user = last_user

    def roleNames(self):
        return {Qt.UserRole + 1 + i: QByteArray(r.encode()) for i, r in enumerate(self._roles)}

    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self._rows)

    def data(self, index, role=Qt.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._rows)):
            return None
        i = role - Qt.UserRole - 1
        return self._rows[index.row()].get(self._roles[i]) if 0 <= i < len(self._roles) else None

    lastIndex = Property(int, lambda self: self._last_index, constant=True)
    lastUser = Property(str, lambda self: self._last_user, constant=True)
    count = Property(int, lambda self: len(self._rows), constant=True)
    disableAvatarsThreshold = Property(int, lambda self: 7, constant=True)
    containsAllUsers = Property(bool, lambda self: True, constant=True)
    primary = Property(int, lambda self: 0, constant=True)


class Layout(QObject):
    def __init__(self, short, long):
        super().__init__()
        self._s, self._l = short, long

    shortName = Property(str, lambda self: self._s, constant=True)
    longName = Property(str, lambda self: self._l, constant=True)


class Keyboard(QObject):
    numLockStateChanged = Signal()
    capsLockStateChanged = Signal()
    currentLayoutChanged = Signal()
    layoutsChanged = Signal()

    def __init__(self):
        super().__init__()
        self._caps = False
        self._num = True
        self._current = 0
        self._layouts = [Layout("us", "English (US)"), Layout("de", "German")]

    def _set_caps(self, v):
        self._caps = v
        self.capsLockStateChanged.emit()

    def _set_num(self, v):
        self._num = v
        self.numLockStateChanged.emit()

    def _set_current(self, v):
        self._current = v
        self.currentLayoutChanged.emit()

    capsLock = Property(bool, lambda self: self._caps, _set_caps, notify=capsLockStateChanged)
    numLock = Property(bool, lambda self: self._num, _set_num, notify=numLockStateChanged)
    currentLayout = Property(int, lambda self: self._current, _set_current, notify=currentLayoutChanged)
    layouts = Property(list, lambda self: self._layouts, notify=layoutsChanged)
    enabled = Property(bool, lambda self: True, constant=True)


class ThemeConfig(QObject):
    """theme.conf overlaid with theme.conf.user, as SDDM's ThemeConfig does."""

    def __init__(self, path, user_path=None, overrides=()):
        super().__init__()
        self.values = {}
        base = QSettings(path, QSettings.IniFormat)
        for k in base.allKeys():
            self.values[k] = base.value(k)
        for k, v in overrides:
            self.values[k] = v
        user = QSettings(user_path or path + ".user", QSettings.IniFormat)
        for k in user.allKeys():
            v = user.value(k)
            if v is not None and str(v) != "" and v != []:
                self.values[k] = v

    def _value(self, key):
        v = self.values.get(key)
        if isinstance(v, list):  # QSettings splits unquoted commas; QVariant::toString gives ""
            return "" if len(v) != 1 else str(v[0])
        return "" if v is None else str(v)

    @Slot(str, result=str)
    def stringValue(self, key): return self._value(key)

    @Slot(str, result=bool)
    def boolValue(self, key): return self._value(key).lower() == "true"

    @Slot(str, result=int)
    def intValue(self, key):
        try:
            return int(self._value(key))
        except ValueError:
            return 0

    @Slot(str, result=float)
    def realValue(self, key):
        try:
            return float(self._value(key))
        except ValueError:
            return 0.0


USERS = [
    {"name": "alex", "realName": "Alex Rivera", "homeDir": "/home/alex", "icon": "file:///usr/share/sddm/faces/.face.icon", "needsPassword": True},
    {"name": "bhima", "realName": "", "homeDir": "/home/bhima", "icon": "file:///usr/share/sddm/faces/.face.icon", "needsPassword": True},
    {"name": "guest", "realName": "Guest", "homeDir": "/home/guest", "icon": "", "needsPassword": False},
]
SESSIONS = [
    {"directory": "/usr/share/wayland-sessions", "file": "hyprland.desktop", "type": 1, "name": "Hyprland", "exec": "Hyprland", "comment": "An intelligent dynamic tiling Wayland compositor"},
    {"directory": "/usr/share/wayland-sessions", "file": "hyprland-gamemode.desktop", "type": 1, "name": "Hyprland (with Game Mode)", "exec": "hyprland-gamemode", "comment": "Hyprland that can switch to gamescope"},
    {"directory": "/usr/share/wayland-sessions", "file": "plasma.desktop", "type": 1, "name": "Plasma (Wayland)", "exec": "startplasma-wayland", "comment": "Plasma by KDE"},
]


class Harness:
    def __init__(self, args):
        self.args = args
        self.failures = []
        self.proxy = Proxy()
        self.keyboard = Keyboard()
        self.keep = []

    def check(self, cond, what):
        print(("  ok   " if cond else "  FAIL ") + what)
        if not cond:
            self.failures.append(what)

    def make_view(self, scenario, size, users=USERS, primary=True):
        view = QQuickView()
        view.setResizeMode(QQuickView.SizeRootObjectToView)
        w, h = size
        view.resize(w, h)
        ctx = view.rootContext()
        theme_conf = os.path.join(self.args.theme, "theme.conf")
        overrides = [tuple(kv.split("=", 1)) for kv in self.args.set]
        self.config = ThemeConfig(theme_conf, self.args.user_conf, overrides)
        self.users = ListModel(["name", "realName", "homeDir", "icon", "needsPassword"], users, 1, users[1]["name"] if len(users) > 1 else "")
        self.sessions = ListModel(["directory", "file", "type", "name", "exec", "comment"], SESSIONS, 0)
        self.screens = ListModel(["name", "geometry"], [{"name": "DP-1", "geometry": QRect(0, 0, w, h)}])
        for name, obj in (("sddm", self.proxy), ("userModel", self.users), ("sessionModel", self.sessions),
                          ("screenModel", self.screens), ("keyboard", self.keyboard), ("config", self.config)):
            ctx.setContextProperty(name, obj)
        ctx.setContextProperty("primaryScreen", primary)
        ctx.setContextProperty("__sddm_errors", "")
        # Views outlive this call; keep their context objects alive with them.
        self.keep.append((view, self.config, self.users, self.sessions, self.screens))
        view.setSource(QUrl.fromLocalFile(os.path.join(self.args.theme, "Main.qml")))
        if view.status() == QQuickView.Error:
            for e in view.errors():
                print("QML error:", e.toString())
            sys.exit(2)
        view.show()
        view.requestActivate()
        QTest.qWaitForWindowActive(view, 3000)
        return view

    def wait(self, ms):
        QTest.qWait(ms)

    def shot(self, view, name):
        path = os.path.join(self.args.out, f"{name}.png")
        view.grabWindow().save(path)
        print(f"  shot {path}")

    def type(self, view, text):
        for ch in text:
            for kind in (QEvent.KeyPress, QEvent.KeyRelease):
                QGuiApplication.sendEvent(view, QKeyEvent(kind, 0, Qt.NoModifier, ch))
        QTest.qWait(20)

    def key(self, view, key):
        QTest.keyClick(view, key)
        QTest.qWait(20)

    def run(self, scenario):
        print(f"[{scenario}]")
        self.proxy.calls.clear()
        self.keyboard.capsLock = False
        size = tuple(int(x) for x in self.args.size.split("x"))
        users = [] if scenario == "manual" else USERS
        view = self.make_view(scenario, size, users, primary=not self.args.secondary)
        root = view.rootObject()
        self.wait(900)

        if scenario == "default":
            self.check(root.property("userName") == "bhima", "starts on userModel.lastUser")
            self.check(root.property("sessionIndex") == 0, "starts on sessionModel.lastIndex")
        elif scenario == "typing":
            self.type(view, "hunter")
            self.wait(200)
        elif scenario == "failed":
            self.type(view, "wrong")
            self.key(view, Qt.Key_Return)
            self.wait(100)
            self.check(root.property("busy"), "busy while the daemon answers")
            self.wait(700)
            self.check(self.proxy.calls[:1] == [("login", "bhima", "wrong", 0)], f"login() called with user, password, session: {self.proxy.calls}")
            self.check(root.property("failed") and not root.property("busy"), "shows the failure and re-enables input")
            self.type(view, "x")
            self.wait(50)
            self.check(not root.property("failed"), "typing again clears the error")
            self.key(view, Qt.Key_Backspace)
            self.type(view, "")
            root.setProperty("failed", True)
            self.wait(300)
        elif scenario == "caps":
            self.keyboard.capsLock = True
            self.type(view, "pass")
            self.wait(300)
        elif scenario == "sessions":
            self.key(view, Qt.Key_F10)
            self.wait(400)
            self.shot(view, "sessions-open")
            self.key(view, Qt.Key_Down)
            self.key(view, Qt.Key_Return)
            self.wait(400)
            self.check(root.property("sessionIndex") == 1, "picking the second session")
            self.type(view, "hunter2")
            self.key(view, Qt.Key_Return)
            self.wait(500)
            self.check(("login", "bhima", "hunter2", 1) in self.proxy.calls, f"login() uses the chosen session: {self.proxy.calls}")
        elif scenario == "users":
            self.key(view, Qt.Key_F2)
            self.wait(400)
            self.shot(view, "users-open")
            self.key(view, Qt.Key_Up)
            self.key(view, Qt.Key_Return)
            self.wait(400)
            self.check(root.property("userName") == "alex", f"picking the first user (got {root.property('userName')})")
            self.type(view, "pw")
            self.key(view, Qt.Key_Return)
            self.wait(500)
            self.check(("login", "alex", "pw", 0) in self.proxy.calls, f"login() uses the chosen user: {self.proxy.calls}")
        elif scenario == "manual":
            self.check(root.property("manualUser"), "no users listed: asks for a user name")
            self.type(view, "root")
            self.key(view, Qt.Key_Tab)
            self.type(view, "pw")
            self.wait(200)
            self.shot(view, "manual-typed")
            self.key(view, Qt.Key_Return)
            self.wait(500)
            self.check(("login", "root", "pw", 0) in self.proxy.calls, f"login() with the typed user: {self.proxy.calls}")
        elif scenario == "success":
            self.type(view, "hunter2")
            self.key(view, Qt.Key_Return)
            self.wait(800)
            self.check(root.property("signedIn") and not root.property("busy"), "fades out after loginSucceeded")
        elif scenario == "power":
            # The last button on the right island is "shut down".
            root.setProperty("armedAction", "")
            root.metaObject()
            from PySide6.QtCore import QMetaObject, Q_ARG
            QMetaObject.invokeMethod(root, "power", Q_ARG("QVariant", "poweroff"))
            self.wait(300)
            self.check(("powerOff",) not in self.proxy.calls, "first click only arms shut down")
            self.shot(view, "power-armed")
            QMetaObject.invokeMethod(root, "power", Q_ARG("QVariant", "poweroff"))
            self.wait(100)
            self.check(("powerOff",) in self.proxy.calls, "second click shuts down")
        elif scenario == "info":
            self.proxy.informationMessage.emit("Your password expires in 3 days")
            self.wait(300)

        if scenario not in ("power",):
            self.shot(view, scenario)
        view.close()
        view.deleteLater()
        self.wait(50)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("theme")
    p.add_argument("--out", default=".")
    p.add_argument("--scenario", nargs="*", default=["default"])
    p.add_argument("--size", default="1920x1080")
    p.add_argument("--set", action="append", default=[], help="theme.conf key=value override")
    p.add_argument("--secondary", action="store_true", help="draw as a non-primary screen")
    p.add_argument("--user-conf", default=None, help="theme.conf.user to read (default: next to theme.conf)")
    args = p.parse_args()
    args.theme = os.path.abspath(args.theme)
    os.makedirs(args.out, exist_ok=True)

    app = QGuiApplication(sys.argv)
    h = Harness(args)
    for s in args.scenario:
        h.run(s)
    if h.failures:
        print(f"{len(h.failures)} check(s) failed")
        return 1
    print("all checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
