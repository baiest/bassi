package com.bassi.nala.ui.core

/**
 * A plain RGBA color, kept free of `android.graphics.Color` so this stays
 * testable with plain JUnit (no Robolectric, no Android stub jar). Converted
 * to a packed `android.graphics.Color` int only at draw time, in
 * [NalaCoreView].
 */
data class RgbaColor(val red: Int, val green: Int, val blue: Int, val alpha: Int = 255)

/**
 * Maps a [CoreStatus] to the color the core should be — pure and separate
 * from any drawing code. Ported 1:1 from `apps/nala-overlay/src/color.rs`:
 * blue at rest, warm while acting, red for an error.
 */
object CoreColor {

    fun statusColor(status: CoreStatus): RgbaColor = when (status) {
        CoreStatus.IDLE -> RgbaColor(40, 110, 210)
        CoreStatus.LISTENING -> RgbaColor(90, 200, 255)
        CoreStatus.SENDING -> RgbaColor(180, 120, 255)
        CoreStatus.SPEAKING -> RgbaColor(60, 220, 130)
        CoreStatus.ERROR -> RgbaColor(230, 60, 60)
    }

    /**
     * The lighter color painted on the point cloud and orbital rings —
     * same hue family as [statusColor], but brighter.
     */
    fun accentColor(status: CoreStatus): RgbaColor = when (status) {
        CoreStatus.IDLE -> RgbaColor(120, 190, 255)
        CoreStatus.LISTENING -> RgbaColor(180, 230, 255)
        CoreStatus.SENDING -> RgbaColor(215, 180, 255)
        CoreStatus.SPEAKING -> RgbaColor(160, 240, 190)
        CoreStatus.ERROR -> RgbaColor(255, 150, 150)
    }

    /** The soft, low-alpha halo painted behind the core. Alpha fixed at 70. */
    fun glowColor(status: CoreStatus): RgbaColor {
        val base = statusColor(status)
        return base.copy(alpha = 70)
    }
}
