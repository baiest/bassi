package com.bassi.nala.settings

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class HostPortTest {

    @Test
    fun parsesAValidHostAndPort() {
        val result = HostPort.parse("192.168.1.10:4181")
        assertEquals("192.168.1.10" to 4181, result)
    }

    @Test
    fun trimsSurroundingWhitespace() {
        val result = HostPort.parse("  192.168.1.10:4181  ")
        assertEquals("192.168.1.10" to 4181, result)
    }

    @Test
    fun splitsOnTheLastColonForIpv6LikeInput() {
        val result = HostPort.parse("fe80::1:4181")
        assertEquals("fe80::1" to 4181, result)
    }

    @Test
    fun rejectsANonNumericPort() {
        assertNull(HostPort.parse("192.168.1.10:abc"))
    }

    @Test
    fun rejectsAnEmptyHost() {
        assertNull(HostPort.parse(":4181"))
    }

    @Test
    fun rejectsMissingColon() {
        assertNull(HostPort.parse("192.168.1.10"))
    }
}
