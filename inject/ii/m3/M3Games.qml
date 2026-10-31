pragma Singleton
import qs
import qs.m3
import qs.modules.common
import qs.modules.common.functions
import QtQuick
import Quickshell
import Quickshell.Io

/*
 * The game library of Material 3 Games (m3-games). The app keeps its library
 * in ~/.cache/m3-games/library.json, which this reads and watches, so the
 * shell never has to scan anything itself. Launching and Game Mode go through
 * the m3-games command line.
 */
Singleton {
    id: root

    // The app's own cache folder (Directories.cache is the shell's, ~/.cache/quickshell).
    readonly property string libraryPath: `${Quickshell.env("XDG_CACHE_HOME") || Quickshell.env("HOME") + "/.cache"}/m3-games/library.json`
    property var games: []
    readonly property var recent: games.filter(g => g.last_played).slice(0, 8)
    property bool available: false
    property bool gameModeActive: false
    property bool sessionReady: false
    property var runningGame: null

    function load() {}

    function run(args) {
        Quickshell.execDetached(["bash", "-c", `PATH="$HOME/.local/bin:$PATH"; exec m3-games "$@"`, "m3-games"].concat(args));
    }

    function launch(id) {
        root.run(["launch", id]);
    }

    /** Leaves Hyprland for gamescope (or nests it when not in the m3 session). */
    function enterGameMode() {
        root.run(["game-mode"]);
    }

    function openApp() {
        root.run([]);
    }

    /** Ask the app to rescan; it rewrites library.json, which we reload. */
    function rescan() {
        rescanProc.running = true;
    }

    function refreshStatus() {
        if (!statusProc.running) statusProc.running = true;
    }

    function coverUrl(game) {
        return game && game.cover ? `file://${game.cover}` : "";
    }

    FileView {
        id: libraryFile
        path: root.libraryPath
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                const parsed = JSON.parse(libraryFile.text());
                root.games = Array.isArray(parsed) ? parsed : [];
            } catch (e) {
                console.warn("[Games] Couldn't parse library.json:", e);
            }
        }
        onLoadFailed: root.games = []
    }

    Process {
        id: availableProc
        running: true
        command: ["bash", "-c", `PATH="$HOME/.local/bin:$PATH"; command -v m3-games`]
        onExited: (exitCode, exitStatus) => {
            root.available = exitCode === 0;
            if (root.available) root.refreshStatus();
        }
    }

    Process {
        id: rescanProc
        command: ["bash", "-c", `PATH="$HOME/.local/bin:$PATH"; m3-games list --json >/dev/null`]
        onExited: libraryFile.reload()
    }

    Process {
        id: statusProc
        command: ["bash", "-c", `PATH="$HOME/.local/bin:$PATH"; m3-games status --json`]
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const s = JSON.parse(text);
                    root.gameModeActive = !!s.game_mode;
                    root.sessionReady = !!s.session_ready;
                    root.runningGame = s.running_game ?? null;
                } catch (e) {}
            }
        }
    }

    // Keep the running-game state fresh while the library panel is open.
    Timer {
        interval: 5000
        repeat: true
        running: root.available && M3.gamesOpen
        triggeredOnStart: true
        onTriggered: root.refreshStatus()
    }
}
