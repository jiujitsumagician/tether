package io.tether.pairing

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HandshakeTest {

    @Test
    fun `two sides derive the same verifier`() {
        val alice = Handshake()
        val bob = Handshake()
        val va = alice.derive(bob.localPubBytes)
        val vb = bob.derive(alice.localPubBytes)
        assertArrayEquals(
            "both sides of the handshake must produce identical verifiers",
            va, vb
        )
        assertEquals(VERIFIER_LEN, va.size)
    }

    @Test
    fun `different pairs produce different verifiers`() {
        val alice = Handshake()
        val bob = Handshake()
        val eve = Handshake()
        val honestVerifier = alice.derive(bob.localPubBytes)
        val mitmVerifier = alice.derive(eve.localPubBytes)
        // Astronomically unlikely (~ 2^-128) for two unrelated DH
        // exchanges to collide. If this ever fails we've broken
        // something far more dramatic than the test.
        assertFalse(
            "pubkey substitution must produce a different verifier (MITM defense)",
            honestVerifier.contentEquals(mitmVerifier)
        )
    }

    @Test
    fun `verifier always derives three emoji indices`() {
        val alice = Handshake()
        val bob = Handshake()
        val verifier = alice.derive(bob.localPubBytes)
        val indices = indicesFromVerifier(verifier)
        val emojis = emojisFromVerifier(verifier)
        assertEquals(3, indices.size)
        assertEquals(3, emojis.size)
        indices.forEach { i -> assertTrue("index $i out of range", i in 0..255) }
        emojis.forEach { e -> assertTrue("emoji empty", e.isNotEmpty()) }
    }

    @Test
    fun `pubkey is 32 bytes`() {
        assertEquals(32, Handshake().localPubBytes.size)
    }
}

class EmojiSetTest {
    @Test
    fun `emoji table has exactly 256 entries`() {
        assertEquals(256, TETHER_EMOJIS.size)
    }

    @Test
    fun `no emoji entry is empty`() {
        TETHER_EMOJIS.forEachIndexed { i, e ->
            assertTrue("emoji $i is empty", e.isNotEmpty())
        }
    }

    @Test
    fun `verifier byte maps to indexed emoji`() {
        val verifier = byteArrayOf(0x05, 0x6C.toByte(), 0xFF.toByte(),
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        val emojis = emojisFromVerifier(verifier)
        assertEquals(TETHER_EMOJIS[5], emojis[0])
        assertEquals(TETHER_EMOJIS[108], emojis[1])
        assertEquals(TETHER_EMOJIS[255], emojis[2])
    }
}
