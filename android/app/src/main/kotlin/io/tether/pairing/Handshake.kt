package io.tether.pairing

import org.bouncycastle.crypto.agreement.X25519Agreement
import org.bouncycastle.crypto.generators.HKDFBytesGenerator
import org.bouncycastle.crypto.digests.SHA256Digest
import org.bouncycastle.crypto.params.HKDFParameters
import org.bouncycastle.crypto.params.X25519KeyGenerationParameters
import org.bouncycastle.crypto.params.X25519PrivateKeyParameters
import org.bouncycastle.crypto.params.X25519PublicKeyParameters
import org.bouncycastle.crypto.generators.X25519KeyPairGenerator
import java.security.SecureRandom

private val HKDF_INFO = "tether/verify/v1".toByteArray()
const val VERIFIER_LEN = 16

class Handshake {
    private val generator = X25519KeyPairGenerator().apply {
        init(X25519KeyGenerationParameters(SecureRandom()))
    }
    private val keyPair = generator.generateKeyPair()
    private val privateKey: X25519PrivateKeyParameters =
        keyPair.private as X25519PrivateKeyParameters
    private val publicKey: X25519PublicKeyParameters =
        keyPair.public as X25519PublicKeyParameters

    val localPubBytes: ByteArray
        get() = publicKey.encoded

    fun derive(peerPubBytes: ByteArray): ByteArray {
        require(peerPubBytes.size == 32) {
            "peer pubkey must be 32 bytes (got ${peerPubBytes.size})"
        }
        val peer = X25519PublicKeyParameters(peerPubBytes, 0)
        val agreement = X25519Agreement().apply { init(privateKey) }
        val shared = ByteArray(agreement.agreementSize)
        agreement.calculateAgreement(peer, shared, 0)

        val hkdf = HKDFBytesGenerator(SHA256Digest())
        hkdf.init(HKDFParameters(shared, null, HKDF_INFO))
        val out = ByteArray(VERIFIER_LEN)
        hkdf.generateBytes(out, 0, out.size)
        return out
    }
}
