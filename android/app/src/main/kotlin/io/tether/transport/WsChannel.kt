package io.tether.transport

import io.tether.pairing.Envelope
import io.tether.pairing.EnvelopeCodec
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.IOException
import java.security.SecureRandom
import java.util.Base64

/**
 * Minimal WebSocket-on-TLS framing for the handshake control channel.
 *
 * We don't need a full WS implementation — only:
 *   1. A client upgrade against `/tether/v1`.
 *   2. Binary frames in/out carrying CBOR-encoded envelopes.
 *
 * RFC 6455 frames are tiny to hand-roll. Doing so avoids pulling in
 * OkHttp's full HTTP machinery just for a single upgrade.
 */
class WsChannel private constructor(private val tls: TlsClient) {
    private val input = tls.inputStream()
    private val output = tls.outputStream()
    private val rng = SecureRandom()

    fun sendEnvelope(env: Envelope) {
        val payload = EnvelopeCodec.encode(env)
        writeFrame(opcode = 0x2, masked = true, payload = payload)
        output.flush()
    }

    fun recvEnvelope(): Envelope? {
        val payload = readFrame() ?: return null
        return EnvelopeCodec.decode(payload)
    }

    fun close() {
        try { writeFrame(opcode = 0x8, masked = true, payload = ByteArray(0)) } catch (_: Throwable) {}
        tls.close()
    }

    private fun writeFrame(opcode: Int, masked: Boolean, payload: ByteArray) {
        val b0 = 0x80 or (opcode and 0x0f) // FIN + opcode
        output.write(b0)
        val len = payload.size
        when {
            len < 126 -> output.write((if (masked) 0x80 else 0x00) or len)
            len < 0x10000 -> {
                output.write((if (masked) 0x80 else 0x00) or 126)
                output.write((len shr 8) and 0xff)
                output.write(len and 0xff)
            }
            else -> {
                output.write((if (masked) 0x80 else 0x00) or 127)
                for (i in 7 downTo 0) output.write(((len.toLong() shr (i * 8)) and 0xffL).toInt())
            }
        }
        if (masked) {
            val mask = ByteArray(4).also { rng.nextBytes(it) }
            output.write(mask)
            for (i in payload.indices) output.write(payload[i].toInt() xor mask[i % 4].toInt())
        } else {
            output.write(payload)
        }
    }

    private fun readFrame(): ByteArray? {
        while (true) {
            val b0 = input.read()
            if (b0 < 0) return null
            val opcode = b0 and 0x0f
            val b1 = input.read()
            if (b1 < 0) return null
            val masked = (b1 and 0x80) != 0
            var len = (b1 and 0x7f).toLong()
            when (len) {
                126L -> {
                    val h = input.read(); val l = input.read()
                    if (h < 0 || l < 0) return null
                    len = ((h and 0xff) shl 8 or (l and 0xff)).toLong()
                }
                127L -> {
                    var n = 0L
                    for (i in 0 until 8) {
                        val b = input.read()
                        if (b < 0) return null
                        n = (n shl 8) or (b and 0xff).toLong()
                    }
                    len = n
                }
            }
            val mask = if (masked) {
                val arr = ByteArray(4)
                if (input.read(arr) != 4) return null
                arr
            } else null
            val payload = ByteArray(len.toInt())
            var read = 0
            while (read < payload.size) {
                val r = input.read(payload, read, payload.size - read)
                if (r < 0) return null
                read += r
            }
            if (mask != null) {
                for (i in payload.indices) payload[i] = (payload[i].toInt() xor mask[i % 4].toInt()).toByte()
            }
            when (opcode) {
                0x1, 0x2 -> return payload      // text or binary
                0x8 -> return null              // close
                0x9 -> writeFrame(0xA, masked = true, payload = payload) // ping → pong
                0xA -> continue                 // pong
                else -> continue                // unknown control frame, ignore
            }
        }
    }

    companion object {
        suspend fun upgrade(tls: TlsClient, pin: String?): WsChannel = withContext(Dispatchers.IO) {
            val path = "/tether/v1" + (pin?.let { "?pin=${urlencode(it)}" } ?: "")
            val key = generateWsKey()
            val req = buildString {
                append("GET ").append(path).append(" HTTP/1.1\r\n")
                append("Host: tether.local\r\n")
                append("Upgrade: websocket\r\n")
                append("Connection: Upgrade\r\n")
                append("Sec-WebSocket-Key: ").append(key).append("\r\n")
                append("Sec-WebSocket-Version: 13\r\n")
                append("\r\n")
            }
            tls.outputStream().write(req.toByteArray())
            tls.outputStream().flush()
            // Read the response headers — we accept any 101 status as
            // success because the server is our own code.
            val responseLine = readLineFromStream(tls.inputStream())
                ?: throw IOException("server closed before WS upgrade")
            if (!responseLine.contains(" 101 ")) {
                throw IOException("WS upgrade failed: $responseLine")
            }
            // Drain headers.
            while (true) {
                val l = readLineFromStream(tls.inputStream()) ?: throw IOException("EOF in WS headers")
                if (l.isEmpty()) break
            }
            WsChannel(tls)
        }

        private fun generateWsKey(): String {
            val bytes = ByteArray(16)
            SecureRandom().nextBytes(bytes)
            return Base64.getEncoder().encodeToString(bytes)
        }

        private fun urlencode(s: String): String =
            s.toByteArray().joinToString("") { b ->
                val c = b.toInt() and 0xff
                if (c in 0x30..0x39 || c in 0x41..0x5a || c in 0x61..0x7a ||
                    c == 0x2d || c == 0x5f || c == 0x2e || c == 0x7e) c.toChar().toString()
                else "%%%02X".format(c)
            }

        private fun readLineFromStream(input: java.io.InputStream): String? {
            val sb = StringBuilder()
            while (true) {
                val b = input.read()
                if (b < 0) return null
                if (b == '\r'.code) {
                    val next = input.read()
                    if (next == '\n'.code) return sb.toString()
                    if (next < 0) return null
                    sb.append('\r')
                    sb.append(next.toChar())
                } else {
                    sb.append(b.toChar())
                }
            }
        }
    }
}
