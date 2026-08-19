import QtQuick
import QtQuick.Controls

Item {
    id: root
    property string mediaType: "hdd"
    property color foreground: "#30343a"
    readonly property string themeIconName:
        mediaType === "hdd" ? "drive-harddisk"
        : mediaType === "sd" ? "media-flash-sd-mmc"
        : "drive-harddisk"

    implicitWidth: 46
    implicitHeight: 46

    // Standard drive types use the installed KDE icon theme.
    ToolButton {
        visible: root.mediaType !== "ssd"
                 && root.mediaType !== "mmc"
                 && root.mediaType !== "usb"
                 && root.mediaType !== "usb_hdd"
                 && root.mediaType !== "removable"
        anchors.fill: parent
        background: null
        enabled: false
        icon.name: root.themeIconName
        icon.width: 42
        icon.height: 42
        icon.color: "transparent"
        display: AbstractButton.IconOnly
    }

    // These simple labels stay readable even when a matching theme icon is absent.
    Rectangle {
        visible: root.mediaType === "ssd"
                 || root.mediaType === "mmc"
                 || root.mediaType === "removable"
        anchors.centerIn: parent
        width: 40
        height: 32
        radius: 3
        color: "#16191d"

        Label {
            visible: root.mediaType === "ssd" || root.mediaType === "mmc"
            anchors.centerIn: parent
            text: root.mediaType === "ssd" ? "SSD" : "MMC"
            color: "white"
            font.pixelSize: 12
            font.weight: Font.Bold
        }

        Rectangle {
            visible: root.mediaType === "removable"
            anchors.centerIn: parent
            width: 30
            height: 16
            radius: 3
            color: "#820101"

            Label {
                anchors.centerIn: parent
                text: "REM"
                color: "white"
                font.pixelSize: 9
                font.weight: Font.Bold
            }
        }
    }

    // The USB artwork is embedded so it is identical on every live system.
    Item {
        visible: root.mediaType === "usb" || root.mediaType === "usb_hdd"
        anchors.centerIn: parent
        width: 48
        height: 48

        Image {
            anchors.fill: parent
            source: "qrc:/qt/qml/org/flufflinux/installer/assets/usb-storage.svg"
            // Qt rasterizes SVG images before the design surface is scaled.
            // A high-resolution source stays sharp on 4K and high-DPI output.
            sourceSize.width: 512
            sourceSize.height: 512
            fillMode: Image.PreserveAspectFit
            smooth: true
            mipmap: true
        }

        // Native text matches the SSD/MMC labels and remains crisp when Qt
        // scales the interface, independent of SVG font support.
        Label {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 3
            text: "USB"
            color: "white"
            font.pixelSize: 9
            font.weight: Font.Bold
        }
    }

    Label {
        visible: root.mediaType === "sd"
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        text: "SD"
        color: root.foreground
        font.pixelSize: 8
        font.weight: Font.Bold
    }
}
