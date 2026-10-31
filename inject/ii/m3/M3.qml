pragma Singleton
import qs
import qs.modules.common
import qs.modules.common.functions
import QtQuick
import Quickshell
import Quickshell.Io

/*
 * Shared state and helpers for the Material 3 integration, added by
 * Material 3 Settings. The hooks in the shell's own files call into this,
 * so they stay one or two lines each.
 *
 * ~/.local/bin is put on PATH because that's where Settings installs apps.
 */
Singleton {
    id: root

    property bool gamesOpen: false
    /** Material 3 Screenshots is installed. */
    property bool screenshotsAvailable: false
    /** Material 3 Settings is installed. */
    property bool settingsAvailable: false

    readonly property string pathPrelude: `PATH="$HOME/.local/bin:$PATH"`

    /**
     * Starts one of the Material 3 apps by binary name, with arguments. When
     * it isn't installed, opens its page in Settings, where it can be.
     */
    function open(program, args) {
        Quickshell.execDetached(["bash", "-c",
            `${root.pathPrelude}
            if command -v "$1" >/dev/null; then shift; exec "$@"; fi
            if command -v m3-settings >/dev/null; then exec m3-settings material-apps "$1"; fi
            notify-send -a Shell "$1 isn't installed" "Install Material 3 Settings to add the Material 3 apps."`,
            "bash", program].concat([program]).concat(args ?? []));
    }

    /** Material 3 Settings, optionally on a page ("network", "material-apps m3-games"). */
    function openSettings(page) {
        root.open("m3-settings", (page ?? "").split(" ").filter(s => s !== ""));
    }

    /** Settings' Material 3 apps page, optionally on one app. */
    function manage(program) {
        root.openSettings(program ? `material-apps ${program}` : "material-apps");
    }

    /**
     * The command for a region capture when Material 3 Screenshots should
     * handle it, or null to let the shell do what it always did.
     * @param {string} kind "copy" or "edit"
     */
    function screenshotCommand(kind, cropToStdout, cleanup) {
        if (!root.screenshotsAvailable || (kind !== "copy" && kind !== "edit"))
            return null;
        const edit = kind === "edit" ? " --edit" : "";
        return ["bash", "-c", `${root.pathPrelude}; ${cropToStdout} | m3-screenshots import -${edit}; ${cleanup}`];
    }

    /**
     * The first-run welcome: Settings' fuller one when it's installed, the
     * shell's own welcome.qml otherwise. Settings keeps first_run.txt the
     * same way welcome.qml does.
     */
    function welcomeCommand(welcomeQmlPath) {
        return `${root.pathPrelude}; if command -v m3-settings >/dev/null; then exec m3-settings --welcome; `
            + `else exec qs -p '${StringUtils.shellSingleQuoteEscape(welcomeQmlPath)}'; fi`;
    }

    Process {
        running: true
        command: ["bash", "-c", `${root.pathPrelude}; command -v m3-screenshots >/dev/null && echo s; command -v m3-settings >/dev/null && echo t; true`]
        stdout: StdioCollector {
            onStreamFinished: {
                root.screenshotsAvailable = text.includes("s");
                root.settingsAvailable = text.includes("t");
            }
        }
    }
}
