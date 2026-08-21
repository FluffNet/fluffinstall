import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window
import org.flufflinux.installer

ApplicationWindow {
    id: window
    width: 920
    height: 600
    minimumWidth: 920
    minimumHeight: 600
    maximumWidth: 920
    maximumHeight: 600
    flags: Qt.Window
           | Qt.CustomizeWindowHint
           | Qt.WindowTitleHint
           | Qt.WindowSystemMenuHint
           | Qt.WindowMinimizeButtonHint
           | Qt.WindowCloseButtonHint
    visible: true
    title: "Fluff Linux Installer"
    color: "#f4f7fb"

    property int page: 0
    property var drives: []
    property int selectedDrive: -1
    property string generatedHostname: ""
    property string lastDriveData: ""
    property string powerWarning: ""

    // One shared button style keeps every installer action the same size and
    // contrast while still showing keyboard focus clearly.
    component InstallerButton: Button {
        id: installerButton
        implicitHeight: 44
        font.pixelSize: 15
        hoverEnabled: true
        highlighted: activeFocus
        palette.buttonText: enabled ? "#202020" : "#777777"
        palette.highlightedText: enabled ? "#202020" : "#777777"
        icon.color: enabled ? "#202020" : "#777777"

        Keys.onPressed: function(event) {
            if (event.key !== Qt.Key_Return && event.key !== Qt.Key_Enter)
                return
            event.accepted = true
            if (installerButton.enabled && !event.isAutoRepeat)
                installerButton.clicked()
        }

        background: Rectangle {
            implicitWidth: 80
            implicitHeight: 44
            radius: 3
            border.width: installerButton.activeFocus || installerButton.highlighted ? 2 : 1
            border.color: installerButton.activeFocus || installerButton.highlighted
                          ? "#820101"
                          : installerButton.hovered && installerButton.enabled
                            ? "#777777" : "#9a9a9a"

            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: !installerButton.enabled ? "#eeeeee"
                         : installerButton.down ? "#c8c8c8"
                         : installerButton.hovered ? "#ffffff"
                         : "#f8f8f8"
                }
                GradientStop {
                    position: 1
                    color: !installerButton.enabled ? "#d7d7d7"
                         : installerButton.down ? "#e7e7e7"
                         : installerButton.hovered ? "#dedede"
                         : "#d2d2d2"
                }
            }
        }
    }

    // Each installation step chooses its icon, color, and wording from the
    // state reported by the Rust backend.
    component InstallationStep: RowLayout {
        required property int step
        required property string waitingText
        required property string activeText
        required property string completedText
        property string detailText: ""
        readonly property string stepState: window.installationStepState(step)

        spacing: 10
        Layout.fillWidth: true
        Layout.minimumHeight: 50

        Item {
            Layout.preferredWidth: 34
            Layout.preferredHeight: 34

            Item {
                id: activeStepSpinner
                anchors.centerIn: parent
                width: 28
                height: 28
                visible: stepState === "active"

                Canvas {
                    anchors.fill: parent

                    onPaint: {
                        const context = getContext("2d")
                        context.reset()
                        context.clearRect(0, 0, width, height)
                        context.beginPath()
                        context.arc(width / 2, height / 2,
                                    Math.min(width, height) / 2 - 3,
                                    0, Math.PI * 1.45)
                        context.lineWidth = 3
                        context.lineCap = "round"
                        context.strokeStyle = "#820101"
                        context.stroke()
                    }
                }

                RotationAnimator on rotation {
                    from: 0
                    to: 360
                    duration: 900
                    loops: Animation.Infinite
                    running: activeStepSpinner.visible
                }
            }

            ToolButton {
                anchors.fill: parent
                visible: stepState !== "active"
                hoverEnabled: false
                focusPolicy: Qt.NoFocus
                background: null
                display: AbstractButton.IconOnly
                icon.name: stepState === "completed" ? "emblem-success"
                           : stepState === "failed" ? "dialog-error"
                           : stepState === "cancelled" ? "dialog-cancel"
                           : "chronometer"
                icon.width: stepState === "failed" ? 34 : 28
                icon.height: stepState === "failed" ? 34 : 28
                icon.color: "transparent"
                opacity: stepState === "waiting" ? 0.4 : 1
            }
        }

        ColumnLayout {
            spacing: 1
            Layout.fillWidth: true

            Label {
                text: stepState === "completed" ? completedText
                    : stepState === "active" ? activeText : waitingText
                font.pixelSize: 15
                font.weight: stepState === "active" ? Font.DemiBold : Font.Normal
                color: stepState === "failed" ? "#b3261e"
                     : stepState === "cancelled" ? "#4f5966"
                     : stepState === "waiting" ? "#7a828c"
                     : stepState === "completed" ? "#218739"
                     : "#26384f"
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }

            Label {
                visible: stepState === "active" && detailText.length > 0
                text: detailText
                font.pixelSize: 12
                color: "#65758b"
                wrapMode: Text.WordWrap
                Layout.fillWidth: true
            }
        }
    }

    InstallerBackend { id: backend }

    Component.onCompleted: {
        generatedHostname = backend.generateHostname()
        refreshDrives()
    }

    function refreshDrives() {
        // Preserve the chosen drive across automatic refreshes by matching its
        // device name instead of its position in the refreshed list.
        const selectedDevice = selectedDrive >= 0 && selectedDrive < drives.length
                             ? drives[selectedDrive].device : ""
        const data = backend.listDrivesData()
        if (data.startsWith("ERROR\t")) {
            lastDriveData = data
            drives = []
            diskError.text = data.substring(6)
            selectedDrive = -1
            return
        }
        if (data === lastDriveData)
            return
        lastDriveData = data
        const parsed = []
        if (data.length > 0) {
            const rows = data.split("\n")
            for (let row of rows) {
                const fields = row.split("\t")
                if (fields.length !== 9)
                    continue
                parsed.push({
                    device: fields[0],
                    model: fields[1],
                    serial: fields[2],
                    size: fields[3],
                    size_bytes: Number(fields[4]),
                    eligible: fields[5] === "1",
                    icon_type: fields[6],
                    media_type: fields[7],
                    partitions: fields[8].length > 0
                                && fields[8] !== "__UNPARTITIONED__"
                                ? fields[8].split("|~|") : [],
                    unpartitioned: fields[8] === "__UNPARTITIONED__"
                })
            }
        }
        drives = parsed
        diskError.text = ""
        selectedDrive = -1
        for (let index = 0; index < parsed.length; ++index) {
            if (parsed[index].device === selectedDevice && parsed[index].eligible) {
                selectedDrive = index
                break
            }
        }
        if (selectedDrive < 0 && parsed.length === 1 && parsed[0].eligible)
            selectedDrive = 0
    }

    function progressInstallationStep() {
        if (backend.overallProgress < 24)
            return 0
        if (backend.overallProgress < 26)
            return 1
        if (backend.overallProgress < 35)
            return 2
        if (backend.overallProgress < 78)
            return 3
        if (backend.overallProgress < 98)
            return 4
        if (backend.overallProgress < 100)
            return 5
        return 6
    }

    function failedInstallationStep() {
        const error = backend.errorMessage.toLowerCase()
        if (error.includes("installation files are missing")
                || error.includes("failed to prepare the drive")
                || error.includes("selected installation drive")
                || error.includes("at least 20 gib"))
            return 0
        if (error.includes("failed to format drive"))
            return 0
        if (error.includes("failed to initiate system installation"))
            return 1
        if (error.includes("failed to install system files"))
            return progressInstallationStep()
        if (error.includes("failed to configure")
                || error.includes("bootloader"))
            return 4
        return Math.min(progressInstallationStep(), 5)
    }

    function activeInstallationStep() {
        if (backend.errorMessage.length > 0)
            return failedInstallationStep()

        const status = backend.statusMessage
        if (status.startsWith("Preparing"))
            return 0
        if (status.startsWith("Formatting"))
            return 0
        if (status.startsWith("Loading system files"))
            return 1
        if (status.startsWith("Verifying system files"))
            return 2
        if (status.startsWith("Installing system files"))
            return backend.overallProgress < 35 ? 1 : 3
        if (status.startsWith("Configuring system"))
            return 4
        if (status.startsWith("Finalizing"))
            return 5
        return progressInstallationStep()
    }

    function installationStepState(step) {
        // A failed or cancelled operation keeps completed steps visible and
        // marks only the step that was active when work stopped.
        if (backend.finished || backend.overallProgress >= 100)
            return "completed"

        const currentStep = activeInstallationStep()
        if (backend.errorMessage.length > 0) {
            if (step < currentStep)
                return "completed"
            if (step === currentStep)
                return backend.errorMessage.startsWith("Installation cancelled.")
                        ? "cancelled" : "failed"
            return "waiting"
        }

        if (step < currentStep)
            return "completed"
        if (step === currentStep)
            return "active"
        return "waiting"
    }

    function installationStepDetail(step) {
        if (installationStepState(step) !== "active")
            return ""
        if (step === 1 || step === 2)
            return backend.detailMessage
        if (step === 3 && backend.totalItems > 0)
            return backend.completedItems + " / " + backend.totalItems
                    + " system packages installed"
        if (step === 4)
            return backend.detailMessage
        return ""
    }

    function refreshPowerStatus() {
        powerWarning = backend.installationPowerWarning()
    }

    function showPage(nextPage) {
        designSurface.forceActiveFocus(Qt.OtherFocusReason)
        page = nextPage
    }

    function showCancellationDialog() {
        if (window.page !== 3 || !backend.installing || backend.cancelling)
            return

        cancelWindow.show()
        cancelWindow.raise()
        cancelWindow.requestActivate()
    }

    onClosing: function(close) {
        if (window.page === 3 && backend.installing) {
            close.accepted = false
            window.showCancellationDialog()
        }
    }

    Timer {
        interval: 2000
        repeat: true
        running: window.page === 1 && !backend.installing
        onTriggered: refreshDrives()
    }

    Timer {
        interval: 5000
        repeat: true
        running: window.page === 2
        onTriggered: refreshPowerStatus()
    }

    Connections {
        target: backend
        function onFinishedChanged() {
            if (backend.finished)
                window.showPage(4)
        }
    }

    Item {
        id: designSurface
        width: 920
        height: 600
        anchors.centerIn: parent
        transformOrigin: Item.Center
        scale: Math.min(window.width / width, window.height / height)

        StackLayout {
            anchors.fill: parent
            anchors.margins: 0
            currentIndex: window.page

        // Welcome
        Item {
            id: welcomePage
            onVisibleChanged: {
                if (visible)
                    Qt.callLater(function() {
                        if (welcomePage.visible)
                            startButton.forceActiveFocus(Qt.TabFocusReason)
                    })
            }

            RowLayout {
                anchors.centerIn: parent
                width: 824
                spacing: 54

                Image {
                    source: "file:///usr/share/pixmaps/flufflinux-logo.svg"
                    sourceSize.width: 190
                    sourceSize.height: 190
                    fillMode: Image.PreserveAspectFit
                    Layout.alignment: Qt.AlignVCenter
                    Layout.preferredWidth: 210
                    Layout.preferredHeight: 210
                }

                ColumnLayout {
                    Layout.preferredWidth: 560
                    Layout.maximumWidth: 560
                    spacing: 20

                    Label {
                        text: "Welcome to the Fluff Linux Installer"
                        font.pixelSize: 30
                        font.weight: Font.Light
                        color: "#202020"
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                    Item {
                        id: introBlock
                        Layout.alignment: Qt.AlignLeft
                        Layout.preferredWidth: 560
                        Layout.maximumWidth: 560
                        Layout.preferredHeight: introText.implicitHeight + 20 + startButton.implicitHeight

                        Label {
                            id: introText
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            text: "This installer will guide you through installing Fluff Linux on this computer or another storage device, such as a portable drive.\n\nTo continue, press the Start button below."
                            font.pixelSize: 16
                            color: "#54657c"
                            wrapMode: Text.WordWrap
                            lineHeight: 1.25
                        }

                        InstallerButton {
                            id: startButton
                            anchors.top: introText.bottom
                            anchors.topMargin: 20
                            anchors.horizontalCenter: introText.horizontalCenter
                            anchors.horizontalCenterOffset: 90
                            width: 150
                            text: "Start  →"
                            onClicked: {
                                if (window.page === 0)
                                    window.showPage(1)
                            }
                        }
                    }
                }
            }

            Label {
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.rightMargin: 22
                anchors.bottomMargin: 16
                text: "fluffinstall 1.0"
                color: "#202020"
                font.pixelSize: 11
            }
        }

        // Disk selection
        Item {
            id: driveSelectionPage
            onVisibleChanged: {
                if (visible)
                    Qt.callLater(function() {
                        if (!driveSelectionPage.visible)
                            return
                        if (window.selectedDrive >= 0)
                            driveNextButton.forceActiveFocus(Qt.TabFocusReason)
                        else
                            driveBackButton.forceActiveFocus(Qt.TabFocusReason)
                    })
            }

            Connections {
                target: window
                function onSelectedDriveChanged() {
                    if (!driveSelectionPage.visible)
                        return
                    Qt.callLater(function() {
                        if (driveSelectionPage.visible && window.selectedDrive >= 0)
                            driveNextButton.forceActiveFocus(Qt.TabFocusReason)
                    })
                }
            }

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 42
                spacing: 10

                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 4
                    Label {
                        text: "Where do you want to install Fluff Linux?"
                        font.pixelSize: 27
                        font.weight: Font.Light
                        color: "#202020"
                    }
                    Label {
                        text: "Select the drive where Fluff Linux should be installed. The selected drive will be completely erased."
                        font.pixelSize: 15
                        color: "#54657c"
                    }
                }
                Label {
                    id: diskError
                    visible: text.length > 0
                    color: "#b3261e"
                    wrapMode: Text.WordWrap
                    Layout.fillWidth: true
                }

                ListView {
                    id: driveList
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 8
                    model: window.drives
                    ScrollBar.vertical: ScrollBar {
                        policy: ScrollBar.AsNeeded
                    }

                    header: RowLayout {
                        x: 10
                        width: driveList.width - 34
                        height: 30
                        spacing: 12
                        Item { Layout.preferredWidth: 46 }
                        Label {
                            text: "Drives"
                            font.weight: Font.DemiBold
                            color: "#3f4752"
                            Layout.fillWidth: true
                        }
                        Label {
                            text: "Type"
                            font.weight: Font.DemiBold
                            color: "#3f4752"
                            Layout.preferredWidth: 112
                            horizontalAlignment: Text.AlignHCenter
                        }
                        Label {
                            text: "Size"
                            font.weight: Font.DemiBold
                            color: "#3f4752"
                            Layout.preferredWidth: 116
                            horizontalAlignment: Text.AlignHCenter
                        }
                    }

                    delegate: Rectangle {
                        readonly property bool selected: window.selectedDrive === index
                        width: driveList.width - 14
                        height: Math.max(66, driveDetails.implicitHeight + 20)
                        radius: 5
                        color: selected ? "#33820101" : "white"
                        border.color: selected ? "#99820101" : "#cbd5e1"

                        TapHandler {
                            enabled: modelData.eligible
                            gesturePolicy: TapHandler.ReleaseWithinBounds
                            onTapped: window.selectedDrive = index
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.margins: 10
                            spacing: 12
                            DiskIcon {
                                mediaType: modelData.icon_type
                                foreground: modelData.eligible ? "#20242a" : "#b3261e"
                                Layout.preferredWidth: 46
                                Layout.preferredHeight: 46
                            }
                            ColumnLayout {
                                id: driveDetails
                                spacing: 1
                                Layout.fillWidth: true
                                Label {
                                    text: modelData.model
                                    font.pixelSize: 16
                                    minimumPixelSize: 11
                                    fontSizeMode: Text.Fit
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 2
                                    elide: Text.ElideRight
                                    font.weight: Font.DemiBold
                                    color: modelData.eligible ? "#20242a" : "#b3261e"
                                    Layout.fillWidth: true
                                    Layout.preferredHeight: Math.min(implicitHeight, 36)
                                }
                                Label {
                                    text: "Serial: " + modelData.serial
                                    font.pixelSize: 12
                                    color: modelData.eligible ? "#5c6673" : "#b3261e"
                                }
                                Label {
                                    text: "Drive name: /dev/" + modelData.device
                                    font.pixelSize: 12
                                    color: modelData.eligible ? "#5c6673" : "#b3261e"
                                }
                                ColumnLayout {
                                    visible: modelData.partitions.length > 0
                                    spacing: 1
                                    Layout.fillWidth: true
                                    Label {
                                        text: "Data partitions on drive:"
                                        font.pixelSize: 11
                                        font.weight: Font.DemiBold
                                        color: modelData.eligible ? "#20242a" : "#b3261e"
                                    }
                                    Repeater {
                                        model: modelData.partitions
                                        delegate: Label {
                                            required property string modelData
                                            text: "• " + modelData
                                            font.pixelSize: 11
                                            color: "#5c6673"
                                            elide: Text.ElideRight
                                            Layout.fillWidth: true
                                        }
                                    }
                                }
                                Label {
                                    visible: modelData.unpartitioned
                                    text: "Unpartitioned drive"
                                    font.pixelSize: 11
                                    font.weight: Font.DemiBold
                                    color: modelData.eligible ? "#5c6673" : "#b3261e"
                                }
                                Label {
                                    visible: !modelData.eligible
                                    text: "Requires at least 20 GiB of storage"
                                    font.pixelSize: 11
                                    color: "#b3261e"
                                }
                            }
                            Label {
                                text: modelData.media_type
                                font.pixelSize: 13
                                wrapMode: Text.WordWrap
                                color: modelData.eligible ? "#20242a" : "#b3261e"
                                Layout.preferredWidth: 112
                                horizontalAlignment: Text.AlignHCenter
                            }
                            Label {
                                text: modelData.size
                                font.pixelSize: 16
                                color: modelData.eligible ? "#20242a" : "#b3261e"
                                Layout.preferredWidth: 116
                                horizontalAlignment: Text.AlignHCenter
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    InstallerButton {
                        id: driveBackButton
                        text: "←  Back"
                        onClicked: {
                            if (window.page === 1)
                                window.showPage(0)
                        }
                    }
                    Item { Layout.fillWidth: true }
                    InstallerButton {
                        id: driveNextButton
                        text: "Next  →"
                        enabled: window.selectedDrive >= 0
                        onClicked: {
                            if (window.page === 1 && window.selectedDrive >= 0)
                                window.showPage(2)
                        }
                    }
                }
            }
        }

        // Confirmation
        Item {
            id: confirmationPage
            onVisibleChanged: {
                if (visible) {
                    window.refreshPowerStatus()
                    Qt.callLater(function() {
                        if (confirmationPage.visible)
                            noButton.forceActiveFocus(Qt.TabFocusReason)
                    })
                }
            }

            ColumnLayout {
                anchors.centerIn: parent
                anchors.verticalCenterOffset: -24
                width: Math.min(parent.width - 80, 760)
                spacing: 12

                Label {
                    text: "You have selected:"
                    font.pixelSize: 29
                    font.weight: Font.Light
                    color: "#202020"
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: Math.max(96, confirmationDetails.implicitHeight + 28)
                    radius: 5
                    color: "white"
                    border.color: "#cbd5e1"

                    RowLayout {
                        anchors.fill: parent
                        anchors.margins: 14
                        spacing: 14

                        DiskIcon {
                            mediaType: window.selectedDrive >= 0
                                       ? drives[window.selectedDrive].icon_type : "hdd"
                            foreground: "#30343a"
                            Layout.preferredWidth: 46
                            Layout.preferredHeight: 46
                        }
                        ColumnLayout {
                            id: confirmationDetails
                            spacing: 3
                            Layout.fillWidth: true
                            Label {
                                text: window.selectedDrive >= 0
                                      ? drives[window.selectedDrive].model : ""
                                font.pixelSize: 16
                                font.weight: Font.DemiBold
                                color: "#20242a"
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                            }
                            Label {
                                text: window.selectedDrive >= 0
                                      ? "Serial: " + drives[window.selectedDrive].serial
                                      : ""
                                font.pixelSize: 12
                                color: "#5c6673"
                            }
                            Label {
                                text: window.selectedDrive >= 0
                                      ? "Drive name: /dev/" + drives[window.selectedDrive].device : ""
                                font.pixelSize: 12
                                color: "#5c6673"
                            }
                            ColumnLayout {
                                visible: window.selectedDrive >= 0
                                         && drives[window.selectedDrive].partitions.length > 0
                                spacing: 1
                                Layout.fillWidth: true
                                Label {
                                    text: "Data partitions on drive:"
                                    font.pixelSize: 11
                                    font.weight: Font.DemiBold
                                    color: "#20242a"
                                }
                                Repeater {
                                    model: window.selectedDrive >= 0
                                           ? drives[window.selectedDrive].partitions : []
                                    delegate: Label {
                                        required property string modelData
                                        text: "• " + modelData
                                        font.pixelSize: 11
                                        color: "#5c6673"
                                        elide: Text.ElideRight
                                        Layout.fillWidth: true
                                    }
                                }
                            }
                            Label {
                                visible: window.selectedDrive >= 0
                                         && drives[window.selectedDrive].unpartitioned
                                text: "Unpartitioned drive"
                                font.pixelSize: 11
                                font.weight: Font.DemiBold
                                color: "#5c6673"
                            }
                        }
                        ColumnLayout {
                            spacing: 3
                            Layout.preferredWidth: 100
                            Label {
                                text: window.selectedDrive >= 0
                                      ? drives[window.selectedDrive].media_type : ""
                                font.pixelSize: 12
                                color: "#5c6673"
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                            }
                            Label {
                                text: window.selectedDrive >= 0
                                      ? drives[window.selectedDrive].size : ""
                                font.pixelSize: 16
                                color: "#20242a"
                            }
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true

                    Item {
                        Layout.preferredWidth: updatesOption.implicitWidth
                        Layout.preferredHeight: updatesOption.implicitHeight

                        CheckBox {
                            id: updatesOption
                            anchors.fill: parent
                            text: "Download and install updates after installation has finished"
                            checked: false
                            enabled: false

                            indicator: Rectangle {
                                implicitWidth: 20
                                implicitHeight: 20
                                x: updatesOption.leftPadding
                                y: (updatesOption.height - height) / 2
                                radius: 2
                                border.width: 1
                                border.color: "#9a9a9a"

                                gradient: Gradient {
                                    GradientStop { position: 0; color: "#eeeeee" }
                                    GradientStop { position: 1; color: "#d7d7d7" }
                                }

                                Label {
                                    anchors.centerIn: parent
                                    visible: updatesOption.checked
                                    text: "✓"
                                    color: "#555555"
                                    font.pixelSize: 14
                                    font.weight: Font.DemiBold
                                }
                            }

                            contentItem: Text {
                                leftPadding: updatesOption.indicator.width + updatesOption.spacing
                                text: updatesOption.text
                                font: updatesOption.font
                                color: "#777777"
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                        }

                        MouseArea {
                            id: updatesOptionHover
                            anchors.fill: parent
                            hoverEnabled: true
                            acceptedButtons: Qt.NoButton
                        }

                        ToolTip.visible: updatesOptionHover.containsMouse
                        ToolTip.text: "System updates will be available in a future version of FluffInstall."
                    }

                    Item { Layout.fillWidth: true }

                    Item {
                        Layout.preferredWidth: encryptionOption.implicitWidth
                        Layout.preferredHeight: 38

                        InstallerButton {
                            id: encryptionOption
                            anchors.fill: parent
                            text: "Drive encryption"
                            implicitHeight: 38
                            font.pixelSize: 14
                            icon.name: "object-locked"
                            icon.width: 18
                            icon.height: 18
                            enabled: false
                        }

                        MouseArea {
                            id: encryptionOptionHover
                            anchors.fill: parent
                            hoverEnabled: true
                            acceptedButtons: Qt.NoButton
                        }

                        ToolTip.visible: encryptionOptionHover.containsMouse
                        ToolTip.text: "Drive encryption will be available in a future version of FluffInstall."
                    }
                }

                Item { Layout.preferredHeight: 32 }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 1
                    Layout.topMargin: 3
                    Layout.bottomMargin: 3
                    color: "#cbd5e1"
                }

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 68
                    radius: 5
                    color: "#fff0ed"
                    border.color: "#e5a39d"

                    RowLayout {
                        id: warningContent
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12
                        anchors.topMargin: 6
                        anchors.bottomMargin: 6
                        spacing: 10

                        ToolButton {
                            hoverEnabled: false
                            focusPolicy: Qt.NoFocus
                            background: null
                            display: AbstractButton.IconOnly
                            icon.name: "dialog-warning"
                            icon.width: 52
                            icon.height: 52
                            icon.color: "transparent"
                            Layout.preferredWidth: 56
                            Layout.preferredHeight: 56
                        }

                        Label {
                            text: "THIS WILL FORMAT THE SELECTED DRIVE, ERASE ALL DATA ON IT, AND INSTALL FLUFF LINUX!"
                            color: "#8c1d18"
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.NoWrap
                            Layout.fillWidth: true
                        }

                        ToolButton {
                            hoverEnabled: false
                            focusPolicy: Qt.NoFocus
                            background: null
                            display: AbstractButton.IconOnly
                            icon.name: "dialog-warning"
                            icon.width: 52
                            icon.height: 52
                            icon.color: "transparent"
                            Layout.preferredWidth: 56
                            Layout.preferredHeight: 56
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    spacing: 18

                    InstallerButton {
                        id: noButton
                        text: "No"
                        icon.name: "dialog-cancel"
                        Layout.preferredWidth: 112
                        onClicked: {
                            if (window.page === 2)
                                window.showPage(1)
                        }
                    }

                    Label {
                        text: "Continue with the selected drive?"
                        font.pixelSize: 17
                        font.weight: Font.DemiBold
                        color: "#202020"
                        horizontalAlignment: Text.AlignHCenter
                        Layout.fillWidth: true
                    }

                    Item {
                        Layout.preferredWidth: 112
                        Layout.preferredHeight: yesButton.implicitHeight

                        InstallerButton {
                            id: yesButton
                            anchors.fill: parent
                            text: "Yes  →"
                            enabled: window.powerWarning.length === 0
                            onClicked: {
                                if (window.page !== 2 || backend.installing
                                        || backend.finished || window.selectedDrive < 0)
                                    return
                                window.refreshPowerStatus()
                                if (window.powerWarning.length > 0)
                                    return
                                window.showPage(3)
                                backend.startInstallation(
                                    "/dev/" + drives[selectedDrive].device,
                                    generatedHostname
                                )
                            }
                        }

                        MouseArea {
                            id: powerWarningHover
                            anchors.fill: parent
                            acceptedButtons: Qt.NoButton
                            hoverEnabled: true
                        }

                        ToolTip.visible: powerWarningHover.containsMouse
                                         && window.powerWarning.length > 0
                        ToolTip.text: window.powerWarning
                    }
                }

                Item { Layout.preferredHeight: 15 }
            }

            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 6
                width: 760
                text: "By continuing, you agree that FluffNet LLC is not liable for data loss or damage resulting from the use of this installer."
                color: "#7a828c"
                font.pixelSize: 12
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.NoWrap
            }
        }

        // Progress
        Item {
            id: progressPage
            onVisibleChanged: {
                if (visible)
                    Qt.callLater(function() {
                        if (progressPage.visible)
                            progressPage.forceActiveFocus(Qt.OtherFocusReason)
                    })
            }

            Window {
                id: cancelWindow
                width: 520
                height: 230
                minimumWidth: 520
                maximumWidth: 520
                minimumHeight: 230
                maximumHeight: 230
                visible: false
                title: "Cancel installation?"
                transientParent: window
                modality: Qt.WindowModal
                color: "#f4f7fb"
                flags: Qt.Dialog
                       | Qt.WindowTitleHint
                       | Qt.WindowSystemMenuHint
                       | Qt.WindowCloseButtonHint
                       | Qt.WindowStaysOnTopHint

                onVisibleChanged: {
                    if (visible) {
                        Qt.callLater(function() {
                            if (cancelWindow.visible)
                                keepInstallingButton.forceActiveFocus(Qt.TabFocusReason)
                        })
                    } else if (progressPage.visible && backend.installing) {
                        progressPage.forceActiveFocus(Qt.OtherFocusReason)
                    }
                }

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 24
                    spacing: 14

                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 16

                        ToolButton {
                            Layout.preferredWidth: 52
                            Layout.preferredHeight: 52
                            background: null
                            enabled: false
                            icon.name: "dialog-warning"
                            icon.width: 48
                            icon.height: 48
                            icon.color: "transparent"
                            display: AbstractButton.IconOnly
                        }

                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 10

                            Label {
                                text: "Do you want to cancel the installation?"
                                font.pixelSize: 18
                                font.weight: Font.DemiBold
                                color: "#202020"
                                Layout.fillWidth: true
                            }

                            Label {
                                text: "Cancelling now will stop the installation. It will not restore the selected drive's previous partitions, formatting, or data, and the system on the drive may be incomplete."
                                font.pixelSize: 14
                                color: "#4f5966"
                                wrapMode: Text.WordWrap
                                Layout.fillWidth: true
                            }
                        }
                    }

                    Item { Layout.fillHeight: true }

                    RowLayout {
                        Layout.alignment: Qt.AlignHCenter
                        spacing: 64

                        InstallerButton {
                            id: confirmCancellationButton
                            text: "Yes"
                            Layout.preferredWidth: 120
                            onClicked: {
                                if (!cancelWindow.visible || window.page !== 3
                                        || !backend.installing)
                                    return
                                cancelWindow.close()
                                backend.cancelInstallation()
                            }
                        }

                        InstallerButton {
                            id: keepInstallingButton
                            text: "No"
                            Layout.preferredWidth: 120
                            onClicked: {
                                if (cancelWindow.visible)
                                    cancelWindow.close()
                            }
                        }
                    }
                }
            }

            Connections {
                target: backend
                function onInstallingChanged() {
                    if (!backend.installing && cancelWindow.visible)
                        cancelWindow.close()
                }
                function onErrorMessageChanged() {
                    if (backend.errorMessage.length > 0 && progressPage.visible)
                        Qt.callLater(function() {
                            if (progressPage.visible && backend.errorMessage.length > 0)
                                progressCloseButton.forceActiveFocus(Qt.TabFocusReason)
                        })
                }
            }

            ColumnLayout {
                anchors.top: parent.top
                anchors.topMargin: 44
                anchors.horizontalCenter: parent.horizontalCenter
                width: 780
                spacing: 11

                Label {
                    text: backend.errorMessage.startsWith("Installation cancelled.")
                          ? "The installation has been cancelled"
                          : "Installing Fluff Linux"
                    font.pixelSize: 31
                    font.weight: Font.Light
                    color: "#202020"
                    Layout.alignment: Qt.AlignHCenter
                }

                Label {
                    visible: !backend.errorMessage.startsWith("Installation cancelled.")
                    text: "Please wait while Fluff Linux is being installed on the drive. It may take a while.\nPlease keep the computer powered on and the installation media connected."
                    font.pixelSize: 15
                    color: "#65758b"
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                    Layout.fillWidth: true
                }

                Item { Layout.preferredHeight: 12 }

                Item {
                    Layout.fillWidth: true
                    Layout.preferredHeight: stageColumns.implicitHeight

                    RowLayout {
                        id: stageColumns
                        width: 536
                        anchors.horizontalCenter: parent.horizontalCenter
                        anchors.horizontalCenterOffset: 36
                        spacing: 16

                        ColumnLayout {
                            Layout.preferredWidth: 260
                            spacing: 7

                            InstallationStep {
                                step: 0
                                waitingText: "Format drive"
                                activeText: "Formatting drive..."
                                completedText: "Formatted drive"
                            }
                            InstallationStep {
                                step: 1
                                waitingText: "Load system files"
                                activeText: "Loading system files..."
                                completedText: "Loaded system files"
                                detailText: window.installationStepDetail(1)
                            }
                            InstallationStep {
                                step: 2
                                waitingText: "Verify system files"
                                activeText: "Verifying system files..."
                                completedText: "Verified system files"
                                detailText: window.installationStepDetail(2)
                            }
                        }

                        ColumnLayout {
                            Layout.preferredWidth: 260
                            spacing: 7

                            InstallationStep {
                                step: 3
                                waitingText: "Install system files"
                                activeText: "Installing system files..."
                                completedText: "Installed system files"
                                detailText: window.installationStepDetail(3)
                            }
                            InstallationStep {
                                step: 4
                                waitingText: "Configure system"
                                activeText: "Configuring system..."
                                completedText: "Configured system"
                                detailText: window.installationStepDetail(4)
                            }
                            InstallationStep {
                                step: 5
                                waitingText: "Finalize installation"
                                activeText: "Finalizing installation..."
                                completedText: "Finalized installation"
                            }
                        }
                    }
                }

                Label {
                    visible: backend.errorMessage.length > 0
                             && !backend.errorMessage.startsWith("Installation cancelled.")
                    Layout.alignment: Qt.AlignHCenter
                    Layout.topMargin: 8
                    text: "Installation failed"
                    font.pixelSize: 24
                    font.weight: Font.DemiBold
                    color: "#b3261e"
                }

                Label {
                    visible: backend.errorMessage.length > 0
                             && !backend.errorMessage.startsWith("Installation cancelled.")
                    text: backend.errorMessage
                    color: "#b3261e"
                    font.pixelSize: 14
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                    Layout.fillWidth: true
                }

                InstallerButton {
                    id: progressCloseButton
                    visible: backend.errorMessage.length > 0
                    text: "Close"
                    icon.name: "window-close"
                    Layout.preferredWidth: 140
                    Layout.topMargin: 8
                    Layout.alignment: Qt.AlignHCenter
                    transform: Translate { x: 4 }
                    onClicked: {
                        if (window.page === 3 && backend.errorMessage.length > 0)
                            window.close()
                    }
                }
            }

        }

        // Finished
        Item {
            id: finishedPage
            onVisibleChanged: {
                if (visible)
                    Qt.callLater(function() {
                        if (finishedPage.visible)
                            shutdownButton.forceActiveFocus(Qt.TabFocusReason)
                    })
            }

            ColumnLayout {
                anchors.centerIn: parent
                width: Math.min(parent.width - 120, 620)
                spacing: 18

                ToolButton {
                    hoverEnabled: false
                    focusPolicy: Qt.NoFocus
                    opacity: 1
                    background: null
                    display: AbstractButton.IconOnly
                    icon.name: "emblem-success"
                    icon.width: 88
                    icon.height: 88
                    icon.color: "transparent"
                    Layout.preferredWidth: 96
                    Layout.preferredHeight: 96
                    Layout.alignment: Qt.AlignHCenter
                }

                Label {
                    text: "Installation finished successfully!"
                    font.pixelSize: 32
                    font.weight: Font.Light
                    color: "#202020"
                    Layout.alignment: Qt.AlignHCenter
                }
                Label {
                    text: "Please restart the system and remove the installation media\nto continue setting up Fluff Linux."
                    font.pixelSize: 16
                    color: "#54657c"
                    wrapMode: Text.WordWrap
                    horizontalAlignment: Text.AlignHCenter
                    Layout.fillWidth: true
                }
                Item { Layout.preferredHeight: 18 }
                ColumnLayout {
                    spacing: 24
                    Layout.alignment: Qt.AlignHCenter

                    RowLayout {
                        spacing: 12
                        Layout.alignment: Qt.AlignHCenter

                        InstallerButton {
                            id: shutdownButton
                            text: "Shut down"
                            icon.name: "system-shutdown"
                            Layout.preferredWidth: 160
                            onClicked: {
                                if (window.page === 4 && backend.finished)
                                    backend.shutdownSystem()
                            }
                        }

                        InstallerButton {
                            id: restartButton
                            text: "Restart"
                            icon.name: "system-reboot"
                            Layout.preferredWidth: 160
                            onClicked: {
                                if (window.page === 4 && backend.finished)
                                    backend.rebootSystem()
                            }
                        }
                    }

                    InstallerButton {
                        id: finishedCloseButton
                        text: "Close"
                        icon.name: "window-close"
                        Layout.preferredWidth: 160
                        Layout.alignment: Qt.AlignHCenter
                        onClicked: {
                            if (window.page === 4)
                                window.close()
                        }
                    }
                }
            }
        }
        }
    }
}
