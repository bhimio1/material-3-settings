pragma Singleton
import qs
import qs.m3
import qs.modules.common.functions
import QtQuick
import Quickshell
import Quickshell.Io

/*
 * Events from Material 3 Calendar (m3-calendar), for the sidebar calendar.
 * Reads them with `m3-calendar events --json`, which asks the calendar
 * daemon when it runs and reads its offline cache otherwise, and refreshes
 * when the daemon's org.material3.Calendar.Changed signal fires. Without
 * the app installed this stays empty and the calendar looks as it always did.
 */
Singleton {
    id: root

    property bool available: false
    /** "yyyy-MM-dd" → colors of that day's events (at most 3). */
    property var dayColors: ({})
    property date rangeStart: new Date()
    property int rangeDays: 42

    function load() {}

    function isoDate(d) {
        return Qt.formatDate(d, "yyyy-MM-dd");
    }

    /** Fetch the days the calendar grid shows. */
    function setRange(start, days) {
        if (root.isoDate(start) === root.isoDate(root.rangeStart) && days === root.rangeDays && Object.keys(root.dayColors).length > 0)
            return;
        root.rangeStart = start;
        root.rangeDays = days;
        root.refresh();
    }

    function refresh() {
        if (!root.available) return;
        const end = new Date(root.rangeStart);
        end.setDate(end.getDate() + root.rangeDays - 1);
        eventsProc.running = false;
        eventsProc.command = ["bash", "-c", `PATH="$HOME/.local/bin:$PATH"; exec m3-calendar events --from "$1" --to "$2" --json`,
            "bash", root.isoDate(root.rangeStart), root.isoDate(end)];
        eventsProc.running = true;
    }

    /** Open the app, on a day when given. */
    function open(date) {
        M3.open("m3-calendar", date ? ["--view", "day", "--date", root.isoDate(date)] : []);
    }

    function colorsFor(date) {
        return root.dayColors[root.isoDate(date)] ?? [];
    }

    function index(events) {
        const result = {};
        for (const e of events) {
            const day = new Date(e.start * 1000);
            day.setHours(0, 0, 0, 0);
            // An event ending exactly at midnight doesn't touch the next day.
            const last = new Date(Math.max(e.start, e.end - 1) * 1000);
            for (let i = 0; day <= last && i < root.rangeDays; i++) {
                const key = root.isoDate(day);
                const colors = result[key] ?? (result[key] = []);
                if (colors.length < 3) colors.push(e.color || "");
                day.setDate(day.getDate() + 1);
            }
        }
        return result;
    }

    Process {
        running: true
        command: ["bash", "-c", `PATH="$HOME/.local/bin:$PATH"; command -v m3-calendar`]
        onExited: (exitCode, exitStatus) => {
            root.available = exitCode === 0;
            if (root.available) {
                root.refresh();
                changedWatcher.running = true;
            }
        }
    }

    Process {
        id: eventsProc
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const parsed = JSON.parse(text);
                    if (Array.isArray(parsed)) root.dayColors = root.index(parsed);
                } catch (e) {
                    console.warn("[CalendarEvents] Couldn't read events:", e);
                }
            }
        }
    }

    // Live updates: the daemon emits Changed after syncs and edits.
    Process {
        id: changedWatcher
        command: ["gdbus", "monitor", "--session", "--dest", "org.material3.Calendar", "--object-path", "/org/material3/Calendar"]
        stdout: SplitParser {
            onRead: line => {
                if (line.includes("Changed")) debounce.restart();
            }
        }
        onExited: restartWatcher.start()
    }
    Timer {
        id: restartWatcher
        interval: 60 * 1000
        onTriggered: changedWatcher.running = true
    }
    Timer {
        id: debounce
        interval: 500
        onTriggered: root.refresh()
    }

    // Catch up after the day changes or while the daemon isn't running.
    Timer {
        interval: 5 * 60 * 1000
        repeat: true
        running: root.available && GlobalStates.sidebarRightOpen
        onTriggered: root.refresh()
    }
    Connections {
        target: GlobalStates
        function onSidebarRightOpenChanged() {
            if (GlobalStates.sidebarRightOpen) root.refresh();
        }
    }
}
