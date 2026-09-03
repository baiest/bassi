package com.bassi.nala.ui.core

/**
 * What the core is doing right now, driving both its color ([CoreColor])
 * and its pulse ([NalaCoreView]). Mirrors `apps/nala-overlay/src/status.rs`.
 */
enum class CoreStatus { IDLE, LISTENING, SENDING, SPEAKING, ERROR }
