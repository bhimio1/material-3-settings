import QtQuick

// A Material Symbols icon by ligature name, like the shell's MaterialSymbol.
// Without the icon font (missing file, broken install) it shows a plain
// Unicode stand-in so every button still says what it does.
Text {
    id: icon
    property string name: ""
    property real size: 24 * M3.s
    property bool filled: false

    readonly property var fallbacks: ({
        "arrow_right_alt": "→", "power_settings_new": "⏻", "restart_alt": "↻",
        "dark_mode": "☾", "keyboard_alt": "⌨", "keyboard_capslock": "⇪",
        "expand_more": "▾", "expand_less": "▴", "check": "✓", "person": "☺",
        "desktop_windows": "▣", "sports_esports": "◉", "error": "!", "info": "i",
        "progress_activity": "◌", "person_add": "+", "chevron_left": "‹", "chevron_right": "›",
        "water_drop": "◆", "terminal": ">_"
    })
    readonly property bool hasFont: M3.iconFamily.length > 0

    text: hasFont ? name : (fallbacks[name] ?? "•")
    color: M3.colOnSurfaceVariant
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
    renderType: Text.QtRendering
    font.family: hasFont ? M3.iconFamily : M3.fontFamily
    font.pixelSize: hasFont ? size : size * 0.8
    font.weight: filled ? Font.DemiBold : Font.Normal
    font.hintingPreference: Font.PreferNoHinting
    font.variableAxes: hasFont ? ({ "FILL": filled ? 1 : 0, "opsz": Math.min(48, Math.max(20, size)) }) : ({})
}
