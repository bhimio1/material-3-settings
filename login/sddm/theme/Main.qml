// end4-material: an SDDM login screen that looks like end4-pC's lock screen.
//
// SDDM creates one view of this file per screen and hands it the "sddm",
// "userModel", "sessionModel", "screenModel", "keyboard" and "config"
// context properties. Wallpaper, colours and fonts come from the sync folder
// that sddm/sync.sh fills; everything has a default so the screen still
// works when that folder is empty or its files are broken.
import QtQuick
import QtQuick.Effects
import QtQuick.Layouts
import QtQml.Models
import "components"

Rectangle {
    id: root
    width: 1920
    height: 1080
    color: M3.background

    // ---- Per-screen scale ----------------------------------------------------
    Binding {
        target: M3
        property: "s"
        value: Math.max(0.75, Math.min(2.0, Math.min(root.width / 1920, root.height / 1080)))
    }
    readonly property real s: M3.s

    readonly property bool effectsAvailable: GraphicsInfo.api !== GraphicsInfo.Software && GraphicsInfo.api !== GraphicsInfo.Unknown
    readonly property bool showForm: M3.cfgBool("formOnAllScreens", true) || typeof primaryScreen === "undefined" || primaryScreen

    // ---- Fonts -----------------------------------------------------------------
    FontLoader {
        id: iconFont
        source: Qt.resolvedUrl("fonts/MaterialSymbolsLite.ttf")
        onStatusChanged: {
            if (status === FontLoader.Ready) M3.iconFamily = name
            M3.fontsRevision++
        }
        Component.onCompleted: if (status === FontLoader.Ready) M3.iconFamily = name
    }
    // Fonts that only exist in the user's home are copied by sync.sh.
    Instantiator {
        model: M3.cfg("fontFiles", "").split(";").filter(f => /^font-[0-9]\.(ttf|otf)$/.test(f))
        delegate: FontLoader {
            required property string modelData
            source: M3.syncUrl(modelData)
            onStatusChanged: M3.fontsRevision++
        }
    }

    // ---- Users -------------------------------------------------------------------
    Instantiator {
        id: users
        model: typeof userModel !== "undefined" ? userModel : null
        delegate: QtObject {
            required property int index
            required property var model
            readonly property string uname: model.name ?? ""
            readonly property string realName: model.realName ?? ""
            readonly property string icon: String(model.icon ?? "")
            readonly property bool needsPassword: model.needsPassword ?? true
        }
    }
    property int userIndex: -1
    property bool manualUser: false
    readonly property var currentUser: userIndex >= 0 && userIndex < users.count ? users.objectAt(userIndex) : null
    readonly property string userName: manualUser ? userField.text.trim() : (currentUser ? currentUser.uname : "")
    readonly property string displayName: manualUser ? qsTr("Other user")
        : currentUser ? (currentUser.realName.length > 0 ? currentUser.realName : currentUser.uname) : ""

    function avatarSources(u) {
        if (!u) return []
        const list = []
        // The synced copy of the owner's avatar: the greeter can't read ~/.face.icon.
        if (u.uname === M3.cfg("syncUser", "") && M3.syncUrl(M3.cfg("avatar", "")))
            list.push(M3.syncUrl(M3.cfg("avatar", "")))
        // SDDM's generic face means "no picture": show the initial instead.
        if (u.icon.length > 0 && !/\/faces\/\.face\.icon$/.test(u.icon))
            list.push(u.icon)
        return list
    }
    function initialOf(u) {
        const n = u ? (u.realName || u.uname) : ""
        return n.length > 0 ? n.charAt(0).toUpperCase() : "?"
    }

    function pickInitialUser() {
        if (typeof userModel === "undefined" || users.count === 0) {
            manualUser = true
            return
        }
        let i = userModel.lastIndex ?? 0
        if (userModel.lastUser) {
            for (let k = 0; k < users.count; k++)
                if (users.objectAt(k).uname === userModel.lastUser) { i = k; break }
        }
        userIndex = (i >= 0 && i < users.count) ? i : 0
    }

    // ---- Sessions ----------------------------------------------------------------
    Instantiator {
        id: sessions
        model: typeof sessionModel !== "undefined" ? sessionModel : null
        delegate: QtObject {
            required property int index
            required property var model
            readonly property string sname: model.name ?? ""
            readonly property string comment: model.comment ?? ""
            readonly property string file: model.file ?? ""
        }
    }
    property int sessionIndex: -1
    readonly property var currentSession: sessionIndex >= 0 && sessionIndex < sessions.count ? sessions.objectAt(sessionIndex) : null
    function sessionIcon(sess) {
        const n = ((sess?.sname ?? "") + " " + (sess?.file ?? "")).toLowerCase()
        if (n.indexOf("game") >= 0 || n.indexOf("gamescope") >= 0 || n.indexOf("steam") >= 0) return "sports_esports"
        if (n.indexOf("hypr") >= 0) return "water_drop"
        if (n.indexOf("shell") >= 0 || n.indexOf("tty") >= 0) return "terminal"
        return "desktop_windows"
    }

    // ---- Keyboard ------------------------------------------------------------------
    readonly property bool hasKeyboard: typeof keyboard !== "undefined" && keyboard !== null
    readonly property bool capsLock: hasKeyboard && !!keyboard.capsLock
    readonly property var layouts: hasKeyboard && keyboard.enabled !== false && keyboard.layouts ? keyboard.layouts : []
    readonly property string layoutName: {
        if (layouts.length === 0) return ""
        const l = layouts[Math.max(0, Math.min(layouts.length - 1, keyboard.currentLayout))]
        return l ? String(l.shortName || l.longName || "").toUpperCase() : ""
    }

    // ---- Login -------------------------------------------------------------------
    property bool busy: false
    property bool failed: false
    property string message: ""
    readonly property bool canLogin: !busy && userName.length > 0

    function login() {
        if (!canLogin) {
            if (manualUser && userName.length === 0) userField.forceActiveFocus()
            return
        }
        if (typeof sddm === "undefined" || !sddm) return
        busy = true
        failed = false
        message = ""
        sddm.login(userName, passwordField.text, Math.max(0, sessionIndex))
    }
    // A stuck daemon must never leave the field disabled for good.
    Timer {
        id: busyTimeout
        interval: 30000
        running: root.busy
        onTriggered: root.busy = false
    }

    Connections {
        target: typeof sddm !== "undefined" ? sddm : null
        ignoreUnknownSignals: true
        function onLoginFailed() {
            root.busy = false
            root.failed = true
            passwordField.text = ""
            shake.restart()
            passwordField.forceActiveFocus()
        }
        function onLoginSucceeded() {
            root.busy = false
            root.signedIn = true
            fadeOut.start()
        }
        function onInformationMessage(message) {
            root.message = String(message)
        }
    }

    // Power buttons need a second click within 4 s, so a stray click on the
    // login screen doesn't shut the machine down.
    property string armedAction: ""
    Timer { id: disarm; interval: 4000; onTriggered: root.armedAction = "" }
    function power(action) {
        if (armedAction !== action) {
            armedAction = action
            disarm.restart()
            return
        }
        armedAction = ""
        if (typeof sddm === "undefined" || !sddm) return
        if (action === "poweroff") sddm.powerOff()
        else if (action === "reboot") sddm.reboot()
        else if (action === "suspend") sddm.suspend()
    }

    Component.onCompleted: {
        pickInitialUser()
        const last = typeof sessionModel !== "undefined" ? (sessionModel.lastIndex ?? 0) : 0
        sessionIndex = sessions.count > 0 ? ((last >= 0 && last < sessions.count) ? last : 0) : -1
        if (manualUser) userField.forceActiveFocus()
        else passwordField.forceActiveFocus()
        intro.start()
    }

    // ==== Background: blurred wallpaper, like the shell's locked desktop =====
    Item {
        id: backdrop
        anchors.fill: parent

        // Shown when there's no wallpaper at all: a soft wash of the palette.
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: M3.mix(M3.primaryContainer, M3.background, 0.55) }
                GradientStop { position: 1; color: M3.background }
            }
        }

        readonly property bool blur: M3.cfgBool("blur", true)
        readonly property real zoom: blur ? 1.1 : 1.0
        readonly property size decodeSize: Qt.size(Math.ceil(root.width * Screen.devicePixelRatio), Math.ceil(root.height * Screen.devicePixelRatio))

        Image {
            id: sharpWall
            anchors.fill: parent
            scale: backdrop.zoom
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
            sourceSize: backdrop.decodeSize
            // The synced wallpaper first, then a background set in theme.conf.
            property int attempt: 0
            readonly property var candidates: [M3.syncUrl(M3.cfg("wallpaper", "")),
                M3.cfg("background", "").length > 0 ? Qt.resolvedUrl(M3.cfg("background", "")) : ""].filter(u => String(u).length > 0)
            source: attempt < candidates.length ? candidates[attempt] : ""
            onStatusChanged: if (status === Image.Error) attempt++
            visible: status === Image.Ready && !(backdrop.blur && (blurWall.status === Image.Ready || root.effectsAvailable))
        }

        // sync.sh pre-blurs the wallpaper, so no shader is needed.
        Image {
            id: blurWall
            anchors.fill: parent
            scale: backdrop.zoom
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            source: backdrop.blur ? M3.syncUrl(M3.cfg("wallpaperBlurred", "")) : ""
            visible: backdrop.blur && status === Image.Ready
        }

        // Otherwise blur on the GPU, if there is one.
        MultiEffect {
            anchors.fill: sharpWall
            scale: backdrop.zoom
            source: sharpWall
            visible: backdrop.blur && root.effectsAvailable && blurWall.status !== Image.Ready && sharpWall.status === Image.Ready
            blurEnabled: true
            blur: 1.0
            blurMax: 64
            autoPaddingEnabled: false
        }

        // The lock screen dims the wallpaper with its layer-0 colour at 30%.
        Rectangle {
            anchors.fill: parent
            color: M3.background
            opacity: backdrop.blur ? 0.3 : 0.18
        }
    }

    // Clicking anywhere gives the keyboard back to the password field.
    MouseArea {
        anchors.fill: parent
        onClicked: (root.manualUser && root.userName.length === 0 ? userField : passwordField).forceActiveFocus()
    }

    // ==== Clock ====================================================================
    property date now: new Date()
    Timer {
        interval: 1000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: root.now = new Date()
    }
    function safeFormat(key, fallback) {
        const f = M3.cfg(key, fallback)
        return f.length <= 40 && !/[<>{}]/.test(f) ? f : fallback
    }

    Column {
        id: clock
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.height * 0.14
        spacing: -10 * root.s
        opacity: 0

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.locale().toString(root.now, root.safeFormat("timeFormat", "hh:mm"))
            color: M3.colOnBackground
            font.family: M3.clockFamily
            font.pixelSize: 120 * root.s
            font.weight: M3.clockWeight
            style: Text.Raised
            styleColor: M3.alpha(M3.shadow, 0.3)
            renderType: Text.QtRendering
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.locale().toString(root.now, root.safeFormat("dateFormat", "dddd, d MMMM"))
            color: M3.colOnBackground
            font.family: M3.clockFamily
            font.pixelSize: 24 * root.s
            font.weight: 400
            style: Text.Raised
            styleColor: M3.alpha(M3.shadow, 0.3)
        }
    }

    // ==== Who's logging in =========================================================
    Column {
        id: identity
        visible: root.showForm
        anchors.horizontalCenter: parent.horizontalCenter
        // Centred on the screen, but never touching the clock or the toolbars.
        y: Math.max(clock.y + clock.height + 24 * root.s,
                    Math.min(root.height * 0.53 - height / 2, statusArea.y - height - 24 * root.s))
        spacing: 14 * root.s
        opacity: 0

        Avatar {
            id: bigAvatar
            anchors.horizontalCenter: parent.horizontalCenter
            implicitWidth: 112 * root.s
            sources: root.manualUser ? [] : root.avatarSources(root.currentUser)
            initial: root.manualUser ? "?" : root.initialOf(root.currentUser)
            ringWidth: 3 * root.s
            ringColor: M3.alpha(M3.colOnBackground, 0.85)
            MouseArea {
                anchors.fill: parent
                cursorShape: users.count > 1 ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: root.openUserMenu(nameChip)
            }
        }

        // Name, which opens the user list when there's more than one user.
        Rectangle {
            id: nameChip
            visible: !root.manualUser
            anchors.horizontalCenter: parent.horizontalCenter
            height: 44 * root.s
            width: nameRow.implicitWidth + 32 * root.s
            radius: height / 2
            readonly property bool clickable: users.count > 1
            color: clickable && (nameMouse.containsMouse || nameChip.activeFocus) ? M3.alpha(M3.surfaceContainer, 0.75) : "transparent"
            activeFocusOnTab: clickable
            Keys.onReturnPressed: root.openUserMenu(nameChip)
            Keys.onSpacePressed: root.openUserMenu(nameChip)
            Row {
                id: nameRow
                anchors.centerIn: parent
                spacing: 4 * root.s
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.displayName
                    color: M3.colOnBackground
                    font.family: M3.fontFamily
                    font.pixelSize: M3.fontTitle
                    font.weight: 550
                    style: Text.Raised
                    styleColor: M3.alpha(M3.shadow, 0.25)
                }
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    visible: nameChip.clickable
                    name: userMenu.open ? "expand_less" : "expand_more"
                    color: M3.colOnBackground
                    size: 26 * root.s
                }
            }
            MouseArea {
                id: nameMouse
                anchors.fill: parent
                hoverEnabled: true
                enabled: nameChip.clickable
                cursorShape: Qt.PointingHandCursor
                onClicked: root.openUserMenu(nameChip)
            }
        }

        // Typing a user name, when the list is empty or "Other user" was picked.
        Rectangle {
            visible: root.manualUser
            anchors.horizontalCenter: parent.horizontalCenter
            width: 280 * root.s
            height: 48 * root.s
            radius: height / 2
            color: M3.toolbar
            border.width: userField.activeFocus ? 2 * root.s : 0
            border.color: M3.primary
            TextInput {
                id: userField
                anchors.fill: parent
                anchors.leftMargin: 20 * root.s
                anchors.rightMargin: 20 * root.s
                verticalAlignment: TextInput.AlignVCenter
                horizontalAlignment: TextInput.AlignHCenter
                color: M3.colOnSurface
                selectionColor: M3.secondaryContainer
                selectedTextColor: M3.colOnSecondaryContainer
                font.family: M3.fontFamily
                font.pixelSize: M3.fontLarge
                clip: true
                maximumLength: 64
                inputMethodHints: Qt.ImhNoAutoUppercase | Qt.ImhNoPredictiveText
                KeyNavigation.tab: passwordField
                onAccepted: passwordField.forceActiveFocus()
                Keys.onEscapePressed: {
                    if (users.count > 0) { root.manualUser = false; passwordField.forceActiveFocus() }
                }
                Text {
                    anchors.fill: parent
                    verticalAlignment: Text.AlignVCenter
                    horizontalAlignment: Text.AlignHCenter
                    visible: userField.text.length === 0
                    text: qsTr("User name")
                    color: M3.subtext
                    font: userField.font
                }
            }
        }
    }

    // ==== Status line above the toolbars (errors, caps lock, power hints) ===
    Item {
        id: statusArea
        visible: root.showForm
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: bottomBars.top
        anchors.bottomMargin: 14 * root.s
        width: statusChip.width
        height: 36 * root.s

        readonly property string kind: root.armedAction.length > 0 ? "power"
            : root.failed ? "error"
            : root.message.length > 0 ? "info"
            : root.capsLock ? "caps" : ""
        readonly property string text: {
            switch (kind) {
            case "power": return root.armedAction === "poweroff" ? qsTr("Click again to shut down")
                               : root.armedAction === "reboot" ? qsTr("Click again to restart") : qsTr("Click again to suspend")
            case "error": return root.message.length > 0 ? root.message : qsTr("Incorrect password")
            case "info": return root.message
            case "caps": return qsTr("Caps Lock is on")
            }
            return ""
        }

        Rectangle {
            id: statusChip
            anchors.centerIn: parent
            height: parent.height
            width: statusRow.implicitWidth + 28 * root.s
            radius: height / 2
            visible: opacity > 0
            opacity: statusArea.kind.length > 0 ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 180 } }
            color: statusArea.kind === "error" ? M3.errorContainer
                 : statusArea.kind === "caps" || statusArea.kind === "power" ? M3.tertiaryContainer : M3.secondaryContainer
            readonly property color fg: statusArea.kind === "error" ? M3.colOnErrorContainer
                 : statusArea.kind === "caps" || statusArea.kind === "power" ? M3.colOnTertiaryContainer : M3.colOnSecondaryContainer
            Row {
                id: statusRow
                anchors.centerIn: parent
                spacing: 8 * root.s
                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: statusArea.kind === "error" ? "error" : statusArea.kind === "caps" ? "keyboard_capslock"
                        : statusArea.kind === "power" ? "power_settings_new" : "info"
                    filled: true
                    color: statusChip.fg
                    size: 20 * root.s
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: statusArea.text
                    color: statusChip.fg
                    font.family: M3.fontFamily
                    font.pixelSize: M3.fontSmall
                    font.weight: Font.Medium
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth, root.width * 0.7)
                }
            }
        }
    }

    // ==== Toolbars, laid out like the lock screen's three islands ============
    Item {
        id: bottomBars
        visible: root.showForm
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 20 * root.s
        width: root.width
        opacity: 0

        readonly property real gap: 10 * root.s
        // On narrow screens the side islands sit in a row above the main one.
        readonly property bool stacked: leftBar.implicitWidth + mainBar.implicitWidth + rightBar.implicitWidth + gap * 2 > root.width - 32 * root.s
        height: stacked ? M3.toolbarHeight * 2 + gap : M3.toolbarHeight

        Toolbar {
            id: mainBar
            anchors.bottom: parent.bottom
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.horizontalCenterOffset: 0

            Rectangle {
                id: field
                Layout.fillHeight: true
                implicitWidth: 260 * root.s
                radius: height / 2
                color: M3.layer1
                border.width: root.failed ? 2 * root.s : 0
                border.color: M3.error

                TextInput {
                    id: passwordField
                    anchors.fill: parent
                    anchors.leftMargin: 16 * root.s
                    anchors.rightMargin: 16 * root.s
                    verticalAlignment: TextInput.AlignVCenter
                    echoMode: TextInput.Password
                    passwordCharacter: "●"
                    passwordMaskDelay: 0
                    color: M3.colOnSurface
                    selectionColor: M3.secondaryContainer
                    selectedTextColor: M3.colOnSecondaryContainer
                    font.family: M3.fontFamily
                    font.pixelSize: M3.fontSmall
                    font.letterSpacing: 2 * root.s
                    clip: true
                    focus: true
                    enabled: !root.busy
                    inputMethodHints: Qt.ImhSensitiveData | Qt.ImhNoPredictiveText | Qt.ImhHiddenText
                    onTextEdited: { root.failed = false; root.message = "" }
                    onAccepted: root.login()
                    Keys.onEscapePressed: text = ""
                    KeyNavigation.tab: loginButton

                    Text {
                        anchors.fill: parent
                        verticalAlignment: Text.AlignVCenter
                        visible: passwordField.text.length === 0
                        text: root.busy ? qsTr("Signing in…")
                            : (root.manualUser || (root.currentUser?.needsPassword ?? true)) ? qsTr("Enter password")
                            : qsTr("Press Enter to sign in")
                        color: root.failed ? M3.error : M3.subtext
                        font.family: M3.fontFamily
                        font.pixelSize: M3.fontSmall
                        elide: Text.ElideRight
                    }
                }
            }

            ToolbarButton {
                id: loginButton
                icon: "arrow_right_alt"
                toggled: true
                busy: root.busy
                tooltip: qsTr("Sign in")
                enabled: root.canLogin
                onClicked: root.login()
            }

            SequentialAnimation {
                id: shake
                readonly property real d: 12 * root.s
                NumberAnimation { target: mainBar; property: "anchors.horizontalCenterOffset"; to: -shake.d; duration: 50 }
                NumberAnimation { target: mainBar; property: "anchors.horizontalCenterOffset"; to: shake.d; duration: 70 }
                NumberAnimation { target: mainBar; property: "anchors.horizontalCenterOffset"; to: -shake.d * 0.6; duration: 70 }
                NumberAnimation { target: mainBar; property: "anchors.horizontalCenterOffset"; to: shake.d * 0.3; duration: 60 }
                NumberAnimation { target: mainBar; property: "anchors.horizontalCenterOffset"; to: 0; duration: 50 }
            }
        }

        // Left island: session and keyboard layout.
        Toolbar {
            id: leftBar
            anchors.bottom: bottomBars.stacked ? mainBar.top : parent.bottom
            anchors.bottomMargin: bottomBars.stacked ? bottomBars.gap : 0
            x: bottomBars.stacked ? (parent.width - width - rightBar.width - bottomBars.gap) / 2
                                  : mainBar.x - bottomBars.gap - width

            ToolbarButton {
                id: sessionButton
                visible: sessions.count > 0
                icon: root.sessionIcon(root.currentSession)
                text: root.currentSession ? root.currentSession.sname : ""
                tooltip: qsTr("Session")
                onClicked: root.openSessionMenu(sessionButton)
            }
            ToolbarButton {
                id: layoutButton
                visible: root.layoutName.length > 0
                icon: "keyboard_alt"
                text: root.layoutName
                tooltip: qsTr("Keyboard layout")
                onClicked: {
                    if (root.layouts.length > 1)
                        keyboard.currentLayout = (keyboard.currentLayout + 1) % root.layouts.length
                    passwordField.forceActiveFocus()
                }
            }
            ToolbarButton {
                visible: sessions.count === 0 && root.layoutName.length === 0
                icon: "desktop_windows"
                text: typeof sddm !== "undefined" && sddm ? sddm.hostName : ""
                enabled: false
            }
        }

        // Right island: suspend, restart, shut down.
        Toolbar {
            id: rightBar
            anchors.bottom: leftBar.bottom
            x: bottomBars.stacked ? leftBar.x + leftBar.width + bottomBars.gap
                                  : mainBar.x + mainBar.width + bottomBars.gap

            ToolbarButton {
                visible: typeof sddm !== "undefined" && !!sddm && sddm.canSuspend
                icon: "dark_mode"
                tooltip: qsTr("Suspend")
                toggled: root.armedAction === "suspend"
                onClicked: root.power("suspend")
            }
            ToolbarButton {
                visible: typeof sddm !== "undefined" && !!sddm && sddm.canReboot
                icon: "restart_alt"
                tooltip: qsTr("Restart")
                toggled: root.armedAction === "reboot"
                onClicked: root.power("reboot")
            }
            ToolbarButton {
                visible: typeof sddm !== "undefined" && !!sddm && sddm.canPowerOff
                icon: "power_settings_new"
                tooltip: qsTr("Shut down")
                toggled: root.armedAction === "poweroff"
                onClicked: root.power("poweroff")
            }
        }
    }

    // ==== Menus ======================================================================
    function openUserMenu(anchorItem) {
        if (users.count <= 1 && !manualUser) return
        const entries = []
        for (let i = 0; i < users.count; i++) {
            const u = users.objectAt(i)
            entries.push({ text: u.realName || u.uname, subtitle: u.realName ? u.uname : "",
                           avatar: avatarSources(u), initial: initialOf(u),
                           selected: !manualUser && i === userIndex })
        }
        entries.push({ icon: "person_add", text: qsTr("Other user…"), selected: manualUser })
        userMenu.entries = entries
        const p = anchorItem.mapToItem(root, 0, 0)
        userMenu.cardX = p.x + anchorItem.width / 2 - 140 * s
        userMenu.cardBottom = p.y - 8 * s
        userMenu.open = true
    }
    function openSessionMenu(anchorItem) {
        const entries = []
        for (let i = 0; i < sessions.count; i++) {
            const sess = sessions.objectAt(i)
            entries.push({ icon: sessionIcon(sess), text: sess.sname, subtitle: sess.comment, selected: i === sessionIndex })
        }
        sessionMenu.entries = entries
        const p = anchorItem.mapToItem(root, 0, 0)
        sessionMenu.cardX = p.x
        sessionMenu.cardBottom = p.y - 16 * s
        sessionMenu.open = true
    }

    MenuSheet {
        id: userMenu
        title: qsTr("Users")
        onPicked: index => {
            close()
            root.failed = false
            root.message = ""
            passwordField.text = ""
            if (index >= users.count) {
                root.manualUser = true
                userField.text = ""
                userField.forceActiveFocus()
            } else {
                root.manualUser = false
                root.userIndex = index
                passwordField.forceActiveFocus()
            }
        }
        onOpenChanged: if (!open) (root.manualUser && root.userName.length === 0 ? userField : passwordField).forceActiveFocus()
    }
    MenuSheet {
        id: sessionMenu
        title: qsTr("Sessions")
        onPicked: index => {
            close()
            root.sessionIndex = index
        }
        onOpenChanged: if (!open) passwordField.forceActiveFocus()
    }

    // ==== Motion =========================================================================
    ParallelAnimation {
        id: intro
        NumberAnimation { target: clock; property: "opacity"; to: 1; duration: 400 }
        NumberAnimation { target: identity; property: "opacity"; to: 1; duration: 400 }
        NumberAnimation { target: bottomBars; property: "opacity"; to: 1; duration: 300 }
        NumberAnimation {
            target: bottomBars; property: "anchors.bottomMargin"
            from: 0; to: 20 * root.s; duration: 500
            easing.type: Easing.BezierSpline; easing.bezierCurve: M3.curveSpatial
        }
    }
    // After a successful login SDDM closes the view; fade to black meanwhile.
    property bool signedIn: false
    Rectangle {
        id: curtain
        anchors.fill: parent
        z: 1000
        color: "black"
        opacity: 0
        visible: opacity > 0
        MouseArea { anchors.fill: parent; enabled: curtain.visible }
    }
    NumberAnimation { id: fadeOut; target: curtain; property: "opacity"; to: 1; duration: 300 }

    // F10 opens the session list, F2 the user list, like other greeters.
    Shortcut { sequence: "F10"; onActivated: root.openSessionMenu(sessionButton) }
    Shortcut { sequence: "F2"; onActivated: root.openUserMenu(nameChip) }
}
