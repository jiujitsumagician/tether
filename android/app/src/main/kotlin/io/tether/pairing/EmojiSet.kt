package io.tether.pairing

// Mirror of desktop/src-tauri/src/pairing/emoji_code.rs and
// protocol/EMOJI_SET.md. NEVER REORDER — the index is the protocol.

val TETHER_EMOJIS: List<String> = listOf(
    // 0–63 — Animals
    "🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐨", "🐯", "🦁", "🐮", "🐷", "🐽", "🐸",
    "🐵", "🐔", "🐧", "🐦", "🐤", "🦆", "🦅", "🦉", "🦇", "🐺", "🐗", "🐴", "🦄", "🐝", "🪲",
    "🦋", "🐌", "🐞", "🐢", "🐍", "🦎", "🦖", "🦕", "🐙", "🦑", "🦞", "🦀", "🐡", "🐠", "🐟",
    "🐬", "🐳", "🐋", "🦈", "🐊", "🐅", "🐆", "🦓", "🦍", "🦧", "🐘", "🦒", "🦏", "🐪", "🐫",
    "🦘", "🐃", "🐂", "🦬",
    // 64–111 — Plants / nature / weather / celestial
    "🌵", "🎄", "🌲", "🌳", "🌴", "🌱", "🌿", "☘️", "🍀", "🎍", "🪴", "🍃", "🍂", "🍁", "🌺",
    "🌻", "🌹", "🌷", "🌼", "🌸", "🌾", "💐", "🍄", "🐚", "🪨", "🪵", "🌍", "🌎", "🌏", "🌐",
    "🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘", "🌙", "🪐", "⭐", "🌟", "✨", "⚡", "🔥",
    "❄️", "🌈", "💧",
    // 112–175 — Objects
    "🎵", "🎶", "🎤", "🎧", "🎷", "🎸", "🎹", "🥁", "🎺", "🪗", "🪘", "🪕", "🎻", "📯", "⚽",
    "🏀", "🏈", "⚾", "🥎", "🏐", "🏉", "🎾", "🥏", "🎳", "🏓", "🏸", "🥅", "⛳", "🪁", "🏹",
    "🎣", "🥊", "🥋", "⛸️", "🛷", "🥌", "🎯", "🪃", "🛹", "🪀", "🔧", "🔨", "⚒️", "🛠️", "⛏️",
    "🔩", "⚙️", "🧲", "🔫", "💣", "🧨", "🪓", "🔪", "🛡️", "⚔️", "🗡️", "🏺", "🧱", "🪟", "🪞",
    "🛋️", "🪑", "🛁", "🪠",
    // 176–223 — Travel / vehicles / landmarks
    "🚗", "🚕", "🚙", "🚌", "🚎", "🏎️", "🚓", "🚑", "🚒", "🚐", "🛻", "🚚", "🚛", "🚜", "🛴",
    "🚲", "🛵", "🏍️", "🛺", "🚝", "🚄", "🚅", "✈️", "🛩️", "🛫", "🚁", "🛸", "🚀", "🛰️", "⛵",
    "🛶", "🚤", "⚓", "🪝", "⛽", "🚧", "🚦", "🚥", "🗿", "🗽", "🎡", "🎢", "🎠", "⛲", "🏖️",
    "🏝️", "🏜️", "🏔️",
    // 224–255 — Shapes / geometric symbols
    "🔺", "🔻", "🔸", "🔶", "🔷", "🔹", "⚪", "⚫", "🟠", "🟡", "🟢", "🟣", "🟤", "🔴", "🟥",
    "🟧", "🟨", "🟩", "🟦", "🟪", "🟫", "⬛", "⬜", "◾", "◽", "▪️", "▫️", "🔲", "🔳", "⏺️",
    "⏹️", "⏯️",
)

// Runtime assertion lives in a property initialiser rather than a
// top-level init block (Kotlin only allows init blocks inside classes).
@Suppress("unused")
private val TETHER_EMOJIS_LEN_CHECK: Boolean = run {
    check(TETHER_EMOJIS.size == 256) {
        "TETHER_EMOJIS must have exactly 256 entries (got ${TETHER_EMOJIS.size})"
    }
    true
}

fun emojisFromVerifier(verifier: ByteArray): List<String> {
    require(verifier.size >= 3) { "verifier must be at least 3 bytes" }
    return listOf(
        TETHER_EMOJIS[verifier[0].toInt() and 0xff],
        TETHER_EMOJIS[verifier[1].toInt() and 0xff],
        TETHER_EMOJIS[verifier[2].toInt() and 0xff],
    )
}

fun indicesFromVerifier(verifier: ByteArray): IntArray {
    require(verifier.size >= 3)
    return intArrayOf(
        verifier[0].toInt() and 0xff,
        verifier[1].toInt() and 0xff,
        verifier[2].toInt() and 0xff,
    )
}
