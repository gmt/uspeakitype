import QtQuick 2.15

Rectangle {
    id: root
    property bool waterfallMode: false
    property var spectrum: []
    property string colorScheme: "fire"
    property var history: []
    radius: 16; color: "#110b08"; border.width: 1; border.color: "#2b1d16"; clip: true
    onSpectrumChanged: {
        if (spectrum.length) {
            const frames = history.slice(-159)
            frames.push(Array.from(spectrum))
            history = frames
        }
        spectroCanvas.requestPaint()
    }
    onWaterfallModeChanged: spectroCanvas.requestPaint()
    onColorSchemeChanged: spectroCanvas.requestPaint()
    function intensityColor(value) {
        const v = Math.max(0, Math.min(1, value))
        const scheme = colorScheme.toLowerCase()
        if (scheme.indexOf("ocean") >= 0 || scheme.indexOf("ice") >= 0) return Qt.hsla(0.65 - v * 0.16, 0.8, 0.07 + v * 0.57, 1)
        if (scheme.indexOf("matrix") >= 0 || scheme.indexOf("green") >= 0) return Qt.hsla(0.34, 0.85, 0.04 + v * 0.6, 1)
        if (scheme.indexOf("rainbow") >= 0) return Qt.hsla((1 - v) * 0.7, 0.85, 0.05 + v * 0.5, 1)
        if (scheme.indexOf("mono") >= 0 || scheme.indexOf("gray") >= 0) return Qt.hsla(0, 0, v, 1)
        return Qt.hsla(0.02 + v * 0.14, 0.76, 0.035 + v * 0.55, 1)
    }
    Canvas {
        id: spectroCanvas
        anchors.fill: parent; anchors.margins: 10
        renderTarget: Canvas.Image
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d")
            const w = width, h = height
            ctx.reset()
            ctx.fillStyle = "#100b08"
            ctx.fillRect(0, 0, w, h)
            if (root.waterfallMode) {
                const frames = root.history
                const cellW = w / 160
                for (let x = 0; x < frames.length; ++x) {
                    const frame = frames[x]
                    const cellH = h / Math.max(1, frame.length)
                    for (let y = 0; y < frame.length; ++y) {
                        ctx.fillStyle = root.intensityColor(frame[y])
                        ctx.fillRect((160 - frames.length + x) * cellW, h - (y + 1) * cellH, cellW + 0.5, cellH + 0.5)
                    }
                }
            } else {
                const bins = root.spectrum
                const barW = w / Math.max(1, bins.length)
                for (let i = 0; i < bins.length; ++i) {
                    const v = Math.max(0, Math.min(1, bins[i]))
                    const grad = ctx.createLinearGradient(0, h * (1 - v), 0, h)
                    grad.addColorStop(0, root.intensityColor(v))
                    grad.addColorStop(1, "#382014")
                    ctx.fillStyle = grad
                    ctx.fillRect(i * barW + 0.5, h * (1 - v), Math.max(1, barW - 1), h * v)
                }
            }
            for (let row = 0; row < 7; ++row) {
                ctx.fillStyle = "rgba(177,148,132,0.08)"
                ctx.fillRect(0, row / 6 * h, w, 1)
            }
        }
    }
}
