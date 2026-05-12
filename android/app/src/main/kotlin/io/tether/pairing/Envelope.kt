package io.tether.pairing

import co.nstant.`in`.cbor.CborBuilder
import co.nstant.`in`.cbor.CborDecoder
import co.nstant.`in`.cbor.CborEncoder
import co.nstant.`in`.cbor.model.Array
import co.nstant.`in`.cbor.model.ByteString
import co.nstant.`in`.cbor.model.DataItem
import co.nstant.`in`.cbor.model.Map as CborMap
import co.nstant.`in`.cbor.model.Number
import co.nstant.`in`.cbor.model.UnicodeString
import co.nstant.`in`.cbor.model.UnsignedInteger
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.math.BigInteger

// CBOR envelope shared with the desktop side. We hand-roll the
// encoding/decoding because Bouncy Castle's CBOR is JVM-only and we
// want zero codegen dependencies on the Android side.

data class Envelope(
    val v: Int,
    val type: String,
    val id: Int,
    val inReplyTo: Int?,
    val body: CborMap,
)

object EnvelopeCodec {
    fun encode(env: Envelope): ByteArray {
        val builder = CborBuilder()
        val map = CborMap()
        map.put(UnicodeString("v"), UnsignedInteger(env.v.toLong()))
        map.put(UnicodeString("type"), UnicodeString(env.type))
        map.put(UnicodeString("id"), UnsignedInteger(env.id.toLong()))
        if (env.inReplyTo != null) {
            map.put(UnicodeString("in_reply_to"), UnsignedInteger(env.inReplyTo.toLong()))
        } else {
            map.put(UnicodeString("in_reply_to"), co.nstant.`in`.cbor.model.SimpleValue.NULL)
        }
        map.put(UnicodeString("body"), env.body)
        builder.add(map)
        val out = ByteArrayOutputStream()
        CborEncoder(out).encode(builder.build())
        return out.toByteArray()
    }

    fun decode(bytes: ByteArray): Envelope {
        val items = CborDecoder(ByteArrayInputStream(bytes)).decode()
        require(items.size == 1) { "expected exactly one CBOR item" }
        val top = items[0] as? CborMap ?: error("envelope must be a map")
        val v = (top[UnicodeString("v")] as Number).value.toInt()
        val type = (top[UnicodeString("type")] as UnicodeString).string
        val id = (top[UnicodeString("id")] as Number).value.toInt()
        val replyItem = top[UnicodeString("in_reply_to")]
        val inReplyTo: Int? = if (replyItem is Number) replyItem.value.toInt() else null
        val body = top[UnicodeString("body")] as? CborMap
            ?: error("body must be a map")
        return Envelope(v, type, id, inReplyTo, body)
    }
}

// Convenience builders + readers for the three concrete body shapes.

object Bodies {
    fun helloBody(
        deviceType: String,
        deviceName: String,
        protocolVersion: Int,
        ecdhPubkey: ByteArray,
        tlsCertSha256: ByteArray,
    ): CborMap {
        val m = CborMap()
        m.put(UnicodeString("device_type"), UnicodeString(deviceType))
        m.put(UnicodeString("device_name"), UnicodeString(deviceName))
        m.put(UnicodeString("protocol_version"), UnsignedInteger(protocolVersion.toLong()))
        m.put(UnicodeString("ecdh_pubkey"), ByteString(ecdhPubkey))
        m.put(UnicodeString("tls_cert_sha256"), ByteString(tlsCertSha256))
        return m
    }

    fun verifyBody(
        fingerprint: ByteArray,
        emojiIndices: IntArray,
        deviceName: String,
    ): CborMap {
        val m = CborMap()
        m.put(UnicodeString("fingerprint"), ByteString(fingerprint))
        val arr = Array()
        emojiIndices.forEach { arr.add(UnsignedInteger(it.toLong())) }
        m.put(UnicodeString("emoji_indices"), arr)
        m.put(UnicodeString("device_name"), UnicodeString(deviceName))
        return m
    }

    fun confirmBody(): CborMap {
        val m = CborMap()
        m.put(UnicodeString("confirmed"), co.nstant.`in`.cbor.model.SimpleValue.TRUE)
        return m
    }

    fun mismatchBody(reason: String): CborMap {
        val m = CborMap()
        m.put(UnicodeString("reason"), UnicodeString(reason))
        return m
    }

    fun readEcdhPubkey(body: CborMap): ByteArray =
        (body[UnicodeString("ecdh_pubkey")] as ByteString).bytes

    fun readTlsCertSha256(body: CborMap): ByteArray =
        (body[UnicodeString("tls_cert_sha256")] as ByteString).bytes

    fun readFingerprint(body: CborMap): ByteArray =
        (body[UnicodeString("fingerprint")] as ByteString).bytes

    fun readDeviceName(body: CborMap): String =
        (body[UnicodeString("device_name")] as UnicodeString).string

    fun readDeviceType(body: CborMap): String =
        (body[UnicodeString("device_type")] as UnicodeString).string

    fun readReason(body: CborMap): String =
        (body[UnicodeString("reason")] as UnicodeString).string
}

// Helper accessor — co.nstant CBOR's Map doesn't have a Kotlin-friendly
// `get`; this keeps call sites readable.
operator fun CborMap.get(key: DataItem): DataItem? = this.get(key)

@Suppress("unused")
private fun _suppressBigInt() = BigInteger.ONE
