//! Maps a 16-byte HKDF verifier to three emojis from the curated set.
//!
//! The first three bytes of the verifier become indices into the
//! 256-entry table. The bytes range 0..=255, which is exactly the
//! index space.

/// The 256 curated emojis, indexed identically to
/// `protocol/EMOJI_SET.md`. **NEVER REORDER.**
///
/// Both desktop and Android keep an independent copy of this list;
/// the parity test in `test-harness/runners/emoji-set-parity.test.ts`
/// verifies they stay in sync.
pub const EMOJIS: [&str; 256] = [
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
    // 112–175 — Objects: music / sports / tools / household
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
];

const _: () = assert!(EMOJIS.len() == 256, "EMOJIS table must have 256 entries");

/// Pull three emoji out of the 16-byte verifier.
pub fn from_verifier(verifier: &[u8; 16]) -> [&'static str; 3] {
    [
        EMOJIS[verifier[0] as usize],
        EMOJIS[verifier[1] as usize],
        EMOJIS[verifier[2] as usize],
    ]
}

/// The three indices, surfaced separately so the wire payload can
/// carry them as an `[u8; 3]` array.
pub fn indices_from_verifier(verifier: &[u8; 16]) -> [u8; 3] {
    [verifier[0], verifier[1], verifier[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_has_256() {
        assert_eq!(EMOJIS.len(), 256);
    }

    #[test]
    fn indices_round_trip() {
        let verifier = [0x05u8, 0x6c, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let emojis = from_verifier(&verifier);
        assert_eq!(emojis[0], EMOJIS[5]);
        assert_eq!(emojis[1], EMOJIS[108]);
        assert_eq!(emojis[2], EMOJIS[255]);
    }

    #[test]
    fn no_empty_entries() {
        for (i, e) in EMOJIS.iter().enumerate() {
            assert!(!e.is_empty(), "emoji at index {i} was empty");
        }
    }
}
