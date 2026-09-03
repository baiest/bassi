package com.bassi.nala.settings

/** Parses a `host:port` string, tolerating IPv6-like hosts by splitting on the last `:`. */
object HostPort {
    fun parse(raw: String): Pair<String, Int>? {
        val trimmed = raw.trim()
        val separatorIndex = trimmed.lastIndexOf(':')
        if (separatorIndex <= 0) return null
        val port = trimmed.substring(separatorIndex + 1).toIntOrNull() ?: return null
        return trimmed.substring(0, separatorIndex) to port
    }
}
