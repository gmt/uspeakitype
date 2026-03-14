import QtQuick 2.15

Rectangle {
    id: root
    property bool waterfallMode: false
    property real energyLevel: 0.5
    property real phase: 0.0
    property var spectralPalette: [
        [0.08, 0.06, 0.07],
        [0.34, 0.27, 0.36],
        [0.47, 0.23, 0.19],
        [0.78, 0.55, 0.26],
        [0.87, 0.66, 0.31],
        [0.50, 0.56, 0.47]
    ]

    radius: 38
    color: "#080607"
    border.width: 1
    border.color: "#4b3b33"
    clip: true

    function pseudoNoise(a, b) {
        return Math.abs(Math.sin(a * 12.9898 + b * 78.233) * 43758.5453) % 1.0
    }

    function clamp(v, lo, hi) {
        return Math.max(lo, Math.min(hi, v))
    }

    function mix(a, b, t) {
        return a + (b - a) * t
    }

    function paletteColor(t, alpha) {
        const stops = root.spectralPalette
        const scaled = root.clamp(t, 0.0, 1.0) * (stops.length - 1)
        const i = Math.floor(scaled)
        const j = Math.min(stops.length - 1, i + 1)
        const frac = scaled - i
        const a = stops[i]
        const b = stops[j]
        return Qt.rgba(
            root.mix(a[0], b[0], frac),
            root.mix(a[1], b[1], frac),
            root.mix(a[2], b[2], frac),
            alpha
        )
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
            GradientStop { position: 0.0; color: "#0a0708" }
            GradientStop { position: 0.4; color: "#10090b" }
            GradientStop { position: 0.75; color: "#150f12" }
            GradientStop { position: 1.0; color: "#1d1613" }
        }
    }

    Canvas {
        id: spectroCanvas
        anchors.fill: parent
        anchors.margins: 14
        renderTarget: Canvas.Image
        antialiasing: true

        onPaint: {
            const ctx = getContext("2d")
            const w = width
            const h = height
            ctx.reset()
            ctx.clearRect(0, 0, w, h)

            const bg = ctx.createLinearGradient(0, 0, 0, h)
            bg.addColorStop(0.0, "#090607")
            bg.addColorStop(0.52, "#120b0d")
            bg.addColorStop(1.0, "#1d1511")
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

                        const tint = root.clamp(intensity * 0.8 + (1.0 - freq) * 0.2, 0.0, 1.0)
                        ctx.fillStyle = root.paletteColor(tint, 0.95)
                        ctx.fillRect(x * cellW, y * cellH, cellW + 0.9, cellH + 0.9)
                    }
                }
            } else {
                const bars = Math.max(84, Math.floor(w / 10))
                const barW = w / bars
                const floorY = h - 16

                ctx.fillStyle = "rgba(214,157,76,0.12)"
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

                    const barH = envelope * (h - 52)
                    const x = i * barW
                    const y = floorY - barH

                    const grad = ctx.createLinearGradient(0, y, 0, floorY)
                    grad.addColorStop(0.0, "rgba(244,229,196,0.98)")
                    grad.addColorStop(0.28, "rgba(219,167,77,0.96)")
                    grad.addColorStop(0.72, "rgba(122,51,45,0.88)")
                    grad.addColorStop(1.0, "rgba(63,32,38,0.72)")
                    ctx.fillStyle = grad
                    ctx.fillRect(x + 0.6, y, Math.max(2.0, barW - 1.8), barH)
                }
            }

            const refractors = [
                { x: 0.18, width: 0.11, color: "rgba(214,157,76,0.09)", tilt: -8 },
                { x: 0.46, width: 0.14, color: "rgba(111,79,108,0.12)", tilt: 5 },
                { x: 0.67, width: 0.1, color: "rgba(122,51,45,0.11)", tilt: -4 },
                { x: 0.84, width: 0.12, color: "rgba(123,134,95,0.1)", tilt: 6 }
            ]

            for (let r = 0; r < refractors.length; ++r) {
                const band = refractors[r]
                ctx.save()
                ctx.translate(w * band.x, -h * 0.08)
                ctx.rotate(band.tilt * Math.PI / 180.0)
                ctx.fillStyle = band.color
                ctx.fillRect(0, 0, w * band.width, h * 1.16)
                ctx.restore()
            }

            for (let row = 0; row < 7; ++row) {
                const y = row / 6 * h
                ctx.strokeStyle = row === 6 ? "rgba(214,157,76,0.18)" : "rgba(154,124,108,0.08)"
                ctx.lineWidth = 1
                ctx.beginPath()
                ctx.moveTo(0, y + 0.5)
                ctx.lineTo(w, y + 0.5)
                ctx.stroke()
            }

            const vignette = ctx.createLinearGradient(0, 0, 0, h)
            vignette.addColorStop(0.0, "rgba(255,255,255,0.02)")
            vignette.addColorStop(0.7, "rgba(0,0,0,0.0)")
            vignette.addColorStop(1.0, "rgba(0,0,0,0.28)")
            ctx.fillStyle = vignette
            ctx.fillRect(0, 0, w, h)
        }
    }

    Row {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: 18
        spacing: 10

        Rectangle {
            width: modeLabel.implicitWidth + 18
            height: 28
            radius: 14
            color: "#1a1113"
            border.width: 1
            border.color: "#58402d"

            Text {
                id: modeLabel
                anchors.centerIn: parent
                text: root.waterfallMode ? "Waterfall Draft" : "Bar Meter Draft"
                color: "#f3ebe1"
                font.pixelSize: 13
                font.weight: Font.Medium
            }
        }

        Rectangle {
            width: seamLabel.implicitWidth + 18
            height: 28
            radius: 14
            color: "#171012"
            border.width: 1
            border.color: "#6f4f6c"

            Text {
                id: seamLabel
                anchors.centerIn: parent
                text: "Renderer seam"
                color: "#d9c0d0"
                font.pixelSize: 13
                font.weight: Font.Medium
            }
        }
    }

    Text {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 18
        text: "Qt Quick can own the frame, meter rails, status chrome, and overlays while this viewport later becomes a Rust/WGPU host surface."
        color: "#b29f97"
        font.pixelSize: 14
        wrapMode: Text.WordWrap
    }
}
