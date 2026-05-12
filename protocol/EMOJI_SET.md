# Tether emoji verification set

A curated list of **256 visually-distinct, culturally-neutral emoji**
used to display the verification code during pairing. The set is
indexed 0–255; the three bytes of the HKDF-derived verifier directly
map to three entries in this list.

## Selection criteria

Every entry must satisfy ALL of these:

- **No human anatomy.** No faces, no hands, no body parts. Excludes
  most of the "Smileys & Emotion" and "People & Body" blocks.
- **No flags.** Avoids political/regional sensitivity.
- **No food.** Food emoji are heavily redrawn between platforms and
  often regionally specific (🍙 onigiri, 🥖 baguette, etc.).
- **No skin-tone modifiers.** All entries are single codepoints or
  unmodified sequences.
- **No ZWJ sequences.** Avoids platform inconsistency.
- **Visually distinct in 32×32 monochrome.** A user must be able to
  match across two slightly different screens — small details matter.
- **Rendered consistently in Android 8+ and Windows Segoe UI Emoji 11+.**
  Specifically excludes emoji that have been redrawn in Noto Color
  Emoji or Segoe in the last five years.

Indices 0–255 are stable. **Never reorder this list** — the index is
the protocol. New emoji are appended only if the list shrinks (which
it shouldn't).

## The 256

> Both desktop and Android keep a compile-time copy of this list. Any
> mismatch between the two copies will produce different emoji on each
> side of a successful handshake and abort pairing. Run
> `test-harness/runners/emoji-set-parity.test.ts` to verify.

### 0–63 — Animals (terrestrial, avian, aquatic, insectoid)

| idx | emoji | name |
|---:|:---:|---|
|   0 | 🐶 | dog |
|   1 | 🐱 | cat |
|   2 | 🐭 | mouse |
|   3 | 🐹 | hamster |
|   4 | 🐰 | rabbit |
|   5 | 🦊 | fox |
|   6 | 🐻 | bear |
|   7 | 🐼 | panda |
|   8 | 🐨 | koala |
|   9 | 🐯 | tiger face |
|  10 | 🦁 | lion |
|  11 | 🐮 | cow |
|  12 | 🐷 | pig |
|  13 | 🐽 | pig nose |
|  14 | 🐸 | frog |
|  15 | 🐵 | monkey face |
|  16 | 🐔 | chicken |
|  17 | 🐧 | penguin |
|  18 | 🐦 | bird |
|  19 | 🐤 | chick |
|  20 | 🦆 | duck |
|  21 | 🦅 | eagle |
|  22 | 🦉 | owl |
|  23 | 🦇 | bat |
|  24 | 🐺 | wolf |
|  25 | 🐗 | boar |
|  26 | 🐴 | horse face |
|  27 | 🦄 | unicorn |
|  28 | 🐝 | bee |
|  29 | 🪲 | beetle |
|  30 | 🦋 | butterfly |
|  31 | 🐌 | snail |
|  32 | 🐞 | ladybug |
|  33 | 🐢 | turtle |
|  34 | 🐍 | snake |
|  35 | 🦎 | lizard |
|  36 | 🦖 | t-rex |
|  37 | 🦕 | sauropod |
|  38 | 🐙 | octopus |
|  39 | 🦑 | squid |
|  40 | 🦞 | lobster |
|  41 | 🦀 | crab |
|  42 | 🐡 | blowfish |
|  43 | 🐠 | tropical fish |
|  44 | 🐟 | fish |
|  45 | 🐬 | dolphin |
|  46 | 🐳 | spouting whale |
|  47 | 🐋 | whale |
|  48 | 🦈 | shark |
|  49 | 🐊 | crocodile |
|  50 | 🐅 | tiger |
|  51 | 🐆 | leopard |
|  52 | 🦓 | zebra |
|  53 | 🦍 | gorilla |
|  54 | 🦧 | orangutan |
|  55 | 🐘 | elephant |
|  56 | 🦒 | giraffe |
|  57 | 🦏 | rhinoceros |
|  58 | 🐪 | camel |
|  59 | 🐫 | two-hump camel |
|  60 | 🦘 | kangaroo |
|  61 | 🐃 | water buffalo |
|  62 | 🐂 | ox |
|  63 | 🦬 | bison |

### 64–111 — Plants, nature, weather, celestial

| idx | emoji | name |
|---:|:---:|---|
|  64 | 🌵 | cactus |
|  65 | 🎄 | evergreen tree (decorated) |
|  66 | 🌲 | evergreen tree |
|  67 | 🌳 | deciduous tree |
|  68 | 🌴 | palm tree |
|  69 | 🌱 | seedling |
|  70 | 🌿 | herb |
|  71 | ☘️ | shamrock |
|  72 | 🍀 | four-leaf clover |
|  73 | 🎍 | pine decoration |
|  74 | 🪴 | potted plant |
|  75 | 🍃 | leaf in wind |
|  76 | 🍂 | fallen leaf |
|  77 | 🍁 | maple leaf |
|  78 | 🌺 | hibiscus |
|  79 | 🌻 | sunflower |
|  80 | 🌹 | rose |
|  81 | 🌷 | tulip |
|  82 | 🌼 | blossom |
|  83 | 🌸 | cherry blossom |
|  84 | 🌾 | sheaf of rice |
|  85 | 💐 | bouquet |
|  86 | 🍄 | mushroom |
|  87 | 🐚 | spiral shell |
|  88 | 🪨 | rock |
|  89 | 🪵 | wood |
|  90 | 🌍 | globe (Europe-Africa) |
|  91 | 🌎 | globe (Americas) |
|  92 | 🌏 | globe (Asia-Australia) |
|  93 | 🌐 | globe with meridians |
|  94 | 🌑 | new moon |
|  95 | 🌒 | waxing crescent moon |
|  96 | 🌓 | first quarter moon |
|  97 | 🌔 | waxing gibbous moon |
|  98 | 🌕 | full moon |
|  99 | 🌖 | waning gibbous moon |
| 100 | 🌗 | last quarter moon |
| 101 | 🌘 | waning crescent moon |
| 102 | 🌙 | crescent moon |
| 103 | 🪐 | ringed planet |
| 104 | ⭐ | star |
| 105 | 🌟 | glowing star |
| 106 | ✨ | sparkles |
| 107 | ⚡ | high voltage |
| 108 | 🔥 | fire |
| 109 | ❄️ | snowflake |
| 110 | 🌈 | rainbow |
| 111 | 💧 | droplet |

### 112–175 — Objects: music, sports, tools, household

| idx | emoji | name |
|---:|:---:|---|
| 112 | 🎵 | musical note |
| 113 | 🎶 | musical notes |
| 114 | 🎤 | microphone |
| 115 | 🎧 | headphones |
| 116 | 🎷 | saxophone |
| 117 | 🎸 | guitar |
| 118 | 🎹 | piano |
| 119 | 🥁 | drum |
| 120 | 🎺 | trumpet |
| 121 | 🪗 | accordion |
| 122 | 🪘 | long drum |
| 123 | 🪕 | banjo |
| 124 | 🎻 | violin |
| 125 | 📯 | postal horn |
| 126 | ⚽ | soccer ball |
| 127 | 🏀 | basketball |
| 128 | 🏈 | football |
| 129 | ⚾ | baseball |
| 130 | 🥎 | softball |
| 131 | 🏐 | volleyball |
| 132 | 🏉 | rugby ball |
| 133 | 🎾 | tennis |
| 134 | 🥏 | flying disc |
| 135 | 🎳 | bowling |
| 136 | 🏓 | ping pong |
| 137 | 🏸 | badminton |
| 138 | 🥅 | goal net |
| 139 | ⛳ | flag in hole |
| 140 | 🪁 | kite |
| 141 | 🏹 | bow and arrow |
| 142 | 🎣 | fishing pole |
| 143 | 🥊 | boxing glove |
| 144 | 🥋 | martial arts uniform |
| 145 | ⛸️ | ice skate |
| 146 | 🛷 | sled |
| 147 | 🥌 | curling stone |
| 148 | 🎯 | bullseye |
| 149 | 🪃 | boomerang |
| 150 | 🛹 | skateboard |
| 151 | 🪀 | yo-yo |
| 152 | 🔧 | wrench |
| 153 | 🔨 | hammer |
| 154 | ⚒️ | hammer and pick |
| 155 | 🛠️ | hammer and wrench |
| 156 | ⛏️ | pick |
| 157 | 🔩 | nut and bolt |
| 158 | ⚙️ | gear |
| 159 | 🧲 | magnet |
| 160 | 🔫 | water pistol |
| 161 | 💣 | bomb |
| 162 | 🧨 | firecracker |
| 163 | 🪓 | axe |
| 164 | 🔪 | knife |
| 165 | 🛡️ | shield |
| 166 | ⚔️ | crossed swords |
| 167 | 🗡️ | dagger |
| 168 | 🏺 | amphora |
| 169 | 🧱 | brick |
| 170 | 🪟 | window |
| 171 | 🪞 | mirror |
| 172 | 🛋️ | couch |
| 173 | 🪑 | chair |
| 174 | 🛁 | bathtub |
| 175 | 🪠 | plunger |

### 176–223 — Travel, vehicles, landmarks, places

| idx | emoji | name |
|---:|:---:|---|
| 176 | 🚗 | car |
| 177 | 🚕 | taxi |
| 178 | 🚙 | SUV |
| 179 | 🚌 | bus |
| 180 | 🚎 | trolleybus |
| 181 | 🏎️ | race car |
| 182 | 🚓 | police car |
| 183 | 🚑 | ambulance |
| 184 | 🚒 | fire engine |
| 185 | 🚐 | minibus |
| 186 | 🛻 | pickup truck |
| 187 | 🚚 | delivery truck |
| 188 | 🚛 | articulated lorry |
| 189 | 🚜 | tractor |
| 190 | 🛴 | kick scooter |
| 191 | 🚲 | bicycle |
| 192 | 🛵 | motor scooter |
| 193 | 🏍️ | motorcycle |
| 194 | 🛺 | auto rickshaw |
| 195 | 🚝 | monorail |
| 196 | 🚄 | bullet train |
| 197 | 🚅 | high-speed train |
| 198 | ✈️ | airplane |
| 199 | 🛩️ | small airplane |
| 200 | 🛫 | airplane departure |
| 201 | 🚁 | helicopter |
| 202 | 🛸 | flying saucer |
| 203 | 🚀 | rocket |
| 204 | 🛰️ | satellite |
| 205 | ⛵ | sailboat |
| 206 | 🛶 | canoe |
| 207 | 🚤 | speedboat |
| 208 | ⚓ | anchor |
| 209 | 🪝 | hook |
| 210 | ⛽ | fuel pump |
| 211 | 🚧 | construction |
| 212 | 🚦 | vertical traffic light |
| 213 | 🚥 | horizontal traffic light |
| 214 | 🗿 | moai |
| 215 | 🗽 | statue of liberty |
| 216 | 🎡 | ferris wheel |
| 217 | 🎢 | roller coaster |
| 218 | 🎠 | carousel horse |
| 219 | ⛲ | fountain |
| 220 | 🏖️ | beach with umbrella |
| 221 | 🏝️ | desert island |
| 222 | 🏜️ | desert |
| 223 | 🏔️ | snow-capped mountain |

### 224–255 — Shapes, geometric symbols (high contrast, very easy to verify)

| idx | emoji | name |
|---:|:---:|---|
| 224 | 🔺 | red triangle pointed up |
| 225 | 🔻 | red triangle pointed down |
| 226 | 🔸 | small orange diamond |
| 227 | 🔶 | large orange diamond |
| 228 | 🔷 | large blue diamond |
| 229 | 🔹 | small blue diamond |
| 230 | ⚪ | white circle |
| 231 | ⚫ | black circle |
| 232 | 🟠 | orange circle |
| 233 | 🟡 | yellow circle |
| 234 | 🟢 | green circle |
| 235 | 🟣 | purple circle |
| 236 | 🟤 | brown circle |
| 237 | 🔴 | red circle |
| 238 | 🟥 | red square |
| 239 | 🟧 | orange square |
| 240 | 🟨 | yellow square |
| 241 | 🟩 | green square |
| 242 | 🟦 | blue square |
| 243 | 🟪 | purple square |
| 244 | 🟫 | brown square |
| 245 | ⬛ | black large square |
| 246 | ⬜ | white large square |
| 247 | ◾ | black medium-small square |
| 248 | ◽ | white medium-small square |
| 249 | ▪️ | black small square |
| 250 | ▫️ | white small square |
| 251 | 🔲 | black square button |
| 252 | 🔳 | white square button |
| 253 | ⏺️ | record button |
| 254 | ⏹️ | stop button |
| 255 | ⏯️ | play-or-pause button |

## Entropy

256³ = 16,777,216 possible three-emoji codes. ≈24 bits of entropy per
session. Sufficient against a shoulder-surfing MITM during a single
~10-second confirmation window. Each new pairing attempt derives a
fresh shared secret and therefore fresh emojis, so brute-forcing the
display in advance is not possible.
