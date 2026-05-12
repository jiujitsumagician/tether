package io.tether.transport

import android.content.Context
import java.io.File
import java.math.BigInteger
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.MessageDigest
import java.security.SecureRandom
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.util.Date
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLSocket
import javax.net.ssl.SSLSocketFactory
import javax.net.ssl.TrustManager
import javax.net.ssl.X509TrustManager
import org.bouncycastle.cert.X509v3CertificateBuilder
import org.bouncycastle.cert.jcajce.JcaX509CertificateConverter
import org.bouncycastle.cert.jcajce.JcaX509v3CertificateBuilder
import org.bouncycastle.jce.provider.BouncyCastleProvider
import org.bouncycastle.operator.jcajce.JcaContentSignerBuilder
import org.bouncycastle.asn1.x500.X500Name
import java.io.ByteArrayInputStream

/**
 * TLS 1.3 client whose trust store accepts ANY cert. We pin on the
 * cert SHA-256 in the handshake layer (the emoji-confirm gate is what
 * the user uses to verify identity), so chain validation here is
 * deliberately disabled.
 */
class TlsClient private constructor(
    private val socket: SSLSocket,
    val peerCertSha256: ByteArray,
) {
    fun inputStream() = socket.inputStream
    fun outputStream() = socket.outputStream
    fun close() { try { socket.close() } catch (_: Throwable) {} }

    companion object {
        @Synchronized
        fun dial(host: String, port: Int): TlsClient {
            ensureBcInstalled()
            val ctx = SSLContext.getInstance("TLSv1.3")
            ctx.init(null, arrayOf<TrustManager>(AcceptAllTrustManager()), SecureRandom())
            val raw = ctx.socketFactory.createSocket(host, port) as SSLSocket
            // Force TLS 1.3 (rejects 1.2 fallback).
            raw.enabledProtocols = arrayOf("TLSv1.3")
            raw.startHandshake()
            val cert = raw.session.peerCertificates.first() as X509Certificate
            val der = cert.encoded
            val fp = MessageDigest.getInstance("SHA-256").digest(der)
            return TlsClient(raw, fp)
        }

        fun ownCertSha256(context: Context): ByteArray {
            val (_, der) = ensureLocalCert(context)
            return MessageDigest.getInstance("SHA-256").digest(der)
        }

        private fun ensureLocalCert(context: Context): Pair<KeyPair, ByteArray> {
            ensureBcInstalled()
            val dir = File(context.filesDir, "tether-tls")
            dir.mkdirs()
            val derFile = File(dir, "cert.der")
            val keyFile = File(dir, "key.bin")
            if (derFile.exists() && keyFile.exists()) {
                // Reconstruct the pair from disk for fingerprint
                // purposes; we don't actually use the private key on
                // the Android side yet (the phone always dials; never
                // listens for incoming TLS).
                val der = derFile.readBytes()
                val cf = CertificateFactory.getInstance("X.509")
                val cert = cf.generateCertificate(ByteArrayInputStream(der))
                val placeholder = KeyPairGenerator.getInstance("RSA").apply { initialize(2048) }.genKeyPair()
                @Suppress("UNUSED_VARIABLE")
                val _unused = cert
                return placeholder to der
            }
            // Generate a brand-new self-signed cert.
            val kpg = KeyPairGenerator.getInstance("RSA", BouncyCastleProvider.PROVIDER_NAME)
            kpg.initialize(2048)
            val kp = kpg.genKeyPair()
            val name = X500Name("CN=tether.local")
            val now = System.currentTimeMillis()
            val notBefore = Date(now - 24 * 60 * 60 * 1000)
            val notAfter = Date(now + 365L * 10 * 24 * 60 * 60 * 1000)
            val signer = JcaContentSignerBuilder("SHA256withRSA")
                .setProvider(BouncyCastleProvider.PROVIDER_NAME)
                .build(kp.private)
            val builder: X509v3CertificateBuilder =
                JcaX509v3CertificateBuilder(name, BigInteger.valueOf(now), notBefore, notAfter, name, kp.public)
            val holder = builder.build(signer)
            val cert = JcaX509CertificateConverter().setProvider(BouncyCastleProvider.PROVIDER_NAME)
                .getCertificate(holder)
            val der = cert.encoded
            derFile.writeBytes(der)
            keyFile.writeBytes(kp.private.encoded)
            return kp to der
        }

        private fun ensureBcInstalled() {
            if (java.security.Security.getProvider(BouncyCastleProvider.PROVIDER_NAME) == null) {
                java.security.Security.addProvider(BouncyCastleProvider())
            }
        }
    }
}

private class AcceptAllTrustManager : X509TrustManager {
    override fun checkClientTrusted(chain: Array<out X509Certificate>?, authType: String?) {}
    override fun checkServerTrusted(chain: Array<out X509Certificate>?, authType: String?) {}
    override fun getAcceptedIssuers(): Array<X509Certificate> = arrayOf()
}
