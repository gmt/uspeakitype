import QtQuick 2.15

Rectangle {
    id: root
    property bool waterfallMode: false
    property real energyLevel: 0.5
    property real phase: 0.0

    radius: 16
    color: "#110b08"
    border.width: 1
    border.color: "#2b1d16"
    clip: true

    function pseudoNoise(a, b) {
        return Math.abs(Math.sin(a * 12.9898 + b * 78.233) * 43758.5453) % 1.0
    }

    function clamp(v, lo, hi) {
        return Math.max(lo, Math.min(hi, v))
    }

    Timer {
        interval: 75
        running: true
        repeat: true
        onTriggered: {
            root.phase += 0.085
            spectroCanvas.requestPaint()
        }
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#110b08" }
            GradientStop { position: 0.58; color: "#1a120d" }
            GradientStop { position: 1.0; color: "#150f0b" }
        }
    }

    Canvas {
        id: spectroCanvas
        anchors.fill: parent
        anchors.margins: 10
        renderTarget: Canvas.Image
        antialiasing: true

        onPaint: {
            const ctx = getContext("2d")
            const w = width
            const h = height
            ctx.reset()
            ctx.clearRect(0, 0, w, h)

            const bg = ctx.createLinearGradient(0, 0, 0, h)
            bg.addColorStop(0.0, "#0d0806")
            bg.addColorStop(0.62, "#140d09")
            bg.addColorStop(1.0, "#0d0806")
            ctx.fillStyle = bg
            ctx.fillRect(0, 0, w, h)

            if (root.waterfallMode) {
                const columns = Math.max(72, Math.floor(w / 5))
                const rows = Math.max(56, Math.floor(h / 4))
                const cellW = w / columns
                const cellH = h / rows

                for (let x = 0; x < columns; ++x) {
                    const time = root.phase * 1.9 + x / columns * 6.0
                    for (let y = 0; y < rows; ++y) {
                        const freq = 1.0 - y / Math.max(1, rows - 1)
                        const harmonic = Math.sin(time * 1.2 + freq * 14.0)
                        const ridge = Math.sin(time * 0.65 - freq * 19.0 + root.phase * 1.4)
                        const faceArc = Math.sin(time * 0.42 + freq * 8.0 + 1.3)
                        const grain = root.pseudoNoise(x * 0.17 + root.phase, y * 0.11)
                        let intensity =
                            0.18 +
                            0.28 * Math.max(0, harmonic) +
                            0.18 * Math.max(0, ridge) +
                            0.20 * Math.max(0, faceArc) +
                            0.16 * grain

                        intensity *= 0.55 + root.energyLevel * 0.75
                        intensity *= 0.68 + Math.pow(1.0 - freq, 1.7) * 0.7
                        intensity = root.clamp(intensity, 0.0, 1.0)

                        const hue = 0.05 + intensity * 0.15 + freq * 0.02
                        const sat = 0.76
                        const light = 0.15 + intensity * 0.45
                        ctx.fillStyle = Qt.hsla(hue, sat, light, 0.96)
                        ctx.fillRect(x * cellW, y * cellH, cellW + 0.9, cellH + 0.9)
                    }
                }
            } else {
                const bars = Math.max(84, Math.floor(w / 10))
                const barW = w / bars
                const floorY = h - 16

                ctx.fillStyle = "rgba(255,185,84,0.08)"
                ctx.fillRect(0, floorY - 2, w, 3)

                for (let i = 0; i < bars; ++i) {
                    const t = root.phase * 1.5 + i / bars * 7.8
                    let envelope =
                        0.18 +
                        0.22 * Math.max(0, Math.sin(t * 0.8)) +
                        0.26 * Math.max(0, Math.sin(t * 1.7 + 0.8)) +
                        0.12 * root.pseudoNoise(i * 0.31, root.phase * 0.7)

                    const tilt = 0.35 + 0.65 * (1.0 - Math.abs((i / bars) - 0.52) * 1.2)
                    envelope *= tilt
                    envelope *= 0.45 + root.energyLevel * 0.95
                    envelope = root.clamp(envelope, 0.04, 1.0)

                    const barH = envelope * (h - 24)
                    const x = i * barW
                    const y = floorY - barH

                    const topHue = 0.05 + envelope * 0.15
                    const bottomHue = 0.05
                    const grad = ctx.createLinearGradient(0, y, 0, floorY)
                    grad.addColorStop(0.0, Qt.hsla(topHue, 0.76, 0.15 + envelope * 0.45, 0.98))
                    grad.addColorStop(1.0, Qt.hsla(bottomHue, 0.76, 0.15, 0.75))
                    ctx.fillStyle = grad
                    ctx.fillRect(x + 0.6, y, Math.max(2.0, barW - 1.8), barH)
                }
            }

            for (let row = 0; row < 7; ++row) {
                const y = row / 6 * h
                ctx.strokeStyle = row === 6 ? "rgba(255,214,149,0.14)" : "rgba(177,148,132,0.08)"
                ctx.lineWidth = 1
                ctx.beginPath()
                ctx.moveTo(0, y + 0.5)
                ctx.lineTo(w, y + 0.5)
                ctx.stroke()
            }

            const vignette = ctx.createLinearGradient(0, 0, 0, h)
            vignette.addColorStop(0.0, "rgba(0,0,0,0.04)")
            vignette.addColorStop(0.7, "rgba(0,0,0,0.0)")
            vignette.addColorStop(1.0, "rgba(0,0,0,0.22)")
            ctx.fillStyle = vignette
            ctx.fillRect(0, 0, w, h)
        }
    }
}